//! Domaines locaux : `nom.solon.local` (et `service.projet.solon.local` pour Compose) joignables en
//! HTTP **et HTTPS** depuis Windows, **sans publier de port**.
//!
//! Quatre pièces :
//! 1. une **route** Windows vers le réseau des conteneurs (`10.90.0.0/16`) via l'adresse de la machine,
//!    posée au démarrage du moteur et retirée à l'arrêt ; l'agent autorise ce trafic côté Linux ;
//! 2. un bloc géré dans le fichier `hosts` qui fait pointer chaque nom vers `127.0.0.1` ;
//! 3. un mandataire sur `127.0.0.1:80` (HTTP) et `127.0.0.1:443` (HTTPS) qui lit le nom demandé
//!    (`Host` ou SNI) et relaie vers `ip_du_conteneur:port` — le port est le premier port TCP
//!    **exposé** par l'image (80, 8080, 3000… en priorité), publié ou non ;
//! 4. une **autorité de certification locale** (`%ProgramData%\Solon\ca`), créée une fois et installée
//!    dans le magasin racine de la machine, qui signe à la volée un certificat par nom demandé.
//!
//! Un nom inconnu reçoit une page 404 listant les domaines disponibles.

use std::collections::BTreeMap;
use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use solon_core::protocol::ContainerEndpoint;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

pub const SUFFIX: &str = "solon.local";
/// Réseau des conteneurs (voir `default-address-pools` dans `image/rootfs/files/daemon.json`).
pub const CONTAINER_NET: &str = "10.90.0.0";
pub const CONTAINER_MASK: &str = "255.255.0.0";
const HOSTS_BEGIN: &str = "# solon-begin (géré par Solon, ne pas modifier)";
const HOSTS_END: &str = "# solon-end";
/// Marqueurs écrits avant le renommage du projet (Monodon), retirés s'ils sont encore présents.
const LEGACY_HOSTS_BEGIN: &str = "# monodon-begin (géré par Monodon, ne pas modifier)";
const LEGACY_HOSTS_END: &str = "# monodon-end";
/// Ports HTTP habituels, par ordre de préférence quand une image en expose plusieurs.
const PREFERRED_PORTS: &[u16] = &[
    80, 8080, 3000, 8000, 8069, 5000, 4200, 5173, 8888, 9000, 443,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub ip: Ipv4Addr,
    pub port: u16,
}

/// Nom de domaine → cible dans le réseau des conteneurs.
pub type DomainMap = BTreeMap<String, Target>;
pub type SharedDomains = Arc<RwLock<DomainMap>>;

