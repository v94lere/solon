//! Import depuis une installation Docker Desktop existante.
//!
//! Ce qui retient les gens chez Docker Desktop, ce n'est pas l'installation, ce sont leurs données :
//! des bases dans des volumes qu'ils ne veulent pas recréer à la main. On les copie, sans jamais rien
//! toucher du côté de Docker Desktop.
//!
//! Comment : Docker Desktop expose son API sur `\\.\pipe\docker_engine` comme Solon sur
//! `\\.\pipe\solon`. On lui fait écrire chaque volume dans une archive posée dans un dossier
//! temporaire **Windows**, que Solon voit par son propre partage de fichiers ; Solon la déballe dans
//! un volume neuf. Rien ne transite par le programme appelant, et les deux moteurs peuvent tourner en
//! même temps. Les images passent par `docker save` d'un côté et `docker load` de l'autre.
//!
//! Non importés, volontairement : les conteneurs, qui se recréent depuis le fichier Compose, et les
//! projets eux-mêmes, que `solon scan` retrouve déjà.

use std::path::Path;

use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
};
use bollard::{API_DEFAULT_VERSION, Docker};
use futures_util::StreamExt;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::control::{exec, shared_temp, shell_quote};

/// Le pipe de Docker Desktop. Solon ne s'en sert qu'ici, et seulement en lecture.
const DOCKER_DESKTOP_PIPE: &str = r"\\.\pipe\docker_engine";
/// Image minuscule utilisée côté Docker Desktop pour lire un volume (4 Mo).
const HELPER_IMAGE: &str = "public.ecr.aws/docker/library/busybox:1.37";

