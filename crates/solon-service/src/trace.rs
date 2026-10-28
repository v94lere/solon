//! Radiographie côté hôte : reçoit les faits observés par les sondes eBPF et les sert à
//! l'interface.
//!
//! Le stockage vit ici, pas dans la machine : l'invité reste sans état, le disque de données
//! n'est pas pollué par de l'observation, et l'interface parle déjà au service. Pour l'instant
//! un simple anneau en mémoire — une base sur disque n'a de sens qu'une fois qu'il y aura du
//! volume à garder entre deux démarrages, ce qui viendra avec les sondes réseau.
//!
//! Rien ne quitte la machine : ces faits ne sont ni envoyés ni écrits ailleurs qu'ici.

use std::collections::{HashSet, VecDeque};
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
    /// Projets pour lesquels le mandataire garde le début des corps (réglage `trace_bodies`).
    /// Tenu ici parce que c'est ici que le mandataire vient demander ; mis à jour dès que le
    /// réglage change, sans redémarrer le moteur.
    body_projects: Mutex<HashSet<String>>,
}

impl TraceHub {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(CHANNEL);
        Arc::new(Self {
            tx,
            recent: Mutex::new(VecDeque::with_capacity(1024)),
            status: Mutex::new(unavailable("moteur arrêté")),
            body_projects: Mutex::new(HashSet::new()),
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

    /// Oublie ce qui a été observé : tout, ou seulement ce qui se rattache à un projet (bouton
    /// « effacer » de l'interface, qui suit le filtre affiché).
    pub fn clear(&self, project: Option<&str>) {
        let mut recent = self.recent.lock().unwrap();
        match project {
            None => recent.clear(),
            Some(p) => recent.retain(|e| e.project() != Some(p)),
        }
    }

    /// Les projets dont on capture les corps. Remplace la liste entière : c'est le réglage tel
    /// qu'il est enregistré.
    pub fn set_body_projects(&self, projects: &[String]) {
        *self.body_projects.lock().unwrap() = projects.iter().cloned().collect();
    }

    /// Le mandataire doit-il garder les corps des échanges de ce projet ? Sans projet connu
    /// (conteneur hors Compose), jamais : il n'y a pas d'endroit où l'avoir demandé.
    pub fn captures_bodies(&self, project: Option<&str>) -> bool {
        match project {
            Some(p) => self.body_projects.lock().unwrap().contains(p),
            None => false,
        }
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
        connection_of(pid, None)
    }

    fn connection_of(pid: u32, project: Option<&str>) -> TraceEvent {
        TraceEvent::Connection(Box::new(TraceConnection {
            ts_ms: 0,
            outcome: ConnectionOutcome::Established,
            src: TracePeer {
                project: project.map(str::to_owned),
                ..Default::default()
            },
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
        hub.clear(None);
        let backlog = hub.backlog();
        assert_eq!(backlog.len(), 1);
        assert!(matches!(
            backlog[0],
            TraceEvent::Status { active: true, .. }
        ));
    }

    #[test]
    fn effacer_un_projet_laisse_les_autres() {
        let hub = TraceHub::new();
        hub.push(connection_of(1, Some("boutique")));
        hub.push(connection_of(2, Some("blog")));
        hub.push(connection_of(3, None));
        hub.clear(Some("boutique"));
        let pids: Vec<u32> = hub
            .backlog()
            .into_iter()
            .filter_map(|e| match e {
                TraceEvent::Connection(c) => Some(c.pid),
                _ => None,
            })
            .collect();
        assert_eq!(pids, vec![2, 3]);
    }

    #[test]
    fn les_corps_ne_sont_captures_que_la_ou_c_est_demande() {
        let hub = TraceHub::new();
        assert!(!hub.captures_bodies(Some("boutique")));
        hub.set_body_projects(&["boutique".to_owned()]);
        assert!(hub.captures_bodies(Some("boutique")));
        assert!(!hub.captures_bodies(Some("blog")));
        assert!(!hub.captures_bodies(None));
        hub.set_body_projects(&[]);
        assert!(!hub.captures_bodies(Some("boutique")));
    }
}