/// Ne garde que lettres, chiffres et tirets (les `_` et `.` deviennent `-`), en minuscules.
fn sanitize(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for c in label.trim_start_matches('/').chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == '_' || c == '-' || c == '.' {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// Port HTTP probable d'un conteneur : exposé par l'image (préférences ci-dessus), sinon publié, sinon 80.
fn pick_port(e: &ContainerEndpoint) -> u16 {
    for p in PREFERRED_PORTS {
        if e.exposed_tcp.contains(p) {
            return *p;
        }
    }
    if let Some(p) = e.exposed_tcp.first() {
        return *p;
    }
    if let Some(p) = e.published_tcp.first() {
        return *p;
    }
    80
}

/// Table des domaines : pour chaque conteneur en marche ayant une adresse, `nom.solon.local` (et
/// `service.projet.solon.local` si c'est un service Compose) → `ip:port`.
pub fn domains_for(endpoints: &[ContainerEndpoint]) -> DomainMap {
    let mut map = DomainMap::new();
    for e in endpoints {
        let Ok(ip) = e.ip.parse::<Ipv4Addr>() else {
            continue;
        };
        let target = Target {
            ip,
            port: pick_port(e),
        };
        let name = sanitize(&e.name);
        if !name.is_empty() {
            map.entry(format!("{name}.{SUFFIX}")).or_insert(target);
        }
        if let (Some(project), Some(service)) = (&e.compose_project, &e.compose_service) {
            let (p, s) = (sanitize(project), sanitize(service));
            if !p.is_empty() && !s.is_empty() {
                map.entry(format!("{s}.{p}.{SUFFIX}")).or_insert(target);
            }
        }
    }
    map
}

// ---------------------------------------------------------------------------------------------
// Route Windows vers le réseau des conteneurs
// ---------------------------------------------------------------------------------------------

fn route_cmd(args: &[&str]) -> io::Result<bool> {
    let out = std::process::Command::new("route.exe")
        .args(args)
        .output()?;
    Ok(out.status.success())
}

/// Ajoute la route `10.90.0.0/16 → <machine>` (idempotent).
pub fn ensure_route(guest: Ipv4Addr) -> io::Result<()> {
    let g = guest.to_string();
    let _ = route_cmd(&["delete", CONTAINER_NET, "mask", CONTAINER_MASK]);
    if route_cmd(&[
        "add",
        CONTAINER_NET,
        "mask",
        CONTAINER_MASK,
        &g,
        "metric",
        "5",
    ])? {
        tracing::info!(guest = %g, "route vers le réseau des conteneurs ({CONTAINER_NET}/16) ajoutée");
        Ok(())
    } else {
        Err(io::Error::other("route add a échoué"))
    }
}

pub fn remove_route() {
    let _ = route_cmd(&["delete", CONTAINER_NET, "mask", CONTAINER_MASK]);
}

// ---------------------------------------------------------------------------------------------
// Fichier hosts
// ---------------------------------------------------------------------------------------------

fn hosts_path() -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts")
}

/// Réécrit le bloc géré du fichier `hosts` (idempotent : rien n'est écrit si le contenu est identique).
pub fn write_hosts_block(domains: &DomainMap) -> io::Result<()> {
    let path = hosts_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let mut kept = String::with_capacity(current.len());
    let mut skipping = false;
    for line in current.lines() {
        if line.trim() == HOSTS_BEGIN || line.trim() == LEGACY_HOSTS_BEGIN {
            skipping = true;
            continue;
        }
        if line.trim() == HOSTS_END || line.trim() == LEGACY_HOSTS_END {
            skipping = false;
            continue;
        }
        if !skipping {
            kept.push_str(line);
            kept.push_str("\r\n");
        }
    }
    let mut next = kept.trim_end().to_owned();
    if !domains.is_empty() {
        next.push_str("\r\n");
        next.push_str(HOSTS_BEGIN);
        next.push_str("\r\n");
        for name in domains.keys() {
            next.push_str(&format!("127.0.0.1 {name}\r\n"));
        }
        next.push_str(HOSTS_END);
    }
    next.push_str("\r\n");
    if next == current {
        return Ok(());
    }
    // Le fichier `hosts` est surveillé par l'antivirus : juste après une écriture, la suivante peut
    // tomber sur « fichier utilisé par un autre processus » (erreur 32). Constaté quand deux conteneurs
    // d'un même projet démarrent à une seconde d'écart : les adresses du second manquaient. On réessaie.
    let mut last = None;
    for attempt in 0..20 {
        match std::fs::write(&path, &next) {
            Ok(()) => return Ok(()),
            Err(e)
                if e.raw_os_error() == Some(32) || e.kind() == io::ErrorKind::PermissionDenied =>
            {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50 + 50 * attempt));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("hosts : écriture impossible")))
}

// ---------------------------------------------------------------------------------------------
// Autorité de certification locale et certificats à la volée
// ---------------------------------------------------------------------------------------------

