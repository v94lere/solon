//! Domaines locaux : `nom.solon.local` (et `service.projet.solon.local` pour Compose) au lieu de
//! `localhost:<port>`.
//!
//! Deux pièces : (1) un bloc géré dans le fichier `hosts` de Windows qui fait pointer chaque nom
//! vers `127.0.0.1` (le service tourne en LocalSystem, il a le droit d'écrire ce fichier) ;
//! (2) un petit mandataire HTTP sur `127.0.0.1:80` qui lit l'en-tête `Host` de la première requête
//! d'une connexion et la relaie vers le port publié correspondant (les ports publiés sont déjà
//! relayés sur `127.0.0.1` par le service). Les WebSockets passent puisque la copie devient brute
//! après l'en-tête. Un nom inconnu reçoit une page 404 listant les domaines disponibles.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use solon_core::protocol::PortBinding;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

pub const SUFFIX: &str = "solon.local";
const HOSTS_BEGIN: &str = "# solon-begin (géré par Solon, ne pas modifier)";
const HOSTS_END: &str = "# solon-end";

/// Nom de domaine → port hôte (TCP, relayé sur 127.0.0.1).
pub type DomainMap = BTreeMap<String, u16>;
pub type SharedDomains = Arc<RwLock<DomainMap>>;

/// Ne garde que lettres, chiffres et tirets (les `_` deviennent `-`), en minuscules.
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

/// Calcule la table des domaines à partir des ports publiés : pour chaque conteneur ayant au moins un
/// port TCP publié, `nom.solon.local` (et `service.projet.solon.local` si c'est un service Compose)
/// pointe vers son **plus petit** port hôte TCP.
pub fn domains_for(bindings: &[PortBinding]) -> DomainMap {
    let mut by_container: BTreeMap<&str, (u16, &PortBinding)> = BTreeMap::new();
    for b in bindings.iter().filter(|b| b.protocol == "tcp") {
        let entry = by_container
            .entry(b.container_id.as_str())
            .or_insert((b.host_port, b));
        if b.host_port < entry.0 {
            *entry = (b.host_port, b);
        }
    }
    let mut map = DomainMap::new();
    for (_, (port, b)) in by_container {
        let name = sanitize(&b.container_name);
        if !name.is_empty() {
            map.entry(format!("{name}.{SUFFIX}")).or_insert(port);
        }
        if let (Some(project), Some(service)) = (&b.compose_project, &b.compose_service) {
            let (p, s) = (sanitize(project), sanitize(service));
            if !p.is_empty() && !s.is_empty() {
                map.entry(format!("{s}.{p}.{SUFFIX}")).or_insert(port);
            }
        }
    }
    map
}

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
        if line.trim() == HOSTS_BEGIN {
            skipping = true;
            continue;
        }
        if line.trim() == HOSTS_END {
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
    std::fs::write(&path, next)
}

/// Réserve 127.0.0.1:80 ; échoue si le port est pris (IIS, autre serveur local).
pub async fn bind_proxy() -> io::Result<TcpListener> {
    let listener = TcpListener::bind(("127.0.0.1", 80)).await?;
    tracing::info!("domaines locaux : mandataire HTTP sur 127.0.0.1:80 (*.{SUFFIX})");
    Ok(listener)
}

/// Sert le mandataire HTTP sur le port réservé par [`bind_proxy`].
pub async fn serve_proxy(listener: TcpListener, domains: SharedDomains) -> io::Result<()> {
    loop {
        let (client, _) = listener.accept().await?;
        let domains = domains.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(client, domains).await {
                tracing::debug!("domaine local : {e}");
            }
        });
    }
}

async fn handle(mut client: tokio::net::TcpStream, domains: SharedDomains) -> io::Result<()> {
    let mut buf = Vec::with_capacity(8192);
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > 64 * 1024 {
            return Ok(());
        }
        let mut chunk = [0u8; 8192];
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
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
    let Some(port) = target else {
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
    let mut upstream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    upstream.write_all(&buf).await?;
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

fn not_found_page(host: &str, domains: &DomainMap) -> String {
    let mut items = String::new();
    for (name, port) in domains {
        items.push_str(&format!(
            "<li><a href=\"http://{name}/\">{name}</a> <small>→ localhost:{port}</small></li>"
        ));
    }
    if items.is_empty() {
        items.push_str("<li><em>aucun conteneur ne publie de port pour l'instant</em></li>");
    }
    format!(
        "<!doctype html><html lang=\"fr\"><meta charset=\"utf-8\"><title>Solon</title>\
<body style=\"font-family:Segoe UI,sans-serif;max-width:40em;margin:4em auto;color:#222\">\
<h1 style=\"font-weight:600\">Solon</h1><p><code>{host}</code> ne correspond à aucun conteneur.</p>\
<p>Domaines disponibles :</p><ul>{items}</ul></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(
        id: &str,
        name: &str,
        port: u16,
        proto: &str,
        project: Option<&str>,
        service: Option<&str>,
    ) -> PortBinding {
        PortBinding {
            container_id: id.into(),
            protocol: proto.into(),
            host_ip: "0.0.0.0".into(),
            host_port: port,
            container_ip: "10.90.0.2".into(),
            container_port: 80,
            container_name: name.into(),
            compose_project: project.map(str::to_owned),
            compose_service: service.map(str::to_owned),
        }
    }

    #[test]
    fn table_des_domaines() {
        let m = domains_for(&[
            b("1", "/web", 8080, "tcp", None, None),
            b(
                "2",
                "/odoo18-odoo-1",
                8069,
                "tcp",
                Some("odoo18"),
                Some("odoo"),
            ),
            b(
                "2",
                "/odoo18-odoo-1",
                8072,
                "tcp",
                Some("odoo18"),
                Some("odoo"),
            ),
            b("3", "/dns", 53, "udp", None, None),
        ]);
        assert_eq!(m.get("web.solon.local"), Some(&8080));
        assert_eq!(m.get("odoo18-odoo-1.solon.local"), Some(&8069));
        assert_eq!(m.get("odoo.odoo18.solon.local"), Some(&8069));
        assert!(!m.contains_key("dns.solon.local"));
    }

    #[test]
    fn noms_nettoyes() {
        assert_eq!(sanitize("/My_App.v2"), "my-app-v2");
        assert_eq!(sanitize("___"), "");
    }
}
