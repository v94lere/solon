//! Agent invité Solon — version « spike 0b ».
//!
//! Écoute sur AF_VSOCK port 5000 et exécute des commandes texte (une ligne = une commande) :
//! `PING`, `MOUNT <port> <aname> <cible>`, `EXEC <commande sh>`, `BENCH <dossier>`,
//! `BLAST <MiB>` (l'agent envoie), `SINK <MiB>` (l'agent reçoit), `POWEROFF`.
//! Chaque réponse est une ligne JSON `{"ok":bool,"data":...}`.
//!
//! Cette version sert à valider le canal HvSocket et le partage 9P ; l'agent définitif
//! (PID 1, supervision de dockerd, protocole binaire) arrive au bloc 1.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("solon-agent ne s'exécute que sous Linux (cible x86_64-unknown-linux-musl).");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CString;
    use std::fs::File;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::fd::{FromRawFd, RawFd};
    use std::path::Path;
    use std::time::Instant;

    use serde_json::{Value, json};

    pub const CONTROL_PORT: u32 = 5000;

    fn log(msg: &str) {
        eprintln!("[solon-agent] {msg}");
    }

    fn errno_msg(context: &str) -> String {
        format!("{context} : {}", std::io::Error::last_os_error())
    }

    /// Socket vsock brut. Renvoie le descripteur.
    fn vsock_socket() -> Result<RawFd, String> {
        let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
        if fd < 0 { Err(errno_msg("socket(AF_VSOCK)")) } else { Ok(fd) }
    }

    fn sockaddr_vm(cid: u32, port: u32) -> libc::sockaddr_vm {
        let mut addr: libc::sockaddr_vm = unsafe { std::mem::zeroed() };
        addr.svm_family = libc::AF_VSOCK as libc::sa_family_t;
        addr.svm_cid = cid;
        addr.svm_port = port;
        addr
    }

    /// Écoute sur un port vsock (toutes les CID).
    fn vsock_listen(port: u32) -> Result<RawFd, String> {
        let fd = vsock_socket()?;
        let addr = sockaddr_vm(libc::VMADDR_CID_ANY, port);
        let rc = unsafe {
            libc::bind(fd, &addr as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_vm>() as u32)
        };
        if rc < 0 { return Err(errno_msg("bind(vsock)")); }
        if unsafe { libc::listen(fd, 4) } < 0 { return Err(errno_msg("listen(vsock)")); }
        Ok(fd)
    }

    /// Se connecte à l'hôte (CID 2) sur un port : c'est ainsi qu'on rejoint le serveur 9P de HCS.
    fn vsock_connect_host(port: u32) -> Result<RawFd, String> {
        let fd = vsock_socket()?;
        let addr = sockaddr_vm(libc::VMADDR_CID_HOST, port);
        let rc = unsafe {
            libc::connect(fd, &addr as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_vm>() as u32)
        };
        if rc < 0 {
            let e = errno_msg(&format!("connect(vsock host:{port})"));
            unsafe { libc::close(fd) };
            return Err(e);
        }
        Ok(fd)
    }

    /// Monte un partage Plan9 HCS : connexion vsock vers l'hôte puis `mount -t 9p -o trans=fd`.
    fn mount_plan9(port: u32, aname: &str, target: &str, extra: &str) -> Result<Value, String> {
        std::fs::create_dir_all(target).map_err(|e| format!("mkdir {target} : {e}"))?;
        let fd = vsock_connect_host(port)?;
        // Tampons larges, comme le fait hcsshim, pour maximiser le débit 9P.
        let sz: libc::c_int = 4 * 1024 * 1024;
        unsafe {
            libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_SNDBUF, &sz as *const _ as *const libc::c_void, 4);
            libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_RCVBUF, &sz as *const _ as *const libc::c_void, 4);
        }
        // msize par défaut 64 Kio (valeur hcsshim) ; les options supplémentaires peuvent le remplacer.
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
        let rc = unsafe {
            libc::mount(src.as_ptr(), tgt.as_ptr(), fstype.as_ptr(), 0, opts.as_ptr() as *const libc::c_void)
        };
        // Le noyau garde sa propre référence au fd une fois monté.
        unsafe { libc::close(fd) };
        if rc < 0 {
            return Err(format!("mount 9p ({data}) : {}", std::io::Error::last_os_error()));
        }
        Ok(json!({ "target": target, "options": data, "mount_ms": t0.elapsed().as_millis() as u64 }))
    }

    fn exec(cmd: &str) -> Value {
        let t0 = Instant::now();
        match std::process::Command::new("/bin/busybox").arg("sh").arg("-c").arg(cmd).output() {
            Ok(out) => json!({
                "code": out.status.code(),
                "stdout": String::from_utf8_lossy(&out.stdout),
                "stderr": String::from_utf8_lossy(&out.stderr),
                "ms": t0.elapsed().as_millis() as u64,
            }),
            Err(e) => json!({ "error": e.to_string() }),
        }
    }

    /// Micro-banc d'E/S : séquentiel (128 MiB, blocs de 1 MiB) et petits fichiers (1000 × 4 KiB).
    fn bench(dir: &str) -> Result<Value, String> {
        let dir = Path::new(dir);
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let big = dir.join("solon-bench.bin");
        let block = vec![0xA5u8; 1024 * 1024];
        let blocks = 128usize;

        let t = Instant::now();
        {
            let mut f = File::create(&big).map_err(|e| format!("create : {e}"))?;
            for _ in 0..blocks { f.write_all(&block).map_err(|e| format!("write : {e}"))?; }
            f.sync_all().map_err(|e| format!("fsync : {e}"))?;
        }
        let write_s = t.elapsed().as_secs_f64();

        drop_caches();
        let t = Instant::now();
        {
            let mut f = File::open(&big).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 1024 * 1024];
            let mut total = 0usize;
            loop {
                let n = f.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 { break; }
                total += n;
            }
            if total != blocks * 1024 * 1024 { return Err(format!("lecture incomplète : {total}")); }
        }
        let read_s = t.elapsed().as_secs_f64();
        let _ = std::fs::remove_file(&big);

        let small_dir = dir.join("solon-bench-small");
        let _ = std::fs::remove_dir_all(&small_dir);
        std::fs::create_dir_all(&small_dir).map_err(|e| e.to_string())?;
        let n_small = 1000usize;
        let payload = vec![0x5Au8; 4096];

        let t = Instant::now();
        for i in 0..n_small {
            let mut f = File::create(small_dir.join(format!("f{i:04}.dat"))).map_err(|e| e.to_string())?;
            f.write_all(&payload).map_err(|e| e.to_string())?;
        }
        let create_ms = t.elapsed().as_millis() as u64;

        drop_caches();
        let t = Instant::now();
        for i in 0..n_small {
            std::fs::metadata(small_dir.join(format!("f{i:04}.dat"))).map_err(|e| e.to_string())?;
        }
        let stat_ms = t.elapsed().as_millis() as u64;

        let t = Instant::now();
        let mut buf = Vec::with_capacity(4096);
        for i in 0..n_small {
            buf.clear();
            File::open(small_dir.join(format!("f{i:04}.dat"))).map_err(|e| e.to_string())?.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        }
        let read_small_ms = t.elapsed().as_millis() as u64;

        let t = Instant::now();
        let entries = std::fs::read_dir(&small_dir).map_err(|e| e.to_string())?.count();
        let readdir_ms = t.elapsed().as_millis() as u64;

        let t = Instant::now();
        std::fs::remove_dir_all(&small_dir).map_err(|e| e.to_string())?;
        let delete_ms = t.elapsed().as_millis() as u64;

        Ok(json!({
            "seq_write_mib_s": (blocks as f64 / write_s).round(),
            "seq_read_mib_s": (blocks as f64 / read_s).round(),
            "small_files": n_small,
            "small_create_ms": create_ms,
            "small_stat_ms": stat_ms,
            "small_read_ms": read_small_ms,
            "readdir_ms": readdir_ms,
            "readdir_entries": entries,
            "small_delete_ms": delete_ms,
        }))
    }

    fn drop_caches() {
        unsafe { libc::sync() };
        let _ = std::fs::write("/proc/sys/vm/drop_caches", "3\n");
    }

    fn poweroff() -> ! {
        log("arrêt demandé par l'hôte");
        unsafe {
            libc::sync();
            libc::reboot(libc::LINUX_REBOOT_CMD_POWER_OFF);
        }
        loop { std::thread::sleep(std::time::Duration::from_secs(1)); }
    }

    fn reply(out: &mut File, ok: bool, data: Value) -> std::io::Result<()> {
        let line = json!({ "ok": ok, "data": data }).to_string();
        out.write_all(line.as_bytes())?;
        out.write_all(b"\n")?;
        out.flush()
    }

    fn serve_client(fd: RawFd) -> std::io::Result<()> {
        let reader_file = unsafe { File::from_raw_fd(fd) };
        let mut writer = reader_file.try_clone()?;
        let mut reader = BufReader::new(reader_file);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 { return Ok(()); }
            let line = line.trim_end();
            let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
            log(&format!("commande {cmd} {rest}"));
            match cmd {
                "PING" => reply(&mut writer, true, json!("PONG"))?,
                "MOUNT" => {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() < 3 {
                        reply(&mut writer, false, json!("usage : MOUNT <port> <aname> <cible> [options]"))?;
                        continue;
                    }
                    let port: u32 = parts[0].parse().unwrap_or(0);
                    let extra = parts.get(3).copied().unwrap_or("");
                    match mount_plan9(port, parts[1], parts[2], extra) {
                        Ok(v) => reply(&mut writer, true, v)?,
                        Err(e) => reply(&mut writer, false, json!(e))?,
                    }
                }
                "EXEC" => reply(&mut writer, true, exec(rest))?,
                "BENCH" => match bench(rest.trim()) {
                    Ok(v) => reply(&mut writer, true, v)?,
                    Err(e) => reply(&mut writer, false, json!(e))?,
                },
                "BLAST" => {
                    let mib: usize = rest.trim().parse().unwrap_or(64);
                    reply(&mut writer, true, json!({ "mib": mib }))?;
                    let chunk = vec![0x42u8; 1024 * 1024];
                    let t0 = Instant::now();
                    for _ in 0..mib { writer.write_all(&chunk)?; }
                    writer.flush()?;
                    log(&format!("BLAST {mib} MiB envoyés en {} ms", t0.elapsed().as_millis()));
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
                        if n == 0 { break; }
                        remaining -= n;
                    }
                    let s = t0.elapsed().as_secs_f64();
                    reply(&mut writer, true, json!({ "mib_s": (mib as f64 / s).round(), "ms": (s * 1000.0) as u64 }))?;
                }
                "POWEROFF" => {
                    reply(&mut writer, true, json!("bye"))?;
                    poweroff();
                }
                _ => reply(&mut writer, false, json!(format!("commande inconnue : {cmd}")))?,
            }
        }
    }

    pub fn run() {
        log(&format!("démarrage, écoute vsock port {CONTROL_PORT}"));
        let listen_fd = match vsock_listen(CONTROL_PORT) {
            Ok(fd) => fd,
            Err(e) => {
                log(&format!("ÉCHEC écoute vsock : {e}"));
                println!("SOLON-AGENT-FAILED {e}");
                std::thread::sleep(std::time::Duration::from_secs(2));
                poweroff();
            }
        };
        println!("SOLON-AGENT-READY");
        loop {
            let fd = unsafe { libc::accept(listen_fd, std::ptr::null_mut(), std::ptr::null_mut()) };
            if fd < 0 {
                log(&errno_msg("accept"));
                continue;
            }
            log("client hôte connecté");
            std::thread::spawn(move || {
                if let Err(e) = serve_client(fd) {
                    log(&format!("client terminé : {e}"));
                } else {
                    log("client déconnecté");
                }
            });
        }
    }
}
