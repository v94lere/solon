//! Relais DNS de la machine, pour mettre un nom sur les adresses extérieures de la radiographie.
//!
//! Les conteneurs interrogeaient directement les serveurs DNS de l'hôte Windows : personne dans
//! Solon ne voyait passer les réponses, et une adresse extérieure s'affichait en chiffres. Ce
//! relais écoute sur l'adresse de la machine (port 53), transmet chaque question aux serveurs de
//! l'hôte, rend la réponse telle quelle, et note au passage « cette adresse, c'est ce nom ».
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
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Délai accordé à chaque serveur amont avant d'essayer le suivant. Court exprès : le client
/// réessaie de lui-même au bout de 2 s, autant lui répondre avant.
const UPSTREAM_TIMEOUT: Duration = Duration::from_millis(1500);
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
                if let Some((name, ips)) = parse_answers(&answer) {
                    for ip in ips {
                        remember(ip, &name);
                    }
                }
                let _ = reply_socket.send_to(&answer, from);
            }
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        });
        if spawned.is_err() {
            IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Pose la question à chaque serveur amont tour à tour, jusqu'à la première réponse.
fn resolve(query: &[u8], upstream: &[SocketAddr]) -> Option<Vec<u8>> {
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

    #[test]
    fn la_table_se_souvient_du_dernier_nom() {
        remember("203.0.113.9".into(), "a.example");
        remember("203.0.113.9".into(), "b.example");
        assert_eq!(name_of("203.0.113.9").as_deref(), Some("b.example"));
        assert_eq!(name_of("203.0.113.10"), None);
    }
}