#[derive(Debug, Clone, Serialize)]
pub struct ImportItem {
    pub name: String,
    /// Taille en octets, ou `null` quand Docker Desktop ne la donne pas.
    pub size: Option<i64>,
    /// Pour un volume : un conteneur l'utilise. Pour une image : elle est utilisée.
    pub in_use: bool,
    /// Un volume ou une image du même nom existe déjà dans Solon : il ne sera pas écrasé.
    pub exists_here: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSource {
    pub available: bool,
    /// Version de Docker Desktop, quand il répond.
    pub version: Option<String>,
    /// Pourquoi il ne répond pas, le cas échéant (absent, arrêté…).
    pub reason: Option<String>,
    pub volumes: Vec<ImportItem>,
    pub images: Vec<ImportItem>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportReport {
    pub volumes_imported: Vec<String>,
    pub images_imported: Vec<String>,
    /// Nom suivi de la raison : déjà présent, ou l'erreur rencontrée.
    pub skipped: Vec<(String, String)>,
}

fn connect() -> Result<Docker, String> {
    Docker::connect_with_named_pipe(DOCKER_DESKTOP_PIPE, 120, API_DEFAULT_VERSION)
        .map_err(|e| e.to_string())
}

/// Noms des volumes et des images déjà présents dans Solon : on ne veut écraser ni les uns ni les autres.
async fn existing_here() -> (Vec<String>, Vec<String>) {
    let volumes = exec("docker volume ls --format '{{.Name}}'".into(), 30)
        .await
        .map(|(_, out, _)| out.lines().map(|l| l.trim().to_owned()).collect())
        .unwrap_or_default();
    let images = exec(
        "docker image ls --format '{{.Repository}}:{{.Tag}}'".into(),
        30,
    )
    .await
    .map(|(_, out, _)| out.lines().map(|l| l.trim().to_owned()).collect())
    .unwrap_or_default();
    (volumes, images)
}

/// Docker Desktop est-il là, et que contient-il ? Ne modifie rien.
pub async fn probe() -> ImportSource {
    let none = |reason: String| ImportSource {
        available: false,
        version: None,
        reason: Some(reason),
        volumes: Vec::new(),
        images: Vec::new(),
    };
    let docker = match connect() {
        Ok(d) => d,
        Err(e) => return none(e),
    };
    let version = match docker.version().await {
        Ok(v) => v.version,
        Err(e) => return none(e.to_string()),
    };

    let (here_volumes, here_images) = existing_here().await;
    let mut volumes = Vec::new();
    let mut images = Vec::new();

    // `df` donne les tailles ; ses listes ne sont pas typées, on y pioche ce qu'il faut.
    if let Ok(df) = docker.df(None).await {
        if let Some(items) = df.volume_usage.and_then(|v| v.items) {
            for v in items {
                let Some(name) = v.get("Name").and_then(|n| n.as_str()) else {
                    continue;
                };
                let usage = v.get("UsageData");
                volumes.push(ImportItem {
                    name: name.to_owned(),
                    size: usage
                        .and_then(|u| u.get("Size"))
                        .and_then(|s| s.as_i64())
                        .filter(|s| *s >= 0),
                    in_use: usage
                        .and_then(|u| u.get("RefCount"))
                        .and_then(|c| c.as_i64())
                        .is_some_and(|c| c > 0),
                    exists_here: here_volumes.iter().any(|h| h == name),
                });
            }
        }
        if let Some(items) = df.image_usage.and_then(|i| i.items) {
            for i in items {
                let tag = i
                    .get("RepoTags")
                    .and_then(|t| t.as_array())
                    .and_then(|t| t.first())
                    .and_then(|t| t.as_str())
                    .unwrap_or("<none>:<none>");
                if tag.starts_with("<none>") {
                    continue; // image intermédiaire ou sans étiquette : rien à réimporter
                }
                images.push(ImportItem {
                    name: tag.to_owned(),
                    size: i.get("Size").and_then(|s| s.as_i64()).filter(|s| *s >= 0),
                    in_use: i
                        .get("Containers")
                        .and_then(|c| c.as_i64())
                        .is_some_and(|c| c > 0),
                    exists_here: here_images.iter().any(|h| h == tag),
                });
            }
        }
    }

    volumes.sort_by(|a, b| b.size.cmp(&a.size).then(a.name.cmp(&b.name)));
    images.sort_by(|a, b| b.size.cmp(&a.size).then(a.name.cmp(&b.name)));
    ImportSource {
        available: true,
        version,
        reason: None,
        volumes,
        images,
    }
}

/// S'assure que l'image d'aide existe côté Docker Desktop ; la télécharge sinon.
async fn ensure_helper(docker: &Docker) -> Result<(), String> {
    if docker.inspect_image(HELPER_IMAGE).await.is_ok() {
        return Ok(());
    }
    let options = CreateImageOptionsBuilder::default()
        .from_image("public.ecr.aws/docker/library/busybox")
        .tag("1.37")
        .build();
    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(item) = stream.next().await {
        item.map_err(|e| format!("pulling {HELPER_IMAGE}: {e}"))?;
    }
    Ok(())
}

/// Fait écrire le contenu de `volume` par Docker Desktop dans `host_dir\<file>` (dossier Windows).
async fn export_volume(
    docker: &Docker,
    volume: &str,
    host_dir: &Path,
    file: &str,
) -> Result<(), String> {
    let body = ContainerCreateBody {
        image: Some(HELPER_IMAGE.to_owned()),
        cmd: Some(vec![
            "sh".into(),
            "-c".into(),
            format!("tar czf /out/{file} -C /vol ."),
        ]),
        host_config: Some(HostConfig {
            binds: Some(vec![
                format!("{volume}:/vol:ro"),
                format!("{}:/out", host_dir.display()),
            ]),
            auto_remove: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    };
    let name = format!("solon-import-{}", std::process::id());
    let options = CreateContainerOptionsBuilder::default().name(&name).build();
    // Un reste d'un essai précédent empêcherait la création : on nettoie d'abord.
    let _ = docker
        .remove_container(
            &name,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;

    let created = docker
        .create_container(Some(options), body)
        .await
        .map_err(|e| format!("creating the export container: {e}"))?;
    let id = created.id;
    let result = async {
        docker
            .start_container(
                &id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
            .map_err(|e| format!("start: {e}"))?;
        let mut waits =
            docker.wait_container(&id, None::<bollard::query_parameters::WaitContainerOptions>);
        let mut code = 0i64;
        while let Some(w) = waits.next().await {
            match w {
                Ok(w) => code = w.status_code,
                // Docker renvoie une erreur quand le conteneur a déjà fini : le code arrive quand même.
                Err(_) => break,
            }
        }
        if code != 0 {
            return Err(format!("archiving {volume} failed (code {code})"));
        }
        Ok(())
    }
    .await;
    let _ = docker
        .remove_container(
            &id,
            Some(RemoveContainerOptionsBuilder::default().force(true).build()),
        )
        .await;
    result
}

/// Copie les volumes et images choisis de Docker Desktop vers Solon. Ne modifie rien chez la source.
/// `progress(label, done, total)` est appelé avant chaque élément.
pub async fn run<F: FnMut(&str, usize, usize)>(
    volumes: &[String],
    images: &[String],
    mut progress: F,
) -> Result<ImportReport, String> {
    let docker = connect()?;
    docker
        .version()
        .await
        .map_err(|e| format!("Docker Desktop does not answer: {e}"))?;

    let (host_dir, guest_dir) = shared_temp("solon-import").await?;
    let total = volumes.len() + images.len();
    let mut done = 0usize;
    let mut report = ImportReport::default();
    let (here_volumes, here_images) = existing_here().await;

    if !volumes.is_empty() {
        progress(HELPER_IMAGE, done, total);
        ensure_helper(&docker).await?;
    }

    for name in volumes {
        progress(name, done, total);
        done += 1;
        if here_volumes.iter().any(|h| h == name) {
            report
                .skipped
                .push((name.clone(), "already exists in Solon".into()));
            continue;
        }
        let file = format!("{}.tgz", name.replace(['/', '\\', ':'], "_"));
        if let Err(e) = export_volume(&docker, name, &host_dir, &file).await {
            report.skipped.push((name.clone(), e));
            continue;
        }
        let command = format!(
            "docker volume create {n} >/dev/null && tar xzf {t}/{f} -C /var/lib/docker/volumes/{n}/_data",
            n = shell_quote(name),
            t = shell_quote(&guest_dir),
            f = shell_quote(&file)
        );
        match exec(command, 1800).await {
            Ok((0, _, _)) => report.volumes_imported.push(name.clone()),
            Ok((code, _, err)) => report
                .skipped
                .push((name.clone(), format!("Solon: {} (code {code})", err.trim()))),
            Err(e) => report.skipped.push((name.clone(), e)),
        }
        let _ = std::fs::remove_file(host_dir.join(&file));
    }

    for name in images {
        progress(name, done, total);
        done += 1;
        if here_images.iter().any(|h| h == name) {
            report
                .skipped
                .push((name.clone(), "already exists in Solon".into()));
            continue;
        }
        let file = format!("{}.tar", name.replace(['/', '\\', ':'], "_"));
        let path = host_dir.join(&file);
        let written = async {
            let mut out = tokio::fs::File::create(&path)
                .await
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let mut stream = docker.export_image(name);
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| format!("docker save: {e}"))?;
                out.write_all(&chunk).await.map_err(|e| e.to_string())?;
            }
            out.flush().await.map_err(|e| e.to_string())
        }
        .await;
        if let Err(e) = written {
            report.skipped.push((name.clone(), e));
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let command = format!(
            "docker load -i {}/{}",
            shell_quote(&guest_dir),
            shell_quote(&file)
        );
        match exec(command, 1800).await {
            Ok((0, _, _)) => report.images_imported.push(name.clone()),
            Ok((code, _, err)) => report
                .skipped
                .push((name.clone(), format!("Solon: {} (code {code})", err.trim()))),
            Err(e) => report.skipped.push((name.clone(), e)),
        }
        let _ = std::fs::remove_file(&path);
    }

    progress("", total, total);
    let _ = std::fs::remove_dir_all(&host_dir);
    Ok(report)
}
