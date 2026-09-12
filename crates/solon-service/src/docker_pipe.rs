//! Serveur du pipe Docker (`\\.\pipe\solon`) en **mode message**, avec des entrées-sorties Windows
//! directes plutôt que le pipe nommé de tokio.
//!
//! Pourquoi : le client Docker de Windows (go-winio) ne sait signaler « j'ai fini d'écrire » (fin de
//! l'entrée standard d'un `docker exec -i`) qu'en mode message, par un message vide ; c'est le mode de
//! `dockerd` sous Windows. Or mio, sous tokio, perd les données d'un message plus long que son tampon
//! interne de 4 Kio (`ERROR_MORE_DATA` synchrone traité comme un tampon vide) : un `tar | docker exec -i`
//! de 200 Ko n'en livrait que 4 096 octets. Ici, chaque connexion est servie par deux fils bloquants
//! (lecture du pipe → flux tokio, flux tokio → écriture du pipe) sur un descripteur en mode recouvrement,
//! ce qui autorise une lecture et une écriture simultanées, avec `ERROR_MORE_DATA` géré comme il faut.
//! Le mandataire HTTP (`docker_proxy`) voit un [`tokio::io::DuplexStream`] ordinaire.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_MORE_DATA, ERROR_PIPE_CONNECTED,
    HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAGS_AND_ATTRIBUTES, FlushFileBuffers, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::CreateEventW;
use windows::core::HSTRING;

const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const BUF: usize = 64 * 1024;

/// Descripteur du pipe, fermé quand les deux fils d'une connexion ont fini.
struct Pipe(HANDLE);
unsafe impl Send for Pipe {}
unsafe impl Sync for Pipe {}
impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe {
            let _ = DisconnectNamedPipe(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

/// Événement Win32 pour attendre une opération en recouvrement (un par fil).
struct Event(HANDLE);
impl Event {
    fn new() -> windows::core::Result<Self> {
        Ok(Self(unsafe { CreateEventW(None, true, false, None) }?))
    }
}
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn is(e: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    e.code() == code.to_hresult()
}

/// Résultat d'une lecture : octets lus, et « la suite du message arrive » (`ERROR_MORE_DATA`).
fn read_overlapped(
    h: HANDLE,
    ev: &Event,
    buf: &mut [u8],
) -> Result<(usize, bool), windows::core::Error> {
    let mut ov = OVERLAPPED {
        hEvent: ev.0,
        ..Default::default()
    };
    let mut n = 0u32;
    let r = unsafe { ReadFile(h, Some(buf), Some(&mut n), Some(&mut ov)) };
    match r {
        Ok(()) => Ok((n as usize, false)),
        Err(e) if is(&e, ERROR_MORE_DATA) => Ok((n as usize, true)),
        Err(e) if is(&e, ERROR_IO_PENDING) => {
            let mut done = 0u32;
            match unsafe { GetOverlappedResult(h, &ov, &mut done, true) } {
                Ok(()) => Ok((done as usize, false)),
                Err(e) if is(&e, ERROR_MORE_DATA) => Ok((done as usize, true)),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

fn write_overlapped(h: HANDLE, ev: &Event, mut data: &[u8]) -> Result<(), windows::core::Error> {
    while !data.is_empty() {
        let mut ov = OVERLAPPED {
            hEvent: ev.0,
            ..Default::default()
        };
        let mut n = 0u32;
        let r = unsafe { WriteFile(h, Some(data), Some(&mut n), Some(&mut ov)) };
        let written = match r {
            Ok(()) => n as usize,
            Err(e) if is(&e, ERROR_IO_PENDING) => {
                let mut done = 0u32;
                unsafe { GetOverlappedResult(h, &ov, &mut done, true) }?;
                done as usize
            }
            Err(e) => return Err(e),
        };
        data = &data[written.min(data.len())..];
        if written == 0 {
            break;
        }
    }
    Ok(())
}

fn security_attributes(
    sddl: &str,
) -> windows::core::Result<(SECURITY_ATTRIBUTES, PSECURITY_DESCRIPTOR)> {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &HSTRING::from(sddl),
            1,
            &mut sd,
            None,
        )?;
    }
    Ok((
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        },
        sd,
    ))
}

fn create_instance(name: &str, first: bool, sddl: &str) -> Result<HANDLE, String> {
    let (attrs, sd) = security_attributes(sddl).map_err(|e| format!("SDDL invalide : {e}"))?;
    let mut open = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let h = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name),
            FILE_FLAGS_AND_ATTRIBUTES(open),
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            BUF as u32,
            BUF as u32,
            0,
            Some(&attrs as *const SECURITY_ATTRIBUTES),
        )
    };
    unsafe {
        windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(sd.0)));
    }
    if h == INVALID_HANDLE_VALUE {
        return Err(format!(
            "CreateNamedPipe({name}) : {}",
            windows::core::Error::from_thread()
        ));
    }
    Ok(h)
}

