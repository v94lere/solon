//! Protocole hôte ↔ agent invité, partagé par le service Windows et `solon-agent`.
//!
//! Transport : sockets Hyper-V (`AF_HYPERV` côté hôte, `AF_VSOCK` côté invité). Trois ports :
//!
//! | Port | Sens | Contenu |
//! |---|---|---|
//! | [`PORT_CONTROL`] | hôte → invité | requêtes/réponses JSON, une par ligne (`\n`), corrélées par `id` |
//! | [`PORT_DOCKER`] | hôte → invité | octets bruts relayés vers `/run/docker.sock` |
//! | [`PORT_PORTS`] | hôte → invité | relais d'un port publié : une ligne [`PortRelayHeader`], une ligne [`PortRelayAck`], puis octets bruts |
//! | [`PORT_EVENTS`] | hôte → invité | l'hôte se connecte, l'invité pousse des [`AgentEvent`] JSON, un par ligne |
//! | [`PORT_TRACE`] | hôte → invité | idem, mais des [`TraceEvent`] : flux séparé car bien plus bavard |
//!
//! Toutes les connexions sont initiées par l'hôte. Le format JSON par lignes est volontairement
//! simple : la latence mesurée (~0,5 ms par aller-retour) rend inutile un protocole binaire.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const PORT_CONTROL: u32 = 5000;
pub const PORT_DOCKER: u32 = 5001;
pub const PORT_PORTS: u32 = 5002;
pub const PORT_EVENTS: u32 = 5003;
/// Terminal interactif dans la machine (hôte → agent : en-tête JSON puis trames ; agent → hôte : octets bruts du TTY).
pub const PORT_SHELL: u32 = 5004;
/// Exécution d'une commande avec sortie en flux (Compose) : en-tête JSON puis trames agent → hôte.
pub const PORT_EXEC: u32 = 5005;
/// Radiographie : l'hôte se connecte, l'agent pousse des [`TraceEvent`]. Séparé de
/// [`PORT_EVENTS`] parce qu'une rafale d'échanges observés ne doit pas retarder un changement
/// d'état du moteur, ni être perdue avec lui si l'abonné décroche.
pub const PORT_TRACE: u32 = 5007;

/// Trames agent → hôte du canal d'exécution en flux : `[type, len_hi, len_lo, charge]`.
pub const EXEC_FRAME_STDOUT: u8 = 0;
pub const EXEC_FRAME_STDERR: u8 = 1;
/// Charge : code de sortie en décimal ; dernière trame.
pub const EXEC_FRAME_EXIT: u8 = 2;

/// Première ligne envoyée par l'hôte sur le canal d'exécution en flux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecStreamRequest {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Type de trame hôte → agent sur le canal terminal : `[type, len_hi, len_lo, charge utile]`.
pub const SHELL_FRAME_INPUT: u8 = 0;
pub const SHELL_FRAME_RESIZE: u8 = 1;

/// Première ligne envoyée par l'hôte à l'ouverture d'un terminal dans la machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellHeader {
    pub cols: u16,
    pub rows: u16,
    /// Commande à lancer dans le pseudo-terminal (`sh -lc <command>`) au lieu d'un shell interactif.
    #[serde(default)]
    pub command: Option<String>,
}

/// Version du protocole ; l'agent la renvoie dans [`HealthReport`], le service refuse une
/// version majeure différente.
pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------------------------
// Requêtes et réponses (port de contrôle)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    /// État de l'agent, du moteur et du disque de données.
    Health,
    /// Configure l'interface réseau `eth0` (adresse statique fournie par l'hôte, comme WSL2).
    ConfigureNetwork(NetworkConfig),
    /// Monte un partage 9P de l'hôte.
    MountShare(MountShareRequest),
    UnmountShare {
        target: String,
    },
    /// Exécute une commande shell (diagnostic et Compose). Sortie capturée.
    Exec {
        command: String,
        #[serde(default)]
        timeout_s: Option<u64>,
    },
    /// Micro-banc d'entrées/sorties dans un dossier.
    Bench {
        dir: String,
    },
    /// Arrêt propre : conteneurs, moteur, synchronisation, extinction.
    Shutdown {
        #[serde(default = "default_shutdown_timeout")]
        timeout_s: u64,
    },
    /// Demande la liste courante des ports publiés (l'agent la pousse aussi en événement).
    ListPorts,
    /// Compteurs bruts de la machine (processeur, mémoire, disque des données, réseau) pour
    /// l'écran Activité ; l'appelant calcule les débits par différence entre deux appels.
    Metrics,
}

