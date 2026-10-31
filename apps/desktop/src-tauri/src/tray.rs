//! L'icône de la barre des tâches et son menu. Un clic (gauche ou droit) ouvre un menu natif : l'état
//! du moteur, un sous-menu par projet (ouvrir son adresse, démarrer, redémarrer, arrêter), les
//! conteneurs isolés, un terminal avec `solon` prêt, « pourquoi ça casse ? », la mise à jour, le
//! démarrage avec Windows, démarrer/arrêter le moteur, quitter.
//!
//! Le menu est reconstruit entièrement à chaque changement (état du moteur, liste des conteneurs) :
//! c'est simple et les menus natifs sont peu coûteux. Les icônes PNG 32×32 viennent de `icons/tray/`.
//! Les notifications Windows signalent un moteur en échec, un redémarrage de Docker, un disque
//! presque plein et les projets relancés.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use solon_client::containers::{self, Container};
use solon_core::ipc::ServiceCommand;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IconMenuItem, Menu, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::lang::tr;
use crate::service;

/// Au-delà, le menu indique « … et N autres ».
const MAX_LISTED: usize = 12;
/// Valeur du Registre qui lance l'icône à l'ouverture de session.
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Solon";

macro_rules! png {
    ($name:literal) => {
        include_bytes!(concat!("../icons/tray/", $name, ".png"))
    };
}