/// Retire l'héritage des droits sur `dir` et n'accorde le contrôle total qu'à SYSTEM (S-1-5-18) et aux
/// administrateurs (S-1-5-32-544) ; les fichiers déjà présents sont remis en héritage simple (`/reset`)
/// pour qu'ils suivent le dossier. Idempotent. (Pas de `/T` : appliquer `(OI)(CI)` à un fichier lui
/// laisse une liste de droits vide, que même SYSTEM ne peut plus lire.)
fn restrict_to_admins(dir: &Path) {
    let icacls = |args: &[&std::ffi::OsStr]| match std::process::Command::new("icacls.exe")
        .args(args)
        .output()
    {
        Ok(o) if o.status.success() => {}
        Ok(o) => tracing::warn!(
            "droits du dossier de l'autorité : {}",
            String::from_utf8_lossy(&o.stdout).trim()
        ),
        Err(e) => tracing::warn!("icacls introuvable : {e}"),
    };
    icacls(&[
        dir.as_os_str(),
        "/inheritance:r".as_ref(),
        "/grant:r".as_ref(),
        "*S-1-5-18:(OI)(CI)F".as_ref(),
        "*S-1-5-32-544:(OI)(CI)F".as_ref(),
        "/Q".as_ref(),
    ]);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            icacls(&[entry.path().as_os_str(), "/reset".as_ref(), "/Q".as_ref()]);
        }
    }
}

/// Paramètres (fixes) du certificat de l'autorité : 10 ans, contrainte CA, signature de certificats.
fn ca_params() -> io::Result<rcgen::CertificateParams> {
    let mut params =
        rcgen::CertificateParams::new(Vec::<String>::new()).map_err(io::Error::other)?;
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Solon Local CA");
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Solon");
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
        rcgen::KeyUsagePurpose::DigitalSignature,
    ];
    params.not_before = rcgen::date_time_ymd(2026, 1, 1);
    params.not_after = rcgen::date_time_ymd(2036, 1, 1);
    Ok(params)
}

pub struct LocalCa {
    issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
    leaves: std::sync::Mutex<std::collections::HashMap<String, Arc<rustls::sign::CertifiedKey>>>,
}

impl LocalCa {
    /// Charge l'autorité depuis `dir`, ou la crée (clé + certificat, 10 ans) et l'installe dans le
    /// magasin « Autorités de certification racines de confiance » de la machine.
    pub fn load_or_create(dir: &Path) -> io::Result<Arc<LocalCa>> {
        Self::load_or_create_with(dir, true)
    }

    /// `restrict` : réserve le dossier (donc la clé privée) à SYSTEM et aux administrateurs, car
    /// `%ProgramData%` est lisible par tous les utilisateurs par défaut. Désactivable pour les tests
    /// (exécutés sans élévation).
    fn load_or_create_with(dir: &Path, restrict: bool) -> io::Result<Arc<LocalCa>> {
        std::fs::create_dir_all(dir)?;
        if restrict {
            restrict_to_admins(dir);
        }
        let key_path = dir.join("solon-ca.key");
        let cert_path = dir.join("solon-ca.pem");
        let cer_path = dir.join("solon-ca.cer");
        let key_pair = if key_path.is_file() && cert_path.is_file() {
            rcgen::KeyPair::from_pem(&std::fs::read_to_string(&key_path)?)
                .map_err(|e| io::Error::other(format!("clé de l'autorité : {e}")))?
        } else {
            let key = rcgen::KeyPair::generate().map_err(io::Error::other)?;
            let cert = ca_params()?.self_signed(&key).map_err(io::Error::other)?;
            std::fs::write(&key_path, key.serialize_pem())?;
            std::fs::write(&cert_path, cert.pem())?;
            std::fs::write(&cer_path, cert.der())?;
            // Installation dans le magasin racine de la machine (le service tourne en LocalSystem).
            match std::process::Command::new("certutil.exe")
                .args(["-addstore", "-f", "Root"])
                .arg(&cer_path)
                .output()
            {
                Ok(o) if o.status.success() => {
                    tracing::info!(
                        "autorité locale « Solon Local CA » installée dans le magasin racine"
                    );
                    // L'autorité de l'ancien nom du projet (Monodon) n'a plus d'usage.
                    let _ = std::process::Command::new("certutil.exe")
                        .args(["-delstore", "Root", "Monodon Local CA"])
                        .output();
                }
                Ok(o) => tracing::warn!("certutil : {}", String::from_utf8_lossy(&o.stdout).trim()),
                Err(e) => tracing::warn!("certutil introuvable : {e}"),
            }
            key
        };
        // Les paramètres de l'autorité sont déterministes : l'émetteur reconstruit ainsi le même nom
        // et le même identifiant de clé que le certificat écrit sur disque.
        let issuer = rcgen::Issuer::new(ca_params()?, key_pair);
        Ok(Arc::new(LocalCa {
            issuer,
            leaves: Default::default(),
        }))
    }

