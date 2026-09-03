//! Terminal interactif dans la machine (vsock 5004). Chaque connexion de l'hôte ouvre un
//! pseudo-terminal, y lance `/bin/sh -l`, puis relaie : la sortie du TTY vers l'hôte en octets
//! bruts ; de l'hôte vers le TTY des trames `[type, len_hi, len_lo, charge]` (saisie ou
//! redimensionnement JSON `{cols, rows}`). La fin de connexion envoie SIGHUP au shell.

use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::ptr;
use std::sync::Arc;

use solon_core::protocol::{PORT_SHELL, SHELL_FRAME_INPUT, SHELL_FRAME_RESIZE, ShellHeader};

use crate::system::{State, log};
use crate::vsock;

pub fn serve(_state: Arc<State>) {
    let listen_fd = match vsock::listen(PORT_SHELL) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("terminal machine : {e}"));
            return;
        }
    };
    log(&format!("terminal machine à l'écoute (vsock {PORT_SHELL})"));
    loop {
        match vsock::accept(listen_fd) {
            Ok(client) => {
                std::thread::spawn(move || handle(client));
            }
            Err(e) => {
                log(&format!("terminal machine : {e}"));
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

fn handle(client: File) {
    let client_fd = client.into_raw_fd();
    let mut reader = BufReader::new(unsafe { File::from_raw_fd(libc::dup(client_fd)) });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        unsafe { libc::close(client_fd) };
        return;
    }
    let header: ShellHeader =
        serde_json::from_str(line.trim()).unwrap_or(ShellHeader { cols: 80, rows: 24 });

    // Pseudo-terminal.
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let ws = winsize(header.cols, header.rows);
    if unsafe { libc::openpty(&mut master, &mut slave, ptr::null_mut(), ptr::null(), &ws) } != 0 {
        log("terminal machine : openpty a échoué");
        unsafe { libc::close(client_fd) };
        return;
    }

    // Environnement et programme préparés avant fork (pas d'allocation dans l'enfant).
    let program = CString::new("/bin/sh").unwrap();
    let argv: Vec<CString> = vec![CString::new("sh").unwrap(), CString::new("-l").unwrap()];
    let envp: Vec<CString> = [
        "TERM=xterm-256color",
        "HOME=/root",
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "LANG=C.UTF-8",
        "PS1=solon:\\w # ",
    ]
    .iter()
    .map(|s| CString::new(*s).unwrap())
    .collect();
    let mut argv_ptrs: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    argv_ptrs.push(ptr::null());
    let mut envp_ptrs: Vec<*const libc::c_char> = envp.iter().map(|e| e.as_ptr()).collect();
    envp_ptrs.push(ptr::null());

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        log("terminal machine : fork a échoué");
        unsafe {
            libc::close(master);
            libc::close(slave);
            libc::close(client_fd);
        }
        return;
    }
    if pid == 0 {
        // Enfant : nouvelle session, le TTY devient le terminal de contrôle, redirections, exec.
        unsafe {
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
            libc::dup2(slave, 0);
            libc::dup2(slave, 1);
            libc::dup2(slave, 2);
            if slave > 2 {
                libc::close(slave);
            }
            libc::close(master);
            libc::close(client_fd);
            libc::execve(program.as_ptr(), argv_ptrs.as_ptr(), envp_ptrs.as_ptr());
            libc::_exit(127);
        }
    }
    unsafe { libc::close(slave) };

    // TTY → hôte.
    let master_out = unsafe { File::from_raw_fd(libc::dup(master)) };
    let out_fd = client_fd;
    std::thread::spawn(move || {
        let mut from = master_out;
        let mut to = unsafe { File::from_raw_fd(libc::dup(out_fd)) };
        let mut buf = [0u8; 8192];
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
        unsafe { libc::shutdown(out_fd, libc::SHUT_WR) };
    });

    // Hôte → TTY (trames).
    let mut master_in = unsafe { File::from_raw_fd(master) };
    let mut head = [0u8; 3];
    loop {
        if reader.read_exact(&mut head).is_err() {
            break;
        }
        let len = u16::from_be_bytes([head[1], head[2]]) as usize;
        let mut payload = vec![0u8; len];
        if reader.read_exact(&mut payload).is_err() {
            break;
        }
        match head[0] {
            SHELL_FRAME_INPUT => {
                if master_in.write_all(&payload).is_err() {
                    break;
                }
            }
            SHELL_FRAME_RESIZE => {
                if let Ok(h) = serde_json::from_slice::<ShellHeader>(&payload) {
                    let ws = winsize(h.cols, h.rows);
                    unsafe { libc::ioctl(master, libc::TIOCSWINSZ as _, &ws) };
                }
            }
            _ => {}
        }
    }
    // L'hôte a raccroché : le shell reçoit SIGHUP et le glaneur de zombies récolte le processus.
    unsafe {
        libc::kill(pid, libc::SIGHUP);
        libc::close(client_fd);
    }
    drop(master_in);
}