fn icon(bytes: &'static [u8]) -> Option<Image<'static>> {
    Image::from_bytes(bytes).ok()
}

fn engine_label(state: &str) -> String {
    let s = match state {
        "ready" => tr("running", "en marche"),
        "starting" => tr("starting…", "démarrage…"),
        "stopping" => tr("stopping…", "arrêt…"),
        "degraded" => tr("restarting", "redémarrage"),
        "failed" => tr("failed", "en échec"),
        "stopped" => tr("stopped", "arrêté"),
        "service_unavailable" => {
            return tr(
                "Solon service not running",
                "Le service Solon n'est pas en cours d'exécution",
            )
            .to_owned();
        }
        _ => tr("unknown", "inconnu"),
    };
    format!("{} {s}", tr("Engine:", "Moteur :"))
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

/// Icône de la barre des tâches : le cube avec un point coloré selon l'état.
fn tray_icon(state: &str) -> &'static [u8] {
    match state {
        "ready" => png!("tray-ready"),
        "starting" | "stopping" | "degraded" => png!("tray-busy"),
        "failed" | "service_unavailable" => png!("tray-failed"),
        _ => png!("tray-stopped"),
    }
}

fn containers_label(count: usize) -> String {
    match count {
        0 => tr("No containers running", "Aucun conteneur en marche").to_owned(),
        1 => tr("1 container running", "1 conteneur en marche").to_owned(),
        n => {
            if crate::lang::french() {
                format!("{n} conteneurs en marche")
            } else {
                format!("{n} containers running")
            }
        }
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

/// Ce que le menu montre, à part l'état : les conteneurs et si les domaines locaux répondent.
#[derive(Default, Clone, PartialEq, Eq)]
struct View {
    containers: Vec<Container>,
    local_domains: bool,
    tls: bool,
}

/// Construit le menu complet pour un état donné.
fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    state: &str,
    view: Option<&View>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    // Lignes d'information : actives (sinon Windows grise leur icône) ; elles ouvrent un terminal.
    menu.append(&item(
        app,
        "terminal",
        engine_label(state),
        true,
        engine_dot(state),
    )?)?;
    if state == "ready"
        && let Some(v) = view
    {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let running = v.containers.iter().filter(|c| c.running()).count();
        menu.append(&item(
            app,
            "terminal",
            containers_label(running),
            true,
            png!("cube"),
        )?)?;
        // Projets Compose d'abord : un sous-menu par projet ; les conteneurs isolés suivent.
        let mut projects: Vec<(String, usize, usize, Option<String>)> = Vec::new();
        for c in v.containers.iter().filter(|c| c.project.is_some()) {
            let p = c.project.clone().unwrap_or_default();
            match projects.iter_mut().find(|(name, _, _, _)| *name == p) {
                Some(e) => {
                    e.1 += 1;
                    e.2 += usize::from(c.running());
                    if e.3.is_none() && c.running() {
                        e.3 = Some(c.domains()[0].clone());
                    }
                }
                None => projects.push((
                    p,
                    1,
                    usize::from(c.running()),
                    c.running().then(|| c.domains()[0].clone()),
                )),
            }
        }
        let scheme = if v.tls { "https" } else { "http" };
        for (name, total, running, domain) in &projects {
            let title = format!(
                "{} {name}  ({running}/{total})",
                if *running > 0 { "●" } else { "○" }
            );
            let sub = Submenu::with_id(app, format!("p|{name}"), title, true)?;
            if let Some(d) = domain {
                sub.append(&item(
                    app,
                    format!("url|{scheme}://{d}"),
                    format!("{} {scheme}://{d}", tr("Open", "Ouvrir")),
                    v.local_domains,
                    png!("open"),
                )?)?;
                sub.append(&PredefinedMenuItem::separator(app)?)?;
            }
            sub.append(&item(
                app,
                format!("p|start|{name}"),
                tr("Start project", "Démarrer le projet"),
                *running < *total,
                png!("play"),
            )?)?;
            sub.append(&item(
                app,
                format!("p|restart|{name}"),
                tr("Restart project", "Redémarrer le projet"),
                *running > 0,
                png!("restart"),
            )?)?;
            sub.append(&item(
                app,
                format!("p|stop|{name}"),
                tr("Stop project", "Arrêter le projet"),
                *running > 0,
                png!("stop"),
            )?)?;
            menu.append(&sub)?;
        }
        let loose: Vec<&Container> = v
            .containers
            .iter()
            .filter(|c| c.project.is_none())
            .collect();
        for c in loose.iter().take(MAX_LISTED) {
            let title = format!("{} {}", if c.running() { "●" } else { "○" }, c.name);
            let sub = Submenu::with_id(app, format!("c|{}", c.id), title, true)?;
            if c.running() {
                sub.append(&item(
                    app,
                    format!("url|{scheme}://{}", c.domains()[0]),
                    format!("{} {scheme}://{}", tr("Open", "Ouvrir"), c.domains()[0]),
                    v.local_domains,
                    png!("open"),
                )?)?;
                sub.append(&PredefinedMenuItem::separator(app)?)?;
            }
            sub.append(&item(
                app,
                format!("c|start|{}", c.id),
                tr("Start", "Démarrer"),
                !c.running(),
                png!("play"),
            )?)?;
            sub.append(&item(
                app,
                format!("c|restart|{}", c.id),
                tr("Restart", "Redémarrer"),
                c.running(),
                png!("restart"),
            )?)?;
            sub.append(&item(
                app,
                format!("c|stop|{}", c.id),
                tr("Stop", "Arrêter"),
                c.running(),
                png!("stop"),
            )?)?;
            menu.append(&sub)?;
        }
        if loose.len() > MAX_LISTED {
            let more = if crate::lang::french() {
                format!("… et {} autres (solon status)", loose.len() - MAX_LISTED)
            } else {
                format!("… and {} more (solon status)", loose.len() - MAX_LISTED)
            };
            menu.append(&item(app, "terminal", more, true, png!("open"))?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item(
        app,
        "terminal",
        tr(
            "Open a terminal (solon, docker)",
            "Ouvrir un terminal (solon, docker)",
        ),
        true,
        png!("open"),
    )?)?;
    menu.append(&item(
        app,
        "why",
        tr(
            "Why is it broken? (solon why)",
            "Pourquoi ça casse ? (solon why)",
        ),
        state == "ready",
        png!("cube"),
    )?)?;
    menu.append(&item(
        app,
        "update",
        tr("Update Solon…", "Mettre à jour Solon…"),
        true,
        png!("bolt"),
    )?)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        "autostart",
        tr("Start with Windows", "Démarrer avec Windows"),
        true,
        autostart_enabled(),
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item(
        app,
        "start",
        tr("Start engine", "Démarrer le moteur"),
        matches!(state, "stopped" | "failed"),
        png!("bolt"),
    )?)?;
    menu.append(&item(
        app,
        "stop",
        tr("Stop engine", "Arrêter le moteur"),
        matches!(state, "ready" | "degraded"),
        png!("stop"),
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item(
        app,
        "quit",
        tr("Quit Solon", "Quitter Solon"),
        true,
        png!("quit"),
    )?)?;
    Ok(menu)
}

/// `<dossier d'installation>\bin` : `solon.exe` et `docker.exe`.
fn bin_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("bin")))
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Solon\bin"))
}

/// Ouvre une console avec `solon` et `docker` dans le PATH, dans le dossier de l'utilisateur, et y
/// lance `command` (vide : juste l'invite). Windows Terminal s'en empare s'il est le terminal par
/// défaut ; sinon c'est la console classique.
fn open_terminal(command: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let bin = bin_dir();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut new_path = bin.as_os_str().to_owned();
    new_path.push(";");
    new_path.push(path);
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let line = if command.is_empty() {
        "title Solon && solon".to_owned()
    } else {
        format!("title Solon && {command}")
    };
    let shell = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let mut cmd = std::process::Command::new(shell);
    cmd.args(["/K", &line])
        .env("PATH", new_path)
        .creation_flags(CREATE_NEW_CONSOLE);
    if let Some(h) = home {
        cmd.current_dir(h);
    }
    if let Err(e) = cmd.spawn() {
        tracing::warn!("terminal : {e}");
    }
}

fn reg(args: &[&str]) -> bool {
    solon_client::hidden_command("reg.exe")
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_VALUE])
}