    /// Certificat pour `host`, signé par l'autorité, mis en cache.
    fn leaf(&self, host: &str) -> Option<Arc<rustls::sign::CertifiedKey>> {
        if let Some(c) = self.leaves.lock().ok()?.get(host) {
            return Some(c.clone());
        }
        let key = rcgen::KeyPair::generate().ok()?;
        let mut params = rcgen::CertificateParams::new(vec![host.to_owned()]).ok()?;
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, host);
        params.not_before = rcgen::date_time_ymd(2026, 1, 1);
        params.not_after = rcgen::date_time_ymd(2036, 1, 1);
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let cert = params.signed_by(&key, &self.issuer).ok()?;
        let der = rustls::pki_types::CertificateDer::from(cert.der().to_vec());
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der()).ok()?;
        let signing = rustls::crypto::ring::sign::any_supported_type(&key_der).ok()?;
        let ck = Arc::new(rustls::sign::CertifiedKey::new(vec![der], signing));
        self.leaves.lock().ok()?.insert(host.to_owned(), ck.clone());
        Some(ck)
    }
}

impl std::fmt::Debug for LocalCa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalCa")
    }
}

impl rustls::server::ResolvesServerCert for LocalCa {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let host = client_hello.server_name()?.to_ascii_lowercase();
        self.leaf(&host)
    }
}

// ---------------------------------------------------------------------------------------------
// Mandataires HTTP (80) et HTTPS (443)
// ---------------------------------------------------------------------------------------------

/// Réserve 127.0.0.1:80 ; échoue si le port est pris (IIS, autre serveur local).
pub async fn bind_proxy() -> io::Result<TcpListener> {
    let listener = TcpListener::bind(("127.0.0.1", 80)).await?;
    tracing::info!("domaines locaux : mandataire HTTP sur 127.0.0.1:80 (*.{SUFFIX})");
    Ok(listener)
}

/// Réserve 127.0.0.1:443 pour le HTTPS.
pub async fn bind_tls_proxy() -> io::Result<TcpListener> {
    let listener = TcpListener::bind(("127.0.0.1", 443)).await?;
    tracing::info!("domaines locaux : mandataire HTTPS sur 127.0.0.1:443");
    Ok(listener)
}

/// Sert le mandataire HTTP sur le port réservé par [`bind_proxy`].
pub async fn serve_proxy(
    listener: TcpListener,
    domains: SharedDomains,
    sleeper: Arc<crate::sleep::Sleeper>,
) -> io::Result<()> {
    loop {
        let (client, _) = listener.accept().await?;
        let domains = domains.clone();
        let sleeper = sleeper.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(client, domains, sleeper, "http").await {
                tracing::debug!("domaine local : {e}");
            }
        });
    }
}

