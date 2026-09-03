//! Réglages persistants (`settings.json`) et état persistant (`state.json`).

use std::path::Path;

use serde::{Deserialize, Serialize};
use solon_core::ipc::Settings;

/// Processeurs par défaut : tous les cœurs logiques moins deux (gardés pour Windows et l'application),
/// au moins 2. C'est la seule mesure où Docker Desktop devançait Solon (il prend tous les cœurs).
pub fn default_processors() -> u32 {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4) as u32;
    n.saturating_sub(2).clamp(2, 64)
}

/// Réglages par défaut de cette machine (`Settings::default()` ne connaît pas le matériel).
pub fn default_settings() -> Settings {
    Settings {
        processors: default_processors(),
        ..Settings::default()
    }
}

pub fn load_settings(path: &Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("settings.json illisible ({e}), valeurs par défaut");
            default_settings()
        }),
        Err(_) => default_settings(),
    }
}

pub fn save_settings(path: &Path, settings: &Settings) -> std::io::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(settings)?)
}

/// Ce que le service note sur disque pour détecter un arrêt non propre et se rattacher.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedState {
    /// Identifiant de la dernière machine créée (GUID).
    pub vm_id: Option<String>,
    pub endpoint_id: Option<String>,
    /// Adresse IPv4 de l'invité, pour l'afficher après un rattachement.
    #[serde(default)]
    pub guest_address: Option<String>,
    #[serde(default)]
    pub image_version: Option<String>,
    /// `true` dès que la machine est arrêtée proprement par le service.
    pub clean_shutdown: bool,
    pub updated_unix_ms: u64,
}

pub fn load_state(path: &Path) -> PersistedState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_state(path: &Path, state: &PersistedState) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(state)?)?;
    std::fs::rename(&tmp, path)
}

pub fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
