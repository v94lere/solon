//! Radiographie (vsock 5007) : charge les sondes eBPF dans le noyau, lit leur tampon circulaire
//! et pousse les faits observés vers l'hôte.
//!
//! Même forme que `events` : l'hôte se connecte, l'agent pousse des lignes JSON. Le port est
//! séparé parce que ce flux est bien plus bavard, et qu'une rafale d'échanges observés ne doit
//! retarder ni perdre un changement d'état du moteur.
//!
//! Si le noyau refuse les sondes — vérificateur, tracefs absent, droits — on le dit une fois et
//! on s'arrête là : le reste de l'agent continue sans rien savoir de cet échec.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aya::maps::RingBuf;
use aya::programs::TracePoint;
use solon_core::protocol::{PORT_TRACE, TraceEvent};
use solon_trace_common::{KIND_EXEC, RawEvent};

use crate::system::log;
use crate::vsock;

/// Objet BPF compilé hors de l'arbre Windows (voir `crates/solon-trace-ebpf/build.ps1`) et
/// versionné dans le dépôt : bpf-linker ne tourne pas sous Windows, donc la compilation croisée
/// de l'agent ne peut pas le produire.
static OBJECT: &[u8] = include_bytes!("bpf/solon-trace.o");

struct Hub {
    clients: Mutex<Vec<File>>,
    status: Mutex<TraceEvent>,
}

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| Hub {
        clients: Mutex::new(Vec::new()),
        status: Mutex::new(TraceEvent::Status {
            active: false,
            detail: "sondes en cours de chargement".to_owned(),
        }),
    })
}

fn write_event(client: &mut File, event: &TraceEvent) -> std::io::Result<()> {
    let mut line = serde_json::to_string(event).unwrap_or_default();
    line.push('\n');
    client.write_all(line.as_bytes())?;
    client.flush()
}

fn broadcast(event: &TraceEvent) {
    let mut clients = hub().clients.lock().unwrap();
    clients.retain_mut(|c| write_event(c, event).is_ok());
}

fn set_status(active: bool, detail: impl Into<String>) {
    let status = TraceEvent::Status {
        active,
        detail: detail.into(),
    };
    *hub().status.lock().unwrap() = status.clone();
    broadcast(&status);
}

pub fn serve() {
    std::thread::spawn(collect);

    let listen_fd = match vsock::listen(PORT_TRACE) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("radiographie : {e}"));
            return;
        }
    };
    log(&format!("radiographie à l'écoute (vsock {PORT_TRACE})"));
    loop {
        match vsock::accept(listen_fd) {
            Ok(mut client) => {
                // L'abonné qui arrive doit savoir tout de suite si les sondes sont en place,
                // sinon il attend indéfiniment des événements qui ne viendront jamais.
                let status = hub().status.lock().unwrap().clone();
                if write_event(&mut client, &status).is_ok() {
                    hub().clients.lock().unwrap().push(client);
                }
            }
            Err(e) => {
                log(&format!("accept radiographie : {e}"));
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Charge les sondes puis boucle sur leur tampon. Ne revient qu'en cas d'échec du chargement.
fn collect() {
    if let Err(e) = run() {
        log(&format!("radiographie désactivée : {e}"));
        set_status(false, e);
    }
}

fn run() -> Result<(), String> {
    // aya lit l'identifiant du tracepoint dans tracefs ; sans ce montage, l'attachement échoue
    // avec une erreur qui ne dit pas pourquoi.
    crate::system::ensure_tracefs()?;

    let mut bpf = aya::EbpfLoader::new()
        .load(OBJECT)
        .map_err(|e| format!("chargement de l'objet BPF : {e}"))?;

    {
        let program: &mut TracePoint = bpf
            .program_mut("solon_exec")
            .ok_or("programme solon_exec absent de l'objet BPF")?
            .try_into()
            .map_err(|e| format!("solon_exec n'est pas un tracepoint : {e}"))?;
        program
            .load()
            .map_err(|e| format!("vérificateur du noyau (solon_exec) : {e}"))?;
        program
            .attach("sched", "sched_process_exec")
            .map_err(|e| format!("attachement sched_process_exec : {e}"))?;
    }

    let map = bpf.map_mut("EVENTS").ok_or("tampon EVENTS absent")?;
    let mut ring =
        RingBuf::try_from(map).map_err(|e| format!("tampon EVENTS inutilisable : {e}"))?;

    set_status(true, "sondes en place");
    log("radiographie : sondes chargées");

    let clock = Clock::new();
    let mut cgroups = Cgroups::default();
    let fd = ring.as_raw_fd();
    loop {
        // Le tampon ne réveille l'agent que lorsqu'il y a quelque chose à lire ; le délai de
        // 1 s n'est là que pour ne pas rester bloqué indéfiniment si tout est silencieux.
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut pfd, 1, 1000) } < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("poll sur le tampon : {err}"));
        }
        while let Some(item) = ring.next() {
            if let Some(raw) = decode(&item) {
                if let Some(event) = translate(&raw, &clock, &mut cgroups) {
                    broadcast(&event);
                }
            }
        }
    }
}

/// Relit les octets écrits par le noyau. `read_unaligned` parce que rien ne garantit l'alignement
/// de l'entrée dans le tampon, et que la structure vient d'un binaire compilé séparément.
fn decode(bytes: &[u8]) -> Option<RawEvent> {
    if bytes.len() < RawEvent::SIZE {
        return None;
    }
    Some(unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const RawEvent) })
}

