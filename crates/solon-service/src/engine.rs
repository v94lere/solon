//! Machine à états du moteur : provisionnement, supervision, arrêt, récupération.
//!
//! Voir ARCHITECTURE.md §7.1. Toutes les transitions passent par [`Engine::set`] qui publie un
//! [`EngineSnapshot`] aux abonnés (l'application affiche la progression sans polling).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use solon_core::ipc::{EngineSnapshot, EngineState, ProvisionStep, ServiceEvent, Settings};
use solon_core::protocol::{AgentEvent, Command, HealthReport, PROTOCOL_VERSION};
use solon_core::vm::{DiskAttachment, NetworkAdapterConfig, VmConfig};
use solon_core::{ErrorCode, Result, SolonError};
use solon_vm_hcs::{HcsEventKind, HcsVm};
use tokio::sync::{Mutex, broadcast, watch};
use tokio::task::JoinHandle;
use windows::core::GUID;

use crate::agent_client::{AgentClient, AgentEvents};
use crate::network::{self, GuestNetwork};
use crate::paths::{Paths, ResolvedImage};
use crate::ports::PortRelays;
use crate::settings::{self, PersistedState};

pub const CONSOLE_PIPE: &str = r"\\.\pipe\solon-console";

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub paths: Paths,
    /// Dossier contenant `manifest.json`, `vmlinuz`, `initrd.img`, `rootfs.vhd`.
    pub image_dir: PathBuf,
    pub settings: Settings,
}

struct Running {
    vm: Arc<HcsVm>,
    guid: GUID,
    agent: Arc<AgentClient>,
    network: GuestNetwork,
    tasks: Vec<JoinHandle<()>>,
    ports: PortRelays,
    /// Lecteurs Windows partagés vers la machine, par lettre.
    shares: std::collections::HashMap<String, solon_core::vm::HostShare>,
}

