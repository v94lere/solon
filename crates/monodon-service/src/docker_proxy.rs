//! Mandataire de l'API Docker : `\\.\pipe\monodon` → vsock 5001 (dockerd dans la machine).
//!
//! Contrairement au relais brut, ce mandataire lit les requêtes HTTP du client et **traduit les
//! chemins Windows** des montages dans `POST /containers/create` (`HostConfig.Binds`,
//! `HostConfig.Mounts[].Source`) : `C:\Users\v\proj` devient `/mnt/host/c/Users/v/proj`, et le
//! lecteur est partagé avec la machine si ce n'est pas déjà fait. C'est ce qui permet à
//! `docker run -v C:\...` et à `docker compose` (CLI Windows) de fonctionner comme avec Docker Desktop.
//!
//! Tout le reste passe sans modification : corps `Content-Length`, corps `chunked` (envoi de contexte
//! de build), et connexions « hijackées » (`exec`, `attach`) après lesquelles le flux devient brut.
//! La réponse (invité → client) est toujours copiée telle quelle.

use std::io;
use std::time::Duration;

use monodon_core::ipc::DOCKER_PIPE;
use monodon_core::protocol::PORT_DOCKER;
use monodon_hvsock::relay::{DOCKER_PIPE_SDDL, create_server};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::NamedPipeServer;
use windows::core::GUID;

use crate::engine::Engine;

/// Taille maximale d'un corps `containers/create` que l'on accepte de réécrire (au-delà : transmis tel quel).
const MAX_REWRITE_BODY: usize = 8 * 1024 * 1024;
const MAX_HEAD: usize = 1024 * 1024;

pub async fn serve(engine: Engine, vm_id: GUID) -> io::Result<()> {
    let mut server = create_server(DOCKER_PIPE, true, Some(DOCKER_PIPE_SDDL))?;
    tracing::info!(
        pipe = DOCKER_PIPE,
        port = PORT_DOCKER,
        "mandataire API Docker à l'écoute"
    );
    loop {
        server.connect().await?;
        let connected = server;
        server = create_server(DOCKER_PIPE, false, Some(DOCKER_PIPE_SDDL))?;
        let engine = engine.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(engine, connected, vm_id).await {
                tracing::debug!("connexion API Docker terminée : {e}");
            }
        });
    }
}

async fn handle(engine: Engine, pipe: NamedPipeServer, vm_id: GUID) -> io::Result<()> {
    let std_stream = tokio::task::spawn_blocking(move || {
        monodon_hvsock::connect_with_retry(&vm_id, PORT_DOCKER, Duration::from_secs(5))
    })
    .await
    .map_err(io::Error::other)??;
    std_stream.set_nonblocking(true)?;
    let hv = tokio::net::TcpStream::from_std(std_stream)?;
    let (mut pipe_read, mut pipe_write) = tokio::io::split(pipe);
    let (mut hv_read, mut hv_write) = hv.into_split();

    let to_client = async {
        let n = tokio::io::copy(&mut hv_read, &mut pipe_write).await;
        let _ = pipe_write.flush().await;
        n
    };
    let to_guest = async {
        let r = forward_requests(&engine, &mut pipe_read, &mut hv_write).await;
        let _ = hv_write.shutdown().await;
        r
    };
    tokio::select! {
        r = to_guest => { tracing::trace!(?r, "client → invité terminé"); }
        r = to_client => { tracing::trace!(?r, "invité → client terminé"); }
    }
    Ok(())
}