/// Sert le mandataire HTTPS : poignée de main TLS avec un certificat signé par l'autorité locale,
/// puis relais en clair vers le conteneur.
pub async fn serve_tls_proxy(
    listener: TcpListener,
    domains: SharedDomains,
    ca: Arc<LocalCa>,
    sleeper: Arc<crate::sleep::Sleeper>,
) -> io::Result<()> {
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(ca);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    loop {
        let (client, _) = listener.accept().await?;
        let domains = domains.clone();
        let acceptor = acceptor.clone();
        let sleeper = sleeper.clone();
        tokio::spawn(async move {
            match acceptor.accept(client).await {
                Ok(tls) => {
                    if let Err(e) = handle(tls, domains, sleeper, "https").await {
                        tracing::debug!("domaine local (https) : {e}");
                    }
                }
                Err(e) => tracing::debug!("poignée de main TLS : {e}"),
            }
        });
    }
}

/// Lit depuis `r` jusqu'à la fin d'un en-tête HTTP (`\r\n\r\n`) ; `buf` peut déjà contenir un début
/// (reste de la lecture précédente). Renvoie la longueur de l'en-tête, `None` si la connexion se ferme
/// ou si l'en-tête dépasse 64 Ko.
async fn read_head<R: tokio::io::AsyncRead + Unpin>(
    r: &mut R,
    buf: &mut Vec<u8>,
) -> io::Result<Option<usize>> {
    loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(Some(p + 4));
        }
        if buf.len() > 64 * 1024 {
            return Ok(None);
        }
        let mut chunk = [0u8; 8192];
        let n = r.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Corps annoncé par un en-tête de requête.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    Length(u64),
    Chunked,
    /// `Connection: upgrade` (WebSocket) : après l'en-tête, la connexion devient un tunnel brut.
    Upgrade,
}

/// Réécrit l'en-tête d'une requête : conserve la ligne de requête et les en-têtes du client, remplace
/// les en-têtes de transfert par ceux du mandataire (`X-Forwarded-Proto`, `X-Forwarded-Host`,
/// `X-Forwarded-For`, `X-Forwarded-Port`, `X-Real-IP`, `Forwarded`). Les applications derrière un
/// mandataire (WordPress, Odoo en `proxy_mode`, n8n, Nextcloud…) s'en servent pour générer des liens
/// `https://` quand la page a été demandée en HTTPS. Renvoie aussi la nature du corps qui suit.
fn rewrite_head(head: &[u8], scheme: &str) -> (Vec<u8>, Body) {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut out = String::with_capacity(head.len() + 200);
    out.push_str(request_line);
    out.push_str("\r\n");
    let mut host_header = String::new();
    let mut body = Body::Length(0);
    let mut upgrade = false;
    let mut connection_upgrade = false;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            out.push_str(line);
            out.push_str("\r\n");
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("host") {
            host_header = value.to_owned();
        } else if key.eq_ignore_ascii_case("content-length") {
            if let Ok(n) = value.parse::<u64>() {
                if body != Body::Chunked {
                    body = Body::Length(n);
                }
            }
        } else if key.eq_ignore_ascii_case("transfer-encoding") {
            if value.to_ascii_lowercase().contains("chunked") {
                body = Body::Chunked;
            }
        } else if key.eq_ignore_ascii_case("upgrade") {
            upgrade = true;
        } else if key.eq_ignore_ascii_case("connection") {
            if value.to_ascii_lowercase().contains("upgrade") {
                connection_upgrade = true;
            }
        } else if key.eq_ignore_ascii_case("x-forwarded-proto")
            || key.eq_ignore_ascii_case("x-forwarded-host")
            || key.eq_ignore_ascii_case("x-forwarded-for")
            || key.eq_ignore_ascii_case("x-forwarded-port")
            || key.eq_ignore_ascii_case("x-real-ip")
            || key.eq_ignore_ascii_case("forwarded")
        {
            // Remplacés ci-dessous : seul le mandataire fait foi.
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    let port = if scheme == "https" { 443 } else { 80 };
    out.push_str(&format!("X-Forwarded-Proto: {scheme}\r\n"));
    if !host_header.is_empty() {
        out.push_str(&format!("X-Forwarded-Host: {host_header}\r\n"));
    }
    out.push_str(&format!("X-Forwarded-Port: {port}\r\n"));
    out.push_str("X-Forwarded-For: 127.0.0.1\r\n");
    out.push_str("X-Real-IP: 127.0.0.1\r\n");
    if host_header.is_empty() {
        out.push_str(&format!("Forwarded: for=127.0.0.1;proto={scheme}\r\n"));
    } else {
        out.push_str(&format!(
            "Forwarded: for=127.0.0.1;host={host_header};proto={scheme}\r\n"
        ));
    }
    out.push_str("\r\n");
    if upgrade && connection_upgrade {
        body = Body::Upgrade;
    }
    (out.into_bytes(), body)
}

/// Relaie exactement `n` octets de `r` vers `w`, en commençant par ce que `buf` contient déjà.
async fn forward_exact<R, W>(r: &mut R, w: &mut W, buf: &mut Vec<u8>, mut n: u64) -> io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let take = (buf.len() as u64).min(n) as usize;
    if take > 0 {
        w.write_all(&buf[..take]).await?;
        buf.drain(..take);
        n -= take as u64;
    }
    let mut chunk = [0u8; 16384];
    while n > 0 {
        let want = (chunk.len() as u64).min(n) as usize;
        let got = r.read(&mut chunk[..want]).await?;
        if got == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        w.write_all(&chunk[..got]).await?;
        n -= got as u64;
    }
    Ok(())
}

