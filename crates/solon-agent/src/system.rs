//! Devoirs de PID 1 : montages, disque de données, réseau de base, supervision de containerd/dockerd,
//! récolte des processus orphelins, arrêt propre.

use std::collections::HashSet;
use std::ffi::CString;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DATA_DEVICE: &str = "/dev/sdb";
pub const DATA_MOUNT: &str = "/var/lib/solon";
pub const DOCKER_SOCK: &str = "/run/docker.sock";
pub const CONTAINERD_SOCK: &str = "/run/containerd/containerd.sock";

#[derive(Default)]
pub struct State {
    /// PID des processus que l'agent attend lui-même (à ne pas récolter par le glaneur).
    pub tracked: Mutex<HashSet<i32>>,
    pub data_disk: Mutex<Option<DataDiskReport>>,
    pub engine: Mutex<EngineStatus>,
    pub shutting_down: Mutex<bool>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DataDiskReport {
    pub device: String,
    pub formatted_now: bool,
    pub fsck_exit: i32,
    pub fsck_summary: String,
    pub mounted_at: String,
}

impl std::fmt::Display for DataDiskReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} monté sur {} (formaté maintenant : {}, fsck code {} : {})",
            self.device, self.mounted_at, self.formatted_now, self.fsck_exit, self.fsck_summary
        )
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EngineStatus {
    pub containerd_pid: Option<u32>,
    pub dockerd_pid: Option<u32>,
    pub docker_ready_at_uptime_s: Option<f64>,
    pub restarts: u32,
    pub last_error: Option<String>,
}

pub fn log(msg: &str) {
    eprintln!("[solon-agent {:8.3}] {msg}", uptime_secs());
}

pub fn uptime_secs() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0.0)
}

