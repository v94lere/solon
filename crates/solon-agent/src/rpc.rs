//! RPC de contrôle sur vsock 5000. Protocole texte provisoire (une ligne = une commande, réponse =
//! une ligne JSON `{"ok":bool,"data":...}`) ; le protocole définitif arrive avec le service (bloc 2).
//!
//! Commandes : `PING`, `HEALTH`, `MOUNT <port> <aname> <cible> [options]`, `UMOUNT <cible>`,
//! `EXEC <commande sh>`, `BENCH <dossier>`, `BLAST <MiB>`, `SINK <MiB>`, `POWEROFF [timeout_s]`.

use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::system::{self, State, log};
use crate::{bench, vsock};

pub const CONTROL_PORT: u32 = 5000;

pub fn serve(state: Arc<State>) -> ! {
    let listen_fd = match vsock::listen(CONTROL_PORT) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("ÉCHEC écoute RPC : {e}"));
            println!("SOLON-AGENT-FAILED {e}");
            std::thread::sleep(Duration::from_secs(2));
            system::shutdown(&state, Duration::from_secs(1));
        }
    };
    log(&format!("RPC à l'écoute (vsock {CONTROL_PORT})"));
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

fn reply(out: &mut File, ok: bool, data: Value) -> std::io::Result<()> {
    let line = json!({ "ok": ok, "data": data }).to_string();
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

fn mount_plan9(port: u32, aname: &str, target: &str, extra: &str) -> Result<Value, String> {
    std::fs::create_dir_all(target).map_err(|e| format!("mkdir {target} : {e}"))?;
    let fd = vsock::connect_host(port)?;
    let sz: libc::c_int = 4 * 1024 * 1024;
    unsafe {
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_SNDBUF, &sz as *const _ as *const libc::c_void, 4);
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_RCVBUF, &sz as *const _ as *const libc::c_void, 4);
    }
    let mut data = format!("trans=fd,rfdno={fd},wfdno={fd},aname={aname}");
    if !extra.contains("msize=") {
        data.push_str(",msize=65536");
    }
    if !extra.is_empty() {
        data.push(',');
        data.push_str(extra);
    }
    let t0 = Instant::now();
    let src = CString::new("hcs-plan9").unwrap();
    let tgt = CString::new(target).unwrap();
    let fstype = CString::new("9p").unwrap();
    let opts = CString::new(data.clone()).unwrap();
    let rc = unsafe { libc::mount(src.as_ptr(), tgt.as_ptr(), fstype.as_ptr(), 0, opts.as_ptr() as *const libc::c_void) };
    unsafe { libc::close(fd) };
    if rc < 0 {
        return Err(format!("mount 9p ({data}) : {}", std::io::Error::last_os_error()));
    }
    Ok(json!({ "target": target, "options": data, "mount_ms": t0.elapsed().as_millis() as u64 }))
}

fn umount(target: &str) -> Result<(), String> {
    let t = CString::new(target).unwrap();
    if unsafe { libc::umount2(t.as_ptr(), 0) } < 0 {
        Err(format!("umount {target} : {}", std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

fn exec(state: &State, cmd: &str) -> Value {
    let t0 = Instant::now();
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(cmd);
    match system::run_tracked(state, c) {
        Ok(out) => json!({
            "code": out.status.code(),
            "stdout": String::from_utf8_lossy(&out.stdout),
            "stderr": String::from_utf8_lossy(&out.stderr),
            "ms": t0.elapsed().as_millis() as u64,
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

fn health(state: &State) -> Value {
    let engine = state.engine.lock().unwrap().clone();
    let disk = state.data_disk.lock().unwrap().clone();
    let mem = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |k: &str| -> u64 {
        mem.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    json!({
        "uptime_s": system::uptime_secs(),
        "docker_ping": system::docker_ping(),
        "engine": engine,
        "data_disk": disk,
        "mem_total_kb": field("MemTotal"),
        "mem_available_kb": field("MemAvailable"),
        "kernel": std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim(),
    })
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
        let line = line.trim_end();
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "PING" => reply(&mut writer, true, json!("PONG"))?,
            "HEALTH" => reply(&mut writer, true, health(&state))?,
            "MOUNT" => {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() < 3 {
                    reply(&mut writer, false, json!("usage : MOUNT <port> <aname> <cible> [options]"))?;
                    continue;
                }
                let port: u32 = parts[0].parse().unwrap_or(0);
                match mount_plan9(port, parts[1], parts[2], parts.get(3).copied().unwrap_or("")) {
                    Ok(v) => reply(&mut writer, true, v)?,
                    Err(e) => reply(&mut writer, false, json!(e))?,
                }
            }
            "UMOUNT" => match umount(rest.trim()) {
                Ok(()) => reply(&mut writer, true, json!(rest.trim()))?,
                Err(e) => reply(&mut writer, false, json!(e))?,
            },
            "EXEC" => {
                let v = exec(&state, rest);
                reply(&mut writer, true, v)?
            }
            "BENCH" => match bench::run(rest.trim()) {
                Ok(v) => reply(&mut writer, true, v)?,
                Err(e) => reply(&mut writer, false, json!(e))?,
            },
            "BLAST" => {
                let mib: usize = rest.trim().parse().unwrap_or(64);
                reply(&mut writer, true, json!({ "mib": mib }))?;
                let chunk = vec![0x42u8; 1024 * 1024];
                for _ in 0..mib {
                    writer.write_all(&chunk)?;
                }
                writer.flush()?;
            }
            "SINK" => {
                let mib: usize = rest.trim().parse().unwrap_or(64);
                reply(&mut writer, true, json!({ "mib": mib }))?;
                let mut buf = vec![0u8; 1024 * 1024];
                let mut remaining = mib * 1024 * 1024;
                let t0 = Instant::now();
                while remaining > 0 {
                    let want = remaining.min(buf.len());
                    let n = reader.read(&mut buf[..want])?;
                    if n == 0 {
                        break;
                    }
                    remaining -= n;
                }
                let s = t0.elapsed().as_secs_f64();
                reply(&mut writer, true, json!({ "mib_s": (mib as f64 / s).round(), "ms": (s * 1000.0) as u64 }))?;
            }
            "POWEROFF" => {
                let timeout: u64 = rest.trim().parse().unwrap_or(10);
                reply(&mut writer, true, json!("bye"))?;
                system::shutdown(&state, Duration::from_secs(timeout));
            }
            _ => reply(&mut writer, false, json!(format!("commande inconnue : {cmd}")))?,
        }
    }
}
