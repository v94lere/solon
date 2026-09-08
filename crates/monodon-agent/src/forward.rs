//! Relais de l'API Docker (vsock 5001 → `/run/docker.sock`) et copie bidirectionnelle générique.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use monodon_core::protocol::PORT_DOCKER;

use crate::system::{DOCKER_SOCK, State, log};
use crate::vsock;

pub fn serve(_state: Arc<State>) {
    let listen_fd = match vsock::listen(PORT_DOCKER) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("relais API Docker : {e}"));
            return;
        }
    };
    log(&format!(
        "relais API Docker à l'écoute (vsock {PORT_DOCKER})"
    ));
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
            pump_fds(client.into_raw_fd(), docker.into_raw_fd());
        });
    }
}

/// Copie bidirectionnelle entre deux descripteurs de sockets. Quand un sens atteint la fin de
/// flux, la demi-fermeture est propagée (`shutdown(SHUT_WR)`) ; les deux descripteurs sont fermés
/// à la sortie. Prend possession des deux descripteurs.
pub fn pump_fds(a: RawFd, b: RawFd) {
    let a_dup = unsafe { libc::dup(a) };
    let b_dup = unsafe { libc::dup(b) };
    if a_dup < 0 || b_dup < 0 {
        unsafe {
            libc::close(a);
            libc::close(b);
        }
        return;
    }
    let (mut a_read, mut b_write) = unsafe { (File::from_raw_fd(a), File::from_raw_fd(b_dup)) };
    let (mut b_read, mut a_write) = unsafe { (File::from_raw_fd(b), File::from_raw_fd(a_dup)) };

    let forward = std::thread::spawn(move || {
        copy_then_shutdown(&mut a_read, &mut b_write, b_dup);
    });
    copy_then_shutdown(&mut b_read, &mut a_write, a_dup);
    let _ = forward.join();
}

fn copy_then_shutdown(from: &mut File, to: &mut File, to_fd: RawFd) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    }
    unsafe { libc::shutdown(to_fd, libc::SHUT_WR) };
}