/// Lit une ligne terminée par `\r\n` (rendue sans le terminateur), en complétant `buf` au besoin.
async fn read_line<R: tokio::io::AsyncRead + Unpin>(
    r: &mut R,
    buf: &mut Vec<u8>,
) -> io::Result<Vec<u8>> {
    loop {
        if let Some(p) = buf.windows(2).position(|w| w == b"\r\n") {
            let line = buf[..p].to_vec();
            buf.drain(..p + 2);
            return Ok(line);
        }
        if buf.len() > 64 * 1024 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut chunk = [0u8; 8192];
        let n = r.read(&mut chunk).await?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Relaie un corps `Transfer-Encoding: chunked` : chaque morceau (taille en hexadécimal, données,
/// `\r\n`), puis le morceau vide et les éventuels en-têtes de fin.
async fn forward_chunked<R, W>(r: &mut R, w: &mut W, buf: &mut Vec<u8>) -> io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    loop {
        let line = read_line(r, buf).await?;
        let size_text = String::from_utf8_lossy(&line);
        let size_text = size_text.split(';').next().unwrap_or("").trim();
        let size = u64::from_str_radix(size_text, 16).map_err(|_| io::ErrorKind::InvalidData)?;
        w.write_all(&line).await?;
        w.write_all(b"\r\n").await?;
        if size == 0 {
            // En-têtes de fin, jusqu'à la ligne vide.
            loop {
                let trailer = read_line(r, buf).await?;
                w.write_all(&trailer).await?;
                w.write_all(b"\r\n").await?;
                if trailer.is_empty() {
                    return Ok(());
                }
            }
        }
        forward_exact(r, w, buf, size + 2).await?;
    }
}

async fn handle<S>(
    client: S,
    domains: SharedDomains,
    sleeper: Arc<crate::sleep::Sleeper>,
    scheme: &'static str,
) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut client_rx, mut client) = tokio::io::split(client);
    let mut buf = Vec::with_capacity(8192);
    let Some(mut head_end) = read_head(&mut client_rx, &mut buf).await? else {
        return Ok(());
    };
    let host = String::from_utf8_lossy(&buf[..head_end])
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("host")
                .then(|| v.trim().to_ascii_lowercase())
        })
        .unwrap_or_default();
    let host = host.split(':').next().unwrap_or("").to_owned();
    let target = domains.read().await.get(&host).copied();
    let Some(target) = target else {
        let list = domains.read().await;
        let body = not_found_page(&host, &list);
        let resp = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        client.write_all(resp.as_bytes()).await?;
        return Ok(());
    };
    // Réveille le conteneur s'il dort ; la garde le maintient éveillé le temps de la requête.
    let _guard = sleeper.on_connection_ip(&target.ip.to_string()).await;
    let upstream = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect((target.ip, target.port)),
    )
    .await
    {
        Ok(Ok(s)) => s,
        _ => {
            let body = format!(
                "<!doctype html><meta charset=\"utf-8\"><title>Solon</title><body style=\"font-family:Segoe UI,sans-serif;max-width:40em;margin:4em auto;color:#222\"><h1 style=\"font-weight:600\">Solon</h1><p>The container behind <code>{host}</code> is not answering on port {} ({}).</p><p>Check that it listens on that port, or publish the right one.</p></body>",
                target.port, target.ip
            );
            let resp = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            client.write_all(resp.as_bytes()).await?;
            return Ok(());
        }
    };
    // Réponses : copie brute du conteneur vers le client, en parallèle des requêtes.
    let (mut upstream_rx, mut upstream) = upstream.into_split();
    let pump = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut upstream_rx, &mut client).await;
        let _ = client.shutdown().await;
    });
    // Requêtes : chaque en-tête est réécrit (en-têtes `X-Forwarded-*`), le corps relayé tel quel ;
    // la connexion peut porter plusieurs requêtes (keep-alive) ou devenir un tunnel (WebSocket).
    let result: io::Result<()> = async {
        loop {
            let (head, body) = rewrite_head(&buf[..head_end], scheme);
            buf.drain(..head_end);
            upstream.write_all(&head).await?;
            match body {
                Body::Upgrade => {
                    upstream.write_all(&buf).await?;
                    buf.clear();
                    let _ = tokio::io::copy(&mut client_rx, &mut upstream).await;
                    return Ok(());
                }
                Body::Length(n) => {
                    forward_exact(&mut client_rx, &mut upstream, &mut buf, n).await?
                }
                Body::Chunked => forward_chunked(&mut client_rx, &mut upstream, &mut buf).await?,
            }
            match read_head(&mut client_rx, &mut buf).await? {
                Some(n) => head_end = n,
                None => return Ok(()),
            }
        }
    }
    .await;
    let _ = upstream.shutdown().await;
    if result.is_err() {
        pump.abort();
    }
    let _ = pump.await;
    result
}

