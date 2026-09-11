//! Renseignements sur le PC Windows, côté application : ports TCP déjà pris (avant de créer une pile
//! qui les publie) et place sur le disque qui héberge le disque de données du moteur.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::PathBuf;

use serde::Serialize;

/// Un port est « pris » si aucune socket ne peut s'y attacher sur toutes les adresses : serveur
/// Windows local (IIS, un autre Docker, un serveur de développement) ou port déjà publié par Solon.
fn port_in_use(port: u16) -> bool {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).is_err()
        || TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_err()
}

/// Premier port libre à partir de `from` (exclu), dans l'ordre croissant, pour proposer un remplaçant.
fn next_free_port(from: u16, taken: &[u16]) -> Option<u16> {
    let mut p = from.checked_add(1)?;
    for _ in 0..200 {
        if !taken.contains(&p) && !port_in_use(p) {
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
        let mut reserved: Vec<u16> = ports.clone();
        let mut out = Vec::with_capacity(ports.len());
        for port in ports {
            let in_use = port != 0 && port_in_use(port);
            let suggestion = if in_use {
                let s = next_free_port(port, &reserved);
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
    Ok((root.trim_end_matches('\\').to_owned(), total, free_to_caller))
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
        let l = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(port_in_use(port));
        let next = next_free_port(port, &[port]).unwrap();
        assert!(next > port);
        assert!(!port_in_use(next));
        drop(l);
    }

    #[test]
    fn place_sur_le_lecteur() {
        let (drive, total, free) = drive_space(&program_data()).unwrap();
        assert!(drive.ends_with(':'));
        assert!(total > 0 && free <= total);
    }
}
