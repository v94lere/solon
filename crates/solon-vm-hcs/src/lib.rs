//! Pilotage de la machine Solon via l'API Host Compute System (HCS) de Windows.
//!
//! Organisation :
//! - [`schema`] : le document JSON de configuration (schéma HCS v2.x), typé et testable ;
//! - [`hcs`] : enveloppes sûres autour de `computecore.dll` (opérations, compute systems, événements) ;
//! - [`hresult`] : traduction des HRESULT HCS en codes d'erreur stables Solon ;
//! - [`HcsVm`] : l'API de haut niveau utilisée par le service.
//!
//! Rappels HCS importants : un compute system est **éphémère** (il disparaît une fois arrêté),
//! la création exige Administrateur ou le groupe « Hyper-V Administrators », et un compute
//! system encore en marche peut être **rouvert** par son identifiant après redémarrage du
//! processus qui l'a créé.

#![cfg(windows)]

pub mod hcs;
pub mod hresult;
pub mod schema;

use std::time::Duration;

use solon_core::vm::{HostShare, VmConfig};
use solon_core::{ErrorCode, Result, SolonError};

pub use hcs::{ComputeSystem, HcsEvent, HcsEventKind};
pub use schema::{ComputeSystemDocument, ComputeSystemSummary};

/// Valeur du champ `Owner` de tous les compute systems créés par Solon. Sert à retrouver
/// les machines orphelines après un crash du service.
pub const OWNER: &str = "Solon";

/// Machine Solon pilotée par HCS.
pub struct HcsVm {
    id: String,
    system: ComputeSystem,
}

impl HcsVm {
    /// Crée le compute system (sans le démarrer) à partir d'une configuration.
    pub fn create(config: &VmConfig) -> Result<Self> {
        let document = ComputeSystemDocument::from_config(config);
        let json = serde_json::to_string(&document)?;
        tracing::debug!(id = %config.id, "document HCS : {json}");
        let system = ComputeSystem::create(&config.id, &json)?;
        Ok(Self { id: config.id.clone(), system })
    }

    /// Se rattache à un compute system encore en marche (après redémarrage du service).
    pub fn open(id: &str) -> Result<Self> {
        let system = ComputeSystem::open(id)?;
        Ok(Self { id: id.to_owned(), system })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn start(&self) -> Result<()> {
        self.system.start(Duration::from_secs(60))
    }

    /// Demande un arrêt par l'hyperviseur. Sur une VM Linux sans services d'intégration
    /// c'est rarement honoré : l'arrêt normal passe par l'agent invité (`poweroff`).
    pub fn shutdown(&self) -> Result<()> {
        self.system.shutdown(Duration::from_secs(30))
    }

    /// Coupe l'alimentation virtuelle immédiatement.
    pub fn terminate(&self) -> Result<()> {
        self.system.terminate(Duration::from_secs(30))
    }

    /// Attend l'événement de sortie du compute system.
    pub fn wait_exit(&self, timeout: Duration) -> Option<HcsEvent> {
        self.system.wait_for(HcsEventKind::SystemExited, timeout)
    }

    /// Propriétés courantes (JSON brut HCS), utile au diagnostic.
    pub fn properties(&self) -> Result<String> {
        self.system.properties(Duration::from_secs(10))
    }

    /// Identifiant d'exécution de la machine : c'est **lui** (et non l'`Id`) qu'attend
    /// `SOCKADDR_HV.VmId` pour ouvrir un socket Hyper-V vers l'invité.
    pub fn runtime_id(&self) -> Result<String> {
        let props: serde_json::Value = serde_json::from_str(&self.properties()?)?;
        props
            .get("RuntimeId")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| SolonError::internal(format!("RuntimeId absent des propriétés HCS : {props}")))
    }

    /// Ajoute un partage 9P à chaud. L'invité doit ensuite le monter (RPC agent).
    pub fn add_share(&self, share: &HostShare) -> Result<()> {
        let req = schema::ModifySettingRequest {
            resource_path: schema::PLAN9_SHARES_RESOURCE_PATH.into(),
            request_type: schema::RequestType::Add,
            settings: Some(schema::Plan9Share::from_host_share(share)),
        };
        self.system.modify(&serde_json::to_string(&req)?, Duration::from_secs(30)).map(|_| ())
    }

    /// Retire un partage 9P à chaud (l'invité doit l'avoir démonté avant).
    pub fn remove_share(&self, share: &HostShare) -> Result<()> {
        let req = schema::ModifySettingRequest {
            resource_path: schema::PLAN9_SHARES_RESOURCE_PATH.into(),
            request_type: schema::RequestType::Remove,
            settings: Some(schema::Plan9Share::from_host_share(share)),
        };
        self.system.modify(&serde_json::to_string(&req)?, Duration::from_secs(30)).map(|_| ())
    }

    /// Liste les compute systems appartenant à Solon, quel que soit le processus créateur.
    pub fn list_owned() -> Result<Vec<ComputeSystemSummary>> {
        let query = serde_json::json!({ "Owners": [OWNER] }).to_string();
        ComputeSystem::enumerate(&query, Duration::from_secs(10))
    }

    /// Termine toutes les machines Solon dont l'identifiant n'est pas `keep`.
    /// Retourne les identifiants terminés.
    pub fn terminate_orphans(keep: Option<&str>) -> Result<Vec<String>> {
        let mut terminated = Vec::new();
        for summary in Self::list_owned()? {
            if Some(summary.id.as_str()) == keep {
                continue;
            }
            tracing::warn!(id = %summary.id, state = ?summary.state, "machine orpheline détectée, terminaison");
            match ComputeSystem::open(&summary.id) {
                Ok(system) => {
                    if let Err(e) = system.terminate(Duration::from_secs(30)) {
                        // Déjà arrêtée entre l'énumération et la terminaison : pas une erreur.
                        if e.code != ErrorCode::HcsError || e.hresult != Some(hresult::HCS_E_SYSTEM_ALREADY_STOPPED) {
                            return Err(e);
                        }
                    }
                    terminated.push(summary.id);
                }
                Err(e) if e.hresult == Some(hresult::HCS_E_SYSTEM_NOT_FOUND) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(terminated)
    }
}

/// Génère un identifiant de machine (GUID en minuscules, format attendu par HCS).
pub fn new_vm_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl std::fmt::Debug for HcsVm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HcsVm").field("id", &self.id).finish()
    }
}

