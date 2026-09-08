//! Devoirs de PID 1 : montages, disque de données, réseau de base, supervision de containerd/dockerd,
//! récolte des processus orphelins, montages 9P, exécution de commandes, arrêt propre.

use std::collections::HashSet;
use std::ffi::CString;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use monodon_core::protocol::{
    AgentEvent, DataDiskReport, EngineStatus, ExecResult, HealthReport, MountResult,
    MountShareRequest, PROTOCOL_VERSION,
};

use crate::events;
use crate::vsock;

/// Étiquette ext4 (16 octets à l'offset 1144) d'un périphérique bloc, vide si illisible.
fn ext4_label(dev: &str) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = fs::File::open(dev) else {
        return String::new();
    };
    if f.seek(SeekFrom::Start(1024 + 0x78)).is_err() {
        return String::new();
    }
    let mut buf = [0u8; 16];
    if f.read_exact(&mut buf).is_err() {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(16);
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

/// Disque de données : celui étiqueté « monodon-data », sinon le premier disque qui n'est pas la racine
/// (« monodon-root »), c'est-à-dire un disque vierge à formater. L'ordre `/dev/sda`/`/dev/sdb` n'est pas
/// garanti par l'énumération SCSI.
pub fn find_data_device() -> Option<String> {
    let mut blank: Option<String> = None;
    for c in b'a'..=b'z' {
        let dev = format!("/dev/sd{}", c as char);
        if !Path::new(&dev).exists() {
            continue;
        }
        match ext4_label(&dev).as_str() {
            // « solon-data » : disques créés avant le renommage du projet (8 septembre 2026).
            "monodon-data" | "solon-data" => return Some(dev),
            "monodon-root" | "solon-root" => {}
            _ => {
                if blank.is_none() {
                    blank = Some(dev);
                }
            }
        }
    }
    blank
}
pub const DATA_MOUNT: &str = "/var/lib/monodon";
pub const DOCKER_SOCK: &str = "/run/docker.sock";
pub const CONTAINERD_SOCK: &str = "/run/containerd/containerd.sock";

#[derive(Default)]
pub struct State {
    /// PID des processus que l'agent attend lui-même (à ne pas récolter par le glaneur).
    pub tracked: Mutex<HashSet<i32>>,
    pub data_disk: Mutex<Option<DataDiskReport>>,
    pub engine: Mutex<EngineStatus>,
    pub shutting_down: Mutex<bool>,
    pub network_configured: Mutex<bool>,
}

pub fn log(msg: &str) {
    eprintln!("[monodon-agent {:8.3}] {msg}", uptime_secs());
}

pub fn uptime_secs() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0.0)
}

