//! Application de bureau Solon (Tauri v2).
//!
//! - [`service`] : canal de contrôle du service Windows (état du moteur, démarrage/arrêt, réglages) ;
//! - [`docker`] : API Docker via `bollard` sur le pipe exposé par le service ; flux par `Channel`.

mod docker;
mod service;

use std::sync::Arc;

use serde_json::Value;
use solon_core::ipc::{EngineSnapshot, PrereqReport, ServiceCommand, Settings};
use tauri::ipc::Channel;

fn from_value<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("réponse du service illisible : {e}"))
}

#[tauri::command]
async fn engine_status() -> Result<EngineSnapshot, String> {
    from_value(service::call(ServiceCommand::Status).await?)
}

#[tauri::command]
async fn engine_start() -> Result<(), String> {
    service::call(ServiceCommand::Start).await.map(|_| ())
}

#[tauri::command]
async fn engine_stop(force: bool) -> Result<(), String> {
    service::call(ServiceCommand::Stop { force }).await.map(|_| ())
}

#[tauri::command]
async fn engine_restart() -> Result<(), String> {
    service::call(ServiceCommand::Restart).await.map(|_| ())
}

#[tauri::command]
async fn engine_subscribe(channel: Channel<Value>) -> Result<(), String> {
    tokio::spawn(service::subscribe(channel));
    Ok(())
}

#[tauri::command]
async fn prereq_report() -> Result<PrereqReport, String> {
    from_value(service::call(ServiceCommand::Prerequisites).await?)
}

#[tauri::command]
async fn settings_get() -> Result<Settings, String> {
    from_value(service::call(ServiceCommand::GetSettings).await?)
}

#[tauri::command]
async fn settings_set(settings: Settings) -> Result<(), String> {
    service::call(ServiceCommand::SetSettings(settings)).await.map(|_| ())
}

/// Commande shell dans la machine (Compose, diagnostic).
#[tauri::command]
async fn service_exec(command: String, timeout_s: Option<u64>) -> Result<Value, String> {
    service::call(ServiceCommand::Exec { command, timeout_s }).await
}

#[tauri::command]
fn paths_logs_dir() -> String {
    let base = std::env::var_os("ProgramData").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"));
    base.join("Solon").join("logs").to_string_lossy().into_owned()
}

pub fn run() {
    tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(docker::DockerState::default()))
        .invoke_handler(tauri::generate_handler![
            engine_status,
            engine_start,
            engine_stop,
            engine_restart,
            engine_subscribe,
            prereq_report,
            settings_get,
            settings_set,
            service_exec,
            paths_logs_dir,
            docker::containers_list,
            docker::container_inspect,
            docker::container_start,
            docker::container_stop,
            docker::container_restart,
            docker::container_kill,
            docker::container_remove,
            docker::logs_open,
            docker::stream_close,
            docker::stats_open,
            docker::exec_open,
            docker::exec_input,
            docker::exec_resize,
            docker::exec_close,
            docker::docker_events_open,
            docker::images_list,
            docker::image_inspect,
            docker::image_remove,
            docker::image_run,
            docker::volumes_list,
            docker::volume_create,
            docker::volume_remove,
            docker::volume_inspect,
            docker::networks_list,
            docker::network_create,
            docker::network_remove,
            docker::network_inspect,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Solon");
}
