//! Client du canal de contrôle du service Windows (`\\.\pipe\solon-control`).
//!
//! Chaque appel ouvre sa propre connexion (simple, sans état à réparer). L'abonnement aux
//! événements garde une connexion ouverte et pousse chaque événement dans un `Channel` Tauri ;
//! il se reconnecte tout seul et signale l'absence du service par un événement synthétique
//! `service_unavailable`.

use std::time::Duration;

use serde_json::Value;
use solon_core::ipc::{CONTROL_PIPE, IpcRequest, ServiceCommand};
use solon_core::protocol::Response;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

async fn open() -> Result<NamedPipeClient, String> {
    ClientOptions::new()
        .open(CONTROL_PIPE)
        .map_err(|e| format!("service injoignable ({CONTROL_PIPE}) : {e}"))
}

/// Envoie une commande et renvoie le résultat JSON.
pub async fn call(command: ServiceCommand) -> Result<Value, String> {
    let pipe = open().await?;
    let (read, mut write) = tokio::io::split(pipe);
    let mut reader = BufReader::new(read);
    let mut line = serde_json::to_string(&IpcRequest { id: 1, request: command }).map_err(|e| e.to_string())?;
    line.push('\n');
    write.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut reply = String::new();
    tokio::time::timeout(Duration::from_secs(180), reader.read_line(&mut reply))
        .await
        .map_err(|_| "délai dépassé en attendant le service".to_string())?
        .map_err(|e| e.to_string())?;
    let response: Response = serde_json::from_str(reply.trim()).map_err(|e| format!("réponse illisible : {e}"))?;
    if response.ok {
        Ok(response.result.unwrap_or(Value::Null))
    } else {
        Err(response.error.unwrap_or_else(|| "erreur du service".into()))
    }
}

/// Abonnement aux événements du service. Ne rend la main que lorsque le canal Tauri est fermé
/// (la vue s'est démontée) ; entre-temps, reconnexion toutes les 2 s si le service est absent.
pub async fn subscribe(channel: Channel<Value>) {
    loop {
        match subscribe_once(&channel).await {
            Ok(()) => return, // canal fermé côté interface
            Err(e) => {
                tracing::debug!("abonnement au service interrompu : {e}");
                let unavailable = serde_json::json!({ "event": "service_unavailable", "detail": e });
                if channel.send(unavailable).is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn subscribe_once(channel: &Channel<Value>) -> Result<(), String> {
    let pipe = open().await?;
    let (read, mut write) = tokio::io::split(pipe);
    let mut reader = BufReader::new(read);
    let mut line = serde_json::to_string(&IpcRequest { id: 1, request: ServiceCommand::Subscribe }).map_err(|e| e.to_string())?;
    line.push('\n');
    write.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connexion fermée par le service".into());
        }
        let value: Value = match serde_json::from_str(buf.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("id").is_some() {
            continue; // réponse au Subscribe
        }
        if channel.send(value).is_err() {
            return Ok(());
        }
    }
}