/// Marqueur « l'utilisateur a dit non » : on ne réinscrit pas Solon au prochain lancement.
fn opt_out_marker() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Solon").join("no-autostart"))
}

fn set_autostart(on: bool) {
    if on {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        reg(&[
            "add",
            RUN_KEY,
            "/v",
            RUN_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            &format!("\"{exe}\""),
            "/f",
        ]);
        if let Some(m) = opt_out_marker() {
            let _ = std::fs::remove_file(m);
        }
    } else {
        reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
        if let Some(m) = opt_out_marker() {
            if let Some(d) = m.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(m, b"");
        }
    }
}

/// Au premier lancement, Solon s'inscrit au démarrage de session : une icône qui n'est pas là
/// quand le moteur tombe ne sert à rien. L'entrée de menu permet de dire non, et ce non est retenu.
fn autostart_first_time<R: Runtime>(app: &AppHandle<R>) {
    if autostart_enabled() {
        return;
    }
    if opt_out_marker().is_some_and(|m| m.exists()) {
        return;
    }
    set_autostart(true);
    notify(
        app,
        "Solon",
        tr(
            "Solon will start with Windows, so its icon is there when you need it. Untick \"Start with Windows\" in the icon's menu to change that.",
            "Solon démarrera avec Windows, pour que son icône soit là quand vous en avez besoin. Décochez « Démarrer avec Windows » dans le menu de l'icône pour changer ça.",
        ),
    );
}

pub fn notify<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::debug!("notification : {e}");
    }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    autostart_first_time(app);
    let menu = build_menu(app, "unknown", None)?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon(png!("tray-stopped")).expect("icône"))
        .tooltip("Solon")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "terminal" => open_terminal(""),
                "why" => open_terminal("solon why"),
                "update" => open_terminal("solon update"),
                "autostart" => set_autostart(!autostart_enabled()),
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
                    if let Some(url) = id.strip_prefix("url|") {
                        if let Err(e) = tauri_plugin_opener::open_url(url, None::<&str>) {
                            tracing::warn!("ouverture de {url} : {e}");
                        }
                        return;
                    }
                    let mut parts = id.splitn(3, '|');
                    let (kind, action, target) = (parts.next(), parts.next(), parts.next());
                    if let (Some(kind), Some(action), Some(target)) = (kind, action, target) {
                        let (kind, action, target) =
                            (kind.to_owned(), action.to_owned(), target.to_owned());
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let r = match kind.as_str() {
                                "p" => containers::project_act(&action, &target).await,
                                "c" => containers::act(&action, &target).await,
                                _ => Ok(()),
                            };
                            if let Err(e) = r {
                                tracing::warn!("barre des tâches : {action} {target} : {e}");
                                notify(&app, "Solon", &e);
                            }
                        });
                    }
                }
            }
        })
        .build(app)?;

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        follow_state(app_handle, tray).await;
    });
    Ok(())
}

