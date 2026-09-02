//! Icône de la barre des tâches : état du moteur, nombre de conteneurs en marche, démarrer/arrêter,
//! ouvrir la fenêtre, quitter. Fermer la fenêtre principale la cache (l'application reste dans la
//! barre des tâches) ; « Quitter » ferme vraiment. Les libellés suivent la langue de l'interface
//! (mêmes fichiers `locales/*.json` que le frontend, section `tray`).

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use solon_core::ipc::ServiceCommand;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::docker::DockerState;
use crate::service;

const LOCALE_EN: &str = include_str!("../../src/locales/en.json");
const LOCALE_FR: &str = include_str!("../../src/locales/fr.json");

/// Langue courante de la barre des tâches, changée par le frontend (`set_language`).
pub struct TrayLanguage(pub tokio::sync::watch::Sender<String>);

/// Libellés de la section `tray` du fichier de langue demandé (anglais en repli).
fn labels(lang: &str) -> Value {
    let text = if lang.starts_with("fr") {
        LOCALE_FR
    } else {
        LOCALE_EN
    };
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("tray").cloned())
        .unwrap_or(Value::Null)
}

fn label(l: &Value, key: &str) -> String {
    l.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(key)
        .to_owned()
}

fn engine_label(l: &Value, state: &str) -> String {
    if state == "service_unavailable" {
        return label(l, "service_unavailable");
    }
    let known = [
        "ready", "starting", "stopping", "degraded", "failed", "stopped",
    ];
    let key = if known.contains(&state) {
        format!("state_{state}")
    } else {
        "state_unknown".to_owned()
    };
    label(l, "engine").replace("{{state}}", &label(l, &key))
}

fn containers_label(l: &Value, count: u64) -> String {
    match count {
        0 => label(l, "containers_zero"),
        1 => label(l, "containers_one"),
        n => label(l, "containers_other").replace("{{count}}", &n.to_string()),
    }
}

pub struct TrayItems<R: Runtime> {
    pub status: MenuItem<R>,
    pub containers: MenuItem<R>,
    pub open: MenuItem<R>,
    pub start: MenuItem<R>,
    pub stop: MenuItem<R>,
    pub quit: MenuItem<R>,
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let (lang_tx, lang_rx) = tokio::sync::watch::channel(String::from("en"));
    app.manage(TrayLanguage(lang_tx));
    let l = labels("en");
    let status = MenuItem::with_id(
        app,
        "status",
        engine_label(&l, "unknown"),
        false,
        None::<&str>,
    )?;
    let containers = MenuItem::with_id(app, "containers", "", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", label(&l, "open"), true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", label(&l, "start"), true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", label(&l, "stop"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", label(&l, "quit"), true, None::<&str>)?;
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
        open,
        start,
        stop,
        quit,
    });
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        follow_state(app_handle, tray, items, lang_rx).await;
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

/// Suit l'état du service (reconnexion automatique) et la langue, et met à jour le menu.
async fn follow_state<R: Runtime>(
    app: AppHandle<R>,
    tray: tauri::tray::TrayIcon<R>,
    items: Arc<TrayItems<R>>,
    mut lang_rx: tokio::sync::watch::Receiver<String>,
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

    let mut l = labels(&lang_rx.borrow().clone());
    let mut state = String::from("unknown");
    let mut count: Option<u64> = None;
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
                if state != "ready" {
                    count = None;
                }
            }
            Ok(()) = lang_rx.changed() => {
                l = labels(&lang_rx.borrow().clone());
                let _ = items.open.set_text(label(&l, "open"));
                let _ = items.start.set_text(label(&l, "start"));
                let _ = items.stop.set_text(label(&l, "stop"));
                let _ = items.quit.set_text(label(&l, "quit"));
            }
            _ = ticker.tick() => {
                if state == "ready" {
                    count = docker.running_count().await.map(|c| c as u64);
                }
            }
        }
        let status = engine_label(&l, &state);
        let _ = items.status.set_text(&status);
        let _ = items
            .start
            .set_enabled(matches!(state.as_str(), "stopped" | "failed"));
        let _ = items
            .stop
            .set_enabled(matches!(state.as_str(), "ready" | "degraded"));
        let _ = tray.set_tooltip(Some(format!("Solon — {status}")));
        let _ = items
            .containers
            .set_text(count.map(|c| containers_label(&l, c)).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libelles_dans_les_deux_langues() {
        for lang in ["en", "fr"] {
            let l = labels(lang);
            assert!(!engine_label(&l, "ready").contains("{{"));
            assert!(!engine_label(&l, "bizarre").contains("state_"));
            assert!(containers_label(&l, 3).contains('3'));
            assert_ne!(label(&l, "quit"), "quit");
        }
    }
}
