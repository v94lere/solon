//! Radiographie côté hôte : reçoit les faits observés par les sondes eBPF et les sert à
//! l'interface.
//!
//! Le stockage vit ici, pas dans la machine : l'invité reste sans état, le disque de données
//! n'est pas pollué par de l'observation, et l'interface parle déjà au service. Pour l'instant
//! un simple anneau en mémoire — une base sur disque n'a de sens qu'une fois qu'il y aura du
//! volume à garder entre deux démarrages, ce qui viendra avec les sondes réseau.
//!
//! Rien ne quitte la machine : ces faits ne sont ni envoyés ni écrits ailleurs qu'ici.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use solon_core::protocol::TraceEvent;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use windows::core::GUID;

use crate::agent_client::AgentTrace;

/// Faits gardés en mémoire. À 150 octets l'unité, 20 000 tiennent dans 3 Mo — loin du budget de
/// 50 Mo annoncé, et de quoi remonter largement au-delà de ce qu'un écran affiche.
const HISTORY: usize = 20_000;

/// Capacité du canal de diffusion. Un abonné trop lent perd des faits plutôt que de retarder la
/// collecte : une sonde ne doit jamais ralentir ce qu'elle observe.
const CHANNEL: usize = 1_024;

pub struct TraceHub {
    tx: broadcast::Sender<TraceEvent>,
    recent: Mutex<VecDeque<TraceEvent>>,
    /// Dernier état connu des sondes, renvoyé en tête à chaque nouvel abonné pour qu'il sache
    /// tout de suite si le silence vient d'un calme plat ou d'un chargement raté.
    status: Mutex<TraceEvent>,
}

impl TraceHub {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(CHANNEL);
        Arc::new(Self {
            tx,
            recent: Mutex::new(VecDeque::with_capacity(1024)),
            status: Mutex::new(unavailable("moteur arrêté")),
        })
    }

    pub fn push(&self, event: TraceEvent) {
        if matches!(event, TraceEvent::Status { .. }) {
            *self.status.lock().unwrap() = event.clone();
        } else {
            let mut recent = self.recent.lock().unwrap();
            if recent.len() == HISTORY {
                recent.pop_front();
            }
            recent.push_back(event.clone());
        }
        // `send` échoue quand personne n'écoute : c'est le cas courant, pas une erreur.
        let _ = self.tx.send(event);
    }

    /// Ce qu'un nouvel abonné reçoit avant le flux : l'état des sondes puis l'historique.
    pub fn backlog(&self) -> Vec<TraceEvent> {
        let status = self.status.lock().unwrap().clone();
        let recent = self.recent.lock().unwrap();
        let mut out = Vec::with_capacity(recent.len() + 1);
        out.push(status);
        out.extend(recent.iter().cloned());
        out
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TraceEvent> {
        self.tx.subscribe()
    }

    /// Oublie tout ce qui a été observé (bouton « effacer » de l'interface).
    pub fn clear(&self) {
        self.recent.lock().unwrap().clear();
    }

    fn set_unavailable(&self, reason: &str) {
        self.push(unavailable(reason));
    }
}

fn unavailable(detail: &str) -> TraceEvent {
    TraceEvent::Status {
        active: false,
        detail: detail.to_owned(),
    }
}

/// Suit le flux de l'agent tant que la machine tourne. La tâche se termine quand la connexion
/// tombe : l'arrêt du moteur annule de toute façon les tâches de la machine.
pub fn spawn_collector(hub: Arc<TraceHub>, guid: GUID) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = match AgentTrace::connect(&guid, Duration::from_secs(10)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("radiographie indisponible : {e}");
                hub.set_unavailable(&e.to_string());
                return;
            }
        };
        loop {
            match stream.next().await {
                Ok(Some(event)) => hub.push(event),
                Ok(None) => {
                    hub.set_unavailable("flux fermé par l'agent");
                    return;
                }
                Err(e) => {
                    tracing::warn!("flux de radiographie perdu : {e}");
                    hub.set_unavailable(&e.to_string());
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solon_core::protocol::{ConnectionOutcome, TraceConnection, TracePeer};

    fn connection(pid: u32) -> TraceEvent {
        TraceEvent::Connection(Box::new(TraceConnection {
            ts_ms: 0,
            outcome: ConnectionOutcome::Established,
            src: TracePeer::default(),
            dst: TracePeer::default(),
            sport: 1,
            dport: 2,
            comm: "curl".to_owned(),
            pid,
            duration_ms: None,
        }))
    }

    fn en_place() -> TraceEvent {
        TraceEvent::Status {
            active: true,
            detail: "sondes en place".to_owned(),
        }
    }

    #[test]
    fn l_etat_ne_remplit_pas_l_historique() {
        let hub = TraceHub::new();
        hub.push(en_place());
        hub.push(connection(1));
        let backlog = hub.backlog();
        assert_eq!(backlog.len(), 2);
        assert!(matches!(
            backlog[0],
            TraceEvent::Status { active: true, .. }
        ));
        assert!(matches!(&backlog[1], TraceEvent::Connection(c) if c.pid == 1));
    }

    #[test]
    fn l_historique_oublie_les_plus_anciens() {
        let hub = TraceHub::new();
        for pid in 0..(HISTORY as u32 + 10) {
            hub.push(connection(pid));
        }
        let backlog = hub.backlog();
        assert_eq!(
            backlog.len(),
            HISTORY + 1,
            "l'état en tête, puis le plafond"
        );
        assert!(matches!(&backlog[1], TraceEvent::Connection(c) if c.pid == 10));
    }

    #[test]
    fn effacer_garde_l_etat_des_sondes() {
        let hub = TraceHub::new();
        hub.push(en_place());
        hub.push(connection(1));
        hub.clear();
        let backlog = hub.backlog();
        assert_eq!(backlog.len(), 1);
        assert!(matches!(
            backlog[0],
            TraceEvent::Status { active: true, .. }
        ));
    }
}
