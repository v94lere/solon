//! Réveil à la demande : un conteneur qui a déjà reçu du trafic par Monodon (domaine `*.monodon.local`
//! ou port publié) et qui n'en reçoit plus depuis un certain temps est **mis en pause** (`docker
//! pause` : processus gelés, mémoire conservée, zéro processeur). La première connexion suivante le
//! réveille (`docker unpause`, quelques dizaines de millisecondes) avant d'être relayée.
//!
//! Garde-fous : seuls les conteneurs ayant déjà reçu une connexion via Monodon sont concernés (jamais
//! une base de données ou un travailleur de fond qui ne parle qu'en interne) ; jamais tant qu'une
//! connexion relayée est ouverte ; jamais ceux de la liste « toujours éveillé » ; jamais ceux que
//! l'utilisateur a mis en pause lui-même. Un `unpause` ou un arrêt fait par l'utilisateur retire le
//! conteneur de la liste des endormis.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use monodon_core::protocol::{Command, ContainerEndpoint, ExecResult};
use tokio::sync::{Mutex, RwLock};

use crate::agent_client::AgentClient;

#[derive(Debug, Clone, PartialEq)]
pub struct SleepSettings {
    pub enabled: bool,
    pub idle: Duration,
    /// Noms de conteneurs à ne jamais endormir.
    pub never: HashSet<String>,
}

impl Default for SleepSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            idle: Duration::from_secs(600),
            never: HashSet::new(),
        }
    }
}

#[derive(Debug)]
struct Activity {
    last: Instant,
    conns: usize,
}

/// Ports TCP qui signent un service HTTP : un conteneur n'est éligible au sommeil que s'il publie un
/// port ou expose l'un d'eux. Une base de données ou un cache (5432, 3306, 6379…) joints par un clic
/// sur leur adresse ne doivent jamais être endormis : d'autres conteneurs en dépendent en interne.
const HTTP_PORTS: &[u16] = &[
    80, 8080, 3000, 8000, 8069, 5000, 4200, 5173, 8888, 9000, 443, 8443, 4000, 5001, 8081, 8082,
];

#[derive(Default)]
struct State {
    by_ip: HashMap<String, String>,
    by_name: HashMap<String, String>,
    names: HashMap<String, String>,
    /// Conteneurs éligibles au sommeil (port publié ou port HTTP exposé).
    eligible: HashSet<String>,
    activity: HashMap<String, Activity>,
    /// Conteneurs mis en pause par Monodon (identifiants).
    sleeping: HashSet<String>,
}

pub struct Sleeper {
    state: Mutex<State>,
    agent: Mutex<Option<Arc<AgentClient>>>,
    settings: RwLock<SleepSettings>,
    /// Sérialise pause et réveil d'un même conteneur.
    ops: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    changed: tokio::sync::Notify,
}

/// Une connexion relayée en cours : tant qu'elle vit, le conteneur ne s'endort pas.
pub struct ConnGuard {
    sleeper: Arc<Sleeper>,
    id: String,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let sleeper = self.sleeper.clone();
        let id = self.id.clone();
        tokio::spawn(async move {
            let mut st = sleeper.state.lock().await;
            if let Some(a) = st.activity.get_mut(&id) {
                a.conns = a.conns.saturating_sub(1);
                a.last = Instant::now();
            }
        });
    }
}

