//! Canal de contrôle du service Windows, côté application : les appels viennent de
//! `solon_client::control`, et l'abonnement aux événements pousse chaque événement dans un
//! `Channel` Tauri. Il se reconnecte tout seul et signale l'absence du service par un événement
//! synthétique `service_unavailable`.

use std::time::Duration;

use serde_json::Value;
use solon_core::ipc::ServiceCommand;
use tauri::ipc::Channel;

pub use solon_client::control::call;

/// Abonnement aux événements du service. Ne rend la main que lorsque le canal Tauri est fermé
/// (la vue s'est démontée) ; entre-temps, reconnexion toutes les 2 s si le service est absent.
pub async fn subscribe(channel: Channel<Value>) {
    stream(channel, ServiceCommand::Subscribe).await
}

async fn stream(channel: Channel<Value>, command: ServiceCommand) {
    loop {
        let result = solon_client::control::stream(command.clone(), None, |value| {
            channel.send(value).is_ok()
        })
        .await;
        match result {
            Ok(()) => return, // canal fermé côté interface
            Err(e) => {
                tracing::debug!("abonnement au service interrompu : {e}");
                let unavailable =
                    serde_json::json!({ "event": "service_unavailable", "detail": e });
                if channel.send(unavailable).is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
