//! Service Windows de Solon.
//!
//! Le service (compte LocalSystem) est le seul processus qui touche à HCS, HNS et virtdisk. Il :
//! - provisionne et supervise la machine ([`engine`]) ;
//! - expose l'API Docker de l'invité sur `\\.\pipe\solon` (relais HvSocket) ;
//! - relaie les ports publiés vers `localhost` ([`ports`]) ;
//! - sert le canal de contrôle de l'application sur `\\.\pipe\solon-control` ([`ipc`]).
//!
//! Il se lance aussi en console (`solon-service console`) pour le développement et les tests.

#![cfg(windows)]

pub mod agent_client;
pub mod disk;
pub mod docker_proxy;
pub mod domains;
pub mod engine;
pub mod fileserver;
pub mod ipc;
pub mod network;
pub mod paths;
pub mod ports;
pub mod scan;
pub mod settings;
pub mod sleep;
