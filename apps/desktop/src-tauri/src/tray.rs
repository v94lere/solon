//! Icône de la barre des tâches. Un clic (gauche ou droit) ouvre un menu natif avec icônes : état du
//! moteur, la liste des conteneurs avec Démarrer / Redémarrer / Arrêter pour chacun, ouvrir la fenêtre,
//! démarrer/arrêter le moteur, quitter. Fermer la fenêtre principale la cache (l'application reste
//! dans la barre des tâches) ; « Quitter » ferme vraiment. Les libellés suivent la langue de
//! l'interface (mêmes fichiers `locales/*.json` que le frontend, section `tray`).
//!
//! Le menu est reconstruit entièrement à chaque changement (état du moteur, liste des conteneurs,
//! langue) : c'est simple et les menus natifs sont peu coûteux. Les icônes PNG 32×32 viennent de
//! `icons/tray/` (générées par `make.py`).

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use solon_core::ipc::ServiceCommand;
use tauri::image::Image;
use tauri::menu::{IconMenuItem, Menu, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::docker::{DockerState, TrayContainer};
use crate::service;

const LOCALE_EN: &str = include_str!("../../src/locales/en.json");
const LOCALE_FR: &str = include_str!("../../src/locales/fr.json");
/// Au-delà, le menu indique « … et N autres » et renvoie vers la fenêtre.
const MAX_LISTED: usize = 12;

macro_rules! png {
    ($name:literal) => {
        include_bytes!(concat!("../icons/tray/", $name, ".png"))
    };
}

fn icon(bytes: &'static [u8]) -> Option<Image<'static>> {
    Image::from_bytes(bytes).ok()
}

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

/// Point de couleur selon l'état du moteur.
fn engine_dot(state: &str) -> &'static [u8] {
    match state {
        "ready" => png!("dot-green"),
        "starting" | "stopping" | "degraded" => png!("dot-orange"),
        "failed" | "service_unavailable" => png!("dot-red"),
        _ => png!("dot-grey"),
    }
}

fn containers_label(l: &Value, count: u64) -> String {
    match count {
        0 => label(l, "containers_zero"),
        1 => label(l, "containers_one"),
        n => label(l, "containers_other").replace("{{count}}", &n.to_string()),
    }
}

fn item<R: Runtime>(
    app: &AppHandle<R>,
    id: impl Into<tauri::menu::MenuId>,
    text: impl AsRef<str>,
    enabled: bool,
    png: &'static [u8],
) -> tauri::Result<IconMenuItem<R>> {
    IconMenuItem::with_id(app, id, text, enabled, icon(png), None::<&str>)
}

/// Construit le menu complet pour un état donné.
fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    l: &Value,
    state: &str,
    containers: Option<&[TrayContainer]>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    menu.append(&item(
        app,
        "status",
        engine_label(l, state),
        false,
        engine_dot(state),
    )?)?;
    if state == "ready" {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        match containers {
            Some(list) if !list.is_empty() => {
                let running = list.iter().filter(|c| c.running).count() as u64;
                menu.append(&item(
                    app,
                    "count",
                    containers_label(l, running),
                    false,
                    png!("cube"),
                )?)?;
                for c in list.iter().take(MAX_LISTED) {
                    // Un sous-menu ne porte pas d'icône : le point d'état est dans le texte.
                    let title = format!("{} {}", if c.running { "●" } else { "○" }, c.name);
                    let sub = Submenu::with_id(app, format!("c|{}", c.id), title, true)?;
                    sub.append(&item(
                        app,
                        format!("c|start|{}", c.id),
                        label(l, "container_start"),
                        !c.running,
                        png!("play"),
                    )?)?;
                    sub.append(&item(
                        app,
                        format!("c|restart|{}", c.id),
                        label(l, "container_restart"),
                        c.running,
                        png!("restart"),
                    )?)?;
                    sub.append(&item(
                        app,
                        format!("c|stop|{}", c.id),
                        label(l, "container_stop"),
                        c.running,
                        png!("stop"),
                    )?)?;
                    menu.append(&sub)?;
                }
                if list.len() > MAX_LISTED {
                    let more = label(l, "containers_more")
                        .replace("{{count}}", &(list.len() - MAX_LISTED).to_string());
                    menu.append(&item(app, "open", more, true, png!("open"))?)?;
                }
            }
            Some(_) => {
                menu.append(&item(
                    app,
                    "count",
                    containers_label(l, 0),
                    false,
                    png!("cube"),
                )?)?;
            }
            None => {}
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item(app, "open", label(l, "open"), true, png!("open"))?)?;
    menu.append(&item(
        app,
        "start",
        label(l, "start"),
        matches!(state, "stopped" | "failed"),
        png!("bolt"),
    )?)?;
    menu.append(&item(
        app,
        "stop",
        label(l, "stop"),
        matches!(state, "ready" | "degraded"),
        png!("stop"),
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item(app, "quit", label(l, "quit"), true, png!("quit"))?)?;
    Ok(menu)
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let (lang_tx, lang_rx) = tokio::sync::watch::channel(String::from("en"));
    app.manage(TrayLanguage(lang_tx));
    let menu = build_menu(app, &labels("en"), "unknown", None)?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icône"))
        .tooltip("Solon")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
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
                _ => {
                    // `c|<action>|<id>` : action sur un conteneur.
                    let mut parts = id.splitn(3, '|');
                    if let (Some("c"), Some(action), Some(cid)) =
                        (parts.next(), parts.next(), parts.next())
                    {
                        let docker: Arc<DockerState> =
                            app.state::<Arc<DockerState>>().inner().clone();
                        let (action, cid) = (action.to_owned(), cid.to_owned());
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = docker.tray_action(&action, &cid).await {
                                tracing::warn!("barre des tâches : {action} {cid} : {e}");
                            }
                        });
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        follow_state(app_handle, tray, lang_rx).await;
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

/// Suit l'état du service (reconnexion automatique), la liste des conteneurs et la langue ;
/// reconstruit le menu quand l'un d'eux change.
async fn follow_state<R: Runtime>(
    app: AppHandle<R>,
    tray: tauri::tray::TrayIcon<R>,
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
    let mut containers: Option<Vec<TrayContainer>> = None;
    let mut ticker = tokio::time::interval(Duration::from_secs(3));
    loop {
        let changed = tokio::select! {
            Some(ev) = rx.recv() => {
                let new_state = match ev.get("event").and_then(|e| e.as_str()) {
                    Some("state") => ev.get("state").and_then(|s| s.as_str()).unwrap_or("unknown").to_owned(),
                    Some("service_unavailable") => "service_unavailable".to_owned(),
                    _ => continue,
                };
                let changed = new_state != state;
                state = new_state;
                if state != "ready" {
                    containers = None;
                }
                changed
            }
            Ok(()) = lang_rx.changed() => {
                l = labels(&lang_rx.borrow().clone());
                true
            }
            _ = ticker.tick() => {
                if state == "ready" {
                    let fresh = docker.tray_containers().await;
                    let changed = fresh != containers;
                    containers = fresh;
                    changed
                } else {
                    false
                }
            }
        };
        if !changed {
            continue;
        }
        let status = engine_label(&l, &state);
        let _ = tray.set_tooltip(Some(format!("Solon — {status}")));
        match build_menu(&app, &l, &state, containers.as_deref()) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(e) => tracing::warn!("menu de la barre des tâches : {e}"),
        }
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
            for key in [
                "quit",
                "container_start",
                "container_restart",
                "container_stop",
                "containers_more",
            ] {
                assert_ne!(label(&l, key), key, "clé tray.{key} absente en {lang}");
            }
        }
    }

    #[test]
    fn icones_png_decodables() {
        let all: [&[u8]; 11] = [
            png!("dot-green"),
            png!("dot-grey"),
            png!("dot-orange"),
            png!("dot-red"),
            png!("play"),
            png!("stop"),
            png!("restart"),
            png!("open"),
            png!("quit"),
            png!("cube"),
            png!("bolt"),
        ];
        for bytes in all {
            assert!(icon(bytes).is_some());
        }
    }
}
