//! Radiographie (vsock 5007) : charge les sondes eBPF dans le noyau, lit leur tampon circulaire
//! et pousse les faits observés vers l'hôte.
//!
//! Même forme que `events` : l'hôte se connecte, l'agent pousse des lignes JSON. Le port est
//! séparé parce que ce flux est bien plus bavard, et qu'une rafale d'échanges observés ne doit
//! retarder ni perdre un changement d'état du moteur.
//!
//! Le travail de l'agent, ici, est de rendre lisible ce que le noyau rapporte en chiffres : une
//! horloge depuis le démarrage devient une heure, un numéro de cgroup et une adresse IP
//! deviennent un conteneur, un service, un projet.
//!
//! Si le noyau refuse les sondes — vérificateur, tracefs absent, droits — on le dit une fois et
//! on s'arrête là : le reste de l'agent continue sans rien savoir de cet échec.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aya::maps::RingBuf;
use aya::programs::TracePoint;
use solon_core::protocol::{ConnectionOutcome, PORT_TRACE, TraceConnection, TraceEvent, TracePeer};
use solon_trace_common::{AF_INET, KIND_CLOSE, KIND_CONNECT, KIND_CONNECT_FAILED, RawEvent};

use crate::system::log;
use crate::vsock;

/// Objet BPF compilé hors de l'arbre Windows (voir `crates/solon-trace-ebpf/build.ps1`) et
/// versionné dans le dépôt : bpf-linker ne tourne pas sous Windows, donc la compilation croisée
/// de l'agent ne peut pas le produire.
///
/// `include_bytes_aligned` et non `include_bytes` : la lecture de l'ELF se fait sans copie et
/// exige un tampon aligné. Avec `include_bytes`, l'alignement n'est que de un octet et le
/// chargement échoue ou non selon la taille de l'objet — une panne qui apparaît à la première
/// sonde ajoutée, loin de sa cause.
static OBJECT: &[u8] = aya::include_bytes_aligned!("bpf/solon-trace.o");

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
            .program_mut("solon_sock_state")
            .ok_or("programme solon_sock_state absent de l'objet BPF")?
            .try_into()
            .map_err(|e| format!("solon_sock_state n'est pas un tracepoint : {e}"))?;
        program
            .load()
            .map_err(|e| format!("vérificateur du noyau (solon_sock_state) : {e}"))?;
        program
            .attach("sock", "inet_sock_set_state")
            .map_err(|e| format!("attachement inet_sock_set_state : {e}"))?;
    }

    let map = bpf.map_mut("EVENTS").ok_or("tampon EVENTS absent")?;
    let mut ring =
        RingBuf::try_from(map).map_err(|e| format!("tampon EVENTS inutilisable : {e}"))?;

    set_status(true, "sondes en place");
    log("radiographie : sondes chargées");

    let clock = Clock::new();
    let mut names = Names::default();
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
                if let Some(event) = translate(&raw, &clock, &mut names) {
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

fn translate(raw: &RawEvent, clock: &Clock, names: &mut Names) -> Option<TraceEvent> {
    let outcome = match raw.kind {
        KIND_CONNECT => ConnectionOutcome::Established,
        KIND_CONNECT_FAILED => ConnectionOutcome::Failed,
        KIND_CLOSE => ConnectionOutcome::Closed,
        _ => return None,
    };
    let src_ip = format_ip(&raw.saddr, raw.family);
    let dst_ip = format_ip(&raw.daddr, raw.family);
    Some(TraceEvent::Connection(Box::new(TraceConnection {
        ts_ms: clock.to_epoch_ms(raw.ts_ns),
        outcome,
        // L'appelant est connu par son cgroup, relevé dans le contexte de son processus : plus
        // sûr que son adresse, qui est celle de la machine quand le conteneur partage la pile
        // réseau.
        src: names.peer(src_ip, Some(raw.cgroup_id)),
        dst: names.peer(dst_ip, None),
        sport: raw.sport,
        dport: raw.dport,
        comm: String::from_utf8_lossy(raw.comm_bytes()).into_owned(),
        pid: raw.pid,
        duration_ms: (raw.kind == KIND_CLOSE).then_some(raw.duration_ns / 1_000_000),
    })))
}

/// Rend l'adresse telle qu'on l'écrit. Le programme du noyau a déjà ramené les adresses IPv4 en
/// tête des 16 octets, il ne reste qu'à choisir la lecture.
fn format_ip(bytes: &[u8; 16], family: u16) -> String {
    if family == AF_INET {
        Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string()
    } else {
        Ipv6Addr::from(*bytes).to_string()
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
/// Un identifiant inconnu peut venir d'un conteneur qui vient de démarrer : on relit. Mais pas à
/// chaque fait — une extrémité hors conteneur resterait inconnue pour toujours et ferait relire
/// en boucle.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

/// Met un nom sur les deux extrémités d'une connexion.
///
/// Deux sources, parce qu'aucune ne suffit seule : le numéro de cgroup, que le noyau donne pour
/// l'appelant et qui reste juste même quand le conteneur partage la pile réseau de la machine ;
/// et la liste des conteneurs en marche avec leur adresse, tenue par `events`, seule façon de
/// nommer l'appelé.
#[derive(Default)]
struct Names {
    /// Inode du cgroup → identifiant court du conteneur.
    by_cgroup: HashMap<u64, String>,
    /// Adresse → conteneur en marche.
    by_ip: HashMap<String, solon_core::protocol::ContainerEndpoint>,
    last_scan: Option<Instant>,
}

impl Names {
    fn peer(&mut self, ip: String, cgroup_id: Option<u64>) -> TracePeer {
        if self.stale() {
            self.rescan();
        }
        let by_cgroup = cgroup_id.and_then(|id| self.by_cgroup.get(&id)).cloned();
        // L'adresse d'abord — elle donne la fiche complète — mais seulement si elle ne contredit
        // pas le cgroup, qui lui ne se trompe pas.
        let endpoint = self
            .by_ip
            .get(&ip)
            .filter(|e| by_cgroup.as_ref().is_none_or(|id| e.id.starts_with(id)))
            .or_else(|| {
                let id = by_cgroup.as_deref()?;
                self.by_ip.values().find(|e| e.id.starts_with(id))
            });
        match endpoint {
            Some(e) => TracePeer {
                ip,
                container_id: Some(short(&e.id)),
                name: Some(e.name.clone()),
                project: e.compose_project.clone(),
                service: e.compose_service.clone(),
            },
            // Hors de la machine : le relais DNS connaît peut-être le nom qui a été demandé.
            None => TracePeer {
                name: crate::dns::name_of(&ip),
                ip,
                container_id: by_cgroup,
                ..Default::default()
            },
        }
    }

    fn stale(&self) -> bool {
        self.last_scan
            .is_none_or(|t| t.elapsed() >= RESCAN_INTERVAL)
    }

    fn rescan(&mut self) {
        self.last_scan = Some(Instant::now());
        self.by_ip = crate::events::current_endpoints()
            .into_iter()
            .filter(|e| !e.ip.is_empty())
            .map(|e| (e.ip.clone(), e))
            .collect();
        self.by_cgroup.clear();
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
                            self.by_cgroup.insert(meta.ino(), id);
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

/// Identifiant court, comme `docker ps` l'affiche.
fn short(id: &str) -> String {
    id.chars().take(12).collect()
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
    hex.then(|| short(full))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solon_core::protocol::ContainerEndpoint;

    fn names() -> Names {
        // `last_scan` renseigné : les tests ne doivent pas aller lire `/sys/fs/cgroup`.
        Names {
            last_scan: Some(Instant::now()),
            ..Default::default()
        }
    }

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

    #[test]
    fn adresses_v4_et_v6() {
        let mut v4 = [0u8; 16];
        v4[..4].copy_from_slice(&[172, 17, 0, 3]);
        assert_eq!(format_ip(&v4, AF_INET), "172.17.0.3");
        let mut v6 = [0u8; 16];
        v6[0] = 0xfd;
        v6[15] = 1;
        assert_eq!(format_ip(&v6, solon_trace_common::AF_INET6), "fd00::1");
    }

    /// Une extrémité inconnue garde son adresse : mieux vaut une adresse nue qu'un trou.
    #[test]
    fn extremite_inconnue_garde_son_adresse() {
        let peer = names().peer("1.1.1.1".to_owned(), None);
        assert_eq!(peer.ip, "1.1.1.1");
        assert_eq!(peer.container_id, None);
        assert_eq!(peer.label(), "1.1.1.1");
    }

    #[test]
    fn extremite_reconnue_par_son_adresse() {
        let mut names = names();
        names.by_ip.insert(
            "172.18.0.4".to_owned(),
            ContainerEndpoint {
                id: "f".repeat(64),
                name: "boutique-db-1".to_owned(),
                compose_project: Some("boutique".to_owned()),
                compose_service: Some("db".to_owned()),
                ip: "172.18.0.4".to_owned(),
                ..Default::default()
            },
        );
        let peer = names.peer("172.18.0.4".to_owned(), None);
        assert_eq!(peer.container_id.as_deref(), Some("ffffffffffff"));
        assert_eq!(peer.project.as_deref(), Some("boutique"));
        assert_eq!(peer.label(), "db");
    }

    /// Conteneur en réseau partagé : son adresse est celle de la machine et ne figure dans aucune
    /// fiche. Le cgroup, lui, reste juste.
    #[test]
    fn appelant_reconnu_par_son_cgroup_seul() {
        let mut names = names();
        names.by_cgroup.insert(94, "aaaaaaaaaaaa".to_owned());
        let peer = names.peer("10.0.0.2".to_owned(), Some(94));
        assert_eq!(peer.container_id.as_deref(), Some("aaaaaaaaaaaa"));
        assert_eq!(peer.name, None);
    }

    /// Deux conteneurs peuvent se succéder sur la même adresse. Le cgroup tranche.
    #[test]
    fn le_cgroup_l_emporte_sur_une_adresse_perimee() {
        let mut names = names();
        names.by_cgroup.insert(94, "aaaaaaaaaaaa".to_owned());
        names.by_ip.insert(
            "172.18.0.4".to_owned(),
            ContainerEndpoint {
                id: "b".repeat(64),
                name: "autre".to_owned(),
                ip: "172.18.0.4".to_owned(),
                ..Default::default()
            },
        );
        let peer = names.peer("172.18.0.4".to_owned(), Some(94));
        assert_eq!(peer.container_id.as_deref(), Some("aaaaaaaaaaaa"));
        assert_eq!(peer.name, None, "la fiche de l'autre conteneur est écartée");
    }
}