fn mount(
    src: &str,
    target: &str,
    fstype: &str,
    flags: libc::c_ulong,
    data: &str,
) -> Result<(), String> {
    let _ = fs::create_dir_all(target);
    let s = CString::new(src).unwrap();
    let t = CString::new(target).unwrap();
    let f = CString::new(fstype).unwrap();
    let d = CString::new(data).unwrap();
    let rc = unsafe {
        libc::mount(
            s.as_ptr(),
            t.as_ptr(),
            f.as_ptr(),
            flags,
            d.as_ptr() as *const libc::c_void,
        )
    };
    if rc < 0 {
        Err(format!(
            "mount {fstype} {src} → {target} : {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

fn is_mounted(target: &str) -> bool {
    fs::read_to_string("/proc/self/mounts")
        .map(|m| {
            m.lines()
                .any(|l| l.split_whitespace().nth(1) == Some(target))
        })
        .unwrap_or(false)
}

/// Points de montage 9P actuels.
pub fn plan9_mounts() -> Vec<String> {
    fs::read_to_string("/proc/self/mounts")
        .map(|m| {
            m.lines()
                .filter(|l| l.split_whitespace().nth(2) == Some("9p"))
                .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub fn mount_virtual_filesystems() {
    let std_flags = libc::MS_NOSUID | libc::MS_NOEXEC | libc::MS_NODEV;
    let attempts = [
        ("proc", "/proc", "proc", std_flags, ""),
        ("sysfs", "/sys", "sysfs", std_flags, ""),
        ("devtmpfs", "/dev", "devtmpfs", libc::MS_NOSUID, "mode=0755"),
        (
            "tmpfs",
            "/run",
            "tmpfs",
            libc::MS_NOSUID | libc::MS_NODEV,
            "mode=0755",
        ),
        (
            "devpts",
            "/dev/pts",
            "devpts",
            libc::MS_NOSUID | libc::MS_NOEXEC,
            "gid=5,mode=0620,ptmxmode=0666",
        ),
        ("tmpfs", "/dev/shm", "tmpfs", std_flags, "mode=1777"),
        ("mqueue", "/dev/mqueue", "mqueue", std_flags, ""),
        (
            "tmpfs",
            "/tmp",
            "tmpfs",
            libc::MS_NOSUID | libc::MS_NODEV,
            "mode=1777",
        ),
        (
            "cgroup2",
            "/sys/fs/cgroup",
            "cgroup2",
            std_flags,
            "nsdelegate,memory_recursiveprot",
        ),
        (
            "securityfs",
            "/sys/kernel/security",
            "securityfs",
            std_flags,
            "",
        ),
    ];
    for (src, target, fstype, flags, data) in attempts {
        if is_mounted(target) {
            continue;
        }
        if let Err(e) = mount(src, target, fstype, flags, data) {
            log(&format!("AVERTISSEMENT : {e}"));
        }
    }
    let _ = fs::create_dir_all("/run/docker");
    let _ = fs::create_dir_all("/run/containerd");
    let _ = fs::write("/proc/sys/kernel/hostname", "monodon\n");
    if let Ok(ctrls) = fs::read_to_string("/sys/fs/cgroup/cgroup.controllers") {
        let line: String = ctrls.split_whitespace().map(|c| format!("+{c} ")).collect();
        let _ = fs::write("/sys/fs/cgroup/cgroup.subtree_control", line.trim());
    }
}

pub fn configure_network_basics() {
    let _ = fs::write("/proc/sys/net/ipv4/ip_forward", "1\n");
    let _ = fs::write("/proc/sys/net/ipv4/conf/all/forwarding", "1\n");
    let _ = fs::write("/proc/sys/net/ipv6/conf/all/forwarding", "1\n");
    let _ = Command::new("/sbin/ip")
        .args(["link", "set", "lo", "up"])
        .status();
    let _ = Command::new("/sbin/ip")
        .args(["addr", "add", "127.0.0.1/8", "dev", "lo"])
        .stderr(Stdio::null())
        .status();
}

fn has_ext4_superblock(device: &str) -> Result<bool, String> {
    let mut f = fs::File::open(device).map_err(|e| format!("ouverture {device} : {e}"))?;
    f.seek(SeekFrom::Start(0x438)).map_err(|e| e.to_string())?;
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic).map_err(|e| e.to_string())?;
    Ok(u16::from_le_bytes(magic) == 0xEF53)
}

/// Prépare le disque de données : formatage au premier démarrage, `fsck -p` ensuite, montage,
/// puis liaison de `/var/lib/docker` et `/var/lib/containerd` dessus.
pub fn prepare_data_disk(state: &State) -> Result<DataDiskReport, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let data_device = loop {
        if let Some(d) = find_data_device() {
            break d;
        }
        if Instant::now() > deadline {
            return Err("disque de données absent".to_owned());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let data_device = data_device.as_str();
    let mut report = DataDiskReport {
        device: data_device.into(),
        ..Default::default()
    };
    if !has_ext4_superblock(data_device)? {
        log("disque de données vierge : formatage ext4");
        let out = Command::new("/sbin/mkfs.ext4")
            .args([
                "-q",
                "-F",
                "-L",
                "monodon-data",
                "-E",
                "lazy_itable_init=1,lazy_journal_init=1",
                data_device,
            ])
            .output()
            .map_err(|e| format!("mkfs.ext4 : {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "mkfs.ext4 a échoué : {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        report.formatted_now = true;
    }
    let fsck = Command::new("/sbin/e2fsck")
        .args(["-p", data_device])
        .output()
        .map_err(|e| format!("e2fsck : {e}"))?;
    report.fsck_exit = fsck.status.code().unwrap_or(-1);
    report.fsck_summary = String::from_utf8_lossy(&fsck.stdout)
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .to_owned();
    if report.fsck_exit >= 4 {
        return Err(format!(
            "e2fsck code {} : {}",
            report.fsck_exit,
            String::from_utf8_lossy(&fsck.stderr)
        ));
    }
    mount(
        data_device,
        DATA_MOUNT,
        "ext4",
        libc::MS_NOATIME,
        "data=ordered",
    )?;
    report.mounted_at = DATA_MOUNT.into();
    for (sub, target) in [
        ("docker", "/var/lib/docker"),
        ("containerd", "/var/lib/containerd"),
        ("agent", "/var/lib/monodon-agent"),
    ] {
        let src = format!("{DATA_MOUNT}/{sub}");
        let _ = fs::create_dir_all(&src);
        let _ = fs::create_dir_all(target);
        mount(&src, target, "", libc::MS_BIND, "")?;
    }
    *state.data_disk.lock().unwrap() = Some(report.clone());
    Ok(report)
}

/// Remonte, avant dockerd, les lecteurs partagés annoncés par le service dans la ligne de commande du
/// noyau (`monodon.shares=c:9100,d:9101`), pour que les conteneurs qui montent `/mnt/host/<lettre>/…`
/// retrouvent leurs dossiers dès le démarrage.
pub fn mount_boot_shares() {
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let Some(spec) = cmdline
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix("monodon.shares="))
    else {
        return;
    };
    for entry in spec.split(',').filter(|e| !e.is_empty()) {
        let Some((drive, port)) = entry.split_once(':') else {
            continue;
        };
        let Ok(port) = port.parse::<u32>() else {
            continue;
        };
        // Le 9P de Windows reste disponible en secours sous /mnt/host9p ; monodonfs prend /mnt/host,
        // sauf en mode de repli (`monodon.fs=9p`) où le 9P garde /mnt/host.
        let nine_p_root = if crate::monodonfs::legacy_mode() {
            "/mnt/host"
        } else {
            "/mnt/host9p"
        };
        let req = MountShareRequest {
            name: drive.to_owned(),
            port,
            target: format!("{nine_p_root}/{drive}"),
            read_only: false,
            extra_options: String::new(),
        };
        // Le serveur 9P de l'hôte démarre avec la machine : deux essais suffisent.
        let mut last = String::new();
        for attempt in 0..3 {
            match mount_plan9(&req) {
                Ok(r) => {
                    log(&format!(
                        "partage {drive} remonté sur {} (vsock {port}, {} ms)",
                        req.target, r.mount_ms
                    ));
                    crate::monodonfs::mount(drive);
                    last.clear();
                    break;
                }
                Err(e) => {
                    last = e;
                    std::thread::sleep(std::time::Duration::from_millis(200 * (attempt + 1)));
                }
            }
        }
        if !last.is_empty() {
            log(&format!(
                "AVERTISSEMENT partage {drive} non remonté : {last}"
            ));
        }
    }
}

/// Monte un partage Plan9 HCS : connexion vsock vers l'hôte puis `mount -t 9p -o trans=fd`.
pub fn mount_plan9(req: &MountShareRequest) -> Result<MountResult, String> {
    fs::create_dir_all(&req.target).map_err(|e| format!("mkdir {} : {e}", req.target))?;
    let fd = vsock::connect_host(req.port)?;
    let sz: libc::c_int = 4 * 1024 * 1024;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_SNDBUF,
            &sz as *const _ as *const libc::c_void,
            4,
        );
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &sz as *const _ as *const libc::c_void,
            4,
        );
    }
    let mut data = format!("trans=fd,rfdno={fd},wfdno={fd},aname={}", req.name);
    if !req.extra_options.contains("msize=") {
        data.push_str(",msize=65536");
    }
    if !req.extra_options.is_empty() {
        data.push(',');
        data.push_str(&req.extra_options);
    }
    let flags = if req.read_only { libc::MS_RDONLY } else { 0 };
    let t0 = Instant::now();
    let src = CString::new("hcs-plan9").unwrap();
    let tgt = CString::new(req.target.as_str()).unwrap();
    let fstype = CString::new("9p").unwrap();
    let opts = CString::new(data.clone()).unwrap();
    let rc = unsafe {
        libc::mount(
            src.as_ptr(),
            tgt.as_ptr(),
            fstype.as_ptr(),
            flags,
            opts.as_ptr() as *const libc::c_void,
        )
    };
    unsafe { libc::close(fd) };
    if rc < 0 {
        return Err(format!(
            "mount 9p ({data}) : {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(MountResult {
        target: req.target.clone(),
        options: data,
        mount_ms: t0.elapsed().as_millis() as u64,
    })
}

pub fn umount(target: &str) -> Result<(), String> {
    let t = CString::new(target).unwrap();
    if unsafe { libc::umount2(t.as_ptr(), 0) } < 0 {
        Err(format!(
            "umount {target} : {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

fn cmdline_flag(flag: &str) -> bool {
    fs::read_to_string("/proc/cmdline")
        .map(|c| c.split_whitespace().any(|w| w == flag))
        .unwrap_or(false)
}

fn spawn_tracked(state: &State, mut cmd: Command) -> Result<Child, String> {
    let child = cmd
        .spawn()
        .map_err(|e| format!("{:?} : {e}", cmd.get_program()))?;
    state.tracked.lock().unwrap().insert(child.id() as i32);
    Ok(child)
}

/// Exécute une commande en la protégeant du glaneur, renvoie sa sortie.
pub fn run_tracked(state: &State, mut cmd: Command) -> std::io::Result<std::process::Output> {
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let child = cmd.spawn()?;
    let pid = child.id() as i32;
    state.tracked.lock().unwrap().insert(pid);
    let out = child.wait_with_output();
    state.tracked.lock().unwrap().remove(&pid);
    out
}

/// Exécute une ligne shell avec délai maximal (sortie capturée).
pub fn exec(state: &State, command: &str, timeout: Duration) -> ExecResult {
    let t0 = Instant::now();
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ExecResult {
                code: None,
                stdout: String::new(),
                stderr: e.to_string(),
                ms: 0,
                timed_out: false,
            };
        }
    };
    let pid = child.id() as i32;
    state.tracked.lock().unwrap().insert(pid);
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let out_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = so.read_to_end(&mut v);
        v
    });
    let err_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = se.read_to_end(&mut v);
        v
    });
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    state.tracked.lock().unwrap().remove(&pid);
    ExecResult {
        code: status.and_then(|s| s.code()),
        stdout: String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned(),
        stderr: String::from_utf8_lossy(&err_t.join().unwrap_or_default()).into_owned(),
        ms: t0.elapsed().as_millis() as u64,
        timed_out,
    }
}

fn wait_for_socket(path: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if UnixStream::connect(path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// Ping HTTP minimal de dockerd sur son socket Unix.
pub fn docker_ping() -> bool {
    let Ok(mut s) = UnixStream::connect(DOCKER_SOCK) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if s.write_all(b"GET /_ping HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 256];
    matches!(s.read(&mut buf), Ok(n) if n > 0 && String::from_utf8_lossy(&buf[..n]).contains(" 200 "))
}

pub fn health(state: &State) -> HealthReport {
    let mem = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let field = |k: &str| -> u64 {
        mem.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    HealthReport {
        protocol_version: PROTOCOL_VERSION,
        agent_version: env!("CARGO_PKG_VERSION").into(),
        kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_default()
            .trim()
            .to_owned(),
        uptime_s: uptime_secs(),
        docker_ping: docker_ping(),
        engine: state.engine.lock().unwrap().clone(),
        data_disk: state.data_disk.lock().unwrap().clone(),
        mem_total_kb: field("MemTotal"),
        mem_available_kb: field("MemAvailable"),
        network_configured: *state.network_configured.lock().unwrap(),
        mounts: plan9_mounts(),
    }
}

/// Lance containerd puis dockerd et les supervise dans un thread.
pub fn start_engine(state: &Arc<State>) {
    let st = state.clone();
    std::thread::spawn(move || supervise(st));
}

fn engine_commands() -> (Command, Command) {
    let mut containerd = Command::new("/usr/bin/containerd");
    containerd
        .args(["--config", "/etc/containerd/config.toml"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut dockerd = Command::new("/usr/bin/dockerd");
    dockerd.args([
        "--config-file",
        "/etc/docker/daemon.json",
        "--containerd",
        CONTAINERD_SOCK,
    ]);
    if cmdline_flag("monodon.nobridge") {
        dockerd.args(["--bridge=none", "--iptables=false", "--ip6tables=false"]);
    }
    if cmdline_flag("monodon.debug") {
        dockerd.arg("--debug");
    }
    dockerd
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    (containerd, dockerd)
}

fn supervise(state: Arc<State>) {
    let mut restarts = 0u32;
    loop {
        if *state.shutting_down.lock().unwrap() {
            return;
        }
        let (containerd_cmd, dockerd_cmd) = engine_commands();
        let mut containerd = match spawn_tracked(&state, containerd_cmd) {
            Ok(c) => c,
            Err(e) => {
                log(&format!("containerd : {e}"));
                state.engine.lock().unwrap().last_error = Some(e);
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        state.engine.lock().unwrap().containerd_pid = Some(containerd.id());
        if !wait_for_socket(CONTAINERD_SOCK, Duration::from_secs(20)) {
            log("containerd ne répond pas sur son socket");
        }
        let mut dockerd = match spawn_tracked(&state, dockerd_cmd) {
            Ok(c) => c,
            Err(e) => {
                log(&format!("dockerd : {e}"));
                state.engine.lock().unwrap().last_error = Some(e);
                let _ = containerd.kill();
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        state.engine.lock().unwrap().dockerd_pid = Some(dockerd.id());

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut announced = false;
        loop {
            if !announced && docker_ping() {
                announced = true;
                let up = uptime_secs();
                state.engine.lock().unwrap().docker_ready_at_uptime_s = Some(up);
                // Règles d'accès direct de l'hôte, réappliquées après chaque redémarrage de dockerd.
                crate::net::reapply_host_rules();
                println!("MONODON-ENGINE-READY uptime_s={up:.2}");
                events::broadcast(&AgentEvent::EngineReady { uptime_s: up });
            }
            if !announced && Instant::now() > deadline {
                log("dockerd n'a pas répondu en 60 s");
                announced = true;
            }
            match dockerd.try_wait() {
                Ok(Some(status)) => {
                    log(&format!("dockerd terminé : {status}"));
                    events::broadcast(&AgentEvent::EngineDown {
                        exit: status.to_string(),
                        restarts: restarts + 1,
                    });
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    log(&format!("try_wait dockerd : {e}"));
                    break;
                }
            }
            if let Ok(Some(status)) = containerd.try_wait() {
                log(&format!("containerd terminé : {status}"));
                let _ = dockerd.kill();
                let _ = dockerd.wait();
                events::broadcast(&AgentEvent::EngineDown {
                    exit: format!("containerd : {status}"),
                    restarts: restarts + 1,
                });
                break;
            }
            if *state.shutting_down.lock().unwrap() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        {
            let mut t = state.tracked.lock().unwrap();
            t.remove(&(dockerd.id() as i32));
            t.remove(&(containerd.id() as i32));
        }
        let _ = containerd.kill();
        let _ = containerd.wait();
        restarts += 1;
        {
            let mut e = state.engine.lock().unwrap();
            e.restarts = restarts;
            e.dockerd_pid = None;
            e.containerd_pid = None;
            e.docker_ready_at_uptime_s = None;
        }
        let backoff = Duration::from_secs((2u64.pow(restarts.min(5))).min(30));
        log(&format!(
            "redémarrage du moteur dans {backoff:?} (tentative {restarts})"
        ));
        std::thread::sleep(backoff);
    }
}

/// `sync()` périodique : borne la perte de données non synchronisées à ~2 s en cas de coupure
/// brutale (ext4 `data=ordered` ne valide sinon que toutes les 5 s). Coût négligeable au repos.
pub fn start_periodic_sync() {
    std::thread::spawn(|| {
        let mut tick = 0u32;
        let mut warned = false;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            unsafe { libc::sync() };
            tick += 1;
            // Toutes les 60 s : occupation du disque de données ; alerte à 90 %, réarmée sous 80 %.
            if tick % 30 == 0 {
                if let Some((used_pct, free_mb)) = disk_usage("/var/lib/monodon") {
                    if used_pct >= 90 && !warned {
                        warned = true;
                        crate::events::broadcast(
                            &monodon_core::protocol::AgentEvent::DiskPressure { used_pct, free_mb },
                        );
                        log(&format!(
                            "disque de données à {used_pct} % ({free_mb} Mo libres)"
                        ));
                    } else if used_pct < 80 {
                        warned = false;
                    }
                }
            }
        }
    });
}

/// Occupation d'un système de fichiers : (pourcentage utilisé, Mo libres).
pub fn disk_usage(path: &str) -> Option<(u8, u64)> {
    let c = CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let total = st.f_blocks as u64 * st.f_frsize as u64;
    let avail = st.f_bavail as u64 * st.f_frsize as u64;
    if total == 0 {
        return None;
    }
    let used_pct = (((total - avail) * 100) / total).min(100) as u8;
    Some((used_pct, avail / (1024 * 1024)))
}

/// Au repos (charge < 0,2 sur 1 min), libère le cache de pages toutes les 2 min pour que l'hôte
/// puisse récupérer la mémoire (hints HCS `EnableColdDiscardHint`). Coût : relecture du disque
/// racine à la prochaine activité, quelques dizaines de ms.
pub fn start_idle_cache_release() {
    // Toutes les 30 s, si la machine est calme : vider le cache de pages puis **compacter** la
    // mémoire pour former des blocs de 2 Mo libres, seuls signalables à l'hôte par le ballon
    // Hyper-V (« cold discard hint », ordre 9). Sans compactage, la mémoire libérée reste
    // fragmentée et l'hôte ne la récupère pas.
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let load1 = fs::read_to_string("/proc/loadavg")
                .ok()
                .and_then(|s| {
                    s.split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<f64>().ok())
                })
                .unwrap_or(1.0);
            if load1 < 0.5 {
                unsafe { libc::sync() };
                let _ = fs::write("/proc/sys/vm/drop_caches", "1\n");
                let _ = fs::write("/proc/sys/vm/compact_memory", "1\n");
            }
        }
    });
}

/// Récolte les zombies ré-attachés à PID 1, sans voler les enfants attendus par l'agent.
pub fn start_reaper(state: Arc<State>) {
    std::thread::spawn(move || {
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let rc = unsafe {
                libc::waitid(
                    libc::P_ALL,
                    0,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            let pid = if rc == 0 { unsafe { info.si_pid() } } else { 0 };
            if pid > 0 && !state.tracked.lock().unwrap().contains(&pid) {
                let mut status = 0;
                unsafe { libc::waitpid(pid, &mut status, 0) };
                continue;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Arrêt propre : conteneurs, dockerd, containerd, synchronisation, démontage, extinction.
pub fn shutdown(state: &State, timeout: Duration) -> ! {
    *state.shutting_down.lock().unwrap() = true;
    log("arrêt : conteneurs en cours");
    let mut cmd = Command::new("/usr/bin/docker");
    cmd.args(["-H", &format!("unix://{DOCKER_SOCK}"), "ps", "-q"]);
    if let Ok(out) = run_tracked(state, cmd) {
        let ids: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if !ids.is_empty() {
            let mut stop = Command::new("/usr/bin/docker");
            stop.args([
                "-H",
                &format!("unix://{DOCKER_SOCK}"),
                "stop",
                "-t",
                &timeout.as_secs().to_string(),
            ])
            .args(&ids);
            let _ = run_tracked(state, stop);
        }
    }
    let (d, c) = {
        let e = state.engine.lock().unwrap();
        (e.dockerd_pid, e.containerd_pid)
    };
    for pid in [d, c].into_iter().flatten() {
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline
        && Path::new(DOCKER_SOCK).exists()
        && UnixStream::connect(DOCKER_SOCK).is_ok()
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    unsafe { libc::sync() };
    for target in [
        "/var/lib/docker",
        "/var/lib/containerd",
        "/var/lib/monodon-agent",
        DATA_MOUNT,
    ] {
        let t = CString::new(target).unwrap();
        unsafe { libc::umount2(t.as_ptr(), libc::MNT_DETACH) };
    }
    unsafe { libc::sync() };
    log("extinction");
    unsafe { libc::reboot(libc::LINUX_REBOOT_CMD_POWER_OFF) };
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Relevé des compteurs de la machine pour l'écran Activité (voir [`MachineMetrics`]).
pub fn metrics() -> monodon_core::protocol::MachineMetrics {
    use monodon_core::protocol::MachineMetrics;
    let mut m = MachineMetrics {
        uptime_s: uptime_secs(),
        ..Default::default()
    };
    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        if let Some(line) = stat.lines().find(|l| l.starts_with("cpu ")) {
            let v: Vec<u64> = line
                .split_whitespace()
                .skip(1)
                .filter_map(|x| x.parse().ok())
                .collect();
            let total: u64 = v.iter().sum();
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            m.cpu_total_ticks = total;
            m.cpu_busy_ticks = total.saturating_sub(idle);
        }
        m.cpus = stat
            .lines()
            .filter(|l| l.starts_with("cpu") && !l.starts_with("cpu "))
            .count() as u32;
    }
    if let Ok(load) = fs::read_to_string("/proc/loadavg") {
        m.load1 = load
            .split_whitespace()
            .next()
            .and_then(|x| x.parse().ok())
            .unwrap_or(0.0);
    }
    if let Ok(mem) = fs::read_to_string("/proc/meminfo") {
        let field = |k: &str| -> u64 {
            mem.lines()
                .find(|l| l.starts_with(k))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
        };
        m.mem_total_kb = field("MemTotal");
        m.mem_available_kb = field("MemAvailable");
    }
    if let Ok(c) = CString::new(DATA_MOUNT) {
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 {
            let total = st.f_blocks as u64 * st.f_frsize as u64;
            let avail = st.f_bavail as u64 * st.f_frsize as u64;
            m.disk_total_bytes = total;
            m.disk_used_bytes = total.saturating_sub(avail);
        }
    }
    if let Ok(dev) = fs::read_to_string("/proc/net/dev") {
        if let Some(line) = dev.lines().find(|l| l.trim_start().starts_with("eth0:")) {
            let v: Vec<u64> = line
                .split(':')
                .nth(1)
                .unwrap_or("")
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            m.net_rx_bytes = v.first().copied().unwrap_or(0);
            m.net_tx_bytes = v.get(8).copied().unwrap_or(0);
        }
    }
    m.containers_running = crate::events::running_count();
    m
}
