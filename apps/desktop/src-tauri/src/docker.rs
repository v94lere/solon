//! Client Docker de l'application : `bollard` sur le named pipe exposé par le service
//! (`\\.\pipe\solon`). Les flux (journaux, statistiques, exec, événements) sont poussés au
//! frontend par des `Channel` Tauri et fermés explicitement par `stream_close` / `exec_close`.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use base64::Engine as _;
use bollard::exec::{ResizeExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, ExecConfig, HostConfig, NetworkCreateRequest, PortBinding,
    VolumeCreateRequest,
};
use bollard::query_parameters::{
    ListContainersOptionsBuilder, ListImagesOptionsBuilder, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder, RemoveImageOptionsBuilder, StatsOptionsBuilder,
};
use bollard::{API_DEFAULT_VERSION, Docker};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use solon_core::ipc::DOCKER_PIPE;
use tauri::ipc::Channel;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

pub struct ExecSession {
    exec_id: String,
    input: Mutex<Pin<Box<dyn tokio::io::AsyncWrite + Send>>>,
    task: AbortHandle,
}

#[derive(Default)]
pub struct DockerState {
    client: Mutex<Option<Docker>>,
    streams: Mutex<HashMap<u64, AbortHandle>>,
    execs: Mutex<HashMap<u64, Arc<ExecSession>>>,
    next_id: AtomicU64,
}

pub type State<'a> = tauri::State<'a, Arc<DockerState>>;

/// Résumé d'un conteneur pour le menu de la barre des tâches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayContainer {
    pub id: String,
    pub name: String,
    pub running: bool,
}

impl DockerState {
    pub(crate) async fn docker(&self) -> Result<Docker, String> {
        let mut guard = self.client.lock().await;
        if let Some(d) = guard.as_ref() {
            return Ok(d.clone());
        }
        let d = Docker::connect_with_named_pipe(DOCKER_PIPE, 120, API_DEFAULT_VERSION)
            .map_err(|e| format!("connexion Docker : {e}"))?;
        *guard = Some(d.clone());
        Ok(d)
    }

    /// Tous les conteneurs, en marche d'abord puis par nom (menu de la barre des tâches) ;
    /// `None` si Docker ne répond pas.
    pub async fn tray_containers(&self) -> Option<Vec<TrayContainer>> {
        use bollard::models::ContainerSummaryStateEnum as S;
        let docker = self.docker().await.ok()?;
        let list = docker
            .list_containers(Some(
                ListContainersOptionsBuilder::default().all(true).build(),
            ))
            .await
            .ok()?;
        let mut v: Vec<TrayContainer> = list
            .into_iter()
            .map(|c| TrayContainer {
                id: c.id.unwrap_or_default(),
                name: c
                    .names
                    .and_then(|n| n.first().cloned())
                    .map(|n| n.trim_start_matches('/').to_owned())
                    .unwrap_or_default(),
                running: matches!(c.state, Some(S::RUNNING | S::RESTARTING | S::PAUSED)),
            })
            .collect();
        v.sort_by(|a, b| b.running.cmp(&a.running).then_with(|| a.name.cmp(&b.name)));
        Some(v)
    }

    /// Action simple sur un conteneur depuis la barre des tâches.
    pub async fn tray_action(&self, action: &str, id: &str) -> Result<(), String> {
        let docker = self.docker().await?;
        match action {
            "start" => docker.start_container(id, None).await.map_err(err),
            "restart" => docker.restart_container(id, None).await.map_err(err),
            "stop" => docker.stop_container(id, None).await.map_err(err),
            other => Err(format!("action inconnue : {other}")),
        }
    }