fn translate(raw: &RawEvent, clock: &Clock, cgroups: &mut Cgroups) -> Option<TraceEvent> {
    match raw.kind {
        KIND_EXEC => Some(TraceEvent::Exec {
            ts_ms: clock.to_epoch_ms(raw.ts_ns),
            pid: raw.pid,
            comm: String::from_utf8_lossy(raw.comm_bytes()).into_owned(),
            container_id: cgroups.container_of(raw.cgroup_id),
        }),
        _ => None,
    }
}

/// Convertit l'horloge du noyau — monotone depuis le démarrage de la machine — en millisecondes
/// depuis l'époque Unix, seule échelle qui ait un sens côté hôte.
struct Clock {
    /// Époque Unix, en millisecondes, correspondant à `ktime` 0.
    origin_ms: i64,
}

impl Clock {
    fn new() -> Self {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        let mono_ms = ts.tv_sec * 1000 + ts.tv_nsec / 1_000_000;
        let wall_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            origin_ms: wall_ms - mono_ms,
        }
    }

    fn to_epoch_ms(&self, ts_ns: u64) -> u64 {
        (self.origin_ms + (ts_ns / 1_000_000) as i64).max(0) as u64
    }
}

const CGROUP_ROOT: &str = "/sys/fs/cgroup";
/// Un identifiant inconnu peut venir d'un conteneur qui vient de démarrer : on relit l'arbre.
/// Mais pas à chaque événement — un processus hors conteneur resterait inconnu pour toujours et
/// ferait relire l'arbre en boucle.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

/// Correspondance identifiant de cgroup → conteneur.
///
/// Le noyau ne donne que le numéro d'inode du cgroup. dockerd place chaque conteneur dans
/// `/sys/fs/cgroup/docker/<id>`, il suffit donc de parcourir l'arbre et de relever les inodes.
#[derive(Default)]
struct Cgroups {
    by_inode: HashMap<u64, String>,
    last_scan: Option<Instant>,
}

impl Cgroups {
    fn container_of(&mut self, cgroup_id: u64) -> Option<String> {
        if let Some(id) = self.by_inode.get(&cgroup_id) {
            return Some(id.clone());
        }
        let stale = self
            .last_scan
            .is_none_or(|t| t.elapsed() >= RESCAN_INTERVAL);
        if stale {
            self.rescan();
            if let Some(id) = self.by_inode.get(&cgroup_id) {
                return Some(id.clone());
            }
        }
        None
    }

    fn rescan(&mut self) {
        self.last_scan = Some(Instant::now());
        self.by_inode.clear();
        let mut queue = vec![PathBuf::from(CGROUP_ROOT)];
        // Les conteneurs sont à faible profondeur (`docker/<id>`), mais chacun a des sous-cgroups ;
        // la borne évite de parcourir tout l'arbre d'un conteneur très découpé.
        let mut depth = 0;
        while !queue.is_empty() && depth < 4 {
            let mut next = Vec::new();
            for dir in queue.drain(..) {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                        continue;
                    }
                    if let Some(id) = container_id_of(&path) {
                        if let Ok(meta) = std::fs::metadata(&path) {
                            self.by_inode.insert(meta.ino(), id);
                        }
                        // Les sous-cgroups d'un conteneur appartiennent au même conteneur.
                        continue;
                    }
                    next.push(path);
                }
            }
            queue = next;
            depth += 1;
        }
    }
}

/// Reconnaît le dossier de cgroup d'un conteneur et en extrait l'identifiant court.
///
/// Deux écritures circulent : `<id>` tel quel avec le pilote cgroupfs (celui de Solon) et
/// `docker-<id>.scope` avec le pilote systemd, qu'on accepte pour ne pas dépendre du réglage.
fn container_id_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let full = name
        .strip_prefix("docker-")
        .and_then(|r| r.strip_suffix(".scope"))
        .unwrap_or(name);
    let hex = full.len() == 64 && full.bytes().all(|b| b.is_ascii_hexdigit());
    hex.then(|| full[..12].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiant_de_conteneur_reconnu() {
        let id = "a".repeat(64);
        assert_eq!(
            container_id_of(Path::new(&format!("/sys/fs/cgroup/docker/{id}"))),
            Some("aaaaaaaaaaaa".to_owned())
        );
        assert_eq!(
            container_id_of(Path::new(&format!("/sys/fs/cgroup/docker-{id}.scope"))),
            Some("aaaaaaaaaaaa".to_owned())
        );
    }

    #[test]
    fn dossiers_ordinaires_ignores() {
        for p in [
            "/sys/fs/cgroup",
            "/sys/fs/cgroup/docker",
            "/sys/fs/cgroup/init.scope",
        ] {
            assert_eq!(container_id_of(Path::new(p)), None, "{p}");
        }
        // 64 caractères, mais pas tous hexadécimaux.
        let faux = format!("/sys/fs/cgroup/docker/{}", "z".repeat(64));
        assert_eq!(container_id_of(Path::new(&faux)), None);
    }

    #[test]
    fn horloge_recale_sur_l_epoque() {
        let clock = Clock { origin_ms: 1_000 };
        assert_eq!(clock.to_epoch_ms(2_000_000_000), 3_000);
        // Une origine négative ne doit pas déborder en arrière.
        let clock = Clock { origin_ms: -10 };
        assert_eq!(clock.to_epoch_ms(0), 0);
    }

    #[test]
    fn evenement_trop_court_rejete() {
        assert!(decode(&[0u8; RawEvent::SIZE - 1]).is_none());
        assert!(decode(&[0u8; RawEvent::SIZE]).is_some());
    }
}
