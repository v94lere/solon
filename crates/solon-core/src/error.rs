//! Erreurs à codes stables.
//!
//! Chaque échec visible par l'utilisateur porte un [`ErrorCode`] stable, documenté dans le README
//! (section dépannage). Le message est destiné aux journaux ; le texte affiché dans l'interface est
//! traduit à partir du code, jamais à partir du message.

use serde::{Deserialize, Serialize};

/// Codes d'erreur stables. Ne jamais renuméroter ni réutiliser un code retiré.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    // --- Prérequis système ---
    /// La virtualisation matérielle (VT-x / AMD-V) est désactivée dans le BIOS/UEFI.
    VirtualizationDisabledInFirmware,
    /// Édition Windows non prise en charge (Famille).
    UnsupportedWindowsEdition,
    /// Une fonctionnalité Windows requise est désactivée.
    WindowsFeatureMissing,
    /// L'activation d'une fonctionnalité est refusée par une stratégie d'entreprise.
    WindowsFeatureBlockedByPolicy,
    /// L'hyperviseur Windows n'est pas démarré (autre hyperviseur, BCD, VM imbriquée).
    HypervisorNotRunning,
    /// Le service Host Compute (vmcompute) ou HNS est arrêté ou absent.
    HostComputeServiceUnavailable,
    /// Un logiciel de sécurité bloque l'accès aux fichiers ou aux binaires de Solon.
    BlockedBySecuritySoftware,
    /// Droits insuffisants pour créer ou piloter la machine (Administrateur requis).
    InsufficientPrivileges,

    // --- Cycle de vie de la machine ---
    /// Le document de configuration a été refusé par HCS.
    VmConfigurationRejected,
    /// La machine n'a pas démarré dans le délai imparti.
    VmBootTimeout,
    /// L'agent invité ne répond pas.
    AgentUnreachable,
    /// Le moteur de conteneurs ne répond pas.
    EngineUnreachable,
    /// L'image Linux est absente ou corrompue (SHA-256 invalide).
    ImageCorrupted,
    /// Le disque de données n'a pas pu être créé ou ouvert.
    DataDiskError,
    /// Erreur HCS non classée : voir `hresult` dans le message.
    HcsError,

    // --- Divers ---
    /// Erreur d'entrée/sortie côté hôte.
    Io,
    /// Erreur interne (bug) : ne devrait jamais être vue par l'utilisateur.
    Internal,
}

impl ErrorCode {
    /// Tous les codes, pour vérifier la couverture des traductions et de la documentation.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::VirtualizationDisabledInFirmware,
        ErrorCode::UnsupportedWindowsEdition,
        ErrorCode::WindowsFeatureMissing,
        ErrorCode::WindowsFeatureBlockedByPolicy,
        ErrorCode::HypervisorNotRunning,
        ErrorCode::HostComputeServiceUnavailable,
        ErrorCode::BlockedBySecuritySoftware,
        ErrorCode::InsufficientPrivileges,
        ErrorCode::VmConfigurationRejected,
        ErrorCode::VmBootTimeout,
        ErrorCode::AgentUnreachable,
        ErrorCode::EngineUnreachable,
        ErrorCode::ImageCorrupted,
        ErrorCode::DataDiskError,
        ErrorCode::HcsError,
        ErrorCode::Io,
        ErrorCode::Internal,
    ];
}

/// Erreur principale de Solon.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("[{code:?}] {message}")]
pub struct SolonError {
    pub code: ErrorCode,
    pub message: String,
    /// HRESULT Windows d'origine, s'il y en a un (format `0x8037xxxx`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hresult: Option<u32>,
}

impl SolonError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hresult: None,
        }
    }

    pub fn with_hresult(mut self, hresult: u32) -> Self {
        self.hresult = Some(hresult);
        self
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl From<std::io::Error> for SolonError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, e.to_string())
    }
}

impl From<serde_json::Error> for SolonError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorCode::Internal, format!("JSON : {e}"))
    }
}

pub type Result<T> = std::result::Result<T, SolonError>;