    fn next(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    async fn register(&self, handle: AbortHandle) -> u64 {
        let id = self.next();
        self.streams.lock().await.insert(id, handle);
        id
    }
}

fn err(e: bollard::errors::Error) -> String {
    match e {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message,
        } => format!("{message} (HTTP {status_code})"),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------
// Conteneurs
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub async fn containers_list(
    state: State<'_>,
    all: bool,
) -> Result<Vec<bollard::models::ContainerSummary>, String> {
    let docker = state.docker().await?;
    docker
        .list_containers(Some(
            ListContainersOptionsBuilder::default().all(all).build(),
        ))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn container_inspect(
    state: State<'_>,
    id: String,
) -> Result<bollard::models::ContainerInspectResponse, String> {
    state
        .docker()
        .await?
        .inspect_container(&id, None)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn container_start(state: State<'_>, id: String) -> Result<(), String> {
    state
        .docker()
        .await?
        .start_container(&id, None)
        .await
        .map_err(err)
}

/// Renomme un conteneur (`docker rename`). Un conteneur géré par Compose reprendra son nom au
/// prochain `up` : pour ceux-là, c'est le nom du service dans le fichier Compose qui compte.
#[tauri::command]
pub async fn container_rename(state: State<'_>, id: String, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("empty name".into());
    }
    let options = bollard::query_parameters::RenameContainerOptionsBuilder::default()
        .name(name)
        .build();
    state
        .docker()
        .await?
        .rename_container(&id, options)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn container_stop(state: State<'_>, id: String) -> Result<(), String> {
    state
        .docker()
        .await?
        .stop_container(&id, None)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn container_restart(state: State<'_>, id: String) -> Result<(), String> {
    state
        .docker()
        .await?
        .restart_container(&id, None)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn container_kill(state: State<'_>, id: String) -> Result<(), String> {
    state
        .docker()
        .await?
        .kill_container(&id, None)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn container_remove(
    state: State<'_>,
    id: String,
    force: bool,
    volumes: bool,
) -> Result<(), String> {
    state
        .docker()
        .await?
        .remove_container(
            &id,
            Some(
                RemoveContainerOptionsBuilder::default()
                    .force(force)
                    .v(volumes)
                    .build(),
            ),
        )
        .await
        .map_err(err)
}

// ---------------------------------------------------------------------------------------------
// Journaux
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LogChunk {
    pub stream: &'static str,
    pub text: String,
}

#[tauri::command]
pub async fn logs_open(
    state: State<'_>,
    id: String,
    tail: u32,
    timestamps: bool,
    channel: Channel<LogChunk>,
) -> Result<u64, String> {
    let docker = state.docker().await?;
    let options = LogsOptionsBuilder::default()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .timestamps(timestamps)
        .tail(&tail.to_string())
        .build();
    let task = tokio::spawn(async move {
        let mut stream = docker.logs(&id, Some(options));
        while let Some(item) = stream.next().await {
            let chunk = match item {
                Ok(bollard::container::LogOutput::StdOut { message })
                | Ok(bollard::container::LogOutput::Console { message }) => LogChunk {
                    stream: "stdout",
                    text: String::from_utf8_lossy(&message).into_owned(),
                },
                Ok(bollard::container::LogOutput::StdErr { message }) => LogChunk {
                    stream: "stderr",
                    text: String::from_utf8_lossy(&message).into_owned(),
                },
                Ok(_) => continue,
                Err(e) => LogChunk {
                    stream: "error",
                    text: err(e),
                },
            };
            if channel.send(chunk).is_err() {
                return;
            }
        }
        let _ = channel.send(LogChunk {
            stream: "end",
            text: String::new(),
        });
    });
    Ok(state.register(task.abort_handle()).await)
}

#[tauri::command]
pub async fn stream_close(state: State<'_>, stream_id: u64) -> Result<(), String> {
    if let Some(handle) = state.streams.lock().await.remove(&stream_id) {
        handle.abort();
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Statistiques : un canal, un échantillon par seconde et par conteneur en marche
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StatSample {
    pub id: String,
    pub cpu_percent: f64,
    pub mem_usage: u64,
    pub mem_limit: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[tauri::command]
pub async fn stats_open(state: State<'_>, channel: Channel<StatSample>) -> Result<u64, String> {
    let docker = state.docker().await?;
    let task = tokio::spawn(async move {
        let mut per_container: HashMap<String, AbortHandle> = HashMap::new();
        loop {
            let running = match docker
                .list_containers(Some(
                    ListContainersOptionsBuilder::default().all(false).build(),
                ))
                .await
            {
                Ok(list) => list.into_iter().filter_map(|c| c.id).collect::<Vec<_>>(),
                Err(_) => Vec::new(),
            };
            per_container.retain(|id, handle| {
                if running.contains(id) && !handle.is_finished() {
                    true
                } else {
                    handle.abort();
                    false
                }
            });
            for id in running {
                if per_container.contains_key(&id) {
                    continue;
                }
                let docker = docker.clone();
                let channel = channel.clone();
                let cid = id.clone();
                let handle = tokio::spawn(async move {
                    let mut stream = docker.stats(
                        &cid,
                        Some(StatsOptionsBuilder::default().stream(true).build()),
                    );
                    let mut prev: Option<(u64, u64)> = None;
                    while let Some(Ok(s)) = stream.next().await {
                        let cpu = s.cpu_stats.as_ref();
                        let total = cpu
                            .and_then(|c| c.cpu_usage.as_ref())
                            .and_then(|u| u.total_usage)
                            .unwrap_or(0);
                        let system = cpu.and_then(|c| c.system_cpu_usage).unwrap_or(0);
                        let online = cpu.and_then(|c| c.online_cpus).unwrap_or(1).max(1) as f64;
                        let cpu_percent = match prev {
                            Some((pt, ps)) if system > ps && total >= pt => {
                                (total - pt) as f64 / (system - ps) as f64 * online * 100.0
                            }
                            _ => 0.0,
                        };
                        prev = Some((total, system));
                        let mem = s.memory_stats.as_ref();
                        let usage = mem.and_then(|m| m.usage).unwrap_or(0);
                        let inactive = mem
                            .and_then(|m| m.stats.as_ref())
                            .and_then(|st| st.get("inactive_file").copied())
                            .unwrap_or(0);
                        let (mut rx, mut tx) = (0u64, 0u64);
                        if let Some(nets) = &s.networks {
                            for n in nets.values() {
                                rx += n.rx_bytes.unwrap_or(0);
                                tx += n.tx_bytes.unwrap_or(0);
                            }
                        }
                        let sample = StatSample {
                            id: cid.clone(),
                            cpu_percent: (cpu_percent * 10.0).round() / 10.0,
                            mem_usage: usage.saturating_sub(inactive),
                            mem_limit: mem.and_then(|m| m.limit).unwrap_or(0),
                            rx_bytes: rx,
                            tx_bytes: tx,
                        };
                        if channel.send(sample).is_err() {
                            return;
                        }
                    }
                });
                per_container.insert(id, handle.abort_handle());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    Ok(state.register(task.abort_handle()).await)
}

// ---------------------------------------------------------------------------------------------
// Terminal (exec hijacké)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ExecOutput {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[tauri::command]
pub async fn exec_open(
    state: State<'_>,
    id: String,
    cmd: Vec<String>,
    cols: u16,
    rows: u16,
    channel: Channel<ExecOutput>,
) -> Result<u64, String> {
    let docker = state.docker().await?;
    let config = ExecConfig {
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(true),
        cmd: Some(cmd),
        console_size: Some(vec![rows as usize, cols as usize]),
        env: Some(vec!["TERM=xterm-256color".into()]),
        ..Default::default()
    };
    let created = docker.create_exec(&id, config).await.map_err(err)?;
    let exec_id = created.id;
    let started = docker
        .start_exec(
            &exec_id,
            Some(StartExecOptions {
                detach: false,
                tty: true,
                output_capacity: Some(64 * 1024),
            }),
        )
        .await
        .map_err(err)?;
    let (mut output, input) = match started {
        StartExecResults::Attached { output, input } => (output, input),
        StartExecResults::Detached => return Err("exec détaché inattendu".into()),
    };
    let ch = channel.clone();
    let task = tokio::spawn(async move {
        while let Some(item) = output.next().await {
            let out = match item {
                Ok(log) => ExecOutput {
                    kind: "data",
                    data: Some(base64::engine::general_purpose::STANDARD.encode(log.into_bytes())),
                    message: None,
                },
                Err(e) => ExecOutput {
                    kind: "error",
                    data: None,
                    message: Some(err(e)),
                },
            };
            if ch.send(out).is_err() {
                return;
            }
        }
        let _ = ch.send(ExecOutput {
            kind: "end",
            data: None,
            message: None,
        });
    });
    let session = Arc::new(ExecSession {
        exec_id,
        input: Mutex::new(input),
        task: task.abort_handle(),
    });
    let id = state.next();
    state.execs.lock().await.insert(id, session);
    Ok(id)
}

#[tauri::command]
pub async fn exec_input(state: State<'_>, exec_id: u64, data: String) -> Result<(), String> {
    let session = state
        .execs
        .lock()
        .await
        .get(&exec_id)
        .cloned()
        .ok_or("session terminal inconnue")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| e.to_string())?;
    let mut input = session.input.lock().await;
    input.write_all(&bytes).await.map_err(|e| e.to_string())?;
    input.flush().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn exec_resize(
    state: State<'_>,
    exec_id: u64,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let session = state
        .execs
        .lock()
        .await
        .get(&exec_id)
        .cloned()
        .ok_or("session terminal inconnue")?;
    state
        .docker()
        .await?
        .resize_exec(
            &session.exec_id,
            ResizeExecOptions {
                height: rows,
                width: cols,
            },
        )
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn exec_close(state: State<'_>, exec_id: u64) -> Result<(), String> {
    if let Some(session) = state.execs.lock().await.remove(&exec_id) {
        session.task.abort();
        let mut input = session.input.lock().await;
        let _ = input.shutdown().await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Événements Docker (rafraîchissement des listes sans polling)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DockerEvent {
    pub action: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub name: String,
    /// Code de sortie (événement `die`), tel que fourni par Docker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<String>,
}

#[tauri::command]
pub async fn docker_events_open(
    state: State<'_>,
    channel: Channel<DockerEvent>,
) -> Result<u64, String> {
    let docker = state.docker().await?;
    let task = tokio::spawn(async move {
        let mut stream = docker.events(None);
        while let Some(Ok(ev)) = stream.next().await {
            let actor = ev.actor.as_ref();
            let event = DockerEvent {
                action: ev.action.unwrap_or_default(),
                kind: ev
                    .typ
                    .map(|t| format!("{t:?}").to_lowercase())
                    .unwrap_or_default(),
                id: actor.and_then(|a| a.id.clone()).unwrap_or_default(),
                name: actor
                    .and_then(|a| a.attributes.as_ref())
                    .and_then(|a| a.get("name").cloned())
                    .unwrap_or_default(),
                exit_code: actor
                    .and_then(|a| a.attributes.as_ref())
                    .and_then(|m| m.get("exitCode").cloned()),
            };
            if channel.send(event).is_err() {
                return;
            }
        }
    });
    Ok(state.register(task.abort_handle()).await)
}

// ---------------------------------------------------------------------------------------------
// Images, volumes, réseaux
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub async fn images_list(state: State<'_>) -> Result<Vec<bollard::models::ImageSummary>, String> {
    state
        .docker()
        .await?
        .list_images(Some(ListImagesOptionsBuilder::default().all(false).build()))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn image_inspect(
    state: State<'_>,
    id: String,
) -> Result<bollard::models::ImageInspect, String> {
    state.docker().await?.inspect_image(&id).await.map_err(err)
}

#[tauri::command]
pub async fn image_remove(state: State<'_>, id: String, force: bool) -> Result<(), String> {
    state
        .docker()
        .await?
        .remove_image(
            &id,
            Some(RemoveImageOptionsBuilder::default().force(force).build()),
            None,
        )
        .await
        .map(|_| ())
        .map_err(err)
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunSpec {
    pub image: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub cmd: Option<Vec<String>>,
    #[serde(default)]
    pub env: Vec<String>,
    /// Publications `host:container/proto`.
    #[serde(default)]
    pub ports: Vec<RunPort>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunPort {
    pub host: u16,
    pub container: u16,
    #[serde(default = "tcp")]
    pub proto: String,
}

fn tcp() -> String {
    "tcp".into()
}

#[tauri::command]
pub async fn image_run(state: State<'_>, spec: RunSpec) -> Result<String, String> {
    let docker = state.docker().await?;
    let mut exposed: Vec<String> = Vec::new();
    let mut bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for p in &spec.ports {
        let key = format!("{}/{}", p.container, p.proto);
        if !exposed.contains(&key) {
            exposed.push(key.clone());
        }
        bindings
            .entry(key)
            .or_insert_with(|| Some(Vec::new()))
            .get_or_insert_with(Vec::new)
            .push(PortBinding {
                host_ip: Some("0.0.0.0".into()),
                host_port: Some(p.host.to_string()),
            });
    }
    let body = ContainerCreateBody {
        image: Some(spec.image),
        cmd: spec.cmd,
        env: if spec.env.is_empty() {
            None
        } else {
            Some(spec.env)
        },
        exposed_ports: if exposed.is_empty() {
            None
        } else {
            Some(exposed)
        },
        host_config: Some(HostConfig {
            port_bindings: if bindings.is_empty() {
                None
            } else {
                Some(bindings)
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let options = spec.name.map(|n| {
        bollard::query_parameters::CreateContainerOptionsBuilder::default()
            .name(&n)
            .build()
    });
    let created = docker.create_container(options, body).await.map_err(err)?;
    docker
        .start_container(&created.id, None)
        .await
        .map_err(err)?;
    Ok(created.id)
}

#[tauri::command]
pub async fn volumes_list(state: State<'_>) -> Result<bollard::models::VolumeListResponse, String> {
    state
        .docker()
        .await?
        .list_volumes(None::<bollard::query_parameters::ListVolumesOptions>)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn volume_create(
    state: State<'_>,
    name: String,
) -> Result<bollard::models::Volume, String> {
    state
        .docker()
        .await?
        .create_volume(VolumeCreateRequest {
            name: Some(name),
            ..Default::default()
        })
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn volume_remove(state: State<'_>, name: String, force: bool) -> Result<(), String> {
    state
        .docker()
        .await?
        .remove_volume(
            &name,
            Some(
                bollard::query_parameters::RemoveVolumeOptionsBuilder::default()
                    .force(force)
                    .build(),
            ),
        )
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn volume_inspect(
    state: State<'_>,
    name: String,
) -> Result<bollard::models::Volume, String> {
    state
        .docker()
        .await?
        .inspect_volume(&name)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn networks_list(state: State<'_>) -> Result<Vec<bollard::models::Network>, String> {
    state.docker().await?.list_networks(None).await.map_err(err)
}

#[tauri::command]
pub async fn network_create(
    state: State<'_>,
    name: String,
    driver: Option<String>,
) -> Result<String, String> {
    let created = state
        .docker()
        .await?
        .create_network(NetworkCreateRequest {
            name,
            driver: driver.or(Some("bridge".into())),
            ..Default::default()
        })
        .await
        .map_err(err)?;
    Ok(created.id)
}

#[tauri::command]
pub async fn network_remove(state: State<'_>, id: String) -> Result<(), String> {
    state.docker().await?.remove_network(&id).await.map_err(err)
}

#[tauri::command]
pub async fn network_inspect(
    state: State<'_>,
    id: String,
) -> Result<bollard::models::NetworkInspect, String> {
    state
        .docker()
        .await?
        .inspect_network(&id, None)
        .await
        .map_err(err)
}

// ---------------------------------------------------------------------------------------------
// Copie de fichiers (docker cp) : archive tar via l'API, extraite ou construite avec tar.exe de Windows.
// ---------------------------------------------------------------------------------------------

pub(crate) fn windows_tar() -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"));
    root.join("System32").join("tar.exe")
}

pub(crate) fn temp_tar() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "solon-copy-{}-{}.tar",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ))
}

/// Copie `path` (fichier ou dossier du conteneur) dans le dossier Windows `dest_dir` ; renvoie ce dossier.
#[tauri::command]
pub async fn container_copy_from(
    state: State<'_>,
    id: String,
    path: String,
    dest_dir: String,
) -> Result<String, String> {
    let docker = state.docker().await?;
    let opts = bollard::query_parameters::DownloadFromContainerOptionsBuilder::default()
        .path(&path)
        .build();
    let mut stream = docker.download_from_container(&id, Some(opts));
    let tmp = temp_tar();
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(err)?;
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| e.to_string())?;
    let (tmp2, dest2) = (tmp.clone(), dest_dir.clone());
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
    Ok(dest_dir)
}

/// Copie le fichier ou dossier Windows `source` dans le dossier `dest` du conteneur.
#[tauri::command]
pub async fn container_copy_to(
    state: State<'_>,
    id: String,
    source: String,
    dest: String,
) -> Result<(), String> {
    let docker = state.docker().await?;
    let src = std::path::PathBuf::from(&source);
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
    let opts = bollard::query_parameters::UploadToContainerOptionsBuilder::default()
        .path(&dest)
        .build();
    docker
        .upload_to_container(&id, Some(opts), bollard::body_full(bytes.into()))
        .await
        .map_err(err)
}
