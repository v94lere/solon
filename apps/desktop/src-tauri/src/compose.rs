//! Docker Compose : le binaire tourne dans la machine ; le dossier du projet est rendu visible par un
//! partage 9P du lecteur (service `EnsureShare`), puis `docker compose` s'exécute via l'agent.
//! La sortie est capturée en fin d'exécution (pas de flux) : suffisant pour le MVP, documenté.

use std::path::{Path, PathBuf};

use serde::Serialize;
use solon_core::ipc::{ServiceCommand, ShareInfo};

use crate::service;

/// Sortie d'une commande Compose en flux.
#[derive(Debug, Clone, Serialize)]
pub struct ComposeChunk {
    /// `stdout`, `stderr`, `exit` (texte = code) ou `error`.
    pub kind: &'static str,
    pub text: String,
}

/// Exécute `docker compose <args>` dans la machine avec la sortie **en flux** (canal `solon-exec`).
/// Renvoie le code de sortie une fois la commande terminée.
#[tauri::command]
pub async fn compose_stream(
    dir: String,
    args: Vec<String>,
    channel: tauri::ipc::Channel<ComposeChunk>,
) -> Result<i32, String> {
    use solon_core::protocol::{
        EXEC_FRAME_EXIT, EXEC_FRAME_STDERR, EXEC_FRAME_STDOUT, ExecStreamRequest,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let share: ShareInfo = serde_json::from_value(
        service::call(ServiceCommand::EnsureShare {
            host_path: dir.clone(),
        })
        .await?,
    )
    .map_err(|e| format!("réponse du service illisible : {e}"))?;
    let quoted_args: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
    let req = ExecStreamRequest {
        command: format!("docker compose {}", quoted_args.join(" ")),
        cwd: Some(share.guest_path.clone()),
    };
    let mut pipe = open_exec_pipe().await?;
    let mut header = serde_json::to_string(&req).map_err(|e| e.to_string())?;
    header.push('\n');
    pipe.write_all(header.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut head = [0u8; 3];
    loop {
        if let Err(e) = pipe.read_exact(&mut head).await {
            let _ = channel.send(ComposeChunk {
                kind: "error",
                text: format!("connexion interrompue : {e}"),
            });
            return Err(e.to_string());
        }
        let len = u16::from_be_bytes([head[1], head[2]]) as usize;
        let mut payload = vec![0u8; len];
        pipe.read_exact(&mut payload)
            .await
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&payload).into_owned();
        match head[0] {
            EXEC_FRAME_STDOUT => {
                let _ = channel.send(ComposeChunk {
                    kind: "stdout",
                    text,
                });
            }
            EXEC_FRAME_STDERR => {
                let _ = channel.send(ComposeChunk {
                    kind: "stderr",
                    text,
                });
            }
            EXEC_FRAME_EXIT => {
                let code = text.trim().parse::<i32>().unwrap_or(-1);
                let _ = channel.send(ComposeChunk {
                    kind: "exit",
                    text: code.to_string(),
                });
                return Ok(code);
            }
            _ => {}
        }
    }
}

async fn open_exec_pipe() -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String> {
    use tokio::net::windows::named_pipe::ClientOptions;
    const ERROR_PIPE_BUSY: i32 = 231;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match ClientOptions::new().open(solon_core::ipc::EXEC_PIPE) {
            Ok(pipe) => return Ok(pipe),
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY)
                    && std::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            }
            Err(e) => {
                return Err(format!(
                    "canal d'exécution indisponible (le moteur est-il démarré ?) : {e}"
                ));
            }
        }
    }
}

pub const COMPOSE_FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

#[derive(Debug, Clone, Serialize)]
pub struct ComposeProject {
    pub dir: String,
    pub file: String,
    /// Nom de projet par défaut de Compose : nom du dossier en minuscules.
    pub name: String,
}

/// Cherche un fichier Compose dans `dir`.
#[tauri::command]
pub fn compose_detect(dir: String) -> Result<Option<ComposeProject>, String> {
    let path = Path::new(&dir);
    if !path.is_dir() {
        return Err(format!("dossier introuvable : {dir}"));
    }
    for f in COMPOSE_FILES {
        if path.join(f).is_file() {
            let name = path
                .file_name()
                .map(|n| {
                    n.to_string_lossy()
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                        .collect()
                })
                .unwrap_or_default();
            return Ok(Some(ComposeProject {
                dir: dir.clone(),
                file: (*f).to_owned(),
                name,
            }));
        }
    }
    Ok(None)
}

fn compose_file(dir: &str) -> Result<PathBuf, String> {
    let p = compose_detect(dir.to_owned())?
        .ok_or_else(|| format!("aucun fichier Compose dans {dir}"))?;
    Ok(Path::new(&p.dir).join(&p.file))
}

/// Contenu du fichier Compose du projet.
#[tauri::command]
pub fn compose_read(dir: String) -> Result<String, String> {
    std::fs::read_to_string(compose_file(&dir)?).map_err(|e| e.to_string())
}

/// Remplace le fichier Compose : écriture dans un fichier temporaire puis renommage, pour ne
/// jamais laisser un fichier à moitié écrit si l'application s'arrête au mauvais moment.
#[tauri::command]
pub fn compose_write(dir: String, content: String) -> Result<(), String> {
    let target = compose_file(&dir)?;
    let tmp = target.with_extension("solon-tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ComposeResult {
    pub code: Option<i32>,
    pub output: String,
    pub ms: u64,
    pub guest_dir: String,
}

/// Exécute `docker compose <args>` dans le dossier du projet (côté machine). `args` typiques :
/// `["up", "-d"]`, `["down"]`, `["ps"]`.
#[tauri::command]
pub async fn compose_run(
    dir: String,
    args: Vec<String>,
    timeout_s: Option<u64>,
) -> Result<ComposeResult, String> {
    let share: ShareInfo = serde_json::from_value(
        service::call(ServiceCommand::EnsureShare {
            host_path: dir.clone(),
        })
        .await?,
    )
    .map_err(|e| format!("réponse du service illisible : {e}"))?;
    let quoted_args: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
    let command = format!(
        "cd {} && docker compose {} 2>&1",
        shell_quote(&share.guest_path),
        quoted_args.join(" ")
    );
    let timeout = timeout_s.unwrap_or(900);
    let result = service::call(ServiceCommand::Exec {
        command,
        timeout_s: Some(timeout),
    })
    .await?;
    let code = result
        .get("code")
        .and_then(|c| c.as_i64())
        .map(|c| c as i32);
    let stdout = result
        .get("stdout")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_owned();
    let stderr = result
        .get("stderr")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_owned();
    let ms = result.get("ms").and_then(|m| m.as_u64()).unwrap_or(0);
    let timed_out = result
        .get("timed_out")
        .and_then(|t| t.as_bool())
        .unwrap_or(false);
    let mut output = stdout;
    if !stderr.is_empty() {
        output.push_str(&stderr);
    }
    if timed_out {
        output.push_str("\n[délai dépassé]");
    }
    Ok(ComposeResult {
        code,
        output,
        ms,
        guest_dir: share.guest_path,
    })
}

fn shell_quote(s: &str) -> String {
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
