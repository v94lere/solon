//! Client du canal de contrôle du service Windows (`\\.\pipe\solon-control`).
//!
//! Chaque appel ouvre sa propre connexion (simple, sans état à réparer). Un abonnement garde une
//! connexion ouverte et remet chaque événement à l'appelant, qui décide quand s'arrêter.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use solon_core::ipc::{CONTROL_PIPE, IpcRequest, ServiceCommand, ShareInfo};
use solon_core::protocol::Response;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

/// `ERROR_PIPE_BUSY` : toutes les instances du pipe sont prises (le serveur crée la suivante
/// juste après chaque connexion). C'est attendu sous charge ; on réessaie brièvement.
const ERROR_PIPE_BUSY: i32 = 231;

async fn open() -> Result<NamedPipeClient, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match ClientOptions::new().open(CONTROL_PIPE) {
            Ok(pipe) => return Ok(pipe),
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY)
                    && std::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            Err(e) => return Err(format!("Solon service not reachable ({CONTROL_PIPE}): {e}")),
        }
    }
}

fn encode(command: ServiceCommand) -> Result<String, String> {
    let mut line = serde_json::to_string(&IpcRequest {
        id: 1,
        request: command,
    })
    .map_err(|e| e.to_string())?;
    line.push('\n');
    Ok(line)
}

/// Envoie une commande et renvoie le résultat JSON (réponse attendue sous 3 minutes).
pub async fn call(command: ServiceCommand) -> Result<Value, String> {
    call_within(command, Duration::from_secs(180)).await
}

/// Comme [`call`], avec le délai d'attente de la réponse : une commande longue dans la machine
/// (`Exec` avec `timeout_s`) ou une archive de volume ne doivent pas échouer côté client pendant
/// que le service travaille encore.
pub async fn call_within(command: ServiceCommand, wait: Duration) -> Result<Value, String> {
    let pipe = open().await?;
    let (read, mut write) = tokio::io::split(pipe);
    let mut reader = BufReader::new(read);
    write
        .write_all(encode(command)?.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    tokio::time::timeout(wait, reader.read_line(&mut reply))
        .await
        .map_err(|_| "timed out waiting for the Solon service".to_string())?
        .map_err(|e| e.to_string())?;
    let response: Response = serde_json::from_str(reply.trim())
        .map_err(|e| format!("unreadable answer from the service: {e}"))?;
    if response.ok {
        Ok(response.result.unwrap_or(Value::Null))
    } else {
        Err(response.error.unwrap_or_else(|| "service error".into()))
    }
}

/// Abonnement (`Subscribe` ou `TraceSubscribe`) : remet chaque événement à `on_event`, qui rend
/// `false` pour arrêter. `Ok(())` quand l'appelant a arrêté, `Err` si la connexion s'est perdue
/// ou n'a pas pu s'ouvrir : à l'appelant de réessayer s'il le souhaite.
///
/// `idle` : si fourni, l'abonnement s'arrête aussi (en `Ok`) quand aucun événement n'arrive
/// pendant ce délai. C'est ainsi que le CLI sait que l'historique de la radiographie est passé :
/// le service l'envoie d'un trait avant le direct.
pub async fn stream<F>(
    command: ServiceCommand,
    idle: Option<Duration>,
    mut on_event: F,
) -> Result<(), String>
where
    F: FnMut(Value) -> bool,
{
    let pipe = open().await?;
    let (read, mut write) = tokio::io::split(pipe);
    let mut reader = BufReader::new(read);
    write
        .write_all(encode(command)?.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = String::new();
    loop {
        buf.clear();
        let read = reader.read_line(&mut buf);
        let n = match idle {
            Some(d) => match tokio::time::timeout(d, read).await {
                Ok(r) => r.map_err(|e| e.to_string())?,
                Err(_) => return Ok(()),
            },
            None => read.await.map_err(|e| e.to_string())?,
        };
        if n == 0 {
            return Err("connection closed by the service".into());
        }
        let value: Value = match serde_json::from_str(buf.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("id").is_some() {
            // Réponse à la demande d'abonnement : un refus s'arrête ici.
            if value.get("ok") == Some(&Value::Bool(false)) {
                let why = value
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("subscription refused");
                return Err(why.to_owned());
            }
            continue;
        }
        if !on_event(value) {
            return Ok(());
        }
    }
}

/// Exécute une commande shell dans la machine ; rend `(code, stdout, stderr)`.
pub async fn exec(command: String, timeout_s: u64) -> Result<(i32, String, String), String> {
    let v = call_within(
        ServiceCommand::Exec {
            command,
            timeout_s: Some(timeout_s),
        },
        Duration::from_secs(timeout_s + 30),
    )
    .await?;
    let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
    let text = |k: &str| v.get(k).and_then(|s| s.as_str()).unwrap_or("").to_owned();
    Ok((code, text("stdout"), text("stderr")))
}

/// Dossier temporaire Windows visible depuis la machine (`/mnt/host/c/Users/…/Temp/<prefix>-<pid>`).
pub async fn shared_temp(prefix: &str) -> Result<(PathBuf, String), String> {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let share: ShareInfo = serde_json::from_value(
        call(ServiceCommand::EnsureShare {
            host_path: dir.to_string_lossy().into_owned(),
        })
        .await?,
    )
    .map_err(|e| format!("unreadable answer from the service: {e}"))?;
    Ok((dir, share.guest_path))
}

/// Quote POSIX simple pour les commandes envoyées à la machine.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_shell() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
