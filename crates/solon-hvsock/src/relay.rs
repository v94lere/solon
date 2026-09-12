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

/// Descripteur de sécurité par défaut du pipe Docker : SYSTEM et Administrateurs en contrôle
/// total, utilisateurs **interactifs** (session ouverte) en lecture/écriture : l'application et le
/// CLI `docker`, non élevés, s'y connectent ; les comptes de service n'y ont pas accès.
pub const DOCKER_PIPE_SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

/// Crée une instance de serveur de pipe avec un descripteur de sécurité SDDL (mode octets).
pub fn create_server(
    pipe_name: &str,
    first: bool,
    sddl: Option<&str>,
) -> io::Result<NamedPipeServer> {
    create_server_with_mode(pipe_name, first, sddl, false)
}

/// Comme [`create_server`], avec le choix du **mode message**. C'est le mode de `dockerd` sous Windows :
/// le client Docker (go-winio) n'a de « fermeture de l'écriture » qu'en mode message, où il l'exprime
/// par un message vide. Sans cela, `docker exec -i … tar -xf -` ne voit jamais la fin de l'entrée
/// standard et attend indéfiniment (constaté avec VS Code Dev Containers, qui installe son serveur ainsi).
pub fn create_server_with_mode(
    pipe_name: &str,
    first: bool,
    sddl: Option<&str>,
    message_mode: bool,
) -> io::Result<NamedPipeServer> {
    let mut opts = ServerOptions::new();
    opts.first_pipe_instance(first);
    if message_mode {
        opts.pipe_mode(tokio::net::windows::named_pipe::PipeMode::Message)
            .in_buffer_size(64 * 1024)
            .out_buffer_size(64 * 1024);
    }
    match sddl {
        None => opts.create(pipe_name),
        Some(sddl) => {
            use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
            use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
            use windows::core::HSTRING;
            let mut sd = PSECURITY_DESCRIPTOR::default();
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    &HSTRING::from(sddl),
                    1,
                    &mut sd,
                    None,
                )
            }
            .map_err(|e| io::Error::other(format!("SDDL invalide : {e}")))?;
            let mut attrs = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            // SAFETY : `attrs` vit jusqu'à la fin de l'appel ; le descripteur est libéré ensuite.
            let r = unsafe {
                opts.create_with_security_attributes_raw(
                    pipe_name,
                    &mut attrs as *mut _ as *mut std::ffi::c_void,
                )
            };
            unsafe {
                windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
                    sd.0,
                )));
            }
            r
        }
    }
}

/// Sert `pipe_name` (ex. `\\.\pipe\solon`) indéfiniment. À lancer dans une tâche Tokio.
pub async fn serve_named_pipe(pipe_name: String, vm_id: GUID, port: u32) -> io::Result<()> {
    serve_named_pipe_with_sddl(pipe_name, vm_id, port, None).await
}

/// Comme [`serve_named_pipe`], avec un descripteur de sécurité (voir [`DOCKER_PIPE_SDDL`]).
pub async fn serve_named_pipe_with_sddl(
    pipe_name: String,
    vm_id: GUID,
    port: u32,
    sddl: Option<&str>,
) -> io::Result<()> {
    let mut server = create_server(&pipe_name, true, sddl)?;
    tracing::info!(pipe = %pipe_name, port, "relais à l'écoute");
    loop {
        server.connect().await?;
        let connected = server;
        server = create_server(&pipe_name, false, sddl)?;
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
    let std_stream = tokio::task::spawn_blocking(move || {
        super::connect_with_retry(&vm_id, port, Duration::from_secs(5))
    })
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
