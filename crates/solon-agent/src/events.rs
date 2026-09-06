//! Port d'événements (vsock 5003) : l'hôte se connecte, l'agent pousse des `AgentEvent` JSON.
//! Surveille aussi le flux d'événements de dockerd pour tenir à jour la liste des ports publiés.

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use solon_core::protocol::{
    AgentEvent, InspectedContainer, LogLevel, PORT_EVENTS, PROTOCOL_VERSION, PortBinding,
};

use crate::system::{self, State, log};
use crate::vsock;

struct Hub {
    clients: Mutex<Vec<File>>,
    ports: Mutex<Vec<PortBinding>>,
    engine_ready: Mutex<bool>,
    endpoints: Mutex<Vec<solon_core::protocol::ContainerEndpoint>>,
}

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| Hub {
        clients: Mutex::new(Vec::new()),
        ports: Mutex::new(Vec::new()),
        engine_ready: Mutex::new(false),
        endpoints: Mutex::new(Vec::new()),
    })
}

fn write_event(client: &mut File, event: &AgentEvent) -> std::io::Result<()> {
    let mut line = serde_json::to_string(event).unwrap_or_default();
    line.push('\n');
    client.write_all(line.as_bytes())?;
    client.flush()
}

/// Diffuse un événement à tous les hôtes connectés (les connexions mortes sont retirées).
pub fn broadcast(event: &AgentEvent) {
    match event {
        AgentEvent::EngineReady { .. } => *hub().engine_ready.lock().unwrap() = true,
        AgentEvent::EngineDown { .. } => *hub().engine_ready.lock().unwrap() = false,
        AgentEvent::PortsChanged { bindings } => *hub().ports.lock().unwrap() = bindings.clone(),
        _ => {}
    }
    let mut clients = hub().clients.lock().unwrap();
    clients.retain_mut(|c| write_event(c, event).is_ok());
}

#[allow(dead_code)]
pub fn log_event(level: LogLevel, message: impl Into<String>) {
    broadcast(&AgentEvent::Log {
        level,
        message: message.into(),
    });
}

pub fn current_ports() -> Vec<PortBinding> {
    hub().ports.lock().unwrap().clone()
}

