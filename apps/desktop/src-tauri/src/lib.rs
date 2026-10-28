//! Application de bureau Solon (Tauri v2).
//!
//! - [`service`] : canal de contrôle du service Windows (état du moteur, démarrage/arrêt, réglages) ;
//! - [`docker`] : API Docker via `bollard` sur le pipe exposé par le service ; flux par `Channel`.

mod backup;
mod compose;
mod diagnostic;
mod docker;
mod files;
mod git;
mod host;
mod import;
mod service;
mod shell;
mod stacks;
mod tray;
mod update;

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

/// Radiographie : historique puis flux des échanges observés dans la machine.
#[tauri::command]
async fn trace_subscribe(channel: Channel<Value>) -> Result<(), String> {
    tokio::spawn(service::trace_subscribe(channel));
    Ok(())
}

#[tauri::command]
async fn trace_clear(project: Option<String>) -> Result<(), String> {
    service::call(ServiceCommand::TraceClear { project })
        .await
        .map(|_| ())
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

/// Projets Docker présents sur le PC (recherche faite par le service, droits administrateur).
#[tauri::command]
async fn projects_scan() -> Result<Value, String> {
    service::call(ServiceCommand::ScanProjects).await
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

/// Vrai quand Solon tourne depuis un paquet MSIX (Microsoft Store ou paquet autonome).
/// Deux choses en dépendent : les mises à jour, qui viennent alors du Store et non de GitHub, et
/// l'activation des composants Windows, que l'installeur classique fait et que le paquet ne peut pas.
#[tauri::command]
fn app_is_packaged() -> bool {
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len: u32 = 0;
    // Sans tampon, l'appel renvoie ERROR_INSUFFICIENT_BUFFER si le processus a une identité de
    // paquet, et APPMODEL_ERROR_NO_PACKAGE sinon.
    let err = unsafe { GetCurrentPackageFullName(&mut len, None) };
    const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
    err.0 != APPMODEL_ERROR_NO_PACKAGE
}

/// Active les composants Windows requis (Plateforme de machine virtuelle, Hyper-V si l'édition
/// l'a) en lançant `installer\setup-features.ps1` élevé : Windows demande l'autorisation.
/// Nécessaire pour le paquet du Microsoft Store, qui s'installe sans élévation et ne peut donc pas
/// toucher aux composants ; l'installeur classique, lui, l'a déjà fait.
#[tauri::command]
fn prereq_enable_features() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = exe
        .parent()
        .ok_or("dossier de l'application introuvable")?
        .join("installer")
        .join("setup-features.ps1");
    if !script.is_file() {
        return Err(format!("script introuvable : {}", script.display()));
    }
    update::launch_elevated(
        std::path::Path::new("powershell.exe"),
        &format!(
            "-NoProfile -ExecutionPolicy Bypass -File \"{}\"",
            script.display()
        ),
    )
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
        // Ctrl+Alt+S depuis n'importe où : la fenêtre revient et la recherche (Ctrl+K) s'ouvre.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["ctrl+alt+s"])
                .expect("raccourci global")
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        tray::show_main(app);
                        let _ = tauri::Emitter::emit(app, "solon://palette", ());
                    }
                })
                .build(),
        )
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
            update::cleanup_later();
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
            projects_scan,
            system_accent_color,
            compose::compose_detect,
            compose::compose_read,
            compose::compose_write,
            compose::env_read,
            compose::env_write,
            compose::compose_run,
            compose::compose_check,
            compose::compose_stream,
            engine_status,
            engine_start,
            engine_stop,
            engine_restart,
            engine_subscribe,
            trace_subscribe,
            trace_clear,
            prereq_report,
            prereq_enable_features,
            app_is_packaged,
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
            update::update_download,
            update::update_install,
            host::ports_probe,
            backup::project_backup,
            backup::project_backup_info,
            backup::project_restore,
            import::import_probe,
            import::import_run,
            git::git_info,
            git::volumes_clone,
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
