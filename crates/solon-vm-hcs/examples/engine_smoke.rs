//! Bloc 1 : démarre l'image Solon complète (noyau + initrd + rootfs.vhd + data.vhdx), attend le
//! moteur Docker, expose l'API sur `\\.\pipe\solon` et exécute un conteneur depuis le CLI `docker`
//! de Windows. Deux cycles de démarrage pour mesurer le démarrage « à chaud » et la persistance.
//!
//! Usage (en Administrateur) :
//!   engine_smoke <vmlinuz> <initrd> <rootfs.vhd> <data.vhdx> --busybox <busybox.static> [--log f] [--nobridge] [--mem MB]

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use solon_core::vm::{DiskAttachment, VmConfig};
use solon_vm_hcs::{HcsVm, new_vm_id};
use windows::core::GUID;

const PIPE: &str = r"\\.\pipe\solon";
const DOCKER_HOST: &str = "npipe:////./pipe/solon";

struct Out {
    file: Option<std::fs::File>,
    start: Instant,
    failures: u32,
}

impl Out {
    fn line(&mut self, s: &str) {
        let text = format!("[{:6} ms] {s}", self.start.elapsed().as_millis());
        println!("{text}");
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{text}");
        }
    }
    fn check(&mut self, name: &str, ok: bool, detail: &str) {
        self.line(&format!(
            "{} {name} — {detail}",
            if ok { "OK  " } else { "ÉCHEC" }
        ));
        if !ok {
            self.failures += 1;
        }
    }
}

#[derive(Default)]
struct Console {
    lines: Vec<(u128, String)>,
}

fn console_reader(
    pipe: String,
    t0: Instant,
    console: Arc<Mutex<Console>>,
    tx: mpsc::Sender<String>,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let file = loop {
        match OpenOptions::new().read(true).write(true).open(&pipe) {
            Ok(f) => break Some(f),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
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
                    let text = String::from_utf8_lossy(&line)
                        .trim_end_matches(['\r', '\n'])
                        .to_owned();
                    console
                        .lock()
                        .unwrap()
                        .lines
                        .push((t0.elapsed().as_millis(), text.clone()));
                    let _ = tx.send(text);
                }
            }
        }
    }
}

struct Rpc {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Rpc {
    fn connect(vm: &GUID) -> std::io::Result<Self> {
        let s = solon_hvsock::connect_with_retry(vm, 5000, Duration::from_secs(10))?;
        s.set_read_timeout(Some(Duration::from_secs(120)))?;
        Ok(Self {
            reader: BufReader::new(s.try_clone()?),
            writer: s,
        })
    }
    fn call(&mut self, cmd: &str) -> std::io::Result<serde_json::Value> {
        self.writer.write_all(format!("{cmd}\n").as_bytes())?;
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        serde_json::from_str(line.trim()).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e} : {line}"))
        })
    }
}

/// Archive tar minimale (un seul fichier `bin/busybox`, mode 0755) pour `docker import`.
fn busybox_tar(busybox: &[u8]) -> Vec<u8> {
    fn header(name: &str, mode: &str, size: usize, typeflag: u8) -> [u8; 512] {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(mode.as_bytes());
        h[108..115].copy_from_slice(b"0000000");
        h[116..123].copy_from_slice(b"0000000");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[136..147].copy_from_slice(b"00000000000");
        h[148..156].copy_from_slice(b"        ");
        h[156] = typeflag;
        h[257..262].copy_from_slice(b"ustar");
        h[263..265].copy_from_slice(b"00");
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h
    }
    let mut tar = Vec::new();
    tar.extend_from_slice(&header("bin/", "0000755", 0, b'5'));
    tar.extend_from_slice(&header("bin/busybox", "0000755", busybox.len(), b'0'));
    tar.extend_from_slice(busybox);
    while tar.len() % 512 != 0 {
        tar.push(0);
    }
    tar.extend_from_slice(&[0u8; 1024]);
    tar
}

/// Lance le CLI `docker` contre le pipe Solon, avec un délai maximal (un flux qui ne se ferme
/// pas doit faire échouer le test, pas le bloquer).
fn docker(args: &[&str], stdin: Option<&[u8]>) -> (bool, String, u128) {
    use std::process::Stdio;
    let t = Instant::now();
    let mut cmd = Command::new("docker");
    cmd.arg("-H")
        .arg(DOCKER_HOST)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (false, e.to_string(), t.elapsed().as_millis()),
    };
    {
        let mut si = child.stdin.take().unwrap();
        if let Some(data) = stdin {
            let _ = si.write_all(data);
        }
        drop(si);
    }
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = so.read_to_end(&mut v);
        v
    });
    let err_thread = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = se.read_to_end(&mut v);
        v
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    let mut text = format!(
        "{}{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    )
    .trim()
    .to_owned();
    let ok = match status {
        Some(s) => s.success(),
        None => {
            text.push_str(" [DÉLAI DÉPASSÉ 60 s, processus tué]");
            false
        }
    };
    (ok, text, t.elapsed().as_millis())
}

