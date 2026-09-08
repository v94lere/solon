//! Canal de contrôle application ↔ service sur `\\.\pipe\monodon-control` (JSON par lignes).

use std::sync::Arc;

use monodon_core::ipc::{CONTROL_PIPE, IpcRequest, ServiceCommand, ServiceEvent};
use monodon_core::protocol::Response;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::NamedPipeServer;
use tokio::sync::Mutex;

use crate::engine::Engine;
use crate::settings;

/// SYSTEM et Administrateurs en contrôle total, utilisateurs authentifiés en lecture/écriture.
pub const CONTROL_PIPE_SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

/// `quit` : canal vers la boucle principale pour arrêter le processus (None = interdit).
pub type QuitSender = Option<tokio::sync::mpsc::Sender<()>>;

pub async fn serve(
    engine: Engine,
    settings_path: std::path::PathBuf,
    quit: QuitSender,
) -> std::io::Result<()> {
    let mut server =
        monodon_hvsock::relay::create_server(CONTROL_PIPE, true, Some(CONTROL_PIPE_SDDL))?;
    tracing::info!(pipe = CONTROL_PIPE, "canal de contrôle à l'écoute");
    loop {
        server.connect().await?;
        let connected = server;
        server =
            monodon_hvsock::relay::create_server(CONTROL_PIPE, false, Some(CONTROL_PIPE_SDDL))?;
        let engine = engine.clone();
        let settings_path = settings_path.clone();
        let quit = quit.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(connected, engine, settings_path, quit).await {
                tracing::debug!("connexion de contrôle terminée : {e}");
            }
        });
    }
}

async fn handle(
    pipe: NamedPipeServer,
    engine: Engine,
    settings_path: std::path::PathBuf,
    quit: QuitSender,
) -> std::io::Result<()> {
    let (read, write) = tokio::io::split(pipe);
    let writer = Arc::new(Mutex::new(write));
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    let mut subscription: Option<tokio::task::JoinHandle<()>> = None;
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let request: IpcRequest = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(e) => {
                send(
                    &writer,
                    &Response::err(0, format!("requête illisible : {e}")),
                )
                .await?;
                continue;
            }
        };
        let id = request.id;
        let response = match request.request {
            ServiceCommand::Version => Response::ok(
                id,
                serde_json::json!({ "service": env!("CARGO_PKG_VERSION") }),
            ),
            ServiceCommand::Exec { command, timeout_s } => {
                match engine.exec(command, timeout_s.unwrap_or(120)).await {
                    Ok(r) => Response::ok(id, r),
                    Err(e) => Response::err(id, e.to_string()),
                }
            }
            ServiceCommand::EnsureShare { host_path } => {
                match engine.ensure_share(&host_path).await {
                    Ok(info) => Response::ok(id, info),
                    Err(e) => Response::err(id, e.to_string()),
                }
            }
            ServiceCommand::ListShares => Response::ok(id, engine.list_shares().await),
            ServiceCommand::Metrics => match engine.metrics().await {
                Ok(m) => Response::ok(id, m),
                Err(e) => Response::err(id, e.to_string()),
            },
            ServiceCommand::Quit => match &quit {
                Some(q) => {
                    let _ = q.send(()).await;
                    Response::ok(id, serde_json::Value::Null)
                }
                None => Response::err(id, "quit n'est disponible qu'en mode console"),
            },
            ServiceCommand::Status => Response::ok(id, engine.snapshot()),
            ServiceCommand::Prerequisites => {
                let report = tokio::task::spawn_blocking(monodon_prereq::check)
                    .await
                    .unwrap_or_default();
                Response::ok(id, report)
            }
            ServiceCommand::Start => {
                let e = engine.clone();
                tokio::spawn(async move {
                    let _ = e.start().await;
                });
                Response::ok(id, serde_json::Value::Null)
            }
            ServiceCommand::Stop { force } => match engine.stop(force).await {
                Ok(()) => Response::ok(id, serde_json::Value::Null),
                Err(e) => Response::err(id, e.to_string()),
            },
            ServiceCommand::Restart => {
                let e = engine.clone();
                tokio::spawn(async move {
                    let _ = e.restart().await;
                });
                Response::ok(id, serde_json::Value::Null)
            }
            ServiceCommand::GetSettings => {
                Response::ok(id, settings::load_settings(&settings_path))
            }
            ServiceCommand::SetSettings(s) => match settings::save_settings(&settings_path, &s) {
                Ok(()) => {
                    // Le réveil à la demande s'applique tout de suite ; le reste au prochain démarrage.
                    engine.apply_sleep_settings(&s).await;
                    Response::ok(id, serde_json::json!({ "applied_on_next_start": true }))
                }
                Err(e) => Response::err(id, e.to_string()),
            },
            ServiceCommand::Subscribe => {
                if subscription.is_none() {
                    let mut rx = engine.subscribe();
                    let w = writer.clone();
                    // État courant d'abord, puis le flux.
                    let initial = ServiceEvent::State(engine.snapshot());
                    subscription = Some(tokio::spawn(async move {
                        if send_event(&w, &initial).await.is_err() {
                            return;
                        }
                        loop {
                            match rx.recv().await {
                                Ok(ev) => {
                                    if send_event(&w, &ev).await.is_err() {
                                        return;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    tracing::warn!("abonné en retard de {n} événements");
                                }
                                Err(_) => return,
                            }
                        }
                    }));
                }
                Response::ok(id, serde_json::Value::Null)
            }
        };
        send(&writer, &response).await?;
    }
    if let Some(s) = subscription {
        s.abort();
    }
    Ok(())
}

async fn send(
    writer: &Arc<Mutex<tokio::io::WriteHalf<NamedPipeServer>>>,
    response: &Response,
) -> std::io::Result<()> {
    let mut line = serde_json::to_string(response).unwrap_or_default();
    line.push('\n');
    let mut w = writer.lock().await;
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

async fn send_event(
    writer: &Arc<Mutex<tokio::io::WriteHalf<NamedPipeServer>>>,
    event: &ServiceEvent,
) -> std::io::Result<()> {
    let mut line = serde_json::to_string(event).unwrap_or_default();
    line.push('\n');
    let mut w = writer.lock().await;
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}