pub fn serve(state: Arc<State>) {
    {
        let st = state.clone();
        std::thread::spawn(move || watch_docker(st));
    }
    let listen_fd = match vsock::listen(PORT_EVENTS) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("événements : {e}"));
            return;
        }
    };
    log(&format!("événements à l'écoute (vsock {PORT_EVENTS})"));
    loop {
        match vsock::accept(listen_fd) {
            Ok(mut client) => {
                // État initial pour un abonné qui arrive en cours de route.
                let hello = AgentEvent::Hello {
                    protocol_version: PROTOCOL_VERSION,
                    agent_version: env!("CARGO_PKG_VERSION").into(),
                };
                let mut ok = write_event(&mut client, &hello).is_ok();
                if ok && *hub().engine_ready.lock().unwrap() {
                    ok = write_event(
                        &mut client,
                        &AgentEvent::EngineReady {
                            uptime_s: system::uptime_secs(),
                        },
                    )
                    .is_ok();
                }
                if ok {
                    ok = write_event(
                        &mut client,
                        &AgentEvent::PortsChanged {
                            bindings: current_ports(),
                        },
                    )
                    .is_ok();
                }
                if ok {
                    hub().clients.lock().unwrap().push(client);
                }
            }
            Err(e) => {
                log(&format!("accept événements : {e}"));
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn docker_cmd() -> Command {
    let mut c = Command::new("/usr/bin/docker");
    c.arg("-H").arg(format!("unix://{}", system::DOCKER_SOCK));
    c
}

/// Recalcule l'ensemble des ports publiés à partir de `docker ps` + `docker inspect`.
fn refresh_ports(state: &State) {
    let mut ps = docker_cmd();
    ps.args(["ps", "-q", "--no-trunc"]);
    let ids: Vec<String> = match system::run_tracked(state, ps) {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        Ok(out) => {
            log(&format!(
                "docker ps a échoué : {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
            return;
        }
        Err(e) => {
            log(&format!("docker ps : {e}"));
            return;
        }
    };
    let mut bindings = Vec::new();
    let mut endpoints = Vec::new();
    if !ids.is_empty() {
        let mut inspect = docker_cmd();
        inspect.arg("inspect").args(&ids);
        match system::run_tracked(state, inspect) {
            Ok(out) => match serde_json::from_slice::<Vec<InspectedContainer>>(&out.stdout) {
                Ok(list) => {
                    for c in &list {
                        bindings.extend(c.port_bindings());
                        if let Some(e) = c.endpoint() {
                            endpoints.push(e);
                        }
                    }
                }
                Err(e) => log(&format!("docker inspect illisible : {e}")),
            },
            Err(e) => log(&format!("docker inspect : {e}")),
        }
    }
    bindings.sort();
    bindings.dedup();
    endpoints.sort();
    let changed = *hub().ports.lock().unwrap() != bindings;
    if changed {
        log(&format!("ports publiés : {} liaison(s)", bindings.len()));
        broadcast(&AgentEvent::PortsChanged { bindings });
    }
    let ep_changed = *hub().endpoints.lock().unwrap() != endpoints;
    if ep_changed {
        *hub().endpoints.lock().unwrap() = endpoints.clone();
        broadcast(&AgentEvent::EndpointsChanged { endpoints });
    }
}

#[derive(serde::Deserialize)]
struct DockerEvent {
    #[serde(rename = "Action", default)]
    action: String,
    #[serde(rename = "Actor", default)]
    actor: Option<serde_json::Value>,
}

const CRLF: &[u8] = b"\r\n";

/// Lit le flux `GET /events` de dockerd sur son socket Unix (HTTP/1.1, transfert par morceaux).
/// On n'utilise pas le CLI `docker events` : quand sa sortie est redirigée, il ne la vide qu'à la
/// fin, si bien qu'aucun événement n'arrive tant qu'il tourne (constaté au bloc 2).
fn stream_docker_events(on_event: &mut dyn FnMut(DockerEvent)) -> std::io::Result<()> {
    let mut sock = UnixStream::connect(system::DOCKER_SOCK)?;
    let filters = "%7B%22type%22%3A%5B%22container%22%5D%7D"; // {"type":["container"]}
    let request = format!(
        "GET /events?filters={filters} HTTP/1.1\r\nHost: docker\r\nAccept: application/json\r\n\r\n"
    );
    sock.write_all(request.as_bytes())?;

    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let n = sock.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::other("connexion fermée avant les en-têtes"));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
    if !headers.starts_with("HTTP/1.1 200") {
        return Err(std::io::Error::other(format!(
            "réponse inattendue : {}",
            headers.lines().next().unwrap_or("")
        )));
    }
    let chunked = headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    buf.drain(..header_end);

    let mut body: Vec<u8> = Vec::new();
    loop {
        if chunked {
            // Morceaux : "<taille hex>\r\n<données>\r\n" ; un morceau de taille 0 termine le flux.
            while let Some(pos) = buf.windows(2).position(|w| w == CRLF) {
                let size_str = String::from_utf8_lossy(&buf[..pos]).trim().to_string();
                let size = usize::from_str_radix(size_str.split(';').next().unwrap_or("0"), 16)
                    .map_err(|_| {
                        std::io::Error::other(format!("taille de morceau invalide : {size_str}"))
                    })?;
                if size == 0 {
                    return Ok(());
                }
                if buf.len() < pos + 2 + size + 2 {
                    break;
                }
                body.extend_from_slice(&buf[pos + 2..pos + 2 + size]);
                buf.drain(..pos + 2 + size + 2);
            }
        } else {
            body.append(&mut buf);
        }
        // dockerd envoie un objet JSON par événement, terminé par un saut de ligne.
        while let Some(nl) = body.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = body.drain(..=nl).collect();
            if let Ok(ev) = serde_json::from_slice::<DockerEvent>(&line) {
                on_event(ev);
            }
        }
        let n = sock.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Suit les événements de dockerd tant qu'il tourne ; reprend après chaque redémarrage du moteur.
fn watch_docker(state: Arc<State>) {
    loop {
        if !system::docker_ping() {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        refresh_ports(&state);
        log("flux d'événements Docker ouvert");
        let st = state.clone();
        let result = stream_docker_events(&mut |ev| {
            let (id, name) = ev
                .actor
                .as_ref()
                .map(|a| {
                    (
                        a.get("ID")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                        a.get("Attributes")
                            .and_then(|a| a.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    )
                })
                .unwrap_or_default();
            broadcast(&AgentEvent::Container {
                action: ev.action.clone(),
                id,
                name,
            });
            if matches!(
                ev.action.as_str(),
                "start"
                    | "die"
                    | "stop"
                    | "kill"
                    | "destroy"
                    | "restart"
                    | "pause"
                    | "unpause"
                    | "rename"
            ) {
                refresh_ports(&st);
            }
        });
        match result {
            Ok(()) => log("flux d'événements Docker fermé, reprise dans 1 s"),
            Err(e) => log(&format!("flux d'événements Docker : {e}, reprise dans 1 s")),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Nombre de conteneurs en marche au dernier relevé (voir `refresh_ports`).
pub fn running_count() -> u32 {
    hub().endpoints.lock().unwrap().len() as u32
}
