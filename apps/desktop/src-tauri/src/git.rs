//! Environnements par branche Git : lecture de la branche courante d'un dossier (sans exécuter `git`,
//! en lisant `.git/HEAD`, y compris pour un arbre de travail lié) et copie des volumes d'un projet Compose
//! vers un autre (pour repartir d'une branche avec les données d'une autre).

use std::path::{Path, PathBuf};

use serde::Serialize;
use solon_core::ipc::ServiceCommand;

use crate::service;

#[derive(Debug, Serialize)]
pub struct GitInfo {
    /// Racine du dépôt (dossier qui contient `.git`).
    pub root: String,
    /// Nom de la branche (`main`, `feature/x`) ; `None` si HEAD est détaché.
    pub branch: Option<String>,
    /// Abrégé du commit quand HEAD est détaché.
    pub detached: Option<String>,
}

/// Dossier `.git` réel d'un dossier de travail : `.git` (dépôt) ou fichier `.git` (`gitdir: …`, arbre lié).
fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot = root.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    if dot.is_file() {
        let text = std::fs::read_to_string(&dot).ok()?;
        let target = text.strip_prefix("gitdir:")?.trim();
        let p = PathBuf::from(target);
        return Some(if p.is_absolute() { p } else { root.join(p) });
    }
    None
}

/// Branche courante du dossier `dir` (ou du premier parent qui est un dépôt).
#[tauri::command]
pub fn git_info(dir: String) -> Option<GitInfo> {
    let mut cur = Some(PathBuf::from(&dir));
    while let Some(root) = cur {
        if let Some(gd) = git_dir(&root) {
            let head = std::fs::read_to_string(gd.join("HEAD")).ok()?;
            let head = head.trim();
            return Some(match head.strip_prefix("ref: refs/heads/") {
                Some(b) => GitInfo {
                    root: root.to_string_lossy().into_owned(),
                    branch: Some(b.to_owned()),
                    detached: None,
                },
                None => GitInfo {
                    root: root.to_string_lossy().into_owned(),
                    branch: None,
                    detached: Some(head.chars().take(8).collect()),
                },
            });
        }
        cur = root.parent().map(Path::to_path_buf);
    }
    None
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[derive(Debug, Serialize)]
pub struct CloneReport {
    pub volumes: usize,
}

/// Copie les volumes nommés du projet `from` vers le projet `to` (`<to>_<suffixe>`), dans la machine,
/// avec les étiquettes que Compose attend. Les volumes de destination déjà présents sont laissés tels
/// quels (jamais écrasés).
#[tauri::command]
pub async fn volumes_clone(from: String, to: String) -> Result<CloneReport, String> {
    let list_cmd = format!(
        "docker volume ls --filter label=com.docker.compose.project={} --format '{{{{.Name}}}}'",
        shell_quote(&from)
    );
    let v = service::call(ServiceCommand::Exec {
        command: list_cmd,
        timeout_s: Some(30),
    })
    .await?;
    let names: Vec<String> = v
        .get("stdout")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let version = service::call(ServiceCommand::Exec {
        command: "docker compose version --short".into(),
        timeout_s: Some(30),
    })
    .await
    .ok()
    .and_then(|v| {
        v.get("stdout")
            .and_then(|s| s.as_str())
            .map(|s| s.trim().to_owned())
    })
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "2.0.0".into());
    let mut script = String::new();
    for name in &names {
        let suffix = name.strip_prefix(&format!("{from}_")).unwrap_or(name);
        let dest = format!("{to}_{suffix}");
        script.push_str(&format!(
            "docker volume inspect {d} >/dev/null 2>&1 || (docker volume create --label com.docker.compose.project={p} --label com.docker.compose.volume={s} --label com.docker.compose.version={ver} {d} >/dev/null && cp -a /var/lib/docker/volumes/{n}/_data/. /var/lib/docker/volumes/{d}/_data/ && echo cloned {d})\n",
            d = shell_quote(&dest),
            p = shell_quote(&to),
            s = shell_quote(suffix),
            ver = shell_quote(&version),
            n = shell_quote(name)
        ));
    }
    if script.is_empty() {
        return Ok(CloneReport { volumes: 0 });
    }
    let r = service::call(ServiceCommand::Exec {
        command: script,
        timeout_s: Some(1800),
    })
    .await?;
    let code = r.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!(
            "clone des volumes : {}",
            r.get("stderr")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
        ));
    }
    let cloned = r
        .get("stdout")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .lines()
        .filter(|l| l.starts_with("cloned "))
        .count();
    Ok(CloneReport { volumes: cloned })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branche_du_depot_courant() {
        // Ce dépôt (Solon) est un dépôt Git : la branche est lisible sans exécuter git.
        let here = env!("CARGO_MANIFEST_DIR").to_owned();
        let info = git_info(here).expect("dépôt Git");
        assert!(info.branch.is_some() || info.detached.is_some());
        assert!(Path::new(&info.root).join(".git").exists());
    }

    #[test]
    fn dossier_sans_depot() {
        assert!(git_info(std::env::temp_dir().to_string_lossy().into_owned()).is_none());
    }
}
