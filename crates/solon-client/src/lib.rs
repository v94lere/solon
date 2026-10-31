//! Côté client du service Solon, sans interface : ce que l'application de bureau et le CLI `solon`
//! font pareil. Parler au canal de contrôle ([`control`]), reconnaître un projet Compose
//! ([`compose`]), lister les conteneurs ([`containers`]), sauvegarder et restaurer un projet
//! ([`backup`]), exporter un diagnostic ([`diag`]), mettre à jour ([`update`]), copier les volumes
//! d'un projet ([`volumes`]), importer depuis Docker Desktop ([`import`]).
//!
//! Aucune dépendance à Tauri ici : l'application enveloppe ces fonctions dans ses commandes, le
//! CLI les appelle directement.

pub mod backup;
pub mod compose;
pub mod containers;
pub mod control;
pub mod diag;
pub mod import;
pub mod update;
pub mod volumes;

use std::process::Command;

/// Commande Windows sans fenêtre de console (`CREATE_NO_WINDOW`) : lancée depuis une application
/// graphique, une console noire clignoterait sinon à chaque appel.
pub fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// Dossier `%ProgramData%\Solon` : état, réglages et journaux du service.
pub fn program_data() -> std::path::PathBuf {
    std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"))
        .join("Solon")
}