/// Compteurs de la machine à un instant donné. Les compteurs processeur et réseau sont cumulés
/// depuis le démarrage : les pourcentages et débits se déduisent de deux relevés.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MachineMetrics {
    pub uptime_s: f64,
    /// Ticks processeur cumulés (tous cœurs) : occupés et total, d'après `/proc/stat`.
    pub cpu_busy_ticks: u64,
    pub cpu_total_ticks: u64,
    pub cpus: u32,
    pub load1: f64,
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
    /// Disque des données (`/var/lib/solon`) en octets.
    pub disk_total_bytes: u64,
    pub disk_used_bytes: u64,
    /// Octets cumulés sur `eth0` (vers et depuis l'hôte).
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub containers_running: u32,
}

fn default_shutdown_timeout() -> u64 {
    10
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, result: impl Serialize) -> Self {
        Self {
            id,
            ok: true,
            result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }
    pub fn err(id: u64, error: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub interface: String,
    /// Adresse IPv4 de l'invité, ex. `172.30.0.2`.
    pub address: String,
    pub prefix_len: u8,
    pub gateway: String,
    pub dns: Vec<String>,
    #[serde(default)]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub search_domains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountShareRequest {
    /// Nom du partage HCS (= `aname` 9P).
    pub name: String,
    /// Port vsock du serveur 9P de l'hôte.
    pub port: u32,
    /// Point de montage dans l'invité, ex. `/mnt/host/c`.
    pub target: String,
    #[serde(default)]
    pub read_only: bool,
    /// Options 9P supplémentaires (tests de performance uniquement).
    #[serde(default)]
    pub extra_options: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MountResult {
    pub target: String,
    pub options: String,
    pub mount_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub ms: u64,
    #[serde(default)]
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HealthReport {
    pub protocol_version: u32,
    pub agent_version: String,
    pub kernel: String,
    pub uptime_s: f64,
    pub docker_ping: bool,
    pub engine: EngineStatus,
    pub data_disk: Option<DataDiskReport>,
    pub mem_total_kb: u64,
    pub mem_available_kb: u64,
    pub network_configured: bool,
    pub mounts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EngineStatus {
    pub containerd_pid: Option<u32>,
    pub dockerd_pid: Option<u32>,
    pub docker_ready_at_uptime_s: Option<f64>,
    pub restarts: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DataDiskReport {
    pub device: String,
    pub formatted_now: bool,
    pub fsck_exit: i32,
    pub fsck_summary: String,
    pub mounted_at: String,
}

// ---------------------------------------------------------------------------------------------
// Événements (port d'événements, invité → hôte)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Premier message sur la connexion : confirme le protocole.
    Hello {
        protocol_version: u32,
        agent_version: String,
    },
    /// dockerd répond sur son socket.
    EngineReady { uptime_s: f64 },
    /// dockerd s'est arrêté (l'agent va le relancer).
    EngineDown { exit: String, restarts: u32 },
    /// L'ensemble complet des ports publiés a changé (liste idempotente).
    PortsChanged { bindings: Vec<PortBinding> },
    /// Un conteneur a changé d'état (pour rafraîchir l'interface sans polling).
    Container {
        action: String,
        id: String,
        name: String,
    },
    /// Journal de l'agent.
    Log { level: LogLevel, message: String },
    /// Le disque de données dépasse un seuil d'occupation (émis une fois par franchissement).
    DiskPressure { used_pct: u8, free_mb: u64 },
    /// L'ensemble des conteneurs en marche avec leur adresse et leurs ports (liste idempotente) :
    /// sert aux domaines locaux, qui n'exigent aucun port publié.
    EndpointsChanged { endpoints: Vec<ContainerEndpoint> },
}

/// Un fait observé dans le noyau de la machine par les sondes eBPF de la radiographie.
///
/// Le champ `kind` distingue les variantes dans le JSON. Les horodatages sont en millisecondes
/// depuis l'époque Unix, convertis par l'agent : l'horloge du noyau compte depuis le démarrage de
/// la machine et n'aurait aucun sens côté hôte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceEvent {
    /// Premier message sur la connexion : les sondes sont-elles en place. `active = false` avec le
    /// motif si le noyau les a refusées — l'interface le dit au lieu d'afficher un écran vide.
    Status { active: bool, detail: String },
    /// Une connexion TCP ouverte depuis la machine, avec son issue.
    ///
    /// Les connexions entrantes ne sont pas rapportées séparément : entre deux conteneurs, elles
    /// sont l'autre extrémité d'une sortante déjà vue. Ce qui vient de l'extérieur de la machine —
    /// un navigateur, par exemple — passe par le mandataire de Solon, qui le rapportera lui-même.
    ///
    /// La charge est derrière un `Box` : c'est de loin la plus grosse variante, et l'historique
    /// en garde des dizaines de milliers. Le JSON est le même qu'avec des champs en ligne.
    Connection(Box<TraceConnection>),
}

/// Ce qu'on retient d'une connexion observée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceConnection {
    pub ts_ms: u64,
    pub outcome: ConnectionOutcome,
    pub src: TracePeer,
    pub dst: TracePeer,
    pub sport: u16,
    pub dport: u16,
    /// Nom court du programme à l'origine de l'appel.
    pub comm: String,
    pub pid: u32,
    /// Durée de vie de la connexion, pour une fermeture seulement.
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Issue d'une connexion observée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionOutcome {
    Established,
    /// Jamais établie : refusée, filtrée, ou abandonnée avant réponse.
    Failed,
    /// Fermée après avoir servi.
    Closed,
}

/// Une extrémité de connexion, nommée autant que possible.
///
/// L'adresse est toujours là ; le reste est renseigné quand l'agent reconnaît un conteneur de la
/// machine. Sinon l'extrémité est hors de la machine : un service distant, ou l'hôte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TracePeer {
    pub ip: String,
    #[serde(default)]
    pub container_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
}

impl TracePeer {
    /// Ce qu'il faut afficher : le nom du service dans son projet, à défaut celui du conteneur, à
    /// défaut l'adresse.
    pub fn label(&self) -> &str {
        self.service
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or(&self.ip)
    }
}

/// Un conteneur en marche vu du réseau : adresse dans le réseau Docker, ports TCP exposés par
/// l'image et ports conteneur publiés.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct ContainerEndpoint {
    pub id: String,
    /// Sans le `/` initial.
    pub name: String,
    #[serde(default)]
    pub compose_project: Option<String>,
    #[serde(default)]
    pub compose_service: Option<String>,
    pub ip: String,
    #[serde(default)]
    pub exposed_tcp: Vec<u16>,
    #[serde(default)]
    pub published_tcp: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Un port publié par un conteneur, tel qu'annoncé par Docker.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct PortBinding {
    pub container_id: String,
    /// `tcp` ou `udp` (UDP hors périmètre du MVP : relayé plus tard).
    pub protocol: String,
    /// Adresse d'écoute demandée côté hôte (`0.0.0.0`, `127.0.0.1`, `::`…).
    pub host_ip: String,
    pub host_port: u16,
    /// Adresse du conteneur dans le réseau Docker (cible du relais).
    pub container_ip: String,
    pub container_port: u16,
    /// Nom du conteneur sans le `/` initial (domaines locaux `nom.solon.local`).
    #[serde(default)]
    pub container_name: String,
    /// Étiquettes Compose, si le conteneur en vient (`service.projet.solon.local`).
    #[serde(default)]
    pub compose_project: Option<String>,
    #[serde(default)]
    pub compose_service: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Relais de ports (port [`PORT_PORTS`])
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRelayHeader {
    pub container_ip: String,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRelayAck {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Structures Docker minimales lues par l'agent (`docker inspect`)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectedContainer {
    pub id: String,
    pub name: String,
    pub network_settings: InspectedNetworkSettings,
    #[serde(default)]
    pub config: InspectedConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct InspectedConfig {
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// `{"8069/tcp": {}}`
    #[serde(default)]
    pub exposed_ports: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct InspectedNetworkSettings {
    #[serde(default)]
    pub ports: BTreeMap<String, Option<Vec<InspectedHostBinding>>>,
    #[serde(default)]
    pub networks: BTreeMap<String, InspectedNetwork>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InspectedHostBinding {
    pub host_ip: String,
    pub host_port: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct InspectedNetwork {
    #[serde(default, rename = "IPAddress")]
    pub ip_address: String,
}

impl InspectedContainer {
    /// Vue réseau du conteneur pour les domaines locaux ; `None` s'il n'a pas d'adresse.
    pub fn endpoint(&self) -> Option<ContainerEndpoint> {
        let ip = self
            .network_settings
            .networks
            .values()
            .map(|n| n.ip_address.as_str())
            .find(|ip| !ip.is_empty())?
            .to_owned();
        let mut exposed_tcp: Vec<u16> = self
            .config
            .exposed_ports
            .keys()
            .filter_map(|spec| {
                let (port, proto) = spec.split_once('/').unwrap_or((spec, "tcp"));
                (proto == "tcp").then(|| port.parse::<u16>().ok()).flatten()
            })
            .collect();
        exposed_tcp.sort_unstable();
        let mut published_tcp: Vec<u16> = self
            .network_settings
            .ports
            .iter()
            .filter(|(_, b)| b.is_some())
            .filter_map(|(spec, _)| {
                let (port, proto) = spec.split_once('/').unwrap_or((spec, "tcp"));
                (proto == "tcp").then(|| port.parse::<u16>().ok()).flatten()
            })
            .collect();
        published_tcp.sort_unstable();
        Some(ContainerEndpoint {
            id: self.id.clone(),
            name: self.name.trim_start_matches('/').to_owned(),
            compose_project: self
                .config
                .labels
                .get("com.docker.compose.project")
                .cloned(),
            compose_service: self
                .config
                .labels
                .get("com.docker.compose.service")
                .cloned(),
            ip,
            exposed_tcp,
            published_tcp,
        })
    }

    /// Convertit les ports publiés d'un conteneur en liaisons relayables (TCP et UDP).
    pub fn port_bindings(&self) -> Vec<PortBinding> {
        let container_ip = self
            .network_settings
            .networks
            .values()
            .map(|n| n.ip_address.as_str())
            .find(|ip| !ip.is_empty())
            .unwrap_or("")
            .to_owned();
        let mut out = Vec::new();
        if container_ip.is_empty() {
            return out;
        }
        for (spec, bindings) in &self.network_settings.ports {
            let Some(bindings) = bindings else { continue };
            let (port, proto) = spec.split_once('/').unwrap_or((spec, "tcp"));
            let Ok(container_port) = port.parse::<u16>() else {
                continue;
            };
            for b in bindings {
                let Ok(host_port) = b.host_port.parse::<u16>() else {
                    continue;
                };
                out.push(PortBinding {
                    container_id: self.id.clone(),
                    protocol: proto.to_owned(),
                    host_ip: if b.host_ip.is_empty() {
                        "0.0.0.0".into()
                    } else {
                        b.host_ip.clone()
                    },
                    host_port,
                    container_ip: container_ip.clone(),
                    container_port,
                    container_name: self.name.trim_start_matches('/').to_owned(),
                    compose_project: self
                        .config
                        .labels
                        .get("com.docker.compose.project")
                        .cloned(),
                    compose_service: self
                        .config
                        .labels
                        .get("com.docker.compose.service")
                        .cloned(),
                });
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requete_et_reponse_json() {
        let r = Request {
            id: 7,
            command: Command::MountShare(MountShareRequest {
                name: "c".into(),
                port: 9000,
                target: "/mnt/host/c".into(),
                read_only: false,
                extra_options: String::new(),
            }),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"cmd\":\"mount_share\""), "{json}");
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
        let resp = Response::ok(
            7,
            MountResult {
                target: "/mnt/host/c".into(),
                options: "trans=fd".into(),
                mount_ms: 1,
            },
        );
        assert!(
            serde_json::to_string(&resp)
                .unwrap()
                .contains("\"ok\":true")
        );
    }

    #[test]
    fn ports_depuis_inspect() {
        let raw = r#"[{"Id":"abc","Name":"/web","NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"},{"HostIp":"::","HostPort":"8080"}],"443/tcp":null,"53/udp":[{"HostIp":"127.0.0.1","HostPort":"5353"}]},"Networks":{"bridge":{"IPAddress":"172.17.0.2"}}}}]"#;
        let list: Vec<InspectedContainer> = serde_json::from_str(raw).unwrap();
        let b = list[0].port_bindings();
        assert_eq!(b.len(), 3);
        assert!(b.iter().any(|p| p.host_port == 8080
            && p.container_port == 80
            && p.container_ip == "172.17.0.2"
            && p.protocol == "tcp"));
        assert!(
            b.iter()
                .any(|p| p.protocol == "udp" && p.host_ip == "127.0.0.1")
        );
    }

    #[test]
    fn evenement_ports() {
        let e = AgentEvent::PortsChanged { bindings: vec![] };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"event":"ports_changed","bindings":[]}"#
        );
    }

    /// Contrat avec l'interface : `TraceConnection` est derrière un `Box`, mais ses champs
    /// doivent rester à plat à côté de `kind`, comme le TypeScript les lit. Si le `Box` devenait
    /// un objet imbriqué, l'écran se viderait sans la moindre erreur.
    #[test]
    fn un_appel_reste_plat_dans_le_json() {
        let e = TraceEvent::Connection(Box::new(TraceConnection {
            ts_ms: 1_700_000_000_000,
            outcome: ConnectionOutcome::Failed,
            src: TracePeer {
                ip: "172.18.0.3".into(),
                container_id: Some("abc123abc123".into()),
                name: Some("boutique-web-1".into()),
                project: Some("boutique".into()),
                service: Some("web".into()),
            },
            dst: TracePeer {
                ip: "172.18.0.4".into(),
                ..Default::default()
            },
            sport: 51990,
            dport: 5432,
            comm: "python".into(),
            pid: 42,
            duration_ms: None,
        }));
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(v["kind"], "connection");
        assert_eq!(v["outcome"], "failed");
        assert_eq!(v["dport"], 5432);
        assert_eq!(v["src"]["service"], "web");
        assert_eq!(v["dst"]["container_id"], serde_json::Value::Null);
        assert_eq!(serde_json::from_value::<TraceEvent>(v).unwrap(), e);
    }

    #[test]
    fn une_extremite_s_affiche_au_mieux() {
        let mut p = TracePeer {
            ip: "10.0.0.9".into(),
            ..Default::default()
        };
        assert_eq!(p.label(), "10.0.0.9");
        p.name = Some("boutique-db-1".into());
        assert_eq!(p.label(), "boutique-db-1");
        p.service = Some("db".into());
        assert_eq!(p.label(), "db");
    }
}
