//! Relais des ports publiés (vsock 5002) : l'hôte ouvre une connexion par flux TCP entrant, envoie
//! un en-tête (adresse et port du conteneur), l'agent se connecte au conteneur et acquitte, puis
//! les octets circulent dans les deux sens.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::fd::IntoRawFd;
use std::sync::Arc;
use std::time::Duration;

use solon_core::protocol::{PORT_PORTS, PortRelayAck, PortRelayHeader};

use crate::forward::pump_fds;
use crate::system::{State, log};
use crate::vsock;

pub fn serve(_state: Arc<State>) {
    let listen_fd = match vsock::listen(PORT_PORTS) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("relais de ports : {e}"));
            return;
        }
    };
    log(&format!("relais de ports à l'écoute (vsock {PORT_PORTS})"));
    loop {
        match vsock::accept(listen_fd) {
            Ok(client) => {
                std::thread::spawn(move || handle(client));
            }
            Err(e) => {
                log(&format!("accept relais de ports : {e}"));
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn handle(client: std::fs::File) {
    let mut writer = match client.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(client);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let header: PortRelayHeader = match serde_json::from_str(line.trim()) {
        Ok(h) => h,
        Err(e) => {
            let _ = writeln!(
                writer,
                "{}",
                serde_json::to_string(&PortRelayAck {
                    ok: false,
                    error: Some(format!("en-tête illisible : {e}"))
                })
                .unwrap()
            );
            return;
        }
    };
    let addr: SocketAddr =
        match format!("{}:{}", header.container_ip, header.container_port).parse() {
            Ok(a) => a,
            Err(e) => {
                let _ = writeln!(
                    writer,
                    "{}",
                    serde_json::to_string(&PortRelayAck {
                        ok: false,
                        error: Some(e.to_string())
                    })
                    .unwrap()
                );
                return;
            }
        };
    let target = match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(
                writer,
                "{}",
                serde_json::to_string(&PortRelayAck {
                    ok: false,
                    error: Some(format!("connexion au conteneur {addr} : {e}"))
                })
                .unwrap()
            );
            return;
        }
    };
    let _ = target.set_nodelay(true);
    if writeln!(
        writer,
        "{}",
        serde_json::to_string(&PortRelayAck {
            ok: true,
            error: None
        })
        .unwrap()
    )
    .is_err()
    {
        return;
    }
    // Les octets déjà lus par le BufReader après l'en-tête (rares : le client attend l'accusé)
    // sont transmis avant de passer en copie brute.
    let buffered = reader.buffer().to_vec();
    let client = reader.into_inner();
    let mut target = target;
    if !buffered.is_empty() && target.write_all(&buffered).is_err() {
        return;
    }
    drop(writer);
    pump_fds(client.into_raw_fd(), target.into_raw_fd());
}
