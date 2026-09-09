//! Exécution d'une commande avec sortie en flux (vsock 5005), pour Compose et les opérations longues.
//! L'hôte envoie une ligne JSON `{"command": "...", "cwd": "..."}` ; l'agent lance `sh -c` et
//! renvoie des trames `[type, len_hi, len_lo, charge]` : 0 = stdout, 1 = stderr, 2 = code de sortie
//! (texte décimal), puis ferme. Si l'hôte raccroche avant la fin, la commande reçoit SIGTERM.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use solon_core::protocol::{
    EXEC_FRAME_EXIT, EXEC_FRAME_STDERR, EXEC_FRAME_STDOUT, ExecStreamRequest, PORT_EXEC,
};

use crate::system::{State, log};
use crate::vsock;

pub fn serve(state: Arc<State>) {
    let listen_fd = match vsock::listen(PORT_EXEC) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("exécution en flux : {e}"));
            return;
        }
    };
    log(&format!("exécution en flux à l'écoute (vsock {PORT_EXEC})"));
    loop {
        match vsock::accept(listen_fd) {
            Ok(client) => {
                let st = state.clone();
                std::thread::spawn(move || handle(st, client));
            }
            Err(e) => {
                log(&format!("exécution en flux : {e}"));
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

fn write_frame(out: &Mutex<File>, kind: u8, payload: &[u8]) -> bool {
    let mut w = match out.lock() {
        Ok(w) => w,
        Err(_) => return false,
    };
    for chunk in payload.chunks(u16::MAX as usize) {
        let len = (chunk.len() as u16).to_be_bytes();
        if w.write_all(&[kind, len[0], len[1]]).is_err() || w.write_all(chunk).is_err() {
            return false;
        }
    }
    true
}

fn handle(state: Arc<State>, client: File) {
    let client_fd = client.into_raw_fd();
    let mut reader = BufReader::new(unsafe { File::from_raw_fd(libc::dup(client_fd)) });
    let out = Arc::new(Mutex::new(unsafe {
        File::from_raw_fd(libc::dup(client_fd))
    }));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        unsafe { libc::close(client_fd) };
        return;
    }
    let req: ExecStreamRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            write_frame(
                &out,
                EXEC_FRAME_STDERR,
                format!("requête illisible : {e}\n").as_bytes(),
            );
            write_frame(&out, EXEC_FRAME_EXIT, b"2");
            unsafe { libc::close(client_fd) };
            return;
        }
    };
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(&req.command);
    if let Some(cwd) = &req.cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/root")
        .env("TERM", "dumb");
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            write_frame(
                &out,
                EXEC_FRAME_STDERR,
                format!("lancement impossible : {e}\n").as_bytes(),
            );
            write_frame(&out, EXEC_FRAME_EXIT, b"127");
            unsafe { libc::close(client_fd) };
            return;
        }
    };
    let pid = child.id() as i32;
    state.tracked.lock().unwrap().insert(pid);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let pump = |mut src: Box<dyn Read + Send>, kind: u8, out: Arc<Mutex<File>>| {
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match src.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if !write_frame(&out, kind, &buf[..n]) {
                            break;
                        }
                    }
                }
            }
        })
    };
    let t_out = stdout.map(|s| pump(Box::new(s), EXEC_FRAME_STDOUT, out.clone()));
    let t_err = stderr.map(|s| pump(Box::new(s), EXEC_FRAME_STDERR, out.clone()));

    // L'hôte qui raccroche (lecture à zéro) interrompt la commande.
    let watcher = std::thread::spawn(move || {
        let mut b = [0u8; 64];
        loop {
            match reader.read(&mut b) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        unsafe { libc::kill(pid, libc::SIGTERM) };
    });

    let status = child.wait();
    state.tracked.lock().unwrap().remove(&pid);
    if let Some(t) = t_out {
        let _ = t.join();
    }
    if let Some(t) = t_err {
        let _ = t.join();
    }
    let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
    write_frame(&out, EXEC_FRAME_EXIT, code.to_string().as_bytes());
    unsafe {
        libc::shutdown(client_fd, libc::SHUT_WR);
        libc::close(client_fd);
    }
    drop(watcher);
}
