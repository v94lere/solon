//! Protocole application ↔ service Windows, sur le named pipe [`CONTROL_PIPE`].
//!
//! JSON par lignes. Requêtes [`IpcRequest`] corrélées par `id`, réponses [`crate::protocol::Response`].
//! Après `Subscribe`, le service pousse des [`ServiceEvent`] (une ligne JSON chacun, champ `event`)
//! sur la même connexion, entrelacés avec les réponses (les réponses ont un champ `id`, pas les
//! événements).

use serde::{Deserialize, Serialize};

use crate::SolonError;
use crate::protocol::PortBinding;

pub const CONTROL_PIPE: &str = r"\\.\pipe\solon-control";
/// Pipe où le service expose l'API Docker (compatible `docker -H npipe:////./pipe/solon`).
pub const DOCKER_PIPE: &str = r"\\.\pipe\solon";
/// Terminal dans la machine (relais vers le port vsock `PORT_SHELL`).
pub const SHELL_PIPE: &str = r"\\.\pipe\solon-shell";
/// Exécution en flux (Compose) : relais vers le port vsock `PORT_EXEC`.
pub const EXEC_PIPE: &str = r"\\.\pipe\solon-exec";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    #[serde(flatten)]
    pub request: ServiceCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ServiceCommand {
    /// Instantané de l'état du moteur.
    Status,
    /// Démarre (provisionne si nécessaire). Répond dès que la demande est prise en compte ;
    /// suivre `Subscribe` pour la progression.
    Start,
    /// Arrêt propre (ou forcé).
    Stop {
        #[serde(default)]
        force: bool,
    },
    Restart,
    /// Rapport des prérequis système.
    Prerequisites,
    /// Abonne la connexion aux événements.
    Subscribe,
    GetSettings,
    SetSettings(Settings),
    /// Version du service.
    Version,
    /// Arrête le moteur puis le processus du service (mode console et tests uniquement ;
    /// refusé quand le service tourne sous le Gestionnaire de services).
    Quit,
    /// Exécute une commande shell dans la machine (diagnostic, Compose). Sortie capturée.
    Exec {
        command: String,
        #[serde(default)]
        timeout_s: Option<u64>,
    },
    /// Rend un chemin Windows visible dans la machine (partage 9P du lecteur, monté à la demande)
    /// et renvoie le chemin correspondant côté invité.
    EnsureShare {
        host_path: String,
    },
    /// Lecteurs actuellement partagés.
    ListShares,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareInfo {
    /// Lettre du lecteur en minuscule (`c`).
    pub drive: String,
    pub host_root: String,
    /// Point de montage côté invité (`/mnt/host/c`).
    pub guest_root: String,
    /// Chemin demandé, traduit côté invité.
    pub guest_path: String,
    pub mounted_now: bool,
}

/// Traduit un chemin Windows en chemin invité sous `/mnt/host/<lettre>/`.
pub fn guest_path_for(host_path: &str) -> Option<(String, String)> {
    let trimmed = host_path.trim().trim_start_matches(r"\\?\");
    let mut chars = trimmed.chars();
    let letter = chars.next()?.to_ascii_lowercase();
    if !letter.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }
    let rest: String = chars.collect::<String>().replace('\\', "/");
    let rest = rest.trim_start_matches('/');
    let guest = if rest.is_empty() {
        format!("/mnt/host/{letter}")
    } else {
        format!("/mnt/host/{letter}/{rest}")
    };
    Some((letter.to_string(), guest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduction_des_chemins_windows() {
        assert_eq!(
            guest_path_for(r"C:\Users\v\proj").unwrap(),
            ("c".into(), "/mnt/host/c/Users/v/proj".into())
        );
        assert_eq!(
            guest_path_for(r"D:\").unwrap(),
            ("d".into(), "/mnt/host/d".into())
        );
        assert_eq!(guest_path_for(r"\\?\C:\x").unwrap().1, "/mnt/host/c/x");
        assert!(guest_path_for(r"\\server\share").is_none());
        assert!(guest_path_for("relative").is_none());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Stopped,
    Starting,
    Ready,
    /// dockerd est tombé dans l'invité, l'agent le relance.
    Degraded,
    Stopping,
    Failed,
}

/// Étapes du provisionnement, dans l'ordre. Affichées à l'utilisateur pendant le démarrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionStep {
    CheckingPrerequisites,
    VerifyingImage,
    PreparingDataDisk,
    CleaningOrphans,
    CreatingNetwork,
    CreatingMachine,
    Booting,
    WaitingAgent,
    ConfiguringNetwork,
    WaitingEngine,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EngineSnapshot {
    pub state: Option<EngineState>,
    pub step: Option<ProvisionStep>,
    pub error: Option<SolonError>,
    pub vm_id: Option<String>,
    pub image_version: Option<String>,
    pub docker_pipe: Option<String>,
    /// Millisecondes entre l'ordre de démarrage et « moteur prêt », au dernier démarrage.
    pub last_boot_ms: Option<u64>,
    pub ready_since_unix_ms: Option<u64>,
    pub guest_address: Option<String>,
    pub published_ports: Vec<PortBinding>,
    /// Dernier arrêt non propre détecté au démarrage (état précédent laissé en marche).
    pub recovered_from_crash: bool,
    /// Le service s'est rattaché à une machine déjà en marche (redémarrage du service).
    #[serde(default)]
    pub reattached: bool,
    /// Le mandataire des domaines locaux (`*.solon.local` → 127.0.0.1:80) est actif.
    #[serde(default)]
    pub local_domains: bool,
    /// Le mandataire HTTPS (127.0.0.1:443, autorité locale) est actif.
    #[serde(default)]
    pub local_domains_tls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub memory_mb: u64,
    pub processors: u32,
    pub data_disk_gib: u64,
    /// Démarrer le moteur dès l'ouverture de session (sinon au premier lancement de l'app).
    pub autostart: bool,
    /// Repli : partager les dossiers Windows par le 9P de Windows (ancien mécanisme) au lieu de solonfs.
    #[serde(default)]
    pub legacy_file_sharing: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            processors: 4,
            data_disk_gib: 64,
            autostart: false,
            legacy_file_sharing: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ServiceEvent {
    State(EngineSnapshot),
    Container {
        action: String,
        id: String,
        name: String,
    },
    Ports {
        bindings: Vec<PortBinding>,
    },
    Log {
        level: String,
        message: String,
    },
    /// Disque de données du moteur presque plein.
    DiskPressure {
        used_pct: u8,
        free_mb: u64,
    },
}

/// Rapport des prérequis (produit par `solon-prereq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PrereqReport {
    pub ok: bool,
    pub items: Vec<PrereqItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrereqItem {
    /// Identifiant stable (clé de traduction côté interface).
    pub id: String,
    pub ok: bool,
    /// Bloquant : le moteur ne peut pas démarrer.
    pub blocking: bool,
    /// Détail technique (journaux, support).
    pub detail: String,
    pub code: Option<crate::ErrorCode>,
}