struct Inner {
    cfg: EngineConfig,
    snapshot: watch::Sender<EngineSnapshot>,
    events: broadcast::Sender<ServiceEvent>,
    running: Mutex<Option<Running>>,
    /// Sérialise start/stop/restart.
    op: Mutex<()>,
    stopping: std::sync::atomic::AtomicBool,
    /// Domaines locaux `*.solon.local` → port hôte (partagé avec le mandataire HTTP).
    domains: crate::domains::SharedDomains,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Self {
        let (snapshot, _) = watch::channel(EngineSnapshot {
            state: Some(EngineState::Stopped),
            ..Default::default()
        });
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(Inner {
                cfg,
                snapshot,
                events,
                running: Mutex::new(None),
                op: Mutex::new(()),
                stopping: Default::default(),
                domains: Default::default(),
            }),
        }
    }

    pub fn snapshot(&self) -> EngineSnapshot {
        self.inner.snapshot.borrow().clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServiceEvent> {
        self.inner.events.subscribe()
    }

    pub fn watch(&self) -> watch::Receiver<EngineSnapshot> {
        self.inner.snapshot.subscribe()
    }

    pub fn settings(&self) -> Settings {
        self.inner.cfg.settings.clone()
    }

    fn set(&self, f: impl FnOnce(&mut EngineSnapshot)) {
        self.inner.snapshot.send_modify(f);
        let snap = self.snapshot();
        tracing::info!(state = ?snap.state, step = ?snap.step, "état");
        let _ = self.inner.events.send(ServiceEvent::State(snap));
    }

    fn emit(&self, event: ServiceEvent) {
        let _ = self.inner.events.send(event);
    }

    /// Lettres des lecteurs partagés avec la machine en marche (vide si arrêtée).
    pub async fn shared_drives(&self) -> Vec<String> {
        self.inner
            .running
            .lock()
            .await
            .as_ref()
            .map(|r| r.shares.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub async fn is_stopped(&self) -> bool {
        self.inner.running.lock().await.is_none()
    }

    /// Recalcule la table `*.solon.local` et le bloc du fichier `hosts` d'après les ports publiés.
    async fn update_domains(&self, bindings: &[solon_core::protocol::PortBinding]) {
        let map = crate::domains::domains_for(bindings);
        *self.inner.domains.write().await = map.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || crate::domains::write_hosts_block(&map))
            .await
            .unwrap_or_else(|e| Err(std::io::Error::other(e)))
        {
            tracing::warn!("fichier hosts non mis à jour : {e}");
        }
    }

    fn step(&self, step: ProvisionStep) {
        self.set(|s| {
            s.state = Some(EngineState::Starting);
            s.step = Some(step);
            s.error = None;
        });
    }

    /// Démarre le moteur (idempotent). Rend la main quand le moteur est prêt ou en échec.
    pub async fn start(&self) -> Result<()> {
        let _op = self.inner.op.lock().await;
        if self.inner.running.lock().await.is_some() {
            return Ok(());
        }
        self.inner
            .stopping
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let t0 = Instant::now();
        match self.provision().await {
            Ok(running) => {
                let boot_ms = t0.elapsed().as_millis() as u64;
                let guest_address = running.network.address.to_string();
                let vm_id = running.vm.id().to_owned();
                *self.inner.running.lock().await = Some(running);
                self.set(|s| {
                    s.state = Some(EngineState::Ready);
                    s.step = Some(ProvisionStep::Ready);
                    s.error = None;
                    s.vm_id = Some(vm_id);
                    s.last_boot_ms = Some(boot_ms);
                    s.ready_since_unix_ms = Some(settings::now_unix_ms());
                    s.guest_address = Some(guest_address);
                    s.docker_pipe = Some(solon_core::ipc::DOCKER_PIPE.into());
                });
                Ok(())
            }
            Err(e) => {
                tracing::error!("provisionnement : {e}");
                // Nettoyage de ce qui a pu être créé.
                let _ = HcsVm::terminate_orphans(None);
                let _ = network::delete_network();
                self.set(|s| {
                    s.state = Some(EngineState::Failed);
                    s.error = Some(e.clone());
                });
                Err(e)
            }
        }
    }

    async fn provision(&self) -> Result<Running> {
        let cfg = &self.inner.cfg;
        cfg.paths.ensure_dirs()?;
        // Les réglages (mémoire, processeurs, disque) sont relus à chaque démarrage : « appliqués au
        // prochain démarrage du moteur » comme l'annonce l'interface.
        let settings = settings::load_settings(&cfg.paths.settings_file());

        self.step(ProvisionStep::CheckingPrerequisites);
        let prereq = tokio::task::spawn_blocking(solon_prereq::check)
            .await
            .map_err(|e| SolonError::internal(e.to_string()))?;
        if !prereq.ok {
            let first = prereq.items.iter().find(|i| !i.ok && i.blocking);
            let (code, detail) = first
                .map(|i| (i.code.unwrap_or(ErrorCode::Internal), i.detail.clone()))
                .unwrap_or((ErrorCode::Internal, String::new()));
            return Err(SolonError::new(
                code,
                format!("prérequis manquant : {detail}"),
            ));
        }

        self.step(ProvisionStep::CleaningOrphans);
        let state_path = cfg.paths.state_file();
        let previous = settings::load_state(&state_path);
        if let Some(prev_id) = previous.vm_id.as_deref() {
            // Rattachement : le service a redémarré alors que la machine tourne encore ?
            if let Ok(vm) = HcsVm::open(prev_id) {
                if let Ok(rid) = vm.runtime_id() {
                    if let Ok(guid) = GUID::try_from(rid.as_str()) {
                        if let Ok(agent) = AgentClient::connect(&guid, Duration::from_secs(2)).await
                        {
                            if agent.ping().await.is_ok() {
                                tracing::warn!(
                                    id = prev_id,
                                    "rattachement à une machine encore en marche"
                                );
                                let mut running = self
                                    .attach(
                                        Arc::new(vm),
                                        guid,
                                        agent,
                                        previous.endpoint_id.clone(),
                                        boot_shares(&previous.shares),
                                    )
                                    .await?;
                                if let Some(addr) = previous
                                    .guest_address
                                    .as_deref()
                                    .and_then(|a| a.parse().ok())
                                {
                                    running.network.address = addr;
                                }
                                self.set(|s| {
                                    s.image_version = previous.image_version.clone();
                                    s.reattached = true;
                                });
                                return Ok(running);
                            }
                        }
                    }
                }
            }
            if !previous.clean_shutdown {
                tracing::warn!(
                    "le dernier arrêt n'était pas propre (coupure ou crash) ; le disque de données sera vérifié"
                );
                self.set(|s| s.recovered_from_crash = true);
            }
        }
        self.step(ProvisionStep::VerifyingImage);
        let image_dir = cfg.image_dir.clone();
        let image = tokio::task::spawn_blocking(move || ResolvedImage::load_verified(&image_dir))
            .await
            .map_err(|e| SolonError::internal(e.to_string()))?
            .map_err(|e| SolonError::new(ErrorCode::ImageCorrupted, e))?;
        self.set(|s| s.image_version = Some(image.manifest.version.clone()));

        self.step(ProvisionStep::PreparingDataDisk);
        let disk_path = cfg.paths.data_disk();
        let gib = settings.data_disk_gib;
        tokio::task::spawn_blocking(move || crate::disk::ensure_data_disk(&disk_path, gib))
            .await
            .map_err(|e| SolonError::internal(e.to_string()))??;

        let terminated = tokio::task::spawn_blocking(|| HcsVm::terminate_orphans(None))
            .await
            .map_err(|e| SolonError::internal(e.to_string()))??;
        if !terminated.is_empty() {
            tracing::warn!(?terminated, "machines orphelines terminées");
        }

        let vm_id = solon_vm_hcs::new_vm_id();
        self.step(ProvisionStep::CreatingNetwork);
        let id_for_net = vm_id.clone();
        let guest_net =
            tokio::task::spawn_blocking(move || network::ensure_network_and_endpoint(&id_for_net))
                .await
                .map_err(|e| SolonError::internal(e.to_string()))??;

        self.step(ProvisionStep::CreatingMachine);
        // Lecteurs partagés lors des sessions précédentes : déclarés dès la création (le périphérique Plan9
        // n'accepte des ajouts à chaud que s'il existe) et remontés par l'agent avant dockerd.
        let shares = boot_shares(&previous.shares);
        let mut cmdline = image.manifest.kernel_cmdline.clone();
        if !shares.is_empty() {
            let list: Vec<String> = shares
                .iter()
                .map(|sh| format!("{}:{}", sh.name, sh.port))
                .collect();
            cmdline.push_str(&format!(" solon.shares={}", list.join(",")));
        }
        if settings.legacy_file_sharing {
            cmdline.push_str(" solon.fs=9p");
        }
        let vm_config = VmConfig {
            id: vm_id.clone(),
            name: "solon".into(),
            kernel: image.kernel(),
            initrd: image.initrd(),
            cmdline,
            memory_mb: settings.memory_mb,
            processors: settings.processors,
            disks: vec![
                DiskAttachment {
                    path: image.rootfs(),
                    read_only: true,
                },
                DiskAttachment {
                    path: cfg.paths.data_disk(),
                    read_only: false,
                },
            ],
            shares: shares.clone(),
            serial_pipe: Some(CONSOLE_PIPE.into()),
            network_adapter: Some(NetworkAdapterConfig {
                endpoint_id: guest_net.endpoint_id.clone(),
                mac_address: if guest_net.mac_address.is_empty() {
                    None
                } else {
                    Some(guest_net.mac_address.clone())
                },
            }),
        };
        let vm = Arc::new(
            tokio::task::spawn_blocking(move || HcsVm::create(&vm_config))
                .await
                .map_err(|e| SolonError::internal(e.to_string()))??,
        );
        settings::save_state(
            &state_path,
            &PersistedState {
                vm_id: Some(vm_id.clone()),
                endpoint_id: Some(guest_net.endpoint_id.clone()),
                guest_address: Some(guest_net.address.to_string()),
                image_version: Some(image.manifest.version.clone()),
                shares: shares.iter().map(|sh| sh.name.clone()).collect(),
                clean_shutdown: false,
                updated_unix_ms: settings::now_unix_ms(),
            },
        )?;

        self.step(ProvisionStep::Booting);
        let console_task = spawn_console_logger();
        {
            let vm = vm.clone();
            tokio::task::spawn_blocking(move || vm.start())
                .await
                .map_err(|e| SolonError::internal(e.to_string()))??;
        }
        let runtime_id = vm.runtime_id()?;
        tracing::info!(vm_id, runtime_id, "machine démarrée");
        let guid = GUID::try_from(runtime_id.as_str())
            .map_err(|e| SolonError::internal(format!("RuntimeId : {e}")))?;

        self.step(ProvisionStep::WaitingAgent);
        let agent = AgentClient::connect(&guid, Duration::from_secs(30)).await?;
        let health: HealthReport = agent
            .call_typed(Command::Health, Duration::from_secs(10))
            .await?;
        if health.protocol_version != PROTOCOL_VERSION {
            return Err(SolonError::new(
                ErrorCode::ImageCorrupted,
                format!(
                    "agent en protocole {} (service : {PROTOCOL_VERSION})",
                    health.protocol_version
                ),
            ));
        }

        self.step(ProvisionStep::ConfiguringNetwork);
        agent
            .call(
                Command::ConfigureNetwork(guest_net.to_config()),
                Duration::from_secs(20),
            )
            .await?;

        let mut running = self
            .attach(vm, guid, agent, Some(guest_net.endpoint_id.clone()), shares)
            .await?;
        running.network = guest_net;
        running.tasks.push(console_task);
        Ok(running)
    }

    /// Branche les tâches de supervision (événements, relais, sortie) sur une machine démarrée et
    /// attend que dockerd réponde.
    async fn attach(
        &self,
        vm: Arc<HcsVm>,
        guid: GUID,
        agent: Arc<AgentClient>,
        endpoint_id: Option<String>,
        shares: Vec<solon_core::vm::HostShare>,
    ) -> Result<Running> {
        self.step(ProvisionStep::WaitingEngine);
        let mut events = AgentEvents::connect(&guid, Duration::from_secs(10)).await?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut ports = PortRelays::default();
        let mut initial_ports = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let ev = tokio::time::timeout(remaining, events.next())
                .await
                .map_err(|_| {
                    SolonError::new(
                        ErrorCode::EngineUnreachable,
                        "dockerd n'a pas démarré dans le délai",
                    )
                })??;
            match ev {
                Some(AgentEvent::EngineReady { .. }) => break,
                Some(AgentEvent::PortsChanged { bindings }) => initial_ports = bindings,
                Some(AgentEvent::Log { level, message }) => {
                    tracing::debug!(?level, "agent : {message}")
                }
                Some(_) => {}
                None => {
                    return Err(SolonError::new(
                        ErrorCode::AgentUnreachable,
                        "flux d'événements fermé par l'agent",
                    ));
                }
            }
        }
        ports.apply(&initial_ports, guid);
        self.set(|s| s.published_ports = initial_ports.clone());
        self.update_domains(&initial_ports).await;

        let mut tasks = Vec::new();
        // Domaines locaux : mandataire HTTP sur 127.0.0.1:80 (si le port est libre).
        match crate::domains::bind_proxy().await {
            Ok(listener) => {
                self.set(|s| s.local_domains = true);
                let domains = self.inner.domains.clone();
                tasks.push(tokio::spawn(async move {
                    if let Err(e) = crate::domains::serve_proxy(listener, domains).await {
                        tracing::warn!("mandataire des domaines locaux arrêté : {e}");
                    }
                }));
            }
            Err(e) => {
                tracing::warn!("domaines locaux indisponibles (port 80 occupé ?) : {e}");
                self.set(|s| s.local_domains = false);
            }
        }
        tasks.push(self.spawn_event_loop(events, guid));
        tasks.push(self.spawn_exit_watcher(vm.clone()));
        {
            let engine = self.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = crate::docker_proxy::serve(engine, guid).await {
                    tracing::error!("mandataire API Docker arrêté : {e}");
                }
            }));
        }
        {
            // Serveur de fichiers solonfs (FUSE côté invité) : ouvre ses connexions vers l'agent.
            let engine = self.clone();
            tasks.push(tokio::spawn(async move {
                crate::fileserver::serve(engine, guid).await;
            }));
        }
        tasks.push(tokio::spawn(async move {
            if let Err(e) = solon_hvsock::relay::serve_named_pipe_with_sddl(
                solon_core::ipc::EXEC_PIPE.into(),
                guid,
                solon_core::protocol::PORT_EXEC,
                Some(solon_hvsock::relay::DOCKER_PIPE_SDDL),
            )
            .await
            {
                tracing::error!("relais d'exécution en flux arrêté : {e}");
            }
        }));
        tasks.push(tokio::spawn(async move {
            if let Err(e) = solon_hvsock::relay::serve_named_pipe_with_sddl(
                solon_core::ipc::SHELL_PIPE.into(),
                guid,
                solon_core::protocol::PORT_SHELL,
                Some(solon_hvsock::relay::DOCKER_PIPE_SDDL),
            )
            .await
            {
                tracing::error!("relais du terminal machine arrêté : {e}");
            }
        }));

        Ok(Running {
            vm,
            guid,
            agent,
            network: GuestNetwork {
                network_id: String::new(),
                endpoint_id: endpoint_id.unwrap_or_default(),
                mac_address: String::new(),
                address: std::net::Ipv4Addr::UNSPECIFIED,
                prefix_len: 24,
                gateway: std::net::Ipv4Addr::UNSPECIFIED,
                dns: vec![],
            },
            tasks,
            ports,
            shares: shares.into_iter().map(|sh| (sh.name.clone(), sh)).collect(),
        })
    }

    fn spawn_event_loop(&self, mut events: AgentEvents, guid: GUID) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::spawn(async move {
            loop {
                match events.next().await {
                    Ok(Some(AgentEvent::PortsChanged { bindings })) => {
                        if let Some(r) = engine.inner.running.lock().await.as_mut() {
                            r.ports.apply(&bindings, guid);
                        }
                        engine.update_domains(&bindings).await;
                        engine
                            .inner
                            .snapshot
                            .send_modify(|s| s.published_ports = bindings.clone());
                        engine.emit(ServiceEvent::Ports { bindings });
                    }
                    Ok(Some(AgentEvent::Container { action, id, name })) => {
                        engine.emit(ServiceEvent::Container { action, id, name })
                    }
                    Ok(Some(AgentEvent::DiskPressure { used_pct, free_mb })) => {
                        tracing::warn!(used_pct, free_mb, "disque de données presque plein");
                        engine.emit(ServiceEvent::DiskPressure { used_pct, free_mb })
                    }
                    Ok(Some(AgentEvent::EngineDown { exit, restarts })) => {
                        tracing::warn!(exit, restarts, "dockerd arrêté dans l'invité");
                        engine.set(|s| s.state = Some(EngineState::Degraded));
                    }
                    Ok(Some(AgentEvent::EngineReady { .. })) => {
                        if engine.snapshot().state == Some(EngineState::Degraded) {
                            engine.set(|s| s.state = Some(EngineState::Ready));
                        }
                    }
                    Ok(Some(AgentEvent::Log { level, message })) => {
                        engine.emit(ServiceEvent::Log {
                            level: format!("{level:?}").to_lowercase(),
                            message,
                        });
                    }
                    Ok(Some(AgentEvent::Hello { .. })) => {}
                    Ok(None) | Err(_) => {
                        if !engine
                            .inner
                            .stopping
                            .load(std::sync::atomic::Ordering::SeqCst)
                        {
                            tracing::error!("flux d'événements de l'agent perdu");
                            engine.set(|s| {
                                s.state = Some(EngineState::Failed);
                                s.error = Some(SolonError::new(
                                    ErrorCode::AgentUnreachable,
                                    "connexion à l'agent perdue",
                                ));
                            });
                        }
                        return;
                    }
                }
            }
        })
    }

    fn spawn_exit_watcher(&self, vm: Arc<HcsVm>) -> JoinHandle<()> {
        let engine = self.clone();
        tokio::task::spawn_blocking(move || {
            let exit = loop {
                if let Some(e) = vm.wait_exit(Duration::from_secs(1)) {
                    break e;
                }
                if engine
                    .inner
                    .stopping
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    return;
                }
            };
            if !engine
                .inner
                .stopping
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                tracing::error!(?exit.data, "la machine s'est arrêtée de façon inattendue");
                engine.set(|s| {
                    s.state = Some(EngineState::Failed);
                    s.error = Some(SolonError::new(
                        ErrorCode::VmBootTimeout,
                        format!("arrêt inattendu de la machine : {:?}", exit.data),
                    ));
                });
            }
        })
    }

    /// Arrêt propre (ou forcé) ; idempotent.
    pub async fn stop(&self, force: bool) -> Result<()> {
        let _op = self.inner.op.lock().await;
        self.inner
            .stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let Some(mut running) = self.inner.running.lock().await.take() else {
            let _ = HcsVm::terminate_orphans(None);
            let _ = network::delete_network();
            self.set(|s| {
                s.state = Some(EngineState::Stopped);
                s.step = None;
            });
            return Ok(());
        };
        self.set(|s| s.state = Some(EngineState::Stopping));
        running.ports.clear();
        for t in running.tasks.drain(..) {
            t.abort();
        }
        let mut graceful = false;
        if !force {
            match running
                .agent
                .call(Command::Shutdown { timeout_s: 10 }, Duration::from_secs(20))
                .await
            {
                Ok(_) => {
                    let vm = running.vm.clone();
                    let exit =
                        tokio::task::spawn_blocking(move || vm.wait_exit(Duration::from_secs(40)))
                            .await
                            .ok()
                            .flatten();
                    graceful = exit
                        .as_ref()
                        .is_some_and(|e| e.kind == HcsEventKind::SystemExited);
                    if !graceful {
                        tracing::warn!("l'invité n'a pas éteint la machine dans le délai");
                    }
                }
                Err(e) => tracing::warn!("arrêt propre impossible ({e}), terminaison forcée"),
            }
        }
        if !graceful {
            let vm = running.vm.clone();
            let _ = tokio::task::spawn_blocking(move || vm.terminate()).await;
        }
        let _ = tokio::task::spawn_blocking(network::delete_network).await;
        let state_path = self.inner.cfg.paths.state_file();
        let mut st = settings::load_state(&state_path);
        st.clean_shutdown = true;
        st.updated_unix_ms = settings::now_unix_ms();
        let _ = settings::save_state(&state_path, &st);
        self.update_domains(&[]).await;
        self.set(|s| {
            s.state = Some(EngineState::Stopped);
            s.step = None;
            s.local_domains = false;
            s.published_ports.clear();
            s.guest_address = None;
            s.ready_since_unix_ms = None;
        });
        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        self.stop(false).await?;
        self.start().await
    }

    /// Exécute une commande shell dans la machine via l'agent.
    pub async fn exec(
        &self,
        command: String,
        timeout_s: u64,
    ) -> Result<solon_core::protocol::ExecResult> {
        let agent = self
            .inner
            .running
            .lock()
            .await
            .as_ref()
            .map(|r| r.agent.clone())
            .ok_or_else(|| {
                SolonError::new(ErrorCode::EngineUnreachable, "le moteur n'est pas démarré")
            })?;
        agent
            .call_typed(
                Command::Exec {
                    command,
                    timeout_s: Some(timeout_s),
                },
                Duration::from_secs(timeout_s + 5),
            )
            .await
    }

    /// Partage le lecteur d'un chemin Windows vers la machine (une fois par lecteur) et renvoie
    /// le chemin traduit côté invité.
    pub async fn ensure_share(&self, host_path: &str) -> Result<solon_core::ipc::ShareInfo> {
        let (drive, guest_path) = solon_core::ipc::guest_path_for(host_path).ok_or_else(|| {
            SolonError::new(
                ErrorCode::Io,
                format!("chemin non partageable : {host_path}"),
            )
        })?;
        let host_root = format!("{}:\\", drive.to_ascii_uppercase());
        if !std::path::Path::new(&host_root).exists() {
            return Err(SolonError::new(
                ErrorCode::Io,
                format!("lecteur introuvable : {host_root}"),
            ));
        }
        let guest_root = format!("/mnt/host/{drive}");
        // Le 9P de Windows est monté en secours sous /mnt/host9p ; /mnt/host est servi par solonfs
        // (sauf réglage de repli, où le 9P garde /mnt/host).
        let nine_p_target =
            if settings::load_settings(&self.inner.cfg.paths.settings_file()).legacy_file_sharing {
                guest_root.clone()
            } else {
                format!("/mnt/host9p/{drive}")
            };
        let mut guard = self.inner.running.lock().await;
        let running = guard.as_mut().ok_or_else(|| {
            SolonError::new(ErrorCode::EngineUnreachable, "le moteur n'est pas démarré")
        })?;
        let mounted_now = if running.shares.contains_key(&drive) {
            false
        } else {
            let port = 9100 + running.shares.len() as u32;
            let share = solon_core::vm::HostShare {
                name: drive.clone(),
                host_path: host_root.clone().into(),
                port,
                read_only: false,
            };
            let vm = running.vm.clone();
            let s = share.clone();
            tokio::task::spawn_blocking(move || vm.add_share(&s))
                .await
                .map_err(|e| SolonError::internal(e.to_string()))??;
            running
                .agent
                .call(
                    Command::MountShare(solon_core::protocol::MountShareRequest {
                        name: drive.clone(),
                        port,
                        target: nine_p_target,
                        read_only: false,
                        extra_options: String::new(),
                    }),
                    Duration::from_secs(30),
                )
                .await?;
            running.shares.insert(drive.clone(), share);
            tracing::info!(drive, port, "lecteur partagé vers la machine");
            let state_path = self.inner.cfg.paths.state_file();
            let mut st = settings::load_state(&state_path);
            if !st.shares.contains(&drive) {
                st.shares.push(drive.clone());
                st.updated_unix_ms = settings::now_unix_ms();
                if let Err(e) = settings::save_state(&state_path, &st) {
                    tracing::warn!("état des partages non enregistré : {e}");
                }
            }
            true
        };
        Ok(solon_core::ipc::ShareInfo {
            drive,
            host_root,
            guest_root,
            guest_path,
            mounted_now,
        })
    }

    pub async fn list_shares(&self) -> Vec<solon_core::vm::HostShare> {
        self.inner
            .running
            .lock()
            .await
            .as_ref()
            .map(|r| r.shares.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Santé rapportée par l'agent (None si le moteur ne tourne pas).
    pub async fn health(&self) -> Option<HealthReport> {
        let agent = self
            .inner
            .running
            .lock()
            .await
            .as_ref()
            .map(|r| r.agent.clone())?;
        agent
            .call_typed(Command::Health, Duration::from_secs(5))
            .await
            .ok()
    }

    pub async fn guest_guid(&self) -> Option<GUID> {
        self.inner.running.lock().await.as_ref().map(|r| r.guid)
    }
}

/// Lit la console série de l'invité (pipe servi par vmwp) et la journalise (cible `guest`).
fn spawn_console_logger() -> JoinHandle<()> {
    tokio::task::spawn_blocking(|| {
        use std::io::Read;
        let deadline = Instant::now() + Duration::from_secs(15);
        let file = loop {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(CONSOLE_PIPE)
            {
                Ok(f) => break Some(f),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(e) => {
                    tracing::warn!("console invité inaccessible : {e}");
                    break None;
                }
            }
        };
        let Some(mut file) = file else { return };
        let mut buf = [0u8; 4096];
        let mut pending = Vec::new();
        loop {
            match file.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = pending.drain(..=pos).collect();
                        let text = String::from_utf8_lossy(&line)
                            .trim_end_matches(['\r', '\n'])
                            .to_owned();
                        if text.contains("[solon-agent")
                            || text.contains("SOLON-")
                            || text.contains("level=error")
                            || text.contains("level=fatal")
                        {
                            tracing::info!(target: "guest", "{text}");
                        } else {
                            tracing::debug!(target: "guest", "{text}");
                        }
                    }
                }
            }
        }
    })
}

/// Partages à rétablir au démarrage : les lecteurs mémorisés qui existent encore, dans l'ordre d'ajout
/// (le port vsock de chacun est `9100 + index`, comme lors de l'ajout à chaud).
fn boot_shares(drives: &[String]) -> Vec<solon_core::vm::HostShare> {
    drives
        .iter()
        .enumerate()
        .filter_map(|(i, drive)| {
            let host_root = format!("{}:\\", drive.to_ascii_uppercase());
            if !std::path::Path::new(&host_root).exists() {
                tracing::warn!(drive, "lecteur partagé absent, ignoré");
                return None;
            }
            Some(solon_core::vm::HostShare {
                name: drive.clone(),
                host_path: host_root.into(),
                port: 9100 + i as u32,
                read_only: false,
            })
        })
        .collect()
}
