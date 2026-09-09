//! Galerie de piles et détection de projet : sonde un dossier Windows (quels fichiers de projet il
//! contient) pour que l'application propose un environnement, et écrit les fichiers d'un modèle
//! (`compose.yaml` et compagnie) dans un dossier de projet, sans jamais écraser un fichier existant.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize)]
pub struct Probe {
    pub dir: String,
    pub name: String,
    pub has_compose: bool,
    pub has_dockerfile: bool,
    /// Scripts déclarés dans `package.json` (`dev`, `start`, `build`…).
    pub node_scripts: Vec<String>,
    pub node_framework: Option<String>,
    pub python_requirements: bool,
    pub python_pyproject: bool,
    /// Fichiers Python plausibles comme point d'entrée (`main.py`, `app.py`, `manage.py`, `wsgi.py`).
    pub python_entries: Vec<String>,
    pub php_composer: bool,
    pub php_files: usize,
    pub go_mod: bool,
    pub cargo: bool,
    pub java_maven: bool,
    pub java_gradle: bool,
    pub dotnet_projects: Vec<String>,
    pub ruby_gemfile: bool,
    pub index_html: bool,
    /// Sous-dossiers contenant un `__manifest__.py` (modules Odoo).
    pub odoo_addons: Vec<String>,
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Regarde ce qu'un dossier contient (premier niveau, plus un niveau pour les modules Odoo).
#[tauri::command]
pub fn stack_probe(dir: String) -> Result<Probe, String> {
    let root = PathBuf::from(&dir);
    if !root.is_dir() {
        return Err("not a folder".into());
    }
    let mut p = Probe {
        dir: dir.clone(),
        name: file_name(&root),
        ..Default::default()
    };
    let entries: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    for e in &entries {
        let name = file_name(e);
        let lower = name.to_lowercase();
        if e.is_file() {
            match lower.as_str() {
                "compose.yaml" | "compose.yml" | "docker-compose.yaml" | "docker-compose.yml" => {
                    p.has_compose = true
                }
                "dockerfile" => p.has_dockerfile = true,
                "package.json" => {
                    if let Ok(text) = std::fs::read_to_string(e) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(s) = v.get("scripts").and_then(|s| s.as_object()) {
                                p.node_scripts = s.keys().cloned().collect();
                            }
                            let deps = |k: &str| {
                                v.get(k)
                                    .and_then(|d| d.as_object())
                                    .map(|d| d.keys().cloned().collect::<Vec<_>>())
                                    .unwrap_or_default()
                            };
                            let all: Vec<String> =
                                [deps("dependencies"), deps("devDependencies")].concat();
                            p.node_framework = [
                                "next",
                                "nuxt",
                                "@angular/core",
                                "vite",
                                "react",
                                "vue",
                                "svelte",
                                "express",
                                "fastify",
                                "nest",
                            ]
                            .iter()
                            .find(|f| all.iter().any(|d| d == *f))
                            .map(|f| {
                                f.trim_start_matches('@')
                                    .split('/')
                                    .next()
                                    .unwrap_or(f)
                                    .to_owned()
                            });
                        }
                    }
                }
                "requirements.txt" => p.python_requirements = true,
                "pyproject.toml" => p.python_pyproject = true,
                "main.py" | "app.py" | "manage.py" | "wsgi.py" | "server.py" => {
                    p.python_entries.push(name.clone())
                }
                "composer.json" => p.php_composer = true,
                "go.mod" => p.go_mod = true,
                "cargo.toml" => p.cargo = true,
                "pom.xml" => p.java_maven = true,
                "build.gradle" | "build.gradle.kts" => p.java_gradle = true,
                "gemfile" => p.ruby_gemfile = true,
                "index.html" => p.index_html = true,
                _ => {
                    if lower.ends_with(".php") {
                        p.php_files += 1;
                    } else if lower.ends_with(".csproj") || lower.ends_with(".sln") {
                        p.dotnet_projects.push(name.clone());
                    }
                }
            }
        } else if e.is_dir() && !lower.starts_with('.') && lower != "node_modules" {
            if e.join("__manifest__.py").is_file() {
                p.odoo_addons.push(name.clone());
            }
            if (lower == "public" || lower == "www" || lower == "site")
                && e.join("index.html").is_file()
            {
                p.index_html = true;
            }
        }
    }
    p.python_entries.sort();
    p.odoo_addons.sort();
    Ok(p)
}

#[derive(Debug, Deserialize)]
pub struct ScaffoldFile {
    pub path: String,
    pub content: String,
}

/// Écrit les fichiers d'un modèle dans `dir` (créé au besoin) ; refuse d'écraser un fichier existant
/// et refuse les chemins qui sortent du dossier. Renvoie le dossier du projet.
#[tauri::command]
pub fn project_scaffold(dir: String, files: Vec<ScaffoldFile>) -> Result<String, String> {
    let root = PathBuf::from(&dir);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut targets = Vec::new();
    for f in &files {
        let rel = Path::new(&f.path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(format!("invalid file path: {}", f.path));
        }
        let target = root.join(rel);
        if target.exists() {
            return Err(format!("{} already exists", f.path));
        }
        targets.push((target, &f.content));
    }
    for (target, content) in targets {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&target, content).map_err(|e| e.to_string())?;
    }
    let s = root.to_string_lossy().into_owned();
    Ok(s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonde_et_ecriture() {
        let dir = std::env::temp_dir().join(format!("solon-stacks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mon_module")).unwrap();
        std::fs::write(dir.join("mon_module/__manifest__.py"), "{}").unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"dev":"vite"},"dependencies":{"vue":"3"}}"#,
        )
        .unwrap();
        let p = stack_probe(dir.to_string_lossy().into_owned()).unwrap();
        assert_eq!(p.odoo_addons, vec!["mon_module"]);
        assert_eq!(p.node_scripts, vec!["dev"]);
        assert_eq!(p.node_framework.as_deref(), Some("vue"));
        assert!(!p.has_compose);
        let out = project_scaffold(
            dir.join("stack").to_string_lossy().into_owned(),
            vec![ScaffoldFile {
                path: "compose.yaml".into(),
                content: "services: {}\n".into(),
            }],
        )
        .unwrap();
        assert!(Path::new(&out).join("compose.yaml").is_file());
        assert!(
            project_scaffold(
                dir.join("stack").to_string_lossy().into_owned(),
                vec![ScaffoldFile {
                    path: "compose.yaml".into(),
                    content: String::new(),
                }],
            )
            .is_err()
        );
        assert!(
            project_scaffold(
                dir.to_string_lossy().into_owned(),
                vec![ScaffoldFile {
                    path: "../x".into(),
                    content: String::new(),
                }],
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