fn not_found_page(host: &str, domains: &DomainMap) -> String {
    let mut items = String::new();
    for (name, t) in domains {
        items.push_str(&format!(
            "<li><a href=\"http://{name}/\">{name}</a> <small>→ {}:{}</small></li>",
            t.ip, t.port
        ));
    }
    if items.is_empty() {
        items.push_str("<li><em>no running container yet</em></li>");
    }
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Solon</title>\
<body style=\"font-family:Segoe UI,sans-serif;max-width:40em;margin:4em auto;color:#222\">\
<h1 style=\"font-weight:600\">Solon</h1><p><code>{host}</code> does not match any container.</p>\
<p>Available domains:</p><ul>{items}</ul></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(
        id: &str,
        name: &str,
        ip: &str,
        exposed: &[u16],
        published: &[u16],
        project: Option<&str>,
        service: Option<&str>,
    ) -> ContainerEndpoint {
        ContainerEndpoint {
            id: id.into(),
            name: name.into(),
            compose_project: project.map(str::to_owned),
            compose_service: service.map(str::to_owned),
            ip: ip.into(),
            exposed_tcp: exposed.to_vec(),
            published_tcp: published.to_vec(),
        }
    }

    #[test]
    fn table_des_domaines() {
        let m = domains_for(&[
            e("1", "/web", "10.90.0.2", &[], &[8080], None, None),
            e(
                "2",
                "/odoo18-odoo-1",
                "10.90.2.3",
                &[8069, 8071, 8072],
                &[8069],
                Some("odoo18"),
                Some("odoo"),
            ),
            e("3", "/api", "10.90.0.4", &[3000, 9229], &[], None, None),
            e("4", "/nada", "", &[], &[], None, None),
        ]);
        assert_eq!(
            m["web.solon.local"],
            Target {
                ip: "10.90.0.2".parse().unwrap(),
                port: 8080
            }
        );
        assert_eq!(m["odoo.odoo18.solon.local"].port, 8069);
        assert_eq!(m["api.solon.local"].port, 3000);
        assert!(!m.contains_key("nada.solon.local"));
    }

    #[test]
    fn en_tetes_du_mandataire() {
        let head = b"GET /wp-admin/ HTTP/1.1\r\nHost: blog.solon.local\r\nX-Forwarded-Proto: http\r\nCookie: a=b\r\nContent-Length: 12\r\n\r\n";
        let (out, body) = rewrite_head(head, "https");
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("GET /wp-admin/ HTTP/1.1\r\nHost: blog.solon.local\r\n"));
        assert!(text.contains("Cookie: a=b\r\n"));
        assert_eq!(text.matches("X-Forwarded-Proto").count(), 1);
        assert!(text.contains("X-Forwarded-Proto: https\r\n"));
        assert!(text.contains("X-Forwarded-Host: blog.solon.local\r\n"));
        assert!(text.contains("X-Forwarded-Port: 443\r\n"));
        assert!(text.contains("Forwarded: for=127.0.0.1;host=blog.solon.local;proto=https\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
        assert_eq!(body, Body::Length(12));

        let (_, body) = rewrite_head(
            b"POST /x HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n",
            "http",
        );
        assert_eq!(body, Body::Chunked);
        let (out, body) = rewrite_head(
            b"GET /ws HTTP/1.1\r\nHost: a\r\nConnection: keep-alive, Upgrade\r\nUpgrade: websocket\r\n\r\n",
            "https",
        );
        assert_eq!(body, Body::Upgrade);
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("Upgrade: websocket\r\n")
        );
        let (_, body) = rewrite_head(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n", "http");
        assert_eq!(body, Body::Length(0));
    }

    #[tokio::test]
    async fn relais_des_corps() {
        // Corps de longueur connue, puis corps morcelé, lus depuis un tampon partiel.
        let mut input = std::io::Cursor::new(b"cd\r\nefgh".to_vec());
        let mut out = Vec::new();
        let mut buf = b"ab".to_vec();
        forward_exact(&mut input, &mut out, &mut buf, 4)
            .await
            .unwrap();
        assert_eq!(out, b"abcd");
        assert!(buf.is_empty());

        let mut input = std::io::Cursor::new(b"3\r\nabc\r\n0\r\n\r\nSUITE".to_vec());
        let mut out = Vec::new();
        let mut buf = Vec::new();
        forward_chunked(&mut input, &mut out, &mut buf)
            .await
            .unwrap();
        assert_eq!(out, b"3\r\nabc\r\n0\r\n\r\n");
        // Ce qui suit le corps reste dans le tampon pour la requête suivante.
        assert_eq!(buf, b"SUITE");
    }

    #[test]
    fn noms_nettoyes() {
        assert_eq!(sanitize("/My_App.v2"), "my-app-v2");
        assert_eq!(sanitize("___"), "");
    }

    #[test]
    fn autorite_locale_et_certificats() {
        let dir = std::env::temp_dir().join(format!("solon-ca-test-{}", std::process::id()));
        // Sans certutil : la création réussit quand même (installation en avertissement).
        let ca = LocalCa::load_or_create_with(&dir, false).unwrap();
        let leaf = ca.leaf("web.solon.local").unwrap();
        assert_eq!(leaf.cert.len(), 1);
        // Rechargement depuis le disque.
        let ca2 = LocalCa::load_or_create_with(&dir, false).unwrap();
        assert!(ca2.leaf("odoo.odoo18.solon.local").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
