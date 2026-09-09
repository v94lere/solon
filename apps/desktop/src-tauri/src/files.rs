//! Onglet Fichiers : parcourir, télécharger, envoyer, créer et supprimer des fichiers dans un
//! conteneur ou un volume, sans rien exiger de l'image (pas de shell nécessaire).
//!
//! - **Lister, créer, supprimer** : commandes exécutées dans la machine Solon (`ServiceCommand::Exec`,
//!   busybox), sur le système de fichiers fusionné du conteneur (`GraphDriver.Data.MergedDir`, donc
//!   conteneur en marche) ou sur le dossier du volume (`/var/lib/docker/volumes/<nom>/_data`).
//! - **Télécharger, envoyer** : API `archive` de Docker (tar) + `tar.exe` de Windows. Pour un volume,
//!   un conteneur auxiliaire jetable (image vide `solon-empty`, jamais démarré) monte le volume sur `/v`.

use bollard::Docker;
use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, DownloadFromContainerOptionsBuilder,
    RemoveContainerOptionsBuilder, UploadToContainerOptionsBuilder,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use solon_core::ipc::ServiceCommand;
use solon_core::protocol::ExecResult;
use tokio::io::AsyncWriteExt;

use crate::docker::{State, temp_tar, windows_tar};
use crate::service;

const VOLUMES_ROOT: &str = "/var/lib/docker/volumes";
const HELPER_IMAGE: &str = "solon-empty";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    Container { id: String },
    Volume { name: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    /// `dir`, `file`, `link` ou `other`.
    pub kind: &'static str,
    pub size: u64,
    /// Secondes depuis l'époque Unix.
    pub mtime: u64,
    /// Droits en octal (`755`).
    pub mode: String,
}

/// Chemin relatif propre (`a/b/c`, sans `.` ni `..`, vide pour la racine).
fn normalize(path: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => return Err("invalid path".into()),
            other => parts.push(other),
        }
    }
    Ok(parts.join("/"))
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

