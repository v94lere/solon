//! Port de contrôle (vsock 5000) : requêtes/réponses JSON par lignes, protocole `solon_core::protocol`.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::time::Duration;

use solon_core::protocol::{Command, PORT_CONTROL, Request, Response};

use crate::system::{self, State, log};
use crate::{bench, net, vsock};

pub fn serve(state: Arc<State>) -> ! {
    let listen_fd = match vsock::listen(PORT_CONTROL) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("ÉCHEC écoute RPC : {e}"));
            println!("SOLON-AGENT-FAILED {e}");
            std::thread::sleep(Duration::from_secs(2));
            system::shutdown(&state, Duration::from_secs(1));
        }
    };
    log(&format!("RPC à l'écoute (vsock {PORT_CONTROL})"));
    loop {
        match vsock::accept(listen_fd) {
            Ok(client) => {
                let st = state.clone();
                std::thread::spawn(move || {
                    if let Err(e) = serve_client(client, st) {
                        log(&format!("client RPC terminé : {e}"));
                    }
                });
            }
            Err(e) => {
                log(&format!("accept RPC : {e}"));
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn send(out: &mut File, response: &Response) -> std::io::Result<()> {
    let mut line = serde_json::to_string(response)
        .unwrap_or_else(|e| Response::err(response.id, e.to_string()).to_json());
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

trait ToJson {
    fn to_json(&self) -> String;
}
impl ToJson for Response {
    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn serve_client(client: File, state: Arc<State>) -> std::io::Result<()> {
    let mut writer = client.try_clone()?;
    let mut reader = BufReader::new(client);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let request: Request = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(e) => {
                send(
                    &mut writer,
                    &Response::err(0, format!("requête illisible : {e}")),
                )?;
                continue;
            }
        };
        let id = request.id;
        let response = match request.command {
            Command::Ping => Response::ok(id, "pong"),
            Command::Health => Response::ok(id, system::health(&state)),
            Command::ConfigureNetwork(cfg) => {
                crate::net::allow_host_to_containers(cfg.gateway.clone());
                match net::configure(&cfg) {
                    Ok(()) => {
                        *state.network_configured.lock().unwrap() = true;
                        Response::ok(id, serde_json::Value::Null)
                    }
                    Err(e) => Response::err(id, e),
                }
            }
            Command::MountShare(req) => match system::mount_plan9(&req) {
                Ok(r) => {
                    // Même lecteur exposé aussi par solonfs (FUSE), en parallèle du 9P.
                    crate::solonfs::mount(&req.name);
                    Response::ok(id, r)
                }
                Err(e) => Response::err(id, e),
            },
            Command::UnmountShare { target } => match system::umount(&target) {
                Ok(()) => Response::ok(id, serde_json::Value::Null),
                Err(e) => Response::err(id, e),
            },
            Command::Exec { command, timeout_s } => Response::ok(
                id,
                system::exec(
                    &state,
                    &command,
                    Duration::from_secs(timeout_s.unwrap_or(300)),
                ),
            ),
            Command::Bench { dir } => match bench::run(&dir) {
                Ok(v) => Response::ok(id, v),
                Err(e) => Response::err(id, e),
            },
            Command::ListPorts => Response::ok(id, crate::events::current_ports()),
            Command::Metrics => Response::ok(id, system::metrics()),
            Command::Shutdown { timeout_s } => {
                send(&mut writer, &Response::ok(id, "bye"))?;
                system::shutdown(&state, Duration::from_secs(timeout_s));
            }
        };
        send(&mut writer, &response)?;
    }
}
