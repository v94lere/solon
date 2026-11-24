//! Relais DNS de la machine, pour mettre un nom sur les adresses extérieures de la radiographie.
//!
//! Les conteneurs interrogeaient directement les serveurs DNS de l'hôte Windows : personne dans
//! Solon ne voyait passer les réponses, et une adresse extérieure s'affichait en chiffres. Ce
//! relais écoute sur l'adresse de la machine (port 53), transmet chaque question aux serveurs de
//! l'hôte, rend la réponse telle quelle, et note au passage « cette adresse, c'est ce nom ».
//!
//! Il écoute en UDP et en TCP. Le TCP sert aux réponses trop grosses pour un datagramme (le client
//! y revient de lui-même quand la réponse UDP porte le drapeau TC) et aux clients qui le préfèrent.
//! Vers l'amont, une question restée sans réponse en UDP est reposée en TCP : certains réseaux
//! laissent tomber en UDP ce qui n'est pas une simple demande d'adresse (SRV, TXT…), et le client
//! ne voyait alors qu'un délai dépassé (ticket 13).
//!
//! Il ne change rien d'autre : pas de cache, pas de filtrage, pas de réécriture. Et il n'est pas
//! un point de défaillance : `resolv.conf` le cite en premier, les serveurs de l'hôte ensuite, et
//! la bibliothèque de résolution passe au suivant sur délai. S'il ne démarre pas (port pris), la
//! configuration d'avant s'applique telle quelle.
//!
//! Pourquoi en espace utilisateur et pas par une sonde : une sonde sur le port 53 verrait la même
//! chose mais demanderait un programme noyau de plus à maintenir, pour un trafic qui ne compte pas
//! en performance. Le relais tient en deux cents lignes de bibliothèque standard.
//!
//! Rien ne quitte la machine autrement que vers les serveurs que l'hôte utilisait déjà.

// Sous Windows, l'agent n'est compilé que pour ses tests : le relais n'y est appelé par personne.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Délai accordé à chaque serveur amont avant d'essayer le suivant. Court exprès : le client
/// réessaie de lui-même au bout de 2 s, autant lui répondre avant.
const UPSTREAM_TIMEOUT: Duration = Duration::from_millis(1500);
/// Délai de connexion TCP vers un serveur amont. Un serveur sans DNS en TCP refuse tout de suite ;
/// seul un serveur muet coûte ce délai.
const TCP_CONNECT_TIMEOUT: Duration = Duration::from_millis(1000);
/// Une connexion TCP cliente qui ne dit rien pendant ce temps est fermée.
const TCP_CLIENT_IDLE: Duration = Duration::from_secs(5);
/// Au-delà de cette taille, une réponse obtenue en TCP n'est pas rendue telle quelle à un client
/// UDP : il reçoit un message tronqué (drapeau TC) et revient en TCP. 1 232 octets est la taille
/// qui passe partout sans fragmentation (DNS Flag Day 2020).
const UDP_SAFE: usize = 1232;
/// Taille maximale d'un message UDP : une réponse EDNS peut dépasser 4 096 octets, et la tronquer
/// sans le drapeau TC ferait échouer le client.
const MAX_PACKET: usize = 65_535;
/// Questions servies en même temps. Au-delà, les suivantes sont abandonnées : le client réessaie,
/// et surtout une rafale (amont injoignable, application qui boucle) ne crée pas des milliers de
/// fils et de ports éphémères dans une machine de 2 Go.
const MAX_IN_FLIGHT: usize = 64;
/// Noms gardés. Au-delà, les plus anciens partent : un développeur n'a pas des milliers de
/// services distants, et la table doit rester petite dans une machine qui tient en 50 Mo.
const MAX_NAMES: usize = 4096;

struct Entry {
    name: String,
    seen: Instant,
}

static NAMES: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
/// Adresse sur laquelle le relais écoute, une fois démarré.
static LISTENING: OnceLock<String> = OnceLock::new();
static IN_FLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// L'adresse du relais en marche, s'il y en a un.
pub fn relay_address() -> Option<&'static str> {
    LISTENING.get().map(String::as_str)
}

fn names() -> &'static Mutex<HashMap<String, Entry>> {
    NAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Le nom le plus récemment résolu vers cette adresse, s'il est connu.