/// Lit les requêtes HTTP successives du client et les transmet à dockerd, en réécrivant celles qui
/// créent un conteneur. Passe en copie brute après une requête d'« upgrade » (hijack).
async fn forward_requests<R, W>(engine: &Engine, client: &mut R, guest: &mut W) -> io::Result<u64>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
    let mut total = 0u64;
    loop {
        // 1. En-tête complet (jusqu'à la ligne vide).
        let head_end = loop {
            if let Some(pos) = find_head_end(&buf) {
                break pos;
            }
            if buf.len() > MAX_HEAD {
                return raw_tail(&mut buf, client, guest, total).await;
            }
            let mut chunk = [0u8; 16 * 1024];
            let n = client.read(&mut chunk).await?;
            if n == 0 {
                if !buf.is_empty() {
                    guest.write_all(&buf).await?;
                }
                return Ok(total);
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let head_bytes = buf[..head_end].to_vec();
        buf.drain(..head_end);
        let Some(head) = parse_head(&head_bytes) else {
            // Pas du HTTP reconnaissable : on transmet tout brut.
            guest.write_all(&head_bytes).await?;
            return raw_tail(&mut buf, client, guest, total + head_bytes.len() as u64).await;
        };

        // 2. Corps.
        if head.is_create_container && head.content_length.is_some_and(|n| n <= MAX_REWRITE_BODY) {
            let len = head.content_length.unwrap();
            while buf.len() < len {
                let mut chunk = vec![0u8; (len - buf.len()).min(64 * 1024)];
                let n = client.read(&mut chunk).await?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "corps tronqué",
                    ));
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let body: Vec<u8> = buf.drain(..len).collect();
            let (body, drives) = rewrite_create_body(body);
            for drive in drives {
                if let Err(e) = engine.ensure_share(&format!("{drive}:\\")).await {
                    tracing::warn!(drive, "partage impossible pour un montage : {e}");
                }
            }
            let new_head = set_content_length(&head_bytes, body.len());
            guest.write_all(&new_head).await?;
            guest.write_all(&body).await?;
            total += (new_head.len() + body.len()) as u64;
        } else {
            guest.write_all(&head_bytes).await?;
            total += head_bytes.len() as u64;
            if let Some(len) = head.content_length {
                total += copy_exact(&mut buf, client, guest, len).await?;
            } else if head.chunked {
                total += copy_chunked(&mut buf, client, guest).await?;
            }
        }
        guest.flush().await?;

        // 3. Après un hijack, le reste n'est plus du HTTP.
        if head.upgrade {
            return raw_tail(&mut buf, client, guest, total).await;
        }
    }
}

async fn raw_tail<R, W>(
    buf: &mut Vec<u8>,
    client: &mut R,
    guest: &mut W,
    total: u64,
) -> io::Result<u64>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    if !buf.is_empty() {
        guest.write_all(buf).await?;
    }
    let pending = buf.len() as u64;
    buf.clear();
    let n = tokio::io::copy(client, guest).await?;
    Ok(total + pending + n)
}

/// Copie exactement `len` octets de corps (en commençant par ce qui est déjà dans `buf`).
async fn copy_exact<R, W>(
    buf: &mut Vec<u8>,
    client: &mut R,
    guest: &mut W,
    len: usize,
) -> io::Result<u64>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut remaining = len;
    let take = remaining.min(buf.len());
    if take > 0 {
        guest.write_all(&buf[..take]).await?;
        buf.drain(..take);
        remaining -= take;
    }
    let mut chunk = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let n = client.read(&mut chunk[..remaining.min(64 * 1024)]).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "corps tronqué",
            ));
        }
        guest.write_all(&chunk[..n]).await?;
        remaining -= n;
    }
    Ok(len as u64)
}

/// Copie un corps `Transfer-Encoding: chunked` jusqu'au dernier bloc (taille 0) inclus.
async fn copy_chunked<R, W>(buf: &mut Vec<u8>, client: &mut R, guest: &mut W) -> io::Result<u64>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut total = 0u64;
    loop {
        // Ligne de taille.
        let line_end = loop {
            if let Some(p) = buf.windows(2).position(|w| w == b"\r\n") {
                break p;
            }
            let mut chunk = [0u8; 4096];
            let n = client.read(&mut chunk).await?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "chunk tronqué",
                ));
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let size_line = String::from_utf8_lossy(&buf[..line_end]).to_string();
        let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "taille de chunk invalide"))?;
        guest.write_all(&buf[..line_end + 2]).await?;
        total += (line_end + 2) as u64;
        buf.drain(..line_end + 2);
        // Données + CRLF (et pour le dernier bloc : les éventuels trailers puis CRLF final).
        if size == 0 {
            // Trailers jusqu'à une ligne vide.
            let end = loop {
                if let Some(p) = find_head_end_from(buf, 0) {
                    break p;
                }
                if buf.starts_with(b"\r\n") {
                    break 2;
                }
                let mut chunk = [0u8; 4096];
                let n = client.read(&mut chunk).await?;
                if n == 0 {
                    break buf.len();
                }
                buf.extend_from_slice(&chunk[..n]);
            };
            guest.write_all(&buf[..end]).await?;
            total += end as u64;
            buf.drain(..end);
            return Ok(total);
        }
        total += copy_exact(buf, client, guest, size + 2).await?;
    }
}