struct BootResult {
    vm: HcsVm,
    guid: GUID,
    initrd_ok_ms: Option<u128>,
    agent_ready_ms: Option<u128>,
    engine_ready_ms: Option<u128>,
    console: Arc<Mutex<Console>>,
}

fn boot(out: &mut Out, config: &VmConfig) -> Result<BootResult, String> {
    let vm = HcsVm::create(config).map_err(|e| format!("création : {e}"))?;
    let console = Arc::new(Mutex::new(Console::default()));
    let (tx, rx) = mpsc::channel::<String>();
    let t_start = Instant::now();
    {
        let pipe = config.serial_pipe.clone().unwrap();
        let c = console.clone();
        std::thread::spawn(move || console_reader(pipe, t_start, c, tx));
    }
    vm.start().map_err(|e| format!("démarrage : {e}"))?;
    let mut r = BootResult {
        guid: GUID::zeroed(),
        vm,
        initrd_ok_ms: None,
        agent_ready_ms: None,
        engine_ready_ms: None,
        console,
    };
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline && r.engine_ready_ms.is_none() {
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(100)) {
            let ms = t_start.elapsed().as_millis();
            if line.contains("SOLON-INITRD-OK") {
                r.initrd_ok_ms = Some(ms);
            } else if line.contains("SOLON-AGENT-READY") {
                r.agent_ready_ms = Some(ms);
                out.line(&format!("  | {line}"));
            } else if line.contains("SOLON-ENGINE-READY") {
                r.engine_ready_ms = Some(ms);
                out.line(&format!("  | {line}"));
            } else if line.contains("SOLON-") && line.contains("FAILED") {
                out.line(&format!("  | {line}"));
                break;
            } else if line.contains("[solon-agent") || line.contains("AVERTISSEMENT") {
                out.line(&format!("  | {line}"));
            }
        }
    }
    let runtime = r.vm.runtime_id().map_err(|e| e.to_string())?;
    r.guid = GUID::try_from(runtime.as_str()).map_err(|e| e.to_string())?;
    Ok(r)
}

