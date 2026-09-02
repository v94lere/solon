//! Description de la machine Solon, indépendante du moteur de virtualisation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// État observable de la machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmState {
    /// Aucune machine n'existe côté hyperviseur.
    Absent,
    /// Créée mais pas encore démarrée.
    Created,
    /// En cours d'exécution (le noyau tourne ; l'agent n'a pas forcément répondu).
    Running,
    /// Arrêt en cours.
    Stopping,
    /// Arrêtée (le compute system a disparu ou est en cours de nettoyage).
    Stopped,
}

/// Paramètres de démarrage d'une machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmConfig {
    /// Identifiant stable de la machine (GUID). Réutilisé à chaque démarrage pour permettre
    /// le rattachement et la détection des orphelines.
    pub id: String,
    /// Nom lisible, utile dans les outils de diagnostic.
    pub name: String,
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    pub cmdline: String,
    pub memory_mb: u64,
    pub processors: u32,
    /// Disques attachés en SCSI, dans l'ordre des LUN.
    pub disks: Vec<DiskAttachment>,
    /// Named pipe Windows recevant la console série (COM1), pour le diagnostic.
    pub serial_pipe: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskAttachment {
    pub path: PathBuf,
    pub read_only: bool,
}
