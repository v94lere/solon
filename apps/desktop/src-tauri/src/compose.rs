//! Docker Compose : le binaire tourne dans la machine ; le dossier du projet est rendu visible par un
//! partage 9P du lecteur (service `EnsureShare`), puis `docker compose` s'exécute via l'agent.
//! La sortie est capturée en fin d'exécution (pas de flux) : suffisant pour le MVP, documenté.

use std::path::Path;

use serde::Serialize;
use solon_core::ipc::{ServiceCommand, ShareInfo};

use crate::service;

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
