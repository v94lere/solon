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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub memory_mb: u64,
    pub processors: u32,
    pub data_disk_gib: u64,
    /// Démarrer le moteur dès l'ouverture de session (sinon au premier lancement de l'app).
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            processors: 4,
            data_disk_gib: 64,
            autostart: false,
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