/// Attend un client sur l'instance `h` (en recouvrement, bloquant pour ce fil).
fn wait_client(h: HANDLE, ev: &Event) -> Result<(), windows::core::Error> {
    let mut ov = OVERLAPPED {
        hEvent: ev.0,
        ..Default::default()
    };
    match unsafe { ConnectNamedPipe(h, Some(&mut ov)) } {
        Ok(()) => Ok(()),
        Err(e) if is(&e, ERROR_PIPE_CONNECTED) => Ok(()),
        Err(e) if is(&e, ERROR_IO_PENDING) => {
            let mut n = 0u32;
            unsafe { GetOverlappedResult(h, &ov, &mut n, true) }
        }
        Err(e) => Err(e),
    }
}

/// Boucle d'acceptation (fil bloquant) : pour chaque client, un flux tokio est remis à `on_connect`
/// et deux fils relaient les octets entre le pipe et ce flux.
pub fn accept_loop(
    name: &'static str,
    sddl: &'static str,
    rt: tokio::runtime::Handle,
    on_connect: tokio::sync::mpsc::UnboundedSender<DuplexStream>,
) {
    let ev = match Event::new() {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("pipe Docker : événement : {e}");
            return;
        }
    };
    let mut first = true;
    let mut failures = 0u32;
    loop {
        let h = match create_instance(name, first, sddl) {
            Ok(h) => h,
            Err(e) => {
                // Première instance refusée (un ancien processus du service qui finit de s'arrêter
                // tient encore le nom) : on insiste, le pipe doit exister pour toute la vie du service.
                failures += 1;
                if failures == 1 || failures % 30 == 0 {
                    tracing::warn!("pipe Docker : {e} (tentative {failures})");
                }
                std::thread::sleep(std::time::Duration::from_millis(if first {
                    1000
                } else {
                    200
                }));
                continue;
            }
        };
        failures = 0;
        first = false;
        if let Err(e) = wait_client(h, &ev) {
            tracing::debug!("pipe Docker : connexion : {e}");
            drop(Pipe(h));
            continue;
        }
        let (client_side, proxy_side) = tokio::io::duplex(4 * BUF);
        if on_connect.send(proxy_side).is_err() {
            drop(Pipe(h));
            return;
        }
        let pipe = Arc::new(Pipe(h));
        let reader_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (mut rx, mut tx) = tokio::io::split(client_side);
        // Pipe → flux : un message vide (fin d'écriture côté client) ou une erreur ferme le flux.
        {
            let pipe = pipe.clone();
            let rt = rt.clone();
            let reader_done = reader_done.clone();
            std::thread::spawn(move || {
                let Ok(ev) = Event::new() else { return };
                let mut buf = vec![0u8; BUF];
                loop {
                    match read_overlapped(pipe.0, &ev, &mut buf) {
                        Ok((0, false)) => break,
                        Ok((n, _more)) => {
                            if rt.block_on(tx.write_all(&buf[..n])).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            if !is(&e, ERROR_BROKEN_PIPE) {
                                tracing::debug!("pipe Docker : lecture : {e}");
                            }
                            break;
                        }
                    }
                }
                let _ = rt.block_on(tx.shutdown());
                reader_done.store(true, std::sync::atomic::Ordering::SeqCst);
            });
        }
        // Flux → pipe : la fin du flux (dockerd a fermé) déconnecte le client.
        {
            let rt = rt.clone();
            std::thread::spawn(move || {
                let Ok(ev) = Event::new() else { return };
                let mut buf = vec![0u8; BUF];
                loop {
                    match rt.block_on(rx.read(&mut buf)) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if let Err(e) = write_overlapped(pipe.0, &ev, &buf[..n]) {
                                tracing::debug!("pipe Docker : écriture : {e}");
                                break;
                            }
                        }
                    }
                }
                // Fin propre, comme dockerd : un message vide dit au client « fin de la réponse » (son
                // Read renvoie EOF au lieu de « No process is on the other end of the pipe »), puis on
                // lui laisse jusqu'à cinq secondes pour fermer avant de déconnecter l'instance.
                let mut ov = OVERLAPPED {
                    hEvent: ev.0,
                    ..Default::default()
                };
                let mut n = 0u32;
                let r = unsafe { WriteFile(pipe.0, Some(&[]), Some(&mut n), Some(&mut ov)) };
                if let Err(e) = r {
                    if is(&e, ERROR_IO_PENDING) {
                        let mut done = 0u32;
                        let _ = unsafe { GetOverlappedResult(pipe.0, &ov, &mut done, true) };
                    }
                }
                let _ = unsafe { FlushFileBuffers(pipe.0) };
                for _ in 0..50 {
                    if reader_done.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                unsafe {
                    let _ = DisconnectNamedPipe(pipe.0);
                }
            });
        }
    }
}