pub fn name_of(ip: &str) -> Option<String> {
    names().lock().unwrap().get(ip).map(|e| e.name.clone())
}

fn remember(ip: String, name: &str) {
    let mut table = names().lock().unwrap();
    if table.len() >= MAX_NAMES && !table.contains_key(&ip) {
        // On enlève la moitié la plus ancienne d'un coup, plutôt qu'une entrée à chaque fois.
        let mut by_age: Vec<(String, Instant)> =
            table.iter().map(|(k, e)| (k.clone(), e.seen)).collect();
        by_age.sort_by_key(|(_, t)| *t);
        for (k, _) in by_age.into_iter().take(MAX_NAMES / 2) {
            table.remove(&k);
        }
    }
    table.insert(
        ip,
        Entry {
            name: name.to_owned(),
            seen: Instant::now(),
        },
    );
}

/// Démarre le relais sur `address:53`, vers `upstream`. `Ok` si l'écoute est en place ; sinon le
/// motif, et l'appelant garde la configuration directe.
pub fn start(address: &str, upstream: &[String]) -> Result<(), String> {
    if upstream.is_empty() {
        return Err("aucun serveur amont".into());
    }
    let bind: SocketAddr = format!("{address}:53")
        .parse()
        .map_err(|e| format!("adresse d'écoute {address} : {e}"))?;
    let socket = UdpSocket::bind(bind).map_err(|e| format!("écoute {bind} : {e}"))?;
    let upstream: Vec<SocketAddr> = upstream
        .iter()
        .filter_map(|s| format!("{s}:53").parse().ok())
        .collect();
    if upstream.is_empty() {
        return Err("serveurs amont illisibles".into());
    }
    // Le TCP est un plus : s'il ne s'ouvre pas, le relais UDP d'avant reste tel quel.
    match TcpListener::bind(bind) {
        Ok(listener) => {
            let upstream = upstream.clone();
            let _ = std::thread::Builder::new()
                .name("dns-tcp".into())
                .spawn(move || serve_tcp(listener, upstream));
        }
        // `eprintln!` et non `system::log` : ce module se compile aussi sous Windows pour ses tests.
        Err(e) => eprintln!("[solon-agent] relais DNS : pas d'écoute TCP sur {bind} ({e})"),
    }
    std::thread::Builder::new()
        .name("dns".into())
        .spawn(move || serve(socket, upstream))
        .map_err(|e| format!("fil du relais : {e}"))?;
    let _ = LISTENING.set(address.to_owned());
    Ok(())
}

