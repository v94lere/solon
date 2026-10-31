//! Sauvegarde et restauration d'un projet Compose en un seul fichier zip : le fichier Compose, le `.env`
//! et le contenu de chaque volume nommé du projet (archives tar.gz faites dans la machine, directement
//! depuis `/var/lib/docker/volumes/<nom>/_data`, sans image supplémentaire et même si les conteneurs sont
//! arrêtés). La restauration recrée les volumes avec les étiquettes que Compose attend, y remet les
//! données, dépose les fichiers dans le dossier choisi ; l'utilisateur fait ensuite Up.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::compose::{self, COMPOSE_FILES};
use crate::control::{exec, shared_temp, shell_quote};

const MANIFEST: &str = "solon-backup.json";
const VOLUMES_DIR: &str = "volumes";

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    project: String,
    compose_file: String,
    created_unix_ms: u64,
    solon_version: String,
    volumes: Vec<VolumeEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VolumeEntry {
    /// Nom complet du volume (`blog_db`).
    name: String,
    /// Partie propre au projet (`db`), pour renommer si le projet change de nom à la restauration.
    suffix: String,
    /// Fichier dans le zip (`volumes/db.tgz`).
    file: String,
}

#[derive(Debug, Serialize)]
pub struct BackupReport {
    pub path: String,
    pub volumes: usize,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct RestoreReport {
    pub dir: String,
    pub project: String,
    pub volumes: usize,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Volumes nommés du projet, d'après l'étiquette posée par Compose.
async fn project_volumes(project: &str) -> Result<Vec<String>, String> {
    let (code, out, err) = exec(
        format!(
            "docker volume ls --filter label=com.docker.compose.project={} --format '{{{{.Name}}}}'",
            shell_quote(project)
        ),
        30,
    )
    .await?;
    if code != 0 {
        return Err(format!("docker volume ls: {}", err.trim()));
    }
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Sauvegarde `dir` (projet Compose nommé `project`) dans le zip `dest`. `solon_version` est
/// notée dans le manifeste.
pub async fn backup(
    dir: &str,
    project: &str,
    dest: &str,
    solon_version: &str,
    overwrite: bool,
) -> Result<BackupReport, String> {
    if !overwrite && Path::new(dest).exists() {
        return Err(format!(
            "{dest} already exists: choose another name or pass --force"
        ));
    }
    let detected = compose::detect(dir)?.ok_or_else(|| format!("no Compose file in {dir}"))?;
    let root = Path::new(dir);
    let volumes = project_volumes(project).await?;
    let (tmp, guest_tmp) = shared_temp("solon-backup").await?;
    let result: Result<BackupReport, String> = async {
        // Archives des volumes, faites dans la machine (root, accès direct aux données).
        let mut entries = Vec::new();
        for v in &volumes {
            let suffix = v
                .strip_prefix(&format!("{project}_"))
                .unwrap_or(v)
                .to_owned();
            let file = format!("{VOLUMES_DIR}/{suffix}.tgz");
            let cmd = format!(
                "test -d /var/lib/docker/volumes/{n}/_data && tar czf {out}/{s}.tgz -C /var/lib/docker/volumes/{n}/_data .",
                n = shell_quote(v),
                out = shell_quote(&guest_tmp),
                s = shell_quote(&suffix)
            );
            let (code, _, err) = exec(cmd, 3600).await?;
            if code != 0 {
                return Err(format!("archiving volume {v}: {}", err.trim()));
            }
            entries.push(VolumeEntry {
                name: v.clone(),
                suffix,
                file,
            });
        }
        // Le zip : manifeste, fichiers du projet, archives.
        let file = std::fs::File::create(dest).map_err(|e| format!("{dest} : {e}"))?;
        let mut zip = zip::ZipWriter::new(file);
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default();
        let manifest = Manifest {
            format: 1,
            project: project.to_owned(),
            compose_file: detected.file.clone(),
            created_unix_ms: now_unix_ms(),
            solon_version: solon_version.to_owned(),
            volumes: entries,
        };
        zip.start_file(MANIFEST, deflated)
            .and_then(|_| {
                zip.write_all(
                    serde_json::to_string_pretty(&manifest)
                        .unwrap_or_default()
                        .as_bytes(),
                )
                .map_err(Into::into)
            })
            .map_err(|e| e.to_string())?;
        let mut project_files: Vec<String> = vec![detected.file.clone()];
        for extra in [
            ".env",
            "compose.override.yaml",
            "compose.override.yml",
            "docker-compose.override.yml",
        ] {
            if root.join(extra).is_file() && !project_files.iter().any(|f| f == extra) {
                project_files.push(extra.to_owned());
            }
        }
        for name in &project_files {
            let data = std::fs::read(root.join(name)).map_err(|e| format!("{name} : {e}"))?;
            zip.start_file(format!("project/{name}"), deflated)
                .and_then(|_| zip.write_all(&data).map_err(Into::into))
                .map_err(|e| e.to_string())?;
        }
        for entry in &manifest.volumes {
            let src = tmp.join(format!("{}.tgz", entry.suffix));
            let mut f =
                std::fs::File::open(&src).map_err(|e| format!("{} : {e}", src.display()))?;
            zip.start_file(&entry.file, stored)
                .map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
        let bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        Ok(BackupReport {
            path: dest.to_owned(),
            volumes: manifest.volumes.len(),
            bytes,
        })
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// Lit le manifeste d'un zip de sauvegarde (aperçu avant restauration).
pub fn info(zip_path: &str) -> Result<serde_json::Value, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("{zip_path} : {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = archive
        .by_name(MANIFEST)
        .map_err(|_| "this file is not a Solon project backup (no solon-backup.json)".to_owned())?;
    let mut text = String::new();
    entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "project": m.project,
        "compose_file": m.compose_file,
        "created_unix_ms": m.created_unix_ms,
        "solon_version": m.solon_version,
        "volumes": m.volumes.iter().map(|v| v.suffix.clone()).collect::<Vec<_>>(),
    }))
}

/// Restaure le zip `zip_path` dans `target_dir` (créé au besoin ; refusé s'il contient déjà un fichier
/// Compose). Les volumes sont recréés sous le nom du nouveau projet, avec les étiquettes de Compose.
pub async fn restore(zip_path: &str, target_dir: &str) -> Result<RestoreReport, String> {
    let target = PathBuf::from(target_dir);
    std::fs::create_dir_all(&target).map_err(|e| format!("{target_dir} : {e}"))?;
    for f in COMPOSE_FILES {
        if target.join(f).is_file() {
            return Err(format!(
                "{target_dir} already contains {f}: choose an empty folder"
            ));
        }
    }
    let file = std::fs::File::open(zip_path).map_err(|e| format!("{zip_path} : {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let manifest: Manifest = {
        let mut entry = archive.by_name(MANIFEST).map_err(|_| {
            "this file is not a Solon project backup (no solon-backup.json)".to_owned()
        })?;
        let mut text = String::new();
        entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())?
    };
    // Fichiers du projet.
    let mut compose_text = String::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_owned();
        if let Some(rel) = name.strip_prefix("project/") {
            if rel.contains("..") || rel.contains('/') {
                continue;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            if rel == manifest.compose_file {
                compose_text = String::from_utf8_lossy(&data).into_owned();
            }
            std::fs::write(target.join(rel), &data).map_err(|e| format!("{rel} : {e}"))?;
        }
    }
    let project = compose::project_name(&target, &compose_text);
    if manifest.volumes.is_empty() {
        return Ok(RestoreReport {
            dir: target_dir.to_owned(),
            project,
            volumes: 0,
        });
    }
    // Archives des volumes vers le dossier partagé, puis extraction dans la machine.
    let (tmp, guest_tmp) = shared_temp("solon-restore").await?;
    // Extraction des archives d'abord (lecture du zip, synchrone), puis les commandes dans la machine :
    // un `ZipFile` emprunté ne peut pas traverser un `.await`.
    let extracted: Result<(), String> = (|| {
        for v in &manifest.volumes {
            let mut entry = archive
                .by_name(&v.file)
                .map_err(|_| format!("{} missing from the backup", v.file))?;
            let local = tmp.join(format!("{}.tgz", v.suffix));
            let mut out = std::fs::File::create(&local).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    drop(archive);
    if let Err(e) = extracted {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let result: Result<usize, String> = async {
        let (_, version, _) = exec("docker compose version --short".into(), 30).await?;
        let version = version.trim().to_owned();
        let mut done = 0;
        for v in &manifest.volumes {
            let new_name = format!("{project}_{}", v.suffix);
            let cmd = format!(
                "docker volume inspect {n} >/dev/null 2>&1 && echo EXISTS || (docker volume create --label com.docker.compose.project={p} --label com.docker.compose.volume={s} --label com.docker.compose.version={ver} {n} >/dev/null && tar xzf {tmp}/{s}.tgz -C /var/lib/docker/volumes/{n}/_data)",
                n = shell_quote(&new_name),
                p = shell_quote(&project),
                s = shell_quote(&v.suffix),
                ver = shell_quote(if version.is_empty() { "2.0.0" } else { &version }),
                tmp = shell_quote(&guest_tmp)
            );
            let (code, out, err) = exec(cmd, 3600).await?;
            if out.contains("EXISTS") {
                return Err(format!(
                    "volume {new_name} already exists: remove it first or restore into a folder with another name"
                ));
            }
            if code != 0 {
                return Err(format!("restore of volume {new_name}: {}", err.trim()));
            }
            done += 1;
        }
        Ok(done)
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    let volumes = result?;
    Ok(RestoreReport {
        dir: target_dir.to_owned(),
        project,
        volumes,
    })
}