struct Head {
    is_create_container: bool,
    content_length: Option<usize>,
    chunked: bool,
    upgrade: bool,
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    find_head_end_from(buf, 0)
}

fn find_head_end_from(buf: &[u8], from: usize) -> Option<usize> {
    buf.get(from..)?
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| from + p + 4)
}

fn parse_head(head: &[u8]) -> Option<Head> {
    let text = std::str::from_utf8(head).ok()?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    let version = parts.next()?;
    if !version.starts_with("HTTP/") {
        return None;
    }
    let mut h = Head {
        is_create_container: method == "POST" && is_create_path(path),
        content_length: None,
        chunked: false,
        upgrade: false,
    };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => h.content_length = value.parse().ok(),
            "transfer-encoding" => h.chunked = value.to_ascii_lowercase().contains("chunked"),
            "connection" => h.upgrade |= value.to_ascii_lowercase().contains("upgrade"),
            "upgrade" => h.upgrade = true,
            _ => {}
        }
    }
    Some(h)
}

/// `/containers/create`, avec ou sans préfixe de version (`/v1.51/containers/create?name=x`).
fn is_create_path(path: &str) -> bool {
    let p = path.split('?').next().unwrap_or(path);
    let p = if p.starts_with("/v") {
        match p[1..].find('/') {
            Some(i) => &p[1 + i..],
            None => p,
        }
    } else {
        p
    };
    p == "/containers/create"
}

