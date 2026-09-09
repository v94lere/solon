//! Relais des ports publiés : pour chaque liaison annoncée par l'agent, un écouteur TCP sur l'hôte
//! (`localhost:8080` ou `0.0.0.0:8080`) dont chaque connexion est relayée par HvSocket vers le
//! conteneur. Aucune dépendance au réseau virtuel ni au pare-feu pour les liaisons locales.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use solon_core::protocol::{PORT_PORTS, PortBinding, PortRelayAck, PortRelayHeader};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use windows::core::GUID;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    host_ip: String,
    host_port: u16,
    protocol: String,
}

pub struct PortRelays {
    active: HashMap<Key, (PortBinding, JoinHandle<()>)>,
    sleeper: std::sync::Arc<crate::sleep::Sleeper>,
}

impl PortRelays {
    pub fn new(sleeper: std::sync::Arc<crate::sleep::Sleeper>) -> Self {
        Self {
            active: HashMap::new(),
            sleeper,
        }
    }

    /// Applique l'ensemble complet des liaisons : ouvre les nouvelles, ferme celles disparues,
    /// remplace celles dont la cible a changé.
    pub fn apply(&mut self, bindings: &[PortBinding], vm: GUID) {
        let wanted: HashMap<Key, &PortBinding> = bindings
            .iter()
            .filter(|b| b.protocol == "tcp")
            .map(|b| {
                (
                    Key {
                        host_ip: b.host_ip.clone(),
                        host_port: b.host_port,
                        protocol: b.protocol.clone(),
                    },
                    b,
                )
            })
            .collect();
        let stale: Vec<Key> = self
            .active
            .iter()
            .filter(|(k, (b, _))| {
                wanted.get(k).is_none_or(|w| {
                    w.container_ip != b.container_ip || w.container_port != b.container_port
                })
            })
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            if let Some((_, task)) = self.active.remove(&k) {
                task.abort();
                tracing::info!(port = k.host_port, ip = %k.host_ip, "relais de port fermé");
            }
        }
        for (k, b) in wanted {
            if self.active.contains_key(&k) {
                continue;
            }
            let binding = b.clone();
            let task = tokio::spawn(listen(binding.clone(), vm, self.sleeper.clone()));
            self.active.insert(k, (binding, task));
        }
        let udp = bindings.iter().filter(|b| b.protocol == "udp").count();
        if udp > 0 {
            tracing::warn!(udp, "liaisons UDP ignorées (hors périmètre du MVP)");
        }
    }

    pub fn clear(&mut self) {
        for (_, (_, task)) in self.active.drain() {
            task.abort();
        }
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

fn listen_addr(b: &PortBinding) -> Option<SocketAddr> {
    let ip: IpAddr = match b.host_ip.as_str() {
        "" | "0.0.0.0" => IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        "::" => IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
        other => other.parse().ok()?,
    };
    Some(SocketAddr::new(ip, b.host_port))
}

async fn listen(binding: PortBinding, vm: GUID, sleeper: std::sync::Arc<crate::sleep::Sleeper>) {
    let Some(addr) = listen_addr(&binding) else {
        tracing::warn!(?binding, "adresse d'écoute invalide");
        return;
    };
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(%addr, "port publié indisponible sur l'hôte : {e}");
            return;
        }
    };
    tracing::info!(%addr, target = %format!("{}:{}", binding.container_ip, binding.container_port), "relais de port ouvert");
    loop {
        let (client, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::debug!("accept {addr} : {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let b = binding.clone();
        let sleeper = sleeper.clone();
        tokio::spawn(async move {
            // Réveille le conteneur s'il dort ; la garde le maintient éveillé le temps de la connexion.
            let _guard = sleeper.on_connection_name(&b.container_name).await;
            if let Err(e) = relay_connection(client, &b, vm).await {
                tracing::debug!(%peer, "relais terminé : {e}");
            }
        });
    }
}

async fn relay_connection(mut client: TcpStream, b: &PortBinding, vm: GUID) -> std::io::Result<()> {
    let _ = client.set_nodelay(true);
    let std_stream = tokio::task::spawn_blocking(move || {
        solon_hvsock::connect_with_retry(&vm, PORT_PORTS, Duration::from_secs(5))
    })
    .await
    .map_err(std::io::Error::other)??;
    std_stream.set_nonblocking(true)?;
    let mut guest = TcpStream::from_std(std_stream)?;
    let header = PortRelayHeader {
        container_ip: b.container_ip.clone(),
        container_port: b.container_port,
        protocol: b.protocol.clone(),
    };
    let mut line = serde_json::to_string(&header).unwrap();
    line.push('\n');
    guest.write_all(line.as_bytes()).await?;
    let mut reader = BufReader::new(guest);
    let mut ack = String::new();
    reader.read_line(&mut ack).await?;
    let ack: PortRelayAck = serde_json::from_str(ack.trim())
        .map_err(|e| std::io::Error::other(format!("accusé illisible : {e}")))?;
    if !ack.ok {
        return Err(std::io::Error::other(
            ack.error.unwrap_or_else(|| "refusé par l'agent".into()),
        ));
    }
    // Le BufReader n'a rien lu au-delà de l'accusé : l'agent attend nos octets avant d'en envoyer.
    let mut guest = reader.into_inner();
    tokio::io::copy_bidirectional(&mut client, &mut guest).await?;
    Ok(())
}
