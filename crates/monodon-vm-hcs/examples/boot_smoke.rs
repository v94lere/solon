//! Spike 0a : démarre un noyau Linux dans une VM HCS, lit la console série, mesure les temps.
//!
//! Usage (en Administrateur) :
//!   boot_smoke <vmlinuz> <initrd> [--mem MB] [--cpus N] [--cmdline "..."] [--log fichier]
//!
//! Sortie : la console de l'invité horodatée, puis un résumé des temps. Code de retour 0 si le
//! marqueur `MONODON-INIT-OK` a été vu et que la machine s'est arrêtée toute seule.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use monodon_core::vm::VmConfig;
use monodon_vm_hcs::{HcsVm, new_vm_id};

struct Args {
    kernel: PathBuf,
    initrd: PathBuf,
    mem: u64,
    cpus: u32,
    cmdline: String,
    log: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let kernel = PathBuf::from(it.next().expect("chemin du noyau attendu"));
    let initrd = PathBuf::from(it.next().expect("chemin de l'initrd attendu"));
    let mut a = Args {
        kernel,
        initrd,
        mem: 1024,
        cpus: 2,
        cmdline: "console=ttyS0,115200 8250_core.nr_uarts=1 panic=-1 pci=off rdinit=/init".into(),
        log: None,
    };
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--mem" => a.mem = it.next().unwrap().parse().unwrap(),
            "--cpus" => a.cpus = it.next().unwrap().parse().unwrap(),
            "--cmdline" => a.cmdline = it.next().unwrap(),
            "--log" => a.log = Some(PathBuf::from(it.next().unwrap())),
            other => panic!("option inconnue : {other}"),
        }
    }
    a
}

struct Out {
    file: Option<std::fs::File>,
    start: Instant,
}

impl Out {
    fn line(&mut self, s: &str) {
        let ms = self.start.elapsed().as_millis();
        let text = format!("[{ms:6} ms] {s}");
        println!("{text}");
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{text}");
        }
    }
}

