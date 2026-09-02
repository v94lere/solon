//! Icône de la barre des tâches : état du moteur, nombre de conteneurs en marche, démarrer/arrêter,
//! ouvrir la fenêtre, quitter. Fermer la fenêtre principale la cache (l'application reste dans la
//! barre des tâches) ; « Quitter » ferme vraiment.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use solon_core::ipc::ServiceCommand;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::docker::DockerState;
use crate::service;

pub struct TrayItems<R: Runtime> {
    pub status: MenuItem<R>,
    pub containers: MenuItem<R>,
    pub start: MenuItem<R>,
    pub stop: MenuItem<R>,
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Engine: unknown", false, None::<&str>)?;
    let containers = MenuItem::with_id(app, "containers", "", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Solon", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", "Start engine", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop engine", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Solon", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &containers,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &start,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icône"))
        .tooltip("Solon")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "start" => {
                tauri::async_runtime::spawn(async {
                    let _ = service::call(ServiceCommand::Start).await;
                });
            }
            "stop" => {
                tauri::async_runtime::spawn(async {
                    let _ = service::call(ServiceCommand::Stop { force: false }).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    let items = Arc::new(TrayItems {
        status,
        containers,
        start,
        stop,
    });
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        follow_state(app_handle, tray, items).await;
    });
    Ok(())
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Suit l'état du service (reconnexion automatique) et met à jour le menu.
async fn follow_state<R: Runtime>(
    app: AppHandle<R>,
    tray: tauri::tray::TrayIcon<R>,
    items: Arc<TrayItems<R>>,
) {
    let docker: Arc<DockerState> = app.state::<Arc<DockerState>>().inner().clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    // Abonnement au service via un canal Tauri interne.
    let channel = tauri::ipc::Channel::new(move |msg| {
        if let tauri::ipc::InvokeResponseBody::Json(text) = msg {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                let _ = tx.send(v);
            }
        }
        Ok(())
    });
    tokio::spawn(service::subscribe(channel));

    let mut state = String::from("unknown");
    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            Some(ev) = rx.recv() => {
                match ev.get("event").and_then(|e| e.as_str()) {
                    Some("state") => {
                        state = ev.get("state").and_then(|s| s.as_str()).unwrap_or("unknown").to_owned();
                    }
                    Some("service_unavailable") => state = "service_unavailable".into(),
                    _ => continue,
                }
                let label = match state.as_str() {
                    "ready" => "Engine: running",
                    "starting" => "Engine: starting…",
                    "stopping" => "Engine: stopping…",
                    "degraded" => "Engine: restarting",
                    "failed" => "Engine: failed",
                    "stopped" => "Engine: stopped",
                    "service_unavailable" => "Solon service not running",
                    _ => "Engine: unknown",
                };
                let _ = items.status.set_text(label);
                let _ = items.start.set_enabled(matches!(state.as_str(), "stopped" | "failed"));
                let _ = items.stop.set_enabled(matches!(state.as_str(), "ready" | "degraded"));
                let _ = tray.set_tooltip(Some(format!("Solon — {label}")));
                if state != "ready" {
                    let _ = items.containers.set_text("");
                }
            }
            _ = ticker.tick() => {
                if state == "ready" {
                    let count = docker.running_count().await;
                    let text = match count {
                        Some(0) => "No containers running".to_owned(),
                        Some(1) => "1 container running".to_owned(),
                        Some(n) => format!("{n} containers running"),
                        None => String::new(),
                    };
                    let _ = items.containers.set_text(text);
                }
            }
        }
    }
}