fn serve(socket: UdpSocket, upstream: Vec<SocketAddr>) {
    let mut buf = vec![0u8; MAX_PACKET];
    loop {
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let query = buf[..n].to_vec();
        let reply_socket = match socket.try_clone() {
            Ok(s) => s,
            Err(_) => continue,
        };
        use std::sync::atomic::Ordering;
        if IN_FLIGHT.load(Ordering::Relaxed) >= MAX_IN_FLIGHT {
            continue;
        }
        IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        let upstream = upstream.clone();
        // Un fil par question, en nombre borné : le DNS d'un poste de développement se compte en
        // questions par seconde, et un serveur amont lent ne doit pas bloquer les autres.
        let spawned = std::thread::Builder::new().spawn(move || {
            if let Some(answer) = resolve(&query, &upstream) {
                learn(&answer);
                let _ = reply_socket.send_to(&answer, from);
            }
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        });
        if spawned.is_err() {
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

fn learn(answer: &[u8]) {
    if let Some((name, ips)) = parse_answers(answer) {
        for ip in ips {
            remember(ip, &name);
        }
    }
}

/// Accepte les clients TCP. Même plafond que l'UDP : au-delà, la connexion est refermée aussitôt.
fn serve_tcp(listener: TcpListener, upstream: Vec<SocketAddr>) {
    use std::sync::atomic::Ordering;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if IN_FLIGHT.load(Ordering::Relaxed) >= MAX_IN_FLIGHT {
            continue;
        }
        IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        let upstream = upstream.clone();
        let spawned = std::thread::Builder::new().spawn(move || {
            serve_tcp_client(stream, &upstream);
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        });
        if spawned.is_err() {
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Sert un client TCP : des messages précédés de leur longueur sur deux octets, l'un après l'autre,
/// jusqu'à ce qu'il raccroche ou se taise.
fn serve_tcp_client(mut stream: TcpStream, upstream: &[SocketAddr]) {
    let _ = stream.set_read_timeout(Some(TCP_CLIENT_IDLE));
    let _ = stream.set_write_timeout(Some(TCP_CLIENT_IDLE));
    while let Some(query) = read_framed(&mut stream) {
        // Le client est venu en TCP, le plus souvent parce que la réponse ne tenait pas en UDP :
        // on pose la question en TCP d'abord. Un amont sans TCP (beaucoup de box) répond en UDP.
        let Some(answer) = resolve_tcp(&query, upstream).or_else(|| resolve_udp(&query, upstream))
        else {
            return;
        };
        learn(&answer);
        if write_framed(&mut stream, &answer).is_err() {
            return;
        }
    }
}

fn read_framed(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut len = [0u8; 2];
    stream.read_exact(&mut len).ok()?;
    let mut msg = vec![0u8; u16::from_be_bytes(len) as usize];
    stream.read_exact(&mut msg).ok()?;
    Some(msg)
}

fn write_framed(stream: &mut TcpStream, msg: &[u8]) -> std::io::Result<()> {
    let len = u16::try_from(msg.len()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
    // Un seul envoi : longueur et message dans le même segment.
    let mut out = Vec::with_capacity(msg.len() + 2);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(msg);
    stream.write_all(&out)
}

/// Réponse pour un client UDP : l'UDP d'abord ; si aucun serveur n'a répondu, le TCP.
fn resolve(query: &[u8], upstream: &[SocketAddr]) -> Option<Vec<u8>> {
    if let Some(answer) = resolve_udp(query, upstream) {
        return Some(answer);
    }
    let answer = resolve_tcp(query, upstream)?;
    if answer.len() <= UDP_SAFE {
        Some(answer)
    } else {
        truncated(query)
    }
}

/// La question renvoyée comme réponse vide marquée « tronquée » : le client la repose en TCP.
fn truncated(query: &[u8]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let (_, end) = read_name(query, 12)?;
    let mut m = query.get(..end + 4)?.to_vec();
    m[2] |= 0x80 | 0x02; // QR, TC
    m[3] = 0x80; // RA, RCODE = 0
    m[4..6].copy_from_slice(&1u16.to_be_bytes());
    m[6..12].fill(0); // ni réponse, ni autorité, ni additionnel
    Some(m)
}

/// Pose la question en TCP à chaque serveur amont tour à tour, jusqu'à la première réponse.
fn resolve_tcp(query: &[u8], upstream: &[SocketAddr]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    for server in upstream {
        let Ok(mut s) = TcpStream::connect_timeout(server, TCP_CONNECT_TIMEOUT) else {
            continue;
        };
        if s.set_read_timeout(Some(UPSTREAM_TIMEOUT)).is_err()
            || s.set_write_timeout(Some(UPSTREAM_TIMEOUT)).is_err()
            || write_framed(&mut s, query).is_err()
        {
            continue;
        }
        match read_framed(&mut s) {
            Some(answer) if answer.len() >= 12 && answer[..2] == query[..2] => return Some(answer),
            _ => continue,
        }
    }
    None
}

/// Pose la question en UDP à chaque serveur amont tour à tour, jusqu'à la première réponse.
fn resolve_udp(query: &[u8], upstream: &[SocketAddr]) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let id = &query[..2];
    for server in upstream {
        let Ok(s) = UdpSocket::bind(if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        }) else {
            continue;
        };
        // `connect` : seules les réponses de ce serveur sont acceptées.
        if s.connect(server).is_err()
            || s.set_read_timeout(Some(UPSTREAM_TIMEOUT)).is_err()
            || s.send(query).is_err()
        {
            continue;
        }
        let mut buf = vec![0u8; MAX_PACKET];
        // Une réponse qui ne porte pas notre identifiant est une réponse tardive à autre chose.
        let deadline = Instant::now() + UPSTREAM_TIMEOUT;
        while Instant::now() < deadline {
            match s.recv(&mut buf) {
                Ok(n) if n >= 12 && &buf[..2] == id => return Some(buf[..n].to_vec()),
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
    None
}

/// Le nom demandé et les adresses (A, AAAA) de la section réponse. `None` si le message n'est
/// pas une réponse lisible ou n'apporte aucune adresse.
///
/// Les adresses sont rattachées au **nom demandé**, pas à la cible d'un éventuel `CNAME` : c'est
/// `api.stripe.com` que le développeur reconnaît, pas le nom du répartiteur derrière.
fn parse_answers(msg: &[u8]) -> Option<(String, Vec<String>)> {
    if msg.len() < 12 || msg[2] & 0x80 == 0 {
        return None;
    }
    let qdcount = u16::from_be_bytes([msg[4], msg[5]]) as usize;
    let ancount = u16::from_be_bytes([msg[6], msg[7]]) as usize;
    if qdcount == 0 || ancount == 0 {
        return None;
    }
    let mut pos = 12;
    let (qname, next) = read_name(msg, pos)?;
    pos = next + 4; // QTYPE, QCLASS
    for _ in 1..qdcount {
        let (_, next) = read_name(msg, pos)?;
        pos = next + 4;
    }
    let mut ips = Vec::new();
    for _ in 0..ancount {
        let (_, next) = read_name(msg, pos)?;
        pos = next;
        if pos + 10 > msg.len() {
            break;
        }
        let rtype = u16::from_be_bytes([msg[pos], msg[pos + 1]]);
        let rdlen = u16::from_be_bytes([msg[pos + 8], msg[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlen > msg.len() {
            break;
        }
        let rdata = &msg[pos..pos + rdlen];
        match (rtype, rdlen) {
            (1, 4) => ips.push(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]).to_string()),
            (28, 16) => {
                let mut o = [0u8; 16];
                o.copy_from_slice(rdata);
                ips.push(Ipv6Addr::from(o).to_string());
            }
            _ => {}
        }
        pos += rdlen;
    }
    (!ips.is_empty()).then_some((qname, ips))
}

/// Lit un nom à `pos` (étiquettes, pointeurs de compression compris) ; rend le nom et la position
/// juste après lui dans le flux. Borné contre les pointeurs en boucle.
fn read_name(msg: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end: Option<usize> = None;
    let mut hops = 0;
    loop {
        let len = *msg.get(pos)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | *msg.get(pos + 1)? as usize;
            if end.is_none() {
                end = Some(pos + 2);
            }
            hops += 1;
            if hops > 16 || ptr >= msg.len() {
                return None;
            }
            pos = ptr;
            continue;
        }
        let label = msg.get(pos + 1..pos + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).to_ascii_lowercase());
        pos += 1 + len;
    }
    Some((labels.join("."), end.unwrap_or(pos)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Réponse à `api.example.com A` : un CNAME vers `edge.cdn.net`, puis deux A, avec des
    /// pointeurs de compression comme les serveurs en produisent.
    fn reponse() -> Vec<u8> {
        let mut m = vec![
            0x12, 0x34, // id
            0x81, 0x80, // réponse, récursion
            0, 1, // QDCOUNT
            0, 3, // ANCOUNT
            0, 0, 0, 0,
        ];
        // Question : api.example.com (débute en 12)
        m.extend_from_slice(b"\x03api\x07example\x03com\x00");
        m.extend_from_slice(&[0, 1, 0, 1]);
        // CNAME : nom = pointeur vers 12
        m.extend_from_slice(&[0xC0, 12, 0, 5, 0, 1, 0, 0, 0, 60]);
        let target = b"\x04edge\x03cdn\x03net\x00";
        m.extend_from_slice(&[0, target.len() as u8]);
        let target_pos = m.len();
        m.extend_from_slice(target);
        // A : nom = pointeur vers la cible du CNAME
        for ip in [[93u8, 184, 216, 34], [93, 184, 216, 35]] {
            m.extend_from_slice(&[0xC0, target_pos as u8, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
            m.extend_from_slice(&ip);
        }
        m
    }

    #[test]
    fn les_adresses_sont_rattachees_au_nom_demande() {
        let (name, ips) = parse_answers(&reponse()).unwrap();
        assert_eq!(name, "api.example.com");
        assert_eq!(ips, vec!["93.184.216.34", "93.184.216.35"]);
    }

    #[test]
    fn une_question_sans_reponse_n_apporte_rien() {
        let mut m = reponse();
        m[7] = 0; // ANCOUNT = 0
        assert!(parse_answers(&m).is_none());
        // Une question (pas une réponse) non plus.
        let mut q = reponse();
        q[2] = 0x01;
        assert!(parse_answers(&q).is_none());
        // Et un message tronqué ne panique pas.
        let m = reponse();
        for cut in 0..m.len() {
            let _ = parse_answers(&m[..cut]);
        }
    }

    #[test]
    fn un_pointeur_en_boucle_est_rejete() {
        let mut m = vec![0u8; 12];
        m[2] = 0x80;
        m[5] = 1;
        m[7] = 1;
        m.extend_from_slice(&[0xC0, 12]); // pointe sur lui-même
        assert!(parse_answers(&m).is_none());
    }

    /// Question `_sip._tcp.example.com SRV`, identifiant 0xBEEF, avec un enregistrement OPT.
    fn question_srv() -> Vec<u8> {
        let mut q = vec![0xBE, 0xEF, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 1];
        q.extend_from_slice(b"\x04_sip\x04_tcp\x07example\x03com\x00");
        q.extend_from_slice(&[0, 33, 0, 1]);
        q.extend_from_slice(&[0, 0, 41, 0x10, 0, 0, 0, 0, 0, 0, 0]); // OPT
        q
    }

    /// Faux serveur amont : muet en UDP, il répond en TCP par `answer_len` octets (en-tête de la
    /// question, puis du remplissage). Rend son adresse.
    fn amont_tcp_seulement(answer_len: usize) -> SocketAddr {
        // Le même numéro de port en TCP et en UDP (ouvert mais muet : la question part et rien ne
        // revient). Le port que le système donne en TCP peut être pris en UDP : on recommence.
        let (listener, mute, addr) = (0..50)
            .find_map(|_| {
                let listener = TcpListener::bind("127.0.0.1:0").ok()?;
                let addr = listener.local_addr().ok()?;
                let mute = UdpSocket::bind(addr).ok()?;
                Some((listener, mute, addr))
            })
            .expect("un port libre en TCP et en UDP");
        std::thread::spawn(move || {
            let _mute = mute;
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                while let Some(q) = read_framed(&mut stream) {
                    let mut a = q[..12].to_vec();
                    a[2] |= 0x80;
                    a.resize(answer_len, 0);
                    write_framed(&mut stream, &a).unwrap();
                }
            }
        });
        addr
    }

    #[test]
    fn un_amont_muet_en_udp_est_interroge_en_tcp() {
        let amont = amont_tcp_seulement(200);
        let q = question_srv();
        let a = resolve(&q, &[amont]).expect("réponse par TCP");
        assert_eq!(a.len(), 200);
        assert_eq!(a[..2], q[..2]);
    }

    #[test]
    fn une_grosse_reponse_tcp_revient_tronquee_au_client_udp() {
        let amont = amont_tcp_seulement(3000);
        let q = question_srv();
        let a = resolve(&q, &[amont]).expect("réponse tronquée");
        assert!(a.len() < q.len(), "la question seule, sans l'OPT");
        assert_eq!(a[..2], q[..2]);
        assert_eq!(a[2] & 0x82, 0x82, "réponse, tronquée");
        assert_eq!(a[4..12], [0, 1, 0, 0, 0, 0, 0, 0]);
        let (name, _) = read_name(&a, 12).unwrap();
        assert_eq!(name, "_sip._tcp.example.com");
    }

    #[test]
    fn un_client_tcp_recoit_la_reponse_entiere() {
        let amont = amont_tcp_seulement(3000);
        let relais = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = relais.local_addr().unwrap();
        std::thread::spawn(move || serve_tcp(relais, vec![amont]));
        let mut client = TcpStream::connect(addr).unwrap();
        // Deux questions sur la même connexion.
        for _ in 0..2 {
            write_framed(&mut client, &question_srv()).unwrap();
            let a = read_framed(&mut client).expect("réponse");
            assert_eq!(a.len(), 3000);
        }
    }

    #[test]
    fn une_question_illisible_ne_donne_rien() {
        assert!(truncated(&[0u8; 5]).is_none());
        assert!(resolve_tcp(&[0u8; 5], &[]).is_none());
    }

    #[test]
    fn la_table_se_souvient_du_dernier_nom() {
        remember("203.0.113.9".into(), "a.example");
        remember("203.0.113.9".into(), "b.example");
        assert_eq!(name_of("203.0.113.9").as_deref(), Some("b.example"));
        assert_eq!(name_of("203.0.113.10"), None);
    }
}