/// Se connecte au named pipe de la console (vmwp est le serveur) avec réessais, puis relaie
/// chaque ligne au thread principal.
fn console_reader(pipe: String, tx: mpsc::Sender<String>, deadline: Instant) {
    let file = loop {
        match OpenOptions::new().read(true).write(true).open(&pipe) {
            Ok(f) => break Some(f),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = tx.send(format!("!! console : connexion impossible au pipe : {e}"));
                break None;
            }
        }
    };
    let Some(mut file) = file else { return };
    let _ = tx.send("!! console : connectée".to_owned());
    let mut buf = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = pending.drain(..=pos).collect();
                    let text = String::from_utf8_lossy(&line)
                        .trim_end_matches(['\r', '\n'])
                        .to_owned();
                    if tx.send(text).is_err() {
                        return;
                    }
                }
            }
            Err(_) => break,
        }
    }
    if !pending.is_empty() {
        let _ = tx.send(String::from_utf8_lossy(&pending).into_owned());
    }
    let _ = tx.send("!! console : fermée".to_owned());
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    let args = parse_args();
    let mut out = Out {
        file: args
            .log
            .as_ref()
            .map(|p| std::fs::File::create(p).expect("fichier de log")),
        start: Instant::now(),
    };
    out.line(&format!(
        "noyau={} initrd={} mem={}MB cpus={}",
        args.kernel.display(),
        args.initrd.display(),
        args.mem,
        args.cpus
    ));
    out.line(&format!("cmdline={}", args.cmdline));

    match HcsVm::terminate_orphans(None) {
        Ok(list) if list.is_empty() => out.line("aucune machine Monodon orpheline"),
        Ok(list) => out.line(&format!("orphelines terminées : {list:?}")),
        Err(e) => out.line(&format!("!! énumération des orphelines impossible : {e}")),
    }

    let id = new_vm_id();
    let pipe = format!(r"\\.\pipe\monodon-console-{}", &id[..8]);
    let config = VmConfig {
        id: id.clone(),
        name: "monodon-spike".into(),
        kernel: args.kernel.clone(),
        initrd: args.initrd.clone(),
        cmdline: args.cmdline.clone(),
        memory_mb: args.mem,
        processors: args.cpus,
        disks: vec![],
        shares: vec![],
        serial_pipe: Some(pipe.clone()),
        network_adapter: None,
    };

    let t_create = Instant::now();
    let vm = match HcsVm::create(&config) {
        Ok(vm) => vm,
        Err(e) => {
            out.line(&format!("!! ÉCHEC création : {e}"));
            std::process::exit(2);
        }
    };
    out.line(&format!(
        "créée id={id} en {} ms",
        t_create.elapsed().as_millis()
    ));

    let (tx, rx) = mpsc::channel::<String>();
    let reader_pipe = pipe.clone();
    let reader_deadline = Instant::now() + Duration::from_secs(10);
    std::thread::spawn(move || console_reader(reader_pipe, tx, reader_deadline));

    let t_start = Instant::now();
    if let Err(e) = vm.start() {
        out.line(&format!("!! ÉCHEC démarrage : {e}"));
        let _ = vm.terminate();
        std::process::exit(3);
    }
    let start_ms = t_start.elapsed().as_millis();
    out.line(&format!(
        "démarrée (HcsStartComputeSystem) en {start_ms} ms"
    ));

    let mut first_byte: Option<u128> = None;
    let mut init_start: Option<u128> = None;
    let mut init_ok: Option<u128> = None;
    let overall_deadline = t_start + Duration::from_secs(45);
    let mut exited = None;

    loop {
        while let Ok(line) = rx.try_recv() {
            if !line.starts_with("!!") && first_byte.is_none() {
                first_byte = Some(t_start.elapsed().as_millis());
            }
            if line.contains("MONODON-INIT-START") {
                init_start = Some(t_start.elapsed().as_millis());
            }
            if line.contains("MONODON-INIT-OK") {
                init_ok = Some(t_start.elapsed().as_millis());
            }
            out.line(&format!("  | {line}"));
        }
        if let Some(ev) = vm.wait_exit(Duration::from_millis(50)) {
            exited = Some(ev);
            break;
        }
        if Instant::now() > overall_deadline {
            break;
        }
    }
    // Vide ce qui reste de la console.
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(line) = rx.try_recv() {
        out.line(&format!("  | {line}"));
    }

    let exit_ms = exited
        .as_ref()
        .map(|e| e.at.duration_since(t_start).as_millis());
    match &exited {
        Some(ev) => out.line(&format!(
            "machine arrêtée d'elle-même après {} ms ; données : {:?}",
            exit_ms.unwrap(),
            ev.data
        )),
        None => {
            out.line("!! pas d'arrêt spontané dans le délai : terminaison forcée");
            match vm.properties() {
                Ok(p) => out.line(&format!("propriétés : {p}")),
                Err(e) => out.line(&format!("propriétés indisponibles : {e}")),
            }
            let _ = vm.terminate();
        }
    }

    out.line("---- RÉSUMÉ ----");
    out.line(&format!("HcsStartComputeSystem      : {start_ms} ms"));
    out.line(&format!(
        "premier octet console      : {}",
        first_byte.map_or("jamais".into(), |v| format!("{v} ms"))
    ));
    out.line(&format!(
        "init démarré (userspace)   : {}",
        init_start.map_or("jamais".into(), |v| format!("{v} ms"))
    ));
    out.line(&format!(
        "init terminé (MONODON-INIT-OK): {}",
        init_ok.map_or("jamais".into(), |v| format!("{v} ms"))
    ));
    out.line(&format!(
        "arrêt constaté par HCS     : {}",
        exit_ms.map_or("jamais".into(), |v| format!("{v} ms"))
    ));

    let ok = init_ok.is_some() && exited.is_some();
    out.line(if ok {
        "RÉSULTAT : OK"
    } else {
        "RÉSULTAT : ÉCHEC"
    });
    std::process::exit(if ok { 0 } else { 1 });
}