fn dump_console(out: &mut Out, console: &Arc<Mutex<Console>>, max: usize) {
    let lines = console.lock().unwrap();
    let skip = lines.lines.len().saturating_sub(max);
    for (ms, l) in lines.lines.iter().skip(skip) {
        out.line(&format!("  console+{ms:5}ms | {l}"));
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    let mut args = std::env::args().skip(1);
    let kernel = PathBuf::from(args.next().expect("noyau"));
    let initrd = PathBuf::from(args.next().expect("initrd"));
    let rootfs = PathBuf::from(args.next().expect("rootfs.vhd"));
    let data = PathBuf::from(args.next().expect("data.vhdx"));
    let mut busybox = None;
    let mut log = None;
    let mut nobridge = false;
    let mut mem = 2048u64;
    while let Some(f) = args.next() {
        match f.as_str() {
            "--busybox" => busybox = Some(PathBuf::from(args.next().unwrap())),
            "--log" => log = Some(PathBuf::from(args.next().unwrap())),
            "--nobridge" => nobridge = true,
            "--mem" => mem = args.next().unwrap().parse().unwrap(),
            other => panic!("option inconnue : {other}"),
        }
    }
    let mut out = Out {
        file: log.map(|p| std::fs::File::create(p).unwrap()),
        start: Instant::now(),
        failures: 0,
    };
    let _ = HcsVm::terminate_orphans(None);

    let mut cmdline =
        "console=ttyS0,115200 8250_core.nr_uarts=1 panic=-1 pci=off rdinit=/init".to_owned();
    if nobridge {
        cmdline.push_str(" solon.nobridge");
    }
    let make_config = |id: String| VmConfig {
        id: id.clone(),
        name: "solon-engine".into(),
        kernel: kernel.clone(),
        initrd: initrd.clone(),
        cmdline: cmdline.clone(),
        memory_mb: mem,
        processors: 4,
        disks: vec![
            DiskAttachment {
                path: rootfs.clone(),
                read_only: true,
            },
            DiskAttachment {
                path: data.clone(),
                read_only: false,
            },
        ],
        shares: vec![],
        serial_pipe: Some(format!(r"\\.\pipe\solon-console-{}", &id[..8])),
        network_adapter: None,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();
    let mut boot_times = Vec::new();

    for cycle in 1..=2 {
        out.line(&format!("===== cycle {cycle} ====="));
        let config = make_config(new_vm_id());
        let r = match boot(&mut out, &config) {
            Ok(r) => r,
            Err(e) => {
                out.check(&format!("cycle {cycle} : démarrage"), false, &e);
                break;
            }
        };
        out.check(
            &format!("cycle {cycle} : moteur prêt"),
            r.engine_ready_ms.is_some(),
            &format!(
                "initrd {:?} ms, agent {:?} ms, moteur {:?} ms après HcsStart",
                r.initrd_ok_ms, r.agent_ready_ms, r.engine_ready_ms
            ),
        );
        if r.engine_ready_ms.is_none() {
            dump_console(&mut out, &r.console, 80);
            let _ = r.vm.terminate();
            break;
        }
        boot_times.push(r.engine_ready_ms.unwrap());

        let mut rpc = match Rpc::connect(&r.guid) {
            Ok(x) => x,
            Err(e) => {
                out.check("RPC", false, &e.to_string());
                let _ = r.vm.terminate();
                break;
            }
        };
        match rpc.call("HEALTH") {
            Ok(v) => out.check(
                "HEALTH",
                v["data"]["docker_ping"] == true,
                &v["data"].to_string(),
            ),
            Err(e) => out.check("HEALTH", false, &e.to_string()),
        }

        // Relais de l'API Docker sur le named pipe.
        let relay = rt.spawn(solon_hvsock::relay::serve_named_pipe(
            PIPE.into(),
            r.guid,
            5001,
        ));
        std::thread::sleep(Duration::from_millis(200));

        let (ok, txt, ms) = docker(
            &[
                "version",
                "--format",
                "{{.Server.Version}} api={{.Server.APIVersion}} os={{.Server.Os}}",
            ],
            None,
        );
        out.check(
            "docker version via \\\\.\\pipe\\solon",
            ok,
            &format!("{txt} ({ms} ms)"),
        );
        let (ok, txt, ms) = docker(
            &[
                "info",
                "--format",
                "{{.Driver}} cgroup={{.CgroupDriver}}/{{.CgroupVersion}} kernel={{.KernelVersion}} mem={{.MemTotal}}",
            ],
            None,
        );
        out.check("docker info", ok, &format!("{txt} ({ms} ms)"));

        if cycle == 1 {
            if let Some(bb) = &busybox {
                let data = std::fs::read(bb).expect("busybox");
                let tar = busybox_tar(&data);
                let (ok, txt, ms) = docker(&["import", "-", "solon/busybox:test"], Some(&tar));
                out.check(
                    "docker import (image de test)",
                    ok,
                    &format!("{txt} ({ms} ms)"),
                );
            }
        }
        let (ok, txt, ms) = docker(
            &["images", "--format", "{{.Repository}}:{{.Tag}} {{.Size}}"],
            None,
        );
        out.check(
            &format!("cycle {cycle} : docker images (persistance)"),
            ok && txt.contains("solon/busybox"),
            &format!("{} ({ms} ms)", txt.replace('\n', " | ")),
        );
        let (ok, txt, ms) = docker(
            &[
                "run",
                "--rm",
                "solon/busybox:test",
                "/bin/busybox",
                "echo",
                "bonjour-depuis-le-conteneur",
            ],
            None,
        );
        out.check(
            &format!("cycle {cycle} : docker run --rm"),
            ok && txt.contains("bonjour-depuis-le-conteneur"),
            &format!("{txt} ({ms} ms)"),
        );
        let (ok, txt, ms) = docker(
            &[
                "run",
                "-d",
                "--name",
                "solon-sleeper",
                "solon/busybox:test",
                "/bin/busybox",
                "sleep",
                "300",
            ],
            None,
        );
        out.check(
            "docker run -d",
            ok,
            &format!("{} ({ms} ms)", &txt[..txt.len().min(12)]),
        );
        let (ok, txt, _) = docker(&["ps", "--format", "{{.Names}} {{.Status}}"], None);
        out.check("docker ps", ok && txt.contains("solon-sleeper"), &txt);
        let (ok, _, ms) = docker(&["rm", "-f", "solon-sleeper"], None);
        out.check("docker rm -f", ok, &format!("{ms} ms"));

        relay.abort();
        let _ = rpc.call("POWEROFF 5");
        let exit = r.vm.wait_exit(Duration::from_secs(30));
        out.check(
            &format!("cycle {cycle} : arrêt propre"),
            exit.as_ref()
                .is_some_and(|e| e.data.as_deref().unwrap_or("").contains("GracefulExit")),
            &format!("{:?}", exit.as_ref().and_then(|e| e.data.clone())),
        );
        if exit.is_none() {
            let _ = r.vm.terminate();
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    out.line("---- RÉSUMÉ ----");
    for (i, t) in boot_times.iter().enumerate() {
        out.line(&format!(
            "cycle {} : moteur Docker prêt {t} ms après HcsStartComputeSystem",
            i + 1
        ));
    }
    out.line(&format!(
        "RÉSULTAT : {}",
        if out.failures == 0 { "OK" } else { "ÉCHEC" }
    ));
    out.line(&format!("échecs : {}", out.failures));
    std::process::exit(if out.failures == 0 { 0 } else { 1 });
}