fn set_content_length(head: &[u8], len: usize) -> Vec<u8> {
    let text = String::from_utf8_lossy(head);
    let mut out = String::with_capacity(text.len() + 16);
    let mut replaced = false;
    for (i, line) in text.split("\r\n").enumerate() {
        if i > 0 && line.to_ascii_lowercase().starts_with("content-length:") {
            out.push_str(&format!("Content-Length: {len}\r\n"));
            replaced = true;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    if !replaced {
        out.push_str(&format!("Content-Length: {len}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// Traduit un chemin Windows (`C:\x\y`, `C:/x/y`, `/c/x/y`, `//c/x/y`) en chemin de la machine ;
/// renvoie aussi la lettre du lecteur. `None` si ce n'est pas un chemin Windows.
pub fn translate_host_path(src: &str) -> Option<(String, String)> {
    let bytes = src.as_bytes();
    let (drive, rest) = if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        (bytes[0].to_ascii_lowercase() as char, &src[2..])
    } else {
        // Formes MSYS / Git Bash : /c/Users/x ou //c/Users/x
        let trimmed = src.trim_start_matches('/');
        let slashes = src.len() - trimmed.len();
        if (1..=2).contains(&slashes)
            && !trimmed.is_empty()
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && (trimmed.len() == 1 || trimmed.as_bytes()[1] == b'/')
        {
            (
                trimmed.as_bytes()[0].to_ascii_lowercase() as char,
                &trimmed[1..],
            )
        } else {
            return None;
        }
    };
    let mut rest = rest.replace('\\', "/");
    if rest.is_empty() {
        rest.push('/');
    }
    if !rest.starts_with('/') {
        rest.insert(0, '/');
    }
    let rest = rest.trim_end_matches('/');
    let guest = format!(
        "/mnt/host/{drive}{}",
        if rest.is_empty() { "" } else { rest }
    );
    Some((drive.to_string(), guest))
}

/// Réécrit un montage `Binds` (`src:dst[:opts]`) dont la source est un chemin Windows.
fn rewrite_bind(bind: &str) -> Option<(String, String)> {
    // Source Windows : lettre + ':' ; le séparateur source/destination est le ':' suivant.
    let bytes = bind.as_bytes();
    let (src, tail) = if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let cut = bind[2..].find(':').map(|i| i + 2)?;
        (&bind[..cut], &bind[cut..])
    } else if bind.starts_with('/') {
        let cut = bind.find(':')?;
        (&bind[..cut], &bind[cut..])
    } else {
        return None;
    };
    let (drive, guest) = translate_host_path(src)?;
    Some((drive, format!("{guest}{tail}")))
}

/// Réécrit le JSON d'un `containers/create` ; renvoie le corps (inchangé si rien à faire) et les
/// lecteurs à partager.
pub fn rewrite_create_body(body: Vec<u8>) -> (Vec<u8>, Vec<String>) {
    let Ok(mut v) = serde_json::from_slice::<Value>(&body) else {
        return (body, vec![]);
    };
    let mut drives: Vec<String> = Vec::new();
    let mut changed = false;
    if let Some(hc) = v.get_mut("HostConfig").and_then(|h| h.as_object_mut()) {
        if let Some(binds) = hc.get_mut("Binds").and_then(|b| b.as_array_mut()) {
            for b in binds.iter_mut() {
                if let Some(s) = b.as_str() {
                    if let Some((drive, rewritten)) = rewrite_bind(s) {
                        *b = Value::String(rewritten);
                        if !drives.contains(&drive) {
                            drives.push(drive);
                        }
                        changed = true;
                    }
                }
            }
        }
        if let Some(mounts) = hc.get_mut("Mounts").and_then(|m| m.as_array_mut()) {
            for m in mounts.iter_mut() {
                let is_bind = m.get("Type").and_then(|t| t.as_str()) == Some("bind");
                if !is_bind {
                    continue;
                }
                if let Some(src) = m.get("Source").and_then(|s| s.as_str()) {
                    if let Some((drive, guest)) = translate_host_path(src) {
                        m["Source"] = Value::String(guest);
                        if !drives.contains(&drive) {
                            drives.push(drive);
                        }
                        changed = true;
                    }
                }
            }
        }
    }
    if !changed {
        return (body, vec![]);
    }
    match serde_json::to_vec(&v) {
        Ok(out) => (out, drives),
        Err(_) => (body, vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_windows() {
        assert_eq!(
            translate_host_path(r"C:\Users\v\proj"),
            Some(("c".into(), "/mnt/host/c/Users/v/proj".into()))
        );
        assert_eq!(
            translate_host_path("D:/data/"),
            Some(("d".into(), "/mnt/host/d/data".into()))
        );
        assert_eq!(
            translate_host_path("/c/Users/v"),
            Some(("c".into(), "/mnt/host/c/Users/v".into()))
        );
        assert_eq!(
            translate_host_path("//e/x"),
            Some(("e".into(), "/mnt/host/e/x".into()))
        );
        assert_eq!(
            translate_host_path("C:"),
            Some(("c".into(), "/mnt/host/c".into()))
        );
        assert_eq!(translate_host_path("/var/lib"), None);
        assert_eq!(translate_host_path("named-volume"), None);
        assert_eq!(translate_host_path("/mnt/host/c/x"), None);
    }

    #[test]
    fn binds() {
        assert_eq!(
            rewrite_bind(r"C:\Users\v\proj:/app:ro"),
            Some(("c".into(), "/mnt/host/c/Users/v/proj:/app:ro".into()))
        );
        assert_eq!(rewrite_bind("vol:/data"), None);
        assert_eq!(rewrite_bind("/host/x:/y"), None);
    }

    #[test]
    fn corps_create() {
        let body = br#"{"Image":"busybox","HostConfig":{"Binds":["C:\\proj:/app","data:/data"],"Mounts":[{"Type":"bind","Source":"D:\\x","Target":"/x"},{"Type":"volume","Source":"v","Target":"/v"}]}}"#.to_vec();
        let (out, drives) = rewrite_create_body(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["HostConfig"]["Binds"][0], "/mnt/host/c/proj:/app");
        assert_eq!(v["HostConfig"]["Binds"][1], "data:/data");
        assert_eq!(v["HostConfig"]["Mounts"][0]["Source"], "/mnt/host/d/x");
        assert_eq!(v["HostConfig"]["Mounts"][1]["Source"], "v");
        assert_eq!(drives, vec!["c".to_string(), "d".to_string()]);
    }

    #[test]
    fn entete_http() {
        let h = parse_head(b"POST /v1.51/containers/create?name=x HTTP/1.1\r\nHost: docker\r\nContent-Length: 12\r\n\r\n").unwrap();
        assert!(h.is_create_container);
        assert_eq!(h.content_length, Some(12));
        let h = parse_head(
            b"POST /v1.51/exec/abc/start HTTP/1.1\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n",
        )
        .unwrap();
        assert!(h.upgrade && !h.is_create_container);
        let h = parse_head(b"POST /build HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
        assert!(h.chunked);
        assert!(parse_head(b"garbage").is_none());
        let head = set_content_length(
            b"POST /x HTTP/1.1\r\nContent-Length: 3\r\nHost: a\r\n\r\n",
            42,
        );
        assert_eq!(
            head,
            b"POST /x HTTP/1.1\r\nContent-Length: 42\r\nHost: a\r\n\r\n".to_vec()
        );
    }
}
