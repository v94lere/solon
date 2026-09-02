//! Point d'entrée : service Windows, mode console, installation, et client de contrôle.
//!
//! ```text
//! solon-service run                 lancé par le Gestionnaire de services (ne pas appeler à la main)
//! solon-service console [--start]   premier plan, journaux sur la sortie d'erreur (Administrateur)
//! solon-service install | uninstall crée / supprime le service Windows « SolonService »
//! solon-service status | start | stop [--force] | restart | prereq | watch | version
//!                                   client du canal de contrôle (aucun droit particulier)
//! ```
//! Variables : `SOLON_ROOT` (défaut `%ProgramData%\Solon`), `SOLON_IMAGE_DIR` (défaut `<root>\image`).

#![cfg(windows)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use solon_core::ipc::{CONTROL_PIPE, IpcRequest, ServiceCommand};
use solon_core::protocol::Response;
use solon_service::engine::{Engine, EngineConfig};
use solon_service::paths::Paths;
use solon_service::settings;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const SERVICE_NAME: &str = "SolonService";
const SERVICE_DISPLAY: &str = "Solon";
const SERVICE_DESCRIPTION: &str =
    "Moteur de conteneurs Solon : possède la machine et expose l'API Docker.";

fn engine_config() -> EngineConfig {
    let root = std::env::var_os("SOLON_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(Paths::default_root);
    let paths = Paths::new(root);
    let image_dir = std::env::var_os("SOLON_IMAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.image_dir());
    let settings = settings::load_settings(&paths.settings_file());
    EngineConfig {
        paths,
        image_dir,
        settings,
    }
}

fn init_logging(
    paths: &Paths,
    console: bool,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let _ = paths.ensure_dirs();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,guest=info".parse().unwrap());
    let file_appender = tracing_appender::rolling::daily(paths.logs_dir(), "solon-service.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);
    let registry = tracing_subscriber::registry().with(filter).with(file_layer);
    if console {
        registry
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init();
    } else {
        registry.init();
    }
    Some(guard)
}

/// Cœur commun aux modes console et service : canal de contrôle + moteur.
async fn run_core(
    engine: Engine,
    cfg: &EngineConfig,
    autostart: bool,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    allow_quit: bool,
) {
    let (quit_tx, mut quit_rx) = tokio::sync::mpsc::channel::<()>(1);
    let ipc = tokio::spawn(solon_service::ipc::serve(
        engine.clone(),
        cfg.paths.settings_file(),
        if allow_quit { Some(quit_tx) } else { None },
    ));
    if autostart {
        let e = engine.clone();
        tokio::spawn(async move {
            let _ = e.start().await;
        });
    }
    tokio::select! {
        _ = &mut shutdown => {}
        _ = quit_rx.recv() => { tracing::info!("quit reçu sur le canal de contrôle"); }
    }
    tracing::info!("arrêt demandé");
    ipc.abort();
    let _ = tokio::time::timeout(Duration::from_secs(60), engine.stop(false)).await;
}

fn run_console(autostart: bool) {
    let cfg = engine_config();
    let _guard = init_logging(&cfg.paths, true);
    tracing::info!(root = %cfg.paths.root.display(), image = %cfg.image_dir.display(), "mode console");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        let engine = Engine::new(cfg.clone());
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = tx.send(());
        });
        run_core(engine, &cfg, autostart, rx, true).await;
    });
}

// ---- mode service Windows ----

windows_service::define_windows_service!(ffi_service_main, service_main);

fn service_main(_args: Vec<OsString>) {
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    let cfg = engine_config();
    let _guard = init_logging(&cfg.paths, false);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let tx = std::sync::Mutex::new(Some(tx));
    let handler = move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown | ServiceControl::Preshutdown => {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let status_handle = match service_control_handler::register(SERVICE_NAME, handler) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("enregistrement du gestionnaire de contrôle : {e}");
            return;
        }
    };
    let status = |state: ServiceState, accept: ServiceControlAccept| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(60),
        process_id: None,
    };
    let _ = status_handle.set_service_status(status(
        ServiceState::Running,
        ServiceControlAccept::STOP
            | ServiceControlAccept::SHUTDOWN
            | ServiceControlAccept::PRESHUTDOWN,
    ));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "service démarré");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        let engine = Engine::new(cfg.clone());
        run_core(engine, &cfg, cfg.settings.autostart, rx, false).await;
    });
    let _ = status_handle
        .set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty()));
}

