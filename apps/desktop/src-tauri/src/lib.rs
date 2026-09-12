//! Application de bureau Solon (Tauri v2).
//!
//! - [`service`] : canal de contrôle du service Windows (état du moteur, démarrage/arrêt, réglages) ;
//! - [`docker`] : API Docker via `bollard` sur le pipe exposé par le service ; flux par `Channel`.

mod compose;
mod diagnostic;
mod docker;
mod files;
mod host;
mod service;
mod shell;
mod stacks;
mod tray;

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
    service::call(ServiceCommand::Stop { force })
        .await
        .map(|_| ())
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
    service::call(ServiceCommand::SetSettings(settings))
        .await
        .map(|_| ())
}

/// Couleur d'accent de Windows (`#rrggbb`), lue dans le registre (DWM\AccentColor, ABGR).
#[tauri::command]
fn system_accent_color() -> Option<String> {
    let out = std::process::Command::new("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\DWM",
            "/v",
            "AccentColor",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let hex = text
        .split_whitespace()
        .find(|w| w.starts_with("0x"))?
        .trim_start_matches("0x");
    let v = u32::from_str_radix(hex, 16).ok()?;
    let (r, g, b) = (v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff);
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Compteurs de la machine pour l'écran Activité.
#[tauri::command]
async fn engine_metrics() -> Result<Value, String> {
    service::call(ServiceCommand::Metrics).await
}

/// Commande shell dans la machine (Compose, diagnostic).
#[tauri::command]
async fn service_exec(command: String, timeout_s: Option<u64>) -> Result<Value, String> {
    service::call(ServiceCommand::Exec { command, timeout_s }).await
}

/// Langue de l'interface, relayée à la barre des tâches.
#[tauri::command]
fn set_language(app: tauri::AppHandle, lang: String) {
    if let Some(t) = tauri::Manager::try_state::<tray::TrayLanguage>(&app) {
        let _ = t.0.send(lang);
    }
}

/// Ouvre un dossier dans VS Code : la commande `code` si elle est dans le PATH, sinon l'URL `vscode://`.
#[tauri::command]
fn open_in_vscode(dir: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new("cmd")
        .args(["/C", "code", &dir])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if matches!(status, Ok(s) if s.success()) {
        return Ok(());
    }
    let url = format!("vscode://file/{}", dir.replace('\\', "/"));
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn paths_logs_dir() -> String {
    let base = std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"));
    base.join("Solon")
        .join("logs")
        .to_string_lossy()
        .into_owned()
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    tauri::Builder::default()
        // Une seule instance : un second lancement (raccourci, menu Démarrer, fin d'installation) montre la
        // fenêtre déjà ouverte au lieu d'empiler des processus. Doit être le premier plugin enregistré.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            // Taille et position mémorisées ; pas la visibilité (la fenêtre se ferme dans la barre
            // des tâches : la restaurer masquée laisserait l'application sans fenêtre au lancement).
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .manage(Arc::new(docker::DockerState::default()))
        .manage(Arc::new(shell::ShellState::default()))
        .setup(|app| {
            tray::setup(app.handle())?;
            Ok(())
        })
        // Fermer la fenêtre la cache ; l'application vit dans la barre des tâches.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            engine_metrics,
            system_accent_color,
            compose::compose_detect,
            compose::compose_read,
            compose::compose_write,
            compose::env_read,
            compose::env_write,
            compose::compose_run,
            compose::compose_stream,
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
            open_in_vscode,
            shell::machine_shell_open,
            shell::machine_shell_input,
            shell::machine_shell_resize,
            shell::machine_shell_close,
            diagnostic::diagnostic_export,
            host::ports_probe,
            host::host_disk_info,
            docker::docker_reclaim,
            set_language,
            docker::containers_list,
            docker::container_inspect,
            docker::container_copy_from,
            docker::container_copy_to,
            files::files_list,
            files::files_mkdir,
            files::files_delete,
            files::files_download,
            files::files_upload,
            stacks::stack_probe,
            stacks::project_scaffold,
            docker::container_start,
            docker::container_rename,
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
