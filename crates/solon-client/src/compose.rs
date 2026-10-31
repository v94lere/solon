//! Reconnaître un projet Compose dans un dossier, et le nom que Compose lui donnera.

use std::path::Path;

use serde::Serialize;

/// Noms de fichiers que Compose reconnaît, dans son ordre de préférence.
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

/// Nom de projet par défaut : le nom du dossier, en minuscules, sans caractères spéciaux.
pub fn default_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| {
            n.to_string_lossy()
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect()
        })
        .unwrap_or_default()
}

/// Cherche un fichier Compose dans `dir`.
pub fn detect(dir: &str) -> Result<Option<ComposeProject>, String> {
    let path = Path::new(dir);
    if !path.is_dir() {
        return Err(format!("folder not found: {dir}"));
    }
    for f in COMPOSE_FILES {
        if path.join(f).is_file() {
            return Ok(Some(ComposeProject {
                dir: dir.to_owned(),
                file: (*f).to_owned(),
                name: default_name(path),
            }));
        }
    }
    Ok(None)
}

/// Nom de projet que Compose donnera au dossier `dir` : la clé `name:` du fichier, sinon le nom du
/// dossier.
pub fn project_name(dir: &Path, compose_text: &str) -> String {
    for line in compose_text.lines() {
        if let Some(rest) = line.strip_prefix("name:") {
            let v = rest.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return v.to_owned();
            }
        }
    }
    default_name(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_du_projet() {
        let d = Path::new(r"C:\Users\x\Mon Blog");
        assert_eq!(project_name(d, "services: {}"), "monblog");
        assert_eq!(project_name(d, "name: blog\nservices: {}"), "blog");
        assert_eq!(project_name(d, "name: \"blog\"\n"), "blog");
    }
}
