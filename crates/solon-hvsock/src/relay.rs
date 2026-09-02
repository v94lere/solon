//! Relais named pipe Windows → socket Hyper-V.
//!
//! Chaque client du pipe (`bollard`, le CLI `docker` avec `npipe://`) obtient sa propre connexion
//! vsock vers l'invité ; les octets sont copiés dans les deux sens sans interprétation, ce qui
//! préserve les connexions HTTP « hijackées » (`exec`, `attach`, flux de journaux).

use std::io;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows::core::GUID;

/// Sert `pipe_name` (ex. `\\.\pipe\solon`) indéfiniment. À lancer dans une tâche Tokio.
pub async fn serve_named_pipe(pipe_name: String, vm_id: GUID, port: u32) -> io::Result<()> {
    let mut server = ServerOptions::new().first_pipe_instance(true).create(&pipe_name)?;
    tracing::info!(pipe = %pipe_name, port, "relais à l'écoute");
    loop {
        server.connect().await?;
        let connected = server;
        server = ServerOptions::new().create(&pipe_name)?;
        tokio::spawn(async move {
            if let Err(e) = handle(connected, vm_id, port).await {
                tracing::debug!("connexion relais terminée : {e}");
            }
        });
    }
}

/// Un named pipe Windows n'a pas de demi-fermeture : le client ne voit la fin d'un flux
/// « hijacké » (`docker run`, `exec`, `logs -f`) que si le serveur **déconnecte** le pipe. On copie
/// donc les deux sens séparément et, dès que l'un des deux se termine, on ferme tout.
async fn handle(pipe: NamedPipeServer, vm_id: GUID, port: u32) -> io::Result<()> {
    let std_stream = tokio::task::spawn_blocking(move || super::connect_with_retry(&vm_id, port, Duration::from_secs(5)))
        .await
        .map_err(io::Error::other)??;
    std_stream.set_nonblocking(true)?;
    let hv = tokio::net::TcpStream::from_std(std_stream)?;

    let (mut pipe_read, mut pipe_write) = tokio::io::split(pipe);
    let (mut hv_read, mut hv_write) = hv.into_split();
    let to_guest = async {
        let n = tokio::io::copy(&mut pipe_read, &mut hv_write).await;
        let _ = hv_write.shutdown().await; // fin de flux propagée à dockerd
        n
    };
    let to_client = async {
        let n = tokio::io::copy(&mut hv_read, &mut pipe_write).await;
        let _ = pipe_write.flush().await;
        n
    };
    tokio::select! {
        r = to_guest => { tracing::trace!(octets = ?r, "client → invité terminé"); }
        r = to_client => { tracing::trace!(octets = ?r, "invité → client terminé"); }
    }
    // Sortie de portée : le pipe est déconnecté et le socket Hyper-V fermé, ce qui met fin à
    // l'autre sens.
    Ok(())
}