impl Sleeper {
    pub fn new(settings: SleepSettings) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            agent: Mutex::new(None),
            settings: RwLock::new(settings),
            ops: Mutex::new(HashMap::new()),
            changed: tokio::sync::Notify::new(),
        })
    }

    pub async fn set_settings(&self, s: SleepSettings) {
        *self.settings.write().await = s;
        self.changed.notify_one();
    }

    pub async fn set_agent(&self, agent: Option<Arc<AgentClient>>) {
        *self.agent.lock().await = agent;
        if agent_is_none(&self.agent).await {
            // Moteur arrêté : plus rien d'endormi ni d'actif.
            *self.state.lock().await = State::default();
            self.changed.notify_one();
        }
    }

    /// Nouvelle table des conteneurs en marche (adresse, nom, identifiant).
    pub async fn update_endpoints(&self, endpoints: &[ContainerEndpoint]) {
        let mut st = self.state.lock().await;
        st.by_ip.clear();
        st.by_name.clear();
        st.names.clear();
        st.eligible.clear();
        for e in endpoints {
            st.by_ip.insert(e.ip.clone(), e.id.clone());
            st.by_name.insert(e.name.clone(), e.id.clone());
            st.names.insert(e.id.clone(), e.name.clone());
            if !e.published_tcp.is_empty() || e.exposed_tcp.iter().any(|p| HTTP_PORTS.contains(p)) {
                st.eligible.insert(e.id.clone());
            }
        }
        let alive: HashSet<String> = endpoints.iter().map(|e| e.id.clone()).collect();
        st.activity.retain(|id, _| alive.contains(id));
        let before = st.sleeping.len();
        st.sleeping.retain(|id| alive.contains(id));
        if st.sleeping.len() != before {
            self.changed.notify_one();
        }
    }

    /// Événement Docker : un conteneur repris ou arrêté par l'utilisateur n'est plus « endormi ».
    pub async fn on_container_event(&self, action: &str, id: &str) {
        if matches!(
            action,
            "unpause" | "stop" | "die" | "kill" | "destroy" | "start"
        ) {
            let mut st = self.state.lock().await;
            if st.sleeping.remove(id) {
                self.changed.notify_one();
            }
            if action != "unpause" {
                st.activity.remove(id);
            }
        }
    }

    /// Identifiants des conteneurs endormis par Monodon.
    pub async fn sleeping(&self) -> Vec<String> {
        let st = self.state.lock().await;
        let mut v: Vec<String> = st.sleeping.iter().cloned().collect();
        v.sort();
        v
    }

    /// Attente d'un changement de la liste des endormis (pour publier l'état).
    pub async fn changed(&self) {
        self.changed.notified().await
    }

    /// Connexion entrante vers l'adresse `ip` : réveille si besoin, puis compte la connexion.
    pub async fn on_connection_ip(self: &Arc<Self>, ip: &str) -> Option<ConnGuard> {
        let id = self.state.lock().await.by_ip.get(ip).cloned()?;
        Some(self.on_connection(id).await)
    }

    /// Connexion entrante vers le conteneur nommé `name` (relais de port publié).
    pub async fn on_connection_name(self: &Arc<Self>, name: &str) -> Option<ConnGuard> {
        let id = self.state.lock().await.by_name.get(name).cloned()?;
        Some(self.on_connection(id).await)
    }

    async fn on_connection(self: &Arc<Self>, id: String) -> ConnGuard {
        let asleep = {
            let mut st = self.state.lock().await;
            // Un conteneur non éligible (base de données, cache…) n'entre jamais dans le suivi d'activité,
            // mais se réveille quand même s'il avait été endormi avant un changement de ses ports.
            if st.eligible.contains(&id) {
                let a = st.activity.entry(id.clone()).or_insert(Activity {
                    last: Instant::now(),
                    conns: 0,
                });
                a.conns += 1;
                a.last = Instant::now();
            }
            st.sleeping.contains(&id)
        };
        if asleep {
            let lock = self.op_lock(&id).await;
            let _g = lock.lock().await;
            if self.state.lock().await.sleeping.contains(&id) {
                let name = self.name_of(&id).await;
                let t0 = Instant::now();
                match self.docker(&format!("docker unpause {id}")).await {
                    Ok(_) => tracing::info!(
                        container = %name,
                        ms = t0.elapsed().as_millis(),
                        "conteneur réveillé à la demande"
                    ),
                    Err(e) => tracing::warn!(container = %name, "réveil impossible : {e}"),
                }
                self.state.lock().await.sleeping.remove(&id);
                self.changed.notify_one();
            }
        }
        ConnGuard {
            sleeper: self.clone(),
            id,
        }
    }

    async fn op_lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.ops
            .lock()
            .await
            .entry(id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn name_of(&self, id: &str) -> String {
        self.state
            .lock()
            .await
            .names
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.chars().take(12).collect())
    }

    async fn docker(&self, command: &str) -> Result<ExecResult, String> {
        let agent = self
            .agent
            .lock()
            .await
            .clone()
            .ok_or_else(|| "moteur arrêté".to_string())?;
        let r: ExecResult = agent
            .call_typed(
                Command::Exec {
                    command: command.to_owned(),
                    timeout_s: Some(30),
                },
                Duration::from_secs(40),
            )
            .await
            .map_err(|e| e.to_string())?;
        if r.code != Some(0) {
            return Err(r.stderr.trim().to_owned());
        }
        Ok(r)
    }

    /// Boucle d'endormissement : toutes les 15 s, met en pause les conteneurs inactifs éligibles.
    pub async fn run(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let settings = self.settings.read().await.clone();
            if !settings.enabled || agent_is_none(&self.agent).await {
                continue;
            }
            let candidates: Vec<(String, String)> = {
                let st = self.state.lock().await;
                st.activity
                    .iter()
                    .filter(|(id, a)| {
                        a.conns == 0
                            && a.last.elapsed() >= settings.idle
                            && !st.sleeping.contains(*id)
                    })
                    .map(|(id, _)| (id.clone(), st.names.get(id).cloned().unwrap_or_default()))
                    .filter(|(_, name)| !settings.never.contains(name))
                    .collect()
            };
            for (id, name) in candidates {
                let lock = self.op_lock(&id).await;
                let _g = lock.lock().await;
                // Une connexion a pu arriver entre-temps.
                {
                    let st = self.state.lock().await;
                    match st.activity.get(&id) {
                        Some(a) if a.conns == 0 && a.last.elapsed() >= settings.idle => {}
                        _ => continue,
                    }
                }
                // Seulement s'il est bien en marche (pas mis en pause ou arrêté par l'utilisateur).
                let status = self
                    .docker(&format!("docker inspect -f '{{{{.State.Status}}}}' {id}"))
                    .await
                    .map(|r| r.stdout.trim().to_owned());
                if status.as_deref() != Ok("running") {
                    continue;
                }
                match self.docker(&format!("docker pause {id}")).await {
                    Ok(_) => {
                        tracing::info!(container = %name, idle_s = settings.idle.as_secs(), "conteneur endormi (inactif)");
                        self.state.lock().await.sleeping.insert(id.clone());
                        self.changed.notify_one();
                    }
                    Err(e) => tracing::warn!(container = %name, "mise en pause impossible : {e}"),
                }
            }
        }
    }
}

async fn agent_is_none(agent: &Mutex<Option<Arc<AgentClient>>>) -> bool {
    agent.lock().await.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(id: &str, name: &str, ip: &str) -> ContainerEndpoint {
        ContainerEndpoint {
            id: id.into(),
            name: name.into(),
            ip: ip.into(),
            exposed_tcp: vec![80],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn activite_et_correspondances() {
        let s = Sleeper::new(SleepSettings::default());
        s.update_endpoints(&[ep("a1", "web", "10.90.0.2"), ep("b2", "db", "10.90.0.3")])
            .await;
        // Sans moteur, la connexion est comptée mais rien n'est réveillé.
        let g = s.on_connection_ip("10.90.0.2").await.expect("connu");
        assert!(s.on_connection_ip("10.90.9.9").await.is_none());
        assert_eq!(s.state.lock().await.activity["a1"].conns, 1);
        drop(g);
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(s.state.lock().await.activity["a1"].conns, 0);
        // Un conteneur disparu est oublié.
        s.update_endpoints(&[ep("b2", "db", "10.90.0.3")]).await;
        assert!(!s.state.lock().await.activity.contains_key("a1"));
        assert!(s.on_connection_name("db").await.is_some());
    }
}
