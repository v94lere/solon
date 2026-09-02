//! Relais de l'API Docker : chaque connexion vsock sur le port 5001 est reliée à `/run/docker.sock`.
//! Copie bidirectionnelle par deux threads ; la fermeture d'un côté ferme l'autre (nécessaire
//! pour les flux « hijackés » d'`exec`/`attach`).

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use crate::system::{DOCKER_SOCK, State, log};
use crate::vsock;

pub const DOCKER_API_PORT: u32 = 5001;

pub fn serve(_state: Arc<State>) {
    let listen_fd = match vsock::listen(DOCKER_API_PORT) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("relais API Docker : {e}"));
            return;
        }
    };
    log(&format!("relais API Docker à l'écoute (vsock {DOCKER_API_PORT})"));
    loop {
        let client = match vsock::accept(listen_fd) {
            Ok(c) => c,
            Err(e) => {
                log(&format!("relais : {e}"));
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
        };
        std::thread::spawn(move || {
            let docker = match UnixStream::connect(DOCKER_SOCK) {
                Ok(s) => s,
                Err(e) => {
                    log(&format!("relais : {DOCKER_SOCK} indisponible : {e}"));
                    return;
                }
            };
            pump(client, docker);
        });
    }
}

fn shutdown_write(fd: i32) {
    unsafe { libc::shutdown(fd, libc::SHUT_WR) };
}

fn pump(client: std::fs::File, docker: UnixStream) {
    let mut c_read = client.try_clone().expect("dup client");
    let mut c_write = client;
    let mut d_read = docker.try_clone().expect("dup docker");
    let mut d_write = docker;

    let up = std::thread::spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match c_read.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if d_write.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        shutdown_write(d_write.as_raw_fd());
    });
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match d_read.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if c_write.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    }
    shutdown_write(c_write.as_raw_fd());
    let _ = up.join();
}
