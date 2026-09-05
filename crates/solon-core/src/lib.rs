//! Modèles et interfaces communes de Solon, indépendants de la plateforme.
//!
//! Tout ce qui est spécifique à Windows (HCS, HvSocket, HNS) vit dans d'autres crates ;
//! celui-ci ne contient que les types partagés entre le service, l'application et l'agent.

pub mod error;
pub mod fs;
pub mod ipc;
pub mod protocol;
pub mod vm;

pub use error::{ErrorCode, Result, SolonError};