fn install() -> Result<(), String> {
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    let manager =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)
            .map_err(|e| format!("gestionnaire de services : {e}"))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments: vec![OsString::from("run")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let service = manager
        .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
        .map_err(|e| format!("création : {e}"))?;
    service
        .set_description(SERVICE_DESCRIPTION)
        .map_err(|e| e.to_string())?;
    println!(
        "service {SERVICE_NAME} installé (démarrage automatique). Démarrage : sc start {SERVICE_NAME}"
    );
    Ok(())
}

fn uninstall() -> Result<(), String> {
    use windows_service::service::{ServiceAccess, ServiceState};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| e.to_string())?;
    let service = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .map_err(|e| format!("ouverture : {e}"))?;
    if service
        .query_status()
        .map_err(|e| e.to_string())?
        .current_state
        != ServiceState::Stopped
    {
        let _ = service.stop();
        for _ in 0..60 {
            std::thread::sleep(Duration::from_secs(1));
            if service
                .query_status()
                .map(|s| s.current_state == ServiceState::Stopped)
                .unwrap_or(true)
            {
                break;
            }
        }
    }
    service.delete().map_err(|e| format!("suppression : {e}"))?;
    println!("service {SERVICE_NAME} supprimé");
    Ok(())
}

// ---- client de contrôle ----

async fn control(command: ServiceCommand, watch: bool) -> Result<(), String> {
    use tokio::net::windows::named_pipe::ClientOptions;
    // ERROR_PIPE_BUSY (231) : toutes les instances sont prises un court instant ; on réessaie.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let pipe = loop {
        match ClientOptions::new().open(CONTROL_PIPE) {
            Ok(p) => break p,
            Err(e) if e.raw_os_error() == Some(231) && std::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            Err(e) => {
                return Err(format!(
                    "service injoignable sur {CONTROL_PIPE} : {e} (le service tourne-t-il ?)"
                ));
            }
        }
    };
    let (read, mut write) = tokio::io::split(pipe);
    let mut reader = BufReader::new(read);
    let mut line = serde_json::to_string(&IpcRequest {
        id: 1,
        request: command,
    })
    .unwrap();
    line.push('\n');
    write
        .write_all(line.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    reader
        .read_line(&mut reply)
        .await
        .map_err(|e| e.to_string())?;
    let response: Response =
        serde_json::from_str(reply.trim()).map_err(|e| format!("{e} : {reply}"))?;
    if !response.ok {
        return Err(response.error.unwrap_or_else(|| "erreur".into()));
    }
    if let Some(r) = &response.result {
        if !r.is_null() {
            println!("{}", serde_json::to_string_pretty(r).unwrap());
        }
    }
    if watch {
        let mut sub = serde_json::to_string(&IpcRequest {
            id: 2,
            request: ServiceCommand::Subscribe,
        })
        .unwrap();
        sub.push('\n');
        write
            .write_all(sub.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        loop {
            reply.clear();
            if reader
                .read_line(&mut reply)
                .await
                .map_err(|e| e.to_string())?
                == 0
            {
                break;
            }
            let v: serde_json::Value = serde_json::from_str(reply.trim()).unwrap_or_default();
            if v.get("id").is_some() {
                continue; // réponse au Subscribe
            }
            println!("{}", reply.trim());
            if v.get("event") == Some(&serde_json::Value::String("state".into())) {
                let state = v.get("state").and_then(|s| s.as_str()).unwrap_or("");
                if matches!(state, "ready" | "failed" | "stopped")
                    && !std::env::args().any(|a| a == "--follow")
                {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn usage() -> ! {
    eprintln!(
        "usage : solon-service <run|console [--start]|install|uninstall|status|start|stop [--force]|restart|prereq|watch [--follow]|version>"
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let result: Result<(), String> = match cmd {
        "run" => windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .map_err(|e| format!("dispatcher : {e}")),
        "console" => {
            run_console(args.iter().any(|a| a == "--start"));
            Ok(())
        }
        "install" => install(),
        "uninstall" => uninstall(),
        "version" => {
            println!("solon-service {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "exec" => {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            let command = args[1..].join(" ");
            rt.block_on(control(
                ServiceCommand::Exec {
                    command,
                    timeout_s: Some(120),
                },
                false,
            ))
        }
        "status" | "start" | "stop" | "restart" | "prereq" | "watch" | "quit" => {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            let (command, watch) = match cmd {
                "status" => (ServiceCommand::Status, false),
                "start" => (ServiceCommand::Start, true),
                "stop" => (
                    ServiceCommand::Stop {
                        force: args.iter().any(|a| a == "--force"),
                    },
                    false,
                ),
                "restart" => (ServiceCommand::Restart, true),
                "prereq" => (ServiceCommand::Prerequisites, false),
                "quit" => (ServiceCommand::Quit, false),
                _ => (ServiceCommand::Status, true),
            };
            rt.block_on(control(command, watch))
        }
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}