async fn machine_exec(command: String, timeout_s: u64) -> Result<ExecResult, String> {
    let v = service::call(ServiceCommand::Exec {
        command,
        timeout_s: Some(timeout_s),
    })
    .await?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Dossier racine de la cible dans la machine.
async fn root_dir(docker: &Docker, target: &Target) -> Result<String, String> {
    match target {
        Target::Container { id } => {
            let info = docker
                .inspect_container(id, None)
                .await
                .map_err(|e| e.to_string())?;
            let running = info.state.as_ref().and_then(|s| s.running).unwrap_or(false);
            if !running {
                return Err("container not running".into());
            }
            info.graph_driver
                .and_then(|g| g.data.get("MergedDir").cloned())
                .filter(|d| !d.is_empty())
                .ok_or_else(|| "container filesystem not available".to_string())
        }
        Target::Volume { name } => {
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            {
                return Err("invalid volume name".into());
            }
            Ok(format!("{VOLUMES_ROOT}/{name}/_data"))
        }
    }
}

fn join(root: &str, rel: &str) -> String {
    if rel.is_empty() {
        root.to_owned()
    } else {
        format!("{root}/{rel}")
    }
}

#[tauri::command]
pub async fn files_list(
    state: State<'_>,
    target: Target,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    let docker = state.docker().await?;
    let root = root_dir(&docker, &target).await?;
    let dir = join(&root, &normalize(&path)?);
    // `stat` de busybox ; les motifs sans correspondance échouent silencieusement (2>/dev/null).
    let cmd = format!(
        "cd {} && stat -c '%F|%s|%Y|%a|%n' -- * .[!.]* ..?* 2>/dev/null; true",
        sh_quote(&dir)
    );
    let r = machine_exec(cmd, 30).await?;
    if r.code != Some(0) {
        return Err(if r.stderr.trim().is_empty() {
            format!("cannot open {dir}")
        } else {
            r.stderr.trim().to_owned()
        });
    }
    let mut out = Vec::new();
    for line in r.stdout.lines() {
        let mut it = line.splitn(5, '|');
        let (Some(kind), Some(size), Some(mtime), Some(mode), Some(name)) =
            (it.next(), it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let kind = if kind.starts_with("directory") {
            "dir"
        } else if kind.starts_with("symbolic link") {
            "link"
        } else if kind.starts_with("regular") {
            "file"
        } else {
            "other"
        };
        out.push(FileEntry {
            name: name.to_owned(),
            kind,
            size: size.parse().unwrap_or(0),
            mtime: mtime.parse().unwrap_or(0),
            mode: mode.to_owned(),
        });
    }
    out.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

#[tauri::command]
pub async fn files_mkdir(state: State<'_>, target: Target, path: String) -> Result<(), String> {
    let docker = state.docker().await?;
    let root = root_dir(&docker, &target).await?;
    let rel = normalize(&path)?;
    if rel.is_empty() {
        return Err("invalid path".into());
    }
    let r = machine_exec(format!("mkdir -p -- {}", sh_quote(&join(&root, &rel))), 30).await?;
    if r.code != Some(0) {
        return Err(r.stderr.trim().to_owned());
    }
    Ok(())
}

#[tauri::command]
pub async fn files_delete(state: State<'_>, target: Target, path: String) -> Result<(), String> {
    let docker = state.docker().await?;
    let root = root_dir(&docker, &target).await?;
    let rel = normalize(&path)?;
    if rel.is_empty() {
        return Err("refusing to delete the root".into());
    }
    let r = machine_exec(format!("rm -rf -- {}", sh_quote(&join(&root, &rel))), 120).await?;
    if r.code != Some(0) {
        return Err(r.stderr.trim().to_owned());
    }
    Ok(())
}

/// Conteneur auxiliaire (jamais démarré) qui monte le volume sur `/v`, pour l'API `archive`.
struct Helper {
    docker: Docker,
    id: String,
}

impl Helper {
    async fn create(docker: &Docker, volume: &str) -> Result<Helper, String> {
        // Image vide importée une fois dans la machine (aucun téléchargement).
        let r = machine_exec(
            format!(
                "docker image inspect {HELPER_IMAGE} >/dev/null 2>&1 || head -c 1024 /dev/zero | docker import - {HELPER_IMAGE} >/dev/null"
            ),
            60,
        )
        .await?;
        if r.code != Some(0) {
            return Err(format!("helper image: {}", r.stderr.trim()));
        }
        let name = format!(
            "solon-files-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let body = ContainerCreateBody {
            image: Some(HELPER_IMAGE.to_owned()),
            cmd: Some(vec!["/solon-files-helper".to_owned()]),
            host_config: Some(HostConfig {
                binds: Some(vec![format!("{volume}:/v")]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let created = docker
            .create_container(
                Some(CreateContainerOptionsBuilder::default().name(&name).build()),
                body,
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Helper {
            docker: docker.clone(),
            id: created.id,
        })
    }

    async fn remove(self) {
        let _ = self
            .docker
            .remove_container(
                &self.id,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            )
            .await;
    }
}

/// Archive `path` du conteneur `container` et l'extrait dans `dest_dir`.
async fn archive_to_dir(
    docker: &Docker,
    container: &str,
    path: &str,
    dest_dir: &str,
) -> Result<(), String> {
    let opts = DownloadFromContainerOptionsBuilder::default()
        .path(path)
        .build();
    let mut stream = docker.download_from_container(container, Some(opts));
    let tmp = temp_tar();
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::create_dir_all(dest_dir)
        .await
        .map_err(|e| e.to_string())?;
    let (tmp2, dest2) = (tmp.clone(), dest_dir.to_owned());
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new(windows_tar())
            .arg("-xf")
            .arg(&tmp2)
            .arg("-C")
            .arg(&dest2)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    let _ = tokio::fs::remove_file(&tmp).await;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(())
}

/// Archive le fichier ou dossier Windows `source` et l'envoie dans `dest` du conteneur.
async fn dir_to_container(
    docker: &Docker,
    container: &str,
    source: &str,
    dest: &str,
) -> Result<(), String> {
    let src = std::path::PathBuf::from(source);
    let parent = src
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or("invalid source path")?
        .to_path_buf();
    let name = src.file_name().ok_or("invalid source path")?.to_os_string();
    let tmp = temp_tar();
    let tmp2 = tmp.clone();
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new(windows_tar())
            .arg("-cf")
            .arg(&tmp2)
            .arg("-C")
            .arg(&parent)
            .arg(&name)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let bytes = tokio::fs::read(&tmp).await.map_err(|e| e.to_string())?;
    let _ = tokio::fs::remove_file(&tmp).await;
    let opts = UploadToContainerOptionsBuilder::default()
        .path(dest)
        .build();
    docker
        .upload_to_container(container, Some(opts), bollard::body_full(bytes.into()))
        .await
        .map_err(|e| e.to_string())
}

/// Chemin dans le conteneur (ou l'auxiliaire) pour l'API archive ; la racine donne son contenu (`/.`).
fn archive_path(prefix: &str, rel: &str) -> String {
    if rel.is_empty() {
        format!("{prefix}/.")
    } else {
        format!("{prefix}/{rel}")
    }
}

/// Télécharge `path` (fichier ou dossier) dans le dossier Windows `dest_dir` ; renvoie ce dossier.
#[tauri::command]
pub async fn files_download(
    state: State<'_>,
    target: Target,
    path: String,
    dest_dir: String,
) -> Result<String, String> {
    let docker = state.docker().await?;
    let rel = normalize(&path)?;
    match &target {
        Target::Container { id } => {
            archive_to_dir(&docker, id, &archive_path("", &rel), &dest_dir).await?
        }
        Target::Volume { name } => {
            root_dir(&docker, &target).await?;
            let helper = Helper::create(&docker, name).await?;
            let r = archive_to_dir(&docker, &helper.id, &archive_path("/v", &rel), &dest_dir).await;
            helper.remove().await;
            r?
        }
    }
    Ok(dest_dir)
}

/// Envoie le fichier ou dossier Windows `source` dans le dossier `dest_path` de la cible.
#[tauri::command]
pub async fn files_upload(
    state: State<'_>,
    target: Target,
    dest_path: String,
    source: String,
) -> Result<(), String> {
    let docker = state.docker().await?;
    let rel = normalize(&dest_path)?;
    match &target {
        Target::Container { id } => {
            dir_to_container(&docker, id, &source, &join("/", &rel).replace("//", "/")).await
        }
        Target::Volume { name } => {
            root_dir(&docker, &target).await?;
            let helper = Helper::create(&docker, name).await?;
            let r = dir_to_container(&docker, &helper.id, &source, &join("/v", &rel)).await;
            helper.remove().await;
            r
        }
    }
}
