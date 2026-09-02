//! Spike 0b : canal HvSocket hôte↔invité, partage 9P et micro-bancs.
//!
//! Usage (en Administrateur) :
//!   channel_smoke <vmlinuz> <initrd> <dossier_windows_a_partager> [--log fichier] [--mount-opts "cache=loose"]
//!
//! L'initrd doit lancer `solon-agent` (spike) qui écoute sur vsock 5000.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use solon_core::vm::{HostShare, VmConfig};
use solon_vm_hcs::{HcsVm, new_vm_id};
use windows::core::GUID;

const AGENT_PORT: u32 = 5000;
const SHARE_PORT: u32 = 9000;
const SHARE2_PORT: u32 = 9001;

struct Out {
    file: Option<std::fs::File>,
    start: Instant,
}

impl Out {
    fn line(&mut self, s: &str) {
        let text = format!("[{:6} ms] {s}", self.start.elapsed().as_millis());
        println!("{text}");
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{text}");
        }
    }
}

fn console_reader(pipe: String, tx: mpsc::Sender<String>, deadline: Instant) {
    let file = loop {
        match OpenOptions::new().read(true).write(true).open(&pipe) {
            Ok(f) => break Some(f),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = tx.send(format!("!! console : {e}"));
                break None;
            }
        }
    };
    let Some(mut file) = file else { return };
    let mut buf = [0u8; 4096];
    let mut pending = Vec::new();
    loop {
        match file.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = pending.drain(..=pos).collect();
                    let text = String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']).to_owned();
                    if tx.send(text).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// Client ligne/JSON de l'agent spike.
struct Agent {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Agent {
    fn new(stream: TcpStream) -> std::io::Result<Self> {
        stream.set_read_timeout(Some(Duration::from_secs(120)))?;
        Ok(Self { reader: BufReader::new(stream.try_clone()?), writer: stream })
    }

    fn call(&mut self, cmd: &str) -> std::io::Result<serde_json::Value> {
        self.writer.write_all(cmd.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        serde_json::from_str(line.trim()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e} : {line}")))
    }
}

fn main() {
    tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();
    let mut args = std::env::args().skip(1);
    let kernel = PathBuf::from(args.next().expect("noyau"));
    let initrd = PathBuf::from(args.next().expect("initrd"));
    let share_dir = PathBuf::from(args.next().expect("dossier à partager"));
    let mut log = None;
    let mut mount_opts = String::new();
    let mut bench_opts: Vec<String> = Vec::new();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--log" => log = Some(PathBuf::from(args.next().unwrap())),
            "--mount-opts" => mount_opts = args.next().unwrap(),
            "--bench-opts" => bench_opts.push(args.next().unwrap()),
            other => panic!("option inconnue : {other}"),
        }
    }
    std::fs::create_dir_all(&share_dir).expect("dossier partagé");
    let share_dir = std::fs::canonicalize(&share_dir).unwrap();
    // canonicalize renvoie un préfixe \\?\ que HCS n'aime pas forcément : on le retire.
    let share_dir_str = share_dir.to_string_lossy().trim_start_matches(r"\\?\").to_owned();

    let mut out = Out { file: log.map(|p| std::fs::File::create(p).unwrap()), start: Instant::now() };
    out.line(&format!("partage hôte : {share_dir_str} (port {SHARE_PORT})"));
    let mut failures = 0u32;
    let mut check = |out: &mut Out, name: &str, ok: bool, detail: &str| {
        out.line(&format!("{} {name} — {detail}", if ok { "OK  " } else { "ÉCHEC" }));
        if !ok {
            failures += 1;
        }
    };

    let _ = HcsVm::terminate_orphans(None);
    let id = new_vm_id();
    let pipe = format!(r"\\.\pipe\solon-console-{}", &id[..8]);
    let config = VmConfig {
        id: id.clone(),
        name: "solon-spike-0b".into(),
        kernel,
        initrd,
        cmdline: "console=ttyS0,115200 8250_core.nr_uarts=1 panic=-1 pci=off rdinit=/init".into(),
        memory_mb: 1024,
        processors: 2,
        disks: vec![],
        shares: vec![HostShare { name: "host".into(), host_path: PathBuf::from(&share_dir_str), port: SHARE_PORT, read_only: false }],
        serial_pipe: Some(pipe.clone()),
    };

    let vm = match HcsVm::create(&config) {
        Ok(vm) => vm,
        Err(e) => {
            out.line(&format!("!! création : {e}"));
            std::process::exit(2);
        }
    };
    let (tx, rx) = mpsc::channel::<String>();
    let p = pipe.clone();
    std::thread::spawn(move || console_reader(p, tx, Instant::now() + Duration::from_secs(10)));
    let t_start = Instant::now();
    if let Err(e) = vm.start() {
        out.line(&format!("!! démarrage : {e}"));
        let _ = vm.terminate();
        std::process::exit(3);
    }

    // Attend l'agent sur la console.
    let mut agent_ready: Option<u128> = None;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(100)) {
            if line.contains("SOLON-AGENT-READY") {
                agent_ready = Some(t_start.elapsed().as_millis());
                break;
            }
            if line.contains("SOLON-AGENT-FAILED") || line.contains("solon-agent]") {
                out.line(&format!("  | {line}"));
            }
        }
    }
    check(&mut out, "agent prêt (console)", agent_ready.is_some(), &format!("{:?} ms après le démarrage", agent_ready));
    // Continue à vider la console en arrière-plan dans le log.
    let console_lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    {
        let cl = console_lines.clone();
        std::thread::spawn(move || {
            while let Ok(l) = rx.recv() {
                cl.lock().unwrap().push(l);
            }
        });
    }

    let runtime_id = match vm.runtime_id() {
        Ok(r) => r,
        Err(e) => {
            out.line(&format!("!! RuntimeId : {e}"));
            let _ = vm.terminate();
            std::process::exit(4);
        }
    };
    out.line(&format!("Id={id} RuntimeId={runtime_id}"));
    let vm_guid = GUID::try_from(runtime_id.as_str()).expect("RuntimeId est un GUID");

    // 1. Connexion HvSocket.
    let t = Instant::now();
    let stream = match solon_hvsock::connect_with_retry(&vm_guid, AGENT_PORT, Duration::from_secs(10)) {
        Ok(s) => s,
        Err(e) => {
            check(&mut out, "connexion hvsock", false, &e.to_string());
            let _ = vm.terminate();
            std::process::exit(5);
        }
    };
    check(&mut out, "connexion hvsock", true, &format!("{} ms", t.elapsed().as_millis()));
    let mut agent = Agent::new(stream).unwrap();

    // 2. PING : latence aller-retour.
    let mut rtts = Vec::new();
    let mut ping_ok = true;
    for _ in 0..200 {
        let t = Instant::now();
        match agent.call("PING") {
            Ok(v) if v["data"] == "PONG" => rtts.push(t.elapsed().as_micros()),
            other => {
                ping_ok = false;
                out.line(&format!("  ping inattendu : {other:?}"));
                break;
            }
        }
    }
    rtts.sort_unstable();
    let med = rtts.get(rtts.len() / 2).copied().unwrap_or(0);
    let p99 = rtts.get(rtts.len() * 99 / 100).copied().unwrap_or(0);
    check(&mut out, "PING ×200", ping_ok && rtts.len() == 200, &format!("RTT médian {med} µs, p99 {p99} µs"));

    // 3. Débit hvsock.
    for (cmd, label) in [("BLAST 128", "débit invité→hôte"), ("SINK 128", "débit hôte→invité")] {
        match agent.call(cmd) {
            Ok(v) if v["ok"] == true => {
                let mib = v["data"]["mib"].as_u64().unwrap_or(0) as usize;
                if cmd.starts_with("BLAST") {
                    let t = Instant::now();
                    let mut remaining = mib * 1024 * 1024;
                    let mut buf = vec![0u8; 1024 * 1024];
                    let mut ok = true;
                    while remaining > 0 {
                        let want = remaining.min(buf.len());
                        match agent.reader.read(&mut buf[..want]) {
                            Ok(0) | Err(_) => {
                                ok = false;
                                break;
                            }
                            Ok(n) => remaining -= n,
                        }
                    }
                    let s = t.elapsed().as_secs_f64();
                    check(&mut out, label, ok, &format!("{mib} MiB en {:.0} ms = {:.0} MiB/s", s * 1000.0, mib as f64 / s));
                } else {
                    let chunk = vec![0x24u8; 1024 * 1024];
                    let t = Instant::now();
                    let mut ok = true;
                    for _ in 0..mib {
                        if agent.writer.write_all(&chunk).is_err() {
                            ok = false;
                            break;
                        }
                    }
                    let reply = agent.reader.by_ref().lines().next().and_then(|l| l.ok()).unwrap_or_default();
                    let s = t.elapsed().as_secs_f64();
                    check(&mut out, label, ok, &format!("{mib} MiB en {:.0} ms = {:.0} MiB/s (invité : {reply})", s * 1000.0, mib as f64 / s));
                }
            }
            other => check(&mut out, label, false, &format!("{other:?}")),
        }
    }

    // 4. Montage 9P du partage déclaré à la création.
    let mount_cmd = format!("MOUNT {SHARE_PORT} host /mnt/host {mount_opts}");
    let mounted = match agent.call(mount_cmd.trim()) {
        Ok(v) if v["ok"] == true => {
            check(&mut out, "montage 9P (partage initial)", true, &v["data"].to_string());
            true
        }
        other => {
            check(&mut out, "montage 9P (partage initial)", false, &format!("{other:?}"));
            false
        }
    };

    if mounted {
        // 5. Fichier écrit par l'invité, lu par Windows.
        let _ = std::fs::remove_file(share_dir.join("from-guest.txt"));
        let r = agent.call("EXEC echo bonjour-depuis-linux > /mnt/host/from-guest.txt && ls -la /mnt/host").unwrap();
        let host_read = std::fs::read_to_string(share_dir.join("from-guest.txt")).unwrap_or_default();
        check(&mut out, "invité → hôte (écriture)", host_read.trim() == "bonjour-depuis-linux", &format!("lu côté Windows : {host_read:?} ; ls : {}", r["data"]["stdout"].as_str().unwrap_or("").replace('\n', " / ")));

        // 6. Fichier écrit par Windows, lu par l'invité.
        std::fs::write(share_dir.join("from-windows.txt"), "bonjour-depuis-windows\n").unwrap();
        let r = agent.call("EXEC cat /mnt/host/from-windows.txt").unwrap();
        check(&mut out, "hôte → invité (lecture)", r["data"]["stdout"].as_str().unwrap_or("").trim() == "bonjour-depuis-windows", &r["data"]["stdout"].to_string());

        // 7. Métadonnées Linux (chmod) via LinuxMetadata.
        let r = agent.call("EXEC touch /mnt/host/exec.sh && chmod 755 /mnt/host/exec.sh && stat -c %a /mnt/host/exec.sh").unwrap();
        check(&mut out, "chmod conservé (LinuxMetadata)", r["data"]["stdout"].as_str().unwrap_or("").trim() == "755", &r["data"]["stdout"].to_string());

        // 8. Bancs : 9P (options de montage par défaut) vs tmpfs.
        for (dir, label) in [("/mnt/host", "banc 9P (dossier Windows, options par défaut)"), ("/tmp", "banc tmpfs (RAM, référence)")] {
            match agent.call(&format!("BENCH {dir}")) {
                Ok(v) if v["ok"] == true => check(&mut out, label, true, &v["data"].to_string()),
                other => check(&mut out, label, false, &format!("{other:?}")),
            }
        }
        // 8b. Le même dossier exposé par d'autres partages (un partage HCS n'accepte qu'une session 9P :
        // un second montage du même port échoue avec EFAULT) et monté avec d'autres options (msize, cache).
        for (i, opts) in bench_opts.iter().enumerate() {
            let target = format!("/mnt/bench{i}");
            let label = format!("banc 9P avec « {opts} »");
            let port = 9010 + i as u32;
            let name = format!("bench{i}");
            let extra = HostShare { name: name.clone(), host_path: PathBuf::from(&share_dir_str), port, read_only: false };
            if let Err(e) = vm.add_share(&extra) {
                check(&mut out, &label, false, &format!("ajout du partage : {e}"));
                continue;
            }
            match agent.call(&format!("MOUNT {port} {name} {target} {opts}")) {
                Ok(v) if v["ok"] == true => match agent.call(&format!("BENCH {target}")) {
                    Ok(b) if b["ok"] == true => check(&mut out, &label, true, &format!("{} ; montage {}", b["data"], v["data"]["options"])),
                    other => check(&mut out, &label, false, &format!("{other:?}")),
                },
                other => check(&mut out, &label, false, &format!("montage refusé : {other:?}")),
            }
        }
    }

    // 9. Ajout d'un partage à chaud.
    let hot_dir = share_dir.join("hot-add");
    std::fs::create_dir_all(&hot_dir).unwrap();
    std::fs::write(hot_dir.join("marker.txt"), "chaud\n").unwrap();
    let hot = HostShare { name: "hot".into(), host_path: PathBuf::from(hot_dir.to_string_lossy().trim_start_matches(r"\\?\")), port: SHARE2_PORT, read_only: true };
    match vm.add_share(&hot) {
        Ok(()) => {
            let r = agent.call(&format!("MOUNT {SHARE2_PORT} hot /mnt/hot")).unwrap();
            let ok_mount = r["ok"] == true;
            let r2 = agent.call("EXEC cat /mnt/hot/marker.txt ; echo test > /mnt/hot/should-fail.txt ; echo code=$?").unwrap();
            let stdout = r2["data"]["stdout"].as_str().unwrap_or("").to_owned();
            check(&mut out, "partage ajouté à chaud (lecture seule)", ok_mount && stdout.contains("chaud") && !hot_dir.join("should-fail.txt").exists(), &format!("montage : {} ; {}", r["data"], stdout.replace('\n', " / ")));
            let _ = agent.call("EXEC umount /mnt/hot");
            match vm.remove_share(&hot) {
                Ok(()) => check(&mut out, "partage retiré à chaud", true, ""),
                Err(e) => check(&mut out, "partage retiré à chaud", false, &e.to_string()),
            }
        }
        Err(e) => check(&mut out, "partage ajouté à chaud", false, &e.to_string()),
    }

    // 10. Tokio : le socket Hyper-V passe-t-il dans tokio::net::TcpStream ?
    let tokio_ok = (|| -> std::io::Result<u128> {
        let std_stream = solon_hvsock::connect_once(&vm_guid, AGENT_PORT)?;
        std_stream.set_nonblocking(true)?;
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        rt.block_on(async {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut s = tokio::net::TcpStream::from_std(std_stream)?;
            let t = Instant::now();
            s.write_all(b"PING\n").await?;
            let mut line = String::new();
            let mut r = BufReader::new(&mut s);
            tokio::time::timeout(Duration::from_secs(5), r.read_line(&mut line)).await??;
            if !line.contains("PONG") {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, line));
            }
            Ok(t.elapsed().as_micros())
        })
    })();
    match tokio_ok {
        Ok(us) => check(&mut out, "tokio::net::TcpStream::from_std sur hvsock", true, &format!("PING async en {us} µs")),
        Err(e) => check(&mut out, "tokio::net::TcpStream::from_std sur hvsock", false, &e.to_string()),
    }

    // 11. Arrêt via l'agent.
    let _ = agent.call("POWEROFF");
    let exit = vm.wait_exit(Duration::from_secs(15));
    check(&mut out, "arrêt propre via l'agent", exit.is_some(), &format!("{:?}", exit.as_ref().and_then(|e| e.data.clone())));
    if exit.is_none() {
        let _ = vm.terminate();
    }

    out.line("---- console de l'invité (extraits agent) ----");
    for l in console_lines.lock().unwrap().iter().filter(|l| l.contains("solon-agent") || l.contains("9p") || l.contains("SOLON")) {
        out.line(&format!("  | {l}"));
    }
    out.line(&format!("RÉSULTAT : {}", if failures == 0 { "OK" } else { "ÉCHEC" }));
    out.line(&format!("échecs : {failures}"));
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
