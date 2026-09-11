//! Renseignements sur le PC Windows, côté application : ports TCP déjà pris (avant de créer une pile
//! qui les publie) et place sur le disque qui héberge le disque de données du moteur.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

/// Ports TCP sur lesquels un programme écoute déjà sur ce PC (IPv4 et IPv6, toutes adresses), lus
/// dans la table TCP de Windows (`GetExtendedTcpTable`). Lecture seule : ouvrir une socket d'écoute
/// pour tester aurait fait surgir la demande d'autorisation du pare-feu Windows pour `solon.exe`
/// (constaté sur la 0.1.2). Couvre les serveurs Windows (IIS, serveurs de développement, un autre
/// Docker) et les ports publiés par Solon, que le service réserve sur `0.0.0.0`.
fn listening_ports() -> HashSet<u16> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};
    let mut out = HashSet::new();
    for family in [AF_INET.0 as u32, AF_INET6.0 as u32] {
        let mut size = 0u32;
        // Premier appel : taille nécessaire (ERROR_INSUFFICIENT_BUFFER attendu).
        unsafe {
            GetExtendedTcpTable(
                None,
                &mut size,
                false,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            );
        }
        if size == 0 {
            continue;
        }
        let mut buf = vec![0u8; size as usize + 1024];
        let rc = unsafe {
            GetExtendedTcpTable(
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                false,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if rc != 0 || (buf.len() as u32) < size {
            continue;
        }
        // Les deux tables commencent par `dwNumEntries: u32`, suivi des lignes ; `dwLocalPort` est
        // dans l'ordre réseau, sur les 16 bits de poids faible.
        let count = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let row_size = if family == AF_INET.0 as u32 {
            std::mem::size_of::<MIB_TCPROW_OWNER_PID>()
        } else {
            std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>()
        };
        for i in 0..count {
            let base = 4 + i * row_size;
            if base + row_size > buf.len() {
                break;
            }
            let port_be = if family == AF_INET.0 as u32 {
                let row: MIB_TCPROW_OWNER_PID =
                    unsafe { std::ptr::read_unaligned(buf[base..].as_ptr().cast()) };
                row.dwLocalPort
            } else {
                let row: MIB_TCP6ROW_OWNER_PID =
                    unsafe { std::ptr::read_unaligned(buf[base..].as_ptr().cast()) };
                row.dwLocalPort
            };
            out.insert(u16::from_be((port_be & 0xffff) as u16));
        }
    }
    out
}

/// Premier port libre à partir de `from` (exclu), dans l'ordre croissant, pour proposer un remplaçant.
fn next_free_port(from: u16, taken: &[u16], listening: &HashSet<u16>) -> Option<u16> {
    let mut p = from.checked_add(1)?;
    for _ in 0..200 {
        if !taken.contains(&p) && !listening.contains(&p) {
            return Some(p);
        }
        p = p.checked_add(1)?;
    }
    None
}

#[derive(Debug, Serialize)]
pub struct PortProbe {
    pub port: u16,
    pub in_use: bool,
    /// Port libre à proposer à la place (le suivant disponible), si le port est pris.
    pub suggestion: Option<u16>,
}

/// Pour chaque port demandé : est-il déjà utilisé sur ce PC, et lequel proposer à la place.
/// Les suggestions ne se chevauchent pas entre elles ni avec les ports demandés.
#[tauri::command]
pub async fn ports_probe(ports: Vec<u16>) -> Vec<PortProbe> {
    tokio::task::spawn_blocking(move || {
        let listening = listening_ports();
        let mut reserved: Vec<u16> = ports.clone();
        let mut out = Vec::with_capacity(ports.len());
        for port in ports {
            let in_use = port != 0 && listening.contains(&port);
            let suggestion = if in_use {
                let s = next_free_port(port, &reserved, &listening);
                if let Some(s) = s {
                    reserved.push(s);
                }
                s
            } else {
                None
            };
            out.push(PortProbe {
                port,
                in_use,
                suggestion,
            });
        }
        out
    })
    .await
    .unwrap_or_default()
}

#[derive(Debug, Serialize)]
pub struct HostDiskInfo {
    /// Fichier du disque de données du moteur (`data.vhdx`).
    pub data_disk_path: String,
    /// Place réellement occupée par ce fichier sur le disque Windows (VHDX dynamique : il grossit
    /// avec l'usage et ne rend l'espace qu'après un nettoyage).
    pub data_disk_bytes: u64,
    /// Lecteur Windows qui l'héberge (`C:`), sa capacité et sa place libre.
    pub drive: String,
    pub drive_total_bytes: u64,
    pub drive_free_bytes: u64,
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

/// Chemin du disque de données : `%ProgramData%\Solon\data.vhdx` (ou l'ancien emplacement Monodon
/// tant que la migration n'a pas eu lieu).
fn data_disk_path() -> PathBuf {
    let base = program_data();
    let current = base.join("Solon").join("data.vhdx");
    if current.exists() {
        return current;
    }
    let legacy = base.join("Monodon").join("data.vhdx");
    if legacy.exists() {
        return legacy;
    }
    current
}

/// Capacité et place libre du lecteur qui contient `path` (`GetDiskFreeSpaceExW`).
fn drive_space(path: &std::path::Path) -> Result<(String, u64, u64), String> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::HSTRING;
    let root = path
        .components()
        .next()
        .map(|c| {
            let mut s = c.as_os_str().to_string_lossy().into_owned();
            if !s.ends_with('\\') {
                s.push('\\');
            }
            s
        })
        .unwrap_or_else(|| r"C:\".to_owned());
    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(
            &HSTRING::from(root.as_str()),
            Some(&mut free_to_caller),
            Some(&mut total),
            Some(&mut free),
        )
    }
    .map_err(|e| format!("GetDiskFreeSpaceEx({root}) : {e}"))?;
    Ok((
        root.trim_end_matches('\\').to_owned(),
        total,
        free_to_caller,
    ))
}

/// Place occupée par le disque de données et place libre sur le lecteur Windows qui l'héberge.
#[tauri::command]
pub async fn host_disk_info() -> Result<HostDiskInfo, String> {
    tokio::task::spawn_blocking(|| {
        let path = data_disk_path();
        let data_disk_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let (drive, total, free) = drive_space(&path)?;
        Ok(HostDiskInfo {
            data_disk_path: path.to_string_lossy().into_owned(),
            data_disk_bytes,
            drive,
            drive_total_bytes: total,
            drive_free_bytes: free,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_pris_et_suivant_libre() {
        // Une socket d'écoute ouverte par le test lui-même doit apparaître dans la table TCP.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let listening = listening_ports();
        assert!(listening.contains(&port), "port {port} absent de la table");
        let next = next_free_port(port, &[port], &listening).unwrap();
        assert!(next > port);
        assert!(!listening.contains(&next));
        drop(l);
        assert!(!listening_ports().contains(&port));
    }

    #[test]
    fn place_sur_le_lecteur() {
        let (drive, total, free) = drive_space(&program_data()).unwrap();
        assert!(drive.ends_with(':'));
        assert!(total > 0 && free <= total);
    }
}