fn mount(src: &str, target: &str, fstype: &str, flags: libc::c_ulong, data: &str) -> Result<(), String> {
    let _ = fs::create_dir_all(target);
    let s = CString::new(src).unwrap();
    let t = CString::new(target).unwrap();
    let f = CString::new(fstype).unwrap();
    let d = CString::new(data).unwrap();
    let rc = unsafe { libc::mount(s.as_ptr(), t.as_ptr(), f.as_ptr(), flags, d.as_ptr() as *const libc::c_void) };
    if rc < 0 {
        Err(format!("mount {fstype} {src} → {target} : {}", std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

fn is_mounted(target: &str) -> bool {
    fs::read_to_string("/proc/self/mounts")
        .map(|m| m.lines().any(|l| l.split_whitespace().nth(1) == Some(target)))
        .unwrap_or(false)
}

pub fn mount_virtual_filesystems() {
    let std_flags = libc::MS_NOSUID | libc::MS_NOEXEC | libc::MS_NODEV;
    let attempts = [
        ("proc", "/proc", "proc", std_flags, ""),
        ("sysfs", "/sys", "sysfs", std_flags, ""),
        ("devtmpfs", "/dev", "devtmpfs", libc::MS_NOSUID, "mode=0755"),
        ("tmpfs", "/run", "tmpfs", libc::MS_NOSUID | libc::MS_NODEV, "mode=0755"),
        ("devpts", "/dev/pts", "devpts", libc::MS_NOSUID | libc::MS_NOEXEC, "gid=5,mode=0620,ptmxmode=0666"),
        ("tmpfs", "/dev/shm", "tmpfs", std_flags, "mode=1777"),
        ("mqueue", "/dev/mqueue", "mqueue", std_flags, ""),
        ("tmpfs", "/tmp", "tmpfs", libc::MS_NOSUID | libc::MS_NODEV, "mode=1777"),
        ("cgroup2", "/sys/fs/cgroup", "cgroup2", std_flags, "nsdelegate,memory_recursiveprot"),
        ("securityfs", "/sys/kernel/security", "securityfs", std_flags, ""),
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
    let _ = fs::write("/proc/sys/kernel/hostname", "solon\n");
    // Rend tous les contrôleurs cgroup v2 disponibles aux enfants (dockerd en a besoin).
    if let Ok(ctrls) = fs::read_to_string("/sys/fs/cgroup/cgroup.controllers") {
        let line: String = ctrls.split_whitespace().map(|c| format!("+{c} ")).collect();
        let _ = fs::write("/sys/fs/cgroup/cgroup.subtree_control", line.trim());
    }
}

pub fn configure_network_basics() {
    let _ = fs::write("/proc/sys/net/ipv4/ip_forward", "1\n");
    let _ = fs::write("/proc/sys/net/ipv4/conf/all/forwarding", "1\n");
    let _ = fs::write("/proc/sys/net/ipv6/conf/all/forwarding", "1\n");
    // Interface de boucle locale : sans elle, dockerd ne peut pas ouvrir ses sockets locaux.
    let _ = Command::new("/sbin/ip").args(["link", "set", "lo", "up"]).status();
    let _ = Command::new("/sbin/ip").args(["addr", "add", "127.0.0.1/8", "dev", "lo"]).stderr(Stdio::null()).status();
}

/// Cherche la signature ext4 (magic 0xEF53 à l'octet 0x438 du superbloc).
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
    while !Path::new(DATA_DEVICE).exists() {
        if Instant::now() > deadline {
            return Err(format!("{DATA_DEVICE} absent"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut report = DataDiskReport { device: DATA_DEVICE.into(), ..Default::default() };
    if !has_ext4_superblock(DATA_DEVICE)? {
        log("disque de données vierge : formatage ext4");
        let out = Command::new("/sbin/mkfs.ext4")
            .args(["-q", "-F", "-L", "solon-data", "-E", "lazy_itable_init=1,lazy_journal_init=1", DATA_DEVICE])
            .output()
            .map_err(|e| format!("mkfs.ext4 : {e}"))?;
        if !out.status.success() {
            return Err(format!("mkfs.ext4 a échoué : {}", String::from_utf8_lossy(&out.stderr)));
        }
        report.formatted_now = true;
    }
    // -p : réparations automatiques sans question ; codes 0 et 1 = sain (1 = corrigé).
    let fsck = Command::new("/sbin/e2fsck").args(["-p", DATA_DEVICE]).output().map_err(|e| format!("e2fsck : {e}"))?;
    report.fsck_exit = fsck.status.code().unwrap_or(-1);
    report.fsck_summary = String::from_utf8_lossy(&fsck.stdout).lines().last().unwrap_or("").trim().to_owned();
    if report.fsck_exit >= 4 {
        return Err(format!("e2fsck code {} : {}", report.fsck_exit, String::from_utf8_lossy(&fsck.stderr)));
    }
    mount(DATA_DEVICE, DATA_MOUNT, "ext4", libc::MS_NOATIME, "data=ordered")?;
    report.mounted_at = DATA_MOUNT.into();
    for (sub, target) in [("docker", "/var/lib/docker"), ("containerd", "/var/lib/containerd"), ("agent", "/var/lib/solon-agent")] {
        let src = format!("{DATA_MOUNT}/{sub}");
        let _ = fs::create_dir_all(&src);
        let _ = fs::create_dir_all(target);
        mount(&src, target, "", libc::MS_BIND, "")?;
    }
    *state.data_disk.lock().unwrap() = Some(report.clone());
    Ok(report)
}

/// Options du noyau `solon.*` : `solon.nobridge` désactive le réseau des conteneurs (tests sans
/// module bridge), `solon.debug` rend dockerd verbeux.
fn cmdline_flag(flag: &str) -> bool {
    fs::read_to_string("/proc/cmdline").map(|c| c.split_whitespace().any(|w| w == flag)).unwrap_or(false)
}

fn spawn_tracked(state: &State, mut cmd: Command) -> Result<Child, String> {
    let child = cmd.spawn().map_err(|e| format!("{:?} : {e}", cmd.get_program()))?;
    state.tracked.lock().unwrap().insert(child.id() as i32);
    Ok(child)
}

/// Exécute une commande en la protégeant du glaneur, renvoie sa sortie.
pub fn run_tracked(state: &State, mut cmd: Command) -> std::io::Result<std::process::Output> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    let child = cmd.spawn()?;
    let pid = child.id() as i32;
    state.tracked.lock().unwrap().insert(pid);
    let out = child.wait_with_output();
    state.tracked.lock().unwrap().remove(&pid);
    out
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
    let Ok(mut s) = UnixStream::connect(DOCKER_SOCK) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    if s.write_all(b"GET /_ping HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    matches!(s.read(&mut buf), Ok(n) if n > 0 && String::from_utf8_lossy(&buf[..n]).contains(" 200 "))
}

/// Lance containerd puis dockerd et les supervise dans un thread.
pub fn start_engine(state: &Arc<State>) {
    let st = state.clone();
    std::thread::spawn(move || supervise(st));
}

fn engine_commands() -> (Command, Command) {
    let mut containerd = Command::new("/usr/bin/containerd");
    containerd.args(["--config", "/etc/containerd/config.toml"]).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let mut dockerd = Command::new("/usr/bin/dockerd");
    dockerd.args(["--config-file", "/etc/docker/daemon.json", "--containerd", CONTAINERD_SOCK]);
    if cmdline_flag("solon.nobridge") {
        dockerd.args(["--bridge=none", "--iptables=false", "--ip6tables=false"]);
    }
    if cmdline_flag("solon.debug") {
        dockerd.arg("--debug");
    }
    dockerd.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
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
                println!("SOLON-ENGINE-READY uptime_s={up:.2}");
            }
            if !announced && Instant::now() > deadline {
                log("dockerd n'a pas répondu en 60 s");
                announced = true; // on cesse d'attendre, la supervision continue
            }
            match dockerd.try_wait() {
                Ok(Some(status)) => {
                    log(&format!("dockerd terminé : {status}"));
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
        log(&format!("redémarrage du moteur dans {backoff:?} (tentative {restarts})"));
        std::thread::sleep(backoff);
    }
}

/// Récolte les zombies ré-attachés à PID 1, sans voler les enfants attendus par l'agent.
pub fn start_reaper(state: Arc<State>) {
    std::thread::spawn(move || loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::waitid(libc::P_ALL, 0, &mut info, libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
        let pid = if rc == 0 { unsafe { info.si_pid() } } else { 0 };
        if pid > 0 && !state.tracked.lock().unwrap().contains(&pid) {
            let mut status = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
            continue;
        }
        std::thread::sleep(Duration::from_millis(100));
    });
}

/// Arrêt propre : conteneurs, dockerd, containerd, synchronisation, démontage, extinction.
pub fn shutdown(state: &State, timeout: Duration) -> ! {
    *state.shutting_down.lock().unwrap() = true;
    log("arrêt : conteneurs en cours");
    let mut cmd = Command::new("/usr/bin/docker");
    cmd.args(["-H", &format!("unix://{DOCKER_SOCK}"), "ps", "-q"]);
    if let Ok(out) = run_tracked(state, cmd) {
        let ids: Vec<String> = String::from_utf8_lossy(&out.stdout).split_whitespace().map(str::to_owned).collect();
        if !ids.is_empty() {
            let mut stop = Command::new("/usr/bin/docker");
            stop.args(["-H", &format!("unix://{DOCKER_SOCK}"), "stop", "-t", &timeout.as_secs().to_string()]).args(&ids);
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
    while Instant::now() < deadline && Path::new(DOCKER_SOCK).exists() && UnixStream::connect(DOCKER_SOCK).is_ok() {
        std::thread::sleep(Duration::from_millis(100));
    }
    unsafe { libc::sync() };
    for target in ["/var/lib/docker", "/var/lib/containerd", "/var/lib/solon-agent", DATA_MOUNT] {
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