/// Suit l'état du service (reconnexion automatique) et la liste des conteneurs ; reconstruit le
/// menu quand l'un d'eux change ; notifie ce qui mérite de l'être.
async fn follow_state<R: Runtime>(app: AppHandle<R>, tray: tauri::tray::TrayIcon<R>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let channel = tauri::ipc::Channel::new(move |msg| {
        if let tauri::ipc::InvokeResponseBody::Json(text) = msg
            && let Ok(v) = serde_json::from_str::<Value>(&text)
        {
            let _ = tx.send(v);
        }
        Ok(())
    });
    tokio::spawn(service::subscribe(channel));

    let mut state = String::from("unknown");
    let mut view: Option<View> = None;
    let mut ticker = tokio::time::interval(Duration::from_secs(3));
    loop {
        let changed = tokio::select! {
            Some(ev) = rx.recv() => {
                match ev.get("event").and_then(|e| e.as_str()) {
                    Some("state") => {
                        let new_state = ev.get("state").and_then(|s| s.as_str()).unwrap_or("unknown").to_owned();
                        let changed = new_state != state;
                        if changed {
                            match new_state.as_str() {
                                "failed" => notify(
                                    &app,
                                    tr("Solon: the engine did not start", "Solon : le moteur n'a pas démarré"),
                                    ev.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str())
                                        .unwrap_or(tr("Run `solon status` for the detail.", "Lancez `solon status` pour le détail.")),
                                ),
                                "degraded" => notify(
                                    &app,
                                    tr("Solon: engine restarting", "Solon : redémarrage du moteur"),
                                    tr("Docker stopped unexpectedly and is restarting.", "Docker s'est arrêté de façon inattendue et redémarre."),
                                ),
                                _ => {}
                            }
                        }
                        state = new_state;
                        if state != "ready" {
                            view = None;
                        } else if let Some(v) = view.as_mut() {
                            v.local_domains = ev.get("local_domains").and_then(|b| b.as_bool()).unwrap_or(false);
                            v.tls = ev.get("local_domains_tls").and_then(|b| b.as_bool()).unwrap_or(false);
                        }
                        changed
                    }
                    Some("service_unavailable") => {
                        let changed = state != "service_unavailable";
                        state = "service_unavailable".to_owned();
                        view = None;
                        changed
                    }
                    Some("disk_pressure") => {
                        let pct = ev.get("used_pct").and_then(|v| v.as_u64()).unwrap_or(0);
                        let free = ev.get("free_mb").and_then(|v| v.as_u64()).unwrap_or(0) / 1024;
                        notify(
                            &app,
                            tr("Solon: engine disk almost full", "Solon : disque du moteur presque plein"),
                            &if crate::lang::french() {
                                format!("{pct} % utilisés, {free} Go restants. Supprimez des images ou volumes inutilisés (docker system prune).")
                            } else {
                                format!("{pct} % used, {free} GB left. Remove unused images or volumes (docker system prune).")
                            },
                        );
                        false
                    }
                    Some("resumed") => {
                        let names: Vec<String> = ev.get("projects").and_then(|p| p.as_array())
                            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                            .unwrap_or_default();
                        if !names.is_empty() {
                            notify(
                                &app,
                                tr("Solon: up again", "Solon : c'est reparti"),
                                &format!("{} {}", tr("Restarted after the engine came up:", "Relancé après le démarrage du moteur :"), names.join(", ")),
                            );
                        }
                        false
                    }
                    _ => false,
                }
            }
            _ = ticker.tick() => {
                if state == "ready" {
                    let fresh = match containers::list().await {
                        Ok(list) => {
                            let snap = service::call(ServiceCommand::Status).await.unwrap_or(Value::Null);
                            Some(View {
                                containers: list,
                                local_domains: snap.get("local_domains").and_then(|b| b.as_bool()).unwrap_or(false),
                                tls: snap.get("local_domains_tls").and_then(|b| b.as_bool()).unwrap_or(false),
                            })
                        }
                        Err(_) => None,
                    };
                    let changed = fresh != view;
                    view = fresh;
                    changed
                } else {
                    false
                }
            }
        };
        if !changed {
            continue;
        }
        let _ = tray.set_tooltip(Some(format!("Solon — {}", engine_label(&state))));
        let _ = tray.set_icon(icon(tray_icon(&state)));
        match build_menu(&app, &state, view.as_ref()) {
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
    fn icones_png_decodables() {
        let all: [&[u8]; 15] = [
            png!("tray-ready"),
            png!("tray-busy"),
            png!("tray-failed"),
            png!("tray-stopped"),
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

    #[test]
    fn libelles() {
        assert!(engine_label("ready").contains(':'));
        assert!(containers_label(3).contains('3'));
    }
}
