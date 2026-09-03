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

/// Type de trame hôte → agent sur le canal terminal : `[type, len_hi, len_lo, charge utile]`.
pub const SHELL_FRAME_INPUT: u8 = 0;
pub const SHELL_FRAME_RESIZE: u8 = 1;

/// Première ligne envoyée par l'hôte à l'ouverture d'un terminal dans la machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellHeader {
    pub cols: u16,
    pub rows: u16,
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
}
