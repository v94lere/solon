//! Sockets Hyper-V (`AF_HYPERV`) côté hôte Windows.
//!
//! Un socket Hyper-V est un socket flux ordinaire une fois connecté : on l'enveloppe dans un
//! [`std::net::TcpStream`] (via `FromRawSocket`), ce qui donne gratuitement lecture/écriture,
//! délais, et la conversion vers `tokio::net::TcpStream::from_std`.
//!
//! Adressage : `SOCKADDR_HV { Family = AF_HYPERV, VmId, ServiceId }`. Pour un invité Linux,
//! `ServiceId` encode le port `AF_VSOCK` : `xxxxxxxx-facb-11e6-bd58-64006a7986d3` où les
//! 32 premiers bits valent le port. `VmId` est le `RuntimeId` du compute system HCS.

#![cfg(windows)]

use std::io;
use std::net::TcpStream;
use std::os::windows::io::FromRawSocket;
use std::sync::Once;
use std::time::{Duration, Instant};

use windows::Win32::Networking::WinSock::{
    AF_HYPERV, SOCK_STREAM, SOCKADDR, SOCKET_ERROR, WSADATA, WSAGetLastError, WSAStartup, closesocket, connect, socket,
};
use windows::core::GUID;

/// Protocole brut des sockets Hyper-V (`HV_PROTOCOL_RAW` dans hvsocket.h).
pub const HV_PROTOCOL_RAW: i32 = 1;

/// Modèle de GUID de service pour les invités Linux (`HV_GUID_VSOCK_TEMPLATE`).
pub const VSOCK_TEMPLATE_DATA2: u16 = 0xfacb;
pub const VSOCK_TEMPLATE_DATA3: u16 = 0x11e6;
pub const VSOCK_TEMPLATE_DATA4: [u8; 8] = [0xbd, 0x58, 0x64, 0x00, 0x6a, 0x79, 0x86, 0xd3];

/// `SOCKADDR_HV` (36 octets), déclaré ici pour ne pas dépendre de sa présence dans les bindings.
#[repr(C)]
#[derive(Clone, Copy)]
struct SockaddrHv {
    family: u16,
    reserved: u16,
    vm_id: GUID,
    service_id: GUID,
}

/// GUID de service correspondant à un port vsock Linux.
pub fn service_id_for_port(port: u32) -> GUID {
    GUID { data1: port, data2: VSOCK_TEMPLATE_DATA2, data3: VSOCK_TEMPLATE_DATA3, data4: VSOCK_TEMPLATE_DATA4 }
}

fn ensure_winsock() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut data = WSADATA::default();
        // 2.2
        let rc = unsafe { WSAStartup(0x0202, &mut data) };
        if rc != 0 {
            tracing::error!(rc, "WSAStartup a échoué");
        }
    });
}

fn last_wsa_error(context: &str) -> io::Error {
    let code = unsafe { WSAGetLastError() }.0;
    io::Error::new(io::ErrorKind::Other, format!("{context} : WSA {code}"))
}

/// Ouvre une connexion vers `port` (vsock) dans la machine `vm_id`. Bloquant.
pub fn connect_once(vm_id: &GUID, port: u32) -> io::Result<TcpStream> {
    ensure_winsock();
    let sock = unsafe { socket(AF_HYPERV as i32, SOCK_STREAM, HV_PROTOCOL_RAW) }
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("socket(AF_HYPERV) : {e}")))?;
    let addr = SockaddrHv { family: AF_HYPERV, reserved: 0, vm_id: *vm_id, service_id: service_id_for_port(port) };
    let rc = unsafe { connect(sock, &addr as *const SockaddrHv as *const SOCKADDR, std::mem::size_of::<SockaddrHv>() as i32) };
    if rc == SOCKET_ERROR {
        let err = last_wsa_error(&format!("connect(hvsock port {port})"));
        unsafe { closesocket(sock) };
        return Err(err);
    }
    // SAFETY : socket flux valide et connecté ; TcpStream n'a besoin que d'un SOCKET.
    Ok(unsafe { TcpStream::from_raw_socket(sock.0 as _) })
}

/// Réessaie [`connect_once`] jusqu'à `timeout` (l'agent invité met un instant à écouter).
pub fn connect_with_retry(vm_id: &GUID, port: u32, timeout: Duration) -> io::Result<TcpStream> {
    let deadline = Instant::now() + timeout;
    loop {
        match connect_once(vm_id, port) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_de_service_encode_le_port() {
        let g = service_id_for_port(5000);
        assert_eq!(g.data1, 5000);
        assert_eq!(format!("{g:?}").to_lowercase(), "00001388-facb-11e6-bd58-64006a7986d3");
    }

    #[test]
    fn sockaddr_hv_fait_36_octets() {
        assert_eq!(std::mem::size_of::<SockaddrHv>(), 36);
    }
}
