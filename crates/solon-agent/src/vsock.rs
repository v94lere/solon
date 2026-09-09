//! Sockets `AF_VSOCK` (transport `hv_sock`) : écoute côté invité et connexion vers l'hôte (CID 2).

use std::fs::File;
use std::os::fd::{FromRawFd, RawFd};

fn errno(context: &str) -> String {
    format!("{context} : {}", std::io::Error::last_os_error())
}

fn socket() -> Result<RawFd, String> {
    let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        Err(errno("socket(AF_VSOCK)"))
    } else {
        Ok(fd)
    }
}

fn addr(cid: u32, port: u32) -> libc::sockaddr_vm {
    let mut a: libc::sockaddr_vm = unsafe { std::mem::zeroed() };
    a.svm_family = libc::AF_VSOCK as libc::sa_family_t;
    a.svm_cid = cid;
    a.svm_port = port;
    a
}

const ADDR_LEN: u32 = std::mem::size_of::<libc::sockaddr_vm>() as u32;

pub fn listen(port: u32) -> Result<RawFd, String> {
    let fd = socket()?;
    let a = addr(libc::VMADDR_CID_ANY, port);
    if unsafe { libc::bind(fd, &a as *const _ as *const libc::sockaddr, ADDR_LEN) } < 0 {
        return Err(errno(&format!("bind(vsock:{port})")));
    }
    if unsafe { libc::listen(fd, 16) } < 0 {
        return Err(errno("listen(vsock)"));
    }
    Ok(fd)
}

/// Accepte une connexion et la renvoie sous forme de `File` (lecture/écriture).
pub fn accept(listen_fd: RawFd) -> Result<File, String> {
    let fd = unsafe {
        libc::accept4(
            listen_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            libc::SOCK_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(errno("accept(vsock)"))
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

/// Connexion vers l'hôte (CID 2), utilisée pour rejoindre le serveur 9P de HCS.
pub fn connect_host(port: u32) -> Result<RawFd, String> {
    let fd = socket()?;
    let a = addr(libc::VMADDR_CID_HOST, port);
    if unsafe { libc::connect(fd, &a as *const _ as *const libc::sockaddr, ADDR_LEN) } < 0 {
        let e = errno(&format!("connect(vsock host:{port})"));
        unsafe { libc::close(fd) };
        return Err(e);
    }
    Ok(fd)
}
