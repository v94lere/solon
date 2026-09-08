//! Disque de données : VHDX dynamique créé par Windows (`CreateVirtualDisk`, virtdisk.dll).
//! Le formatage ext4 est fait par l'agent invité au premier démarrage.

use std::path::Path;

use solon_core::{ErrorCode, Result, SolonError};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::Vhd::{
    CREATE_VIRTUAL_DISK_FLAG_NONE, CREATE_VIRTUAL_DISK_PARAMETERS, CREATE_VIRTUAL_DISK_VERSION_2,
    CreateVirtualDisk, VIRTUAL_DISK_ACCESS_NONE, VIRTUAL_STORAGE_TYPE,
    VIRTUAL_STORAGE_TYPE_DEVICE_VHDX, VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
};
use windows::core::{GUID, HSTRING};

pub const DEFAULT_DATA_DISK_GIB: u64 = 64;

/// Crée un VHDX dynamique de `max_size_gib` (espace réellement occupé : quelques Mo au départ).
/// Ne fait rien si le fichier existe déjà.
pub fn ensure_data_disk(path: &Path, max_size_gib: u64) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let storage_type = VIRTUAL_STORAGE_TYPE {
        DeviceId: VIRTUAL_STORAGE_TYPE_DEVICE_VHDX,
        VendorId: VIRTUAL_STORAGE_TYPE_VENDOR_MICROSOFT,
    };
    let mut params = CREATE_VIRTUAL_DISK_PARAMETERS {
        Version: CREATE_VIRTUAL_DISK_VERSION_2,
        ..Default::default()
    };
    params.Anonymous.Version2.UniqueId = GUID::new().unwrap_or_default();
    params.Anonymous.Version2.MaximumSize = max_size_gib * 1024 * 1024 * 1024;
    params.Anonymous.Version2.BlockSizeInBytes = 0; // défaut (32 Mo pour VHDX dynamique)
    params.Anonymous.Version2.SectorSizeInBytes = 512;
    params.Anonymous.Version2.PhysicalSectorSizeInBytes = 4096;

    let mut handle = windows::Win32::Foundation::HANDLE::default();
    let rc = unsafe {
        CreateVirtualDisk(
            &storage_type,
            &HSTRING::from(path.as_os_str()),
            VIRTUAL_DISK_ACCESS_NONE,
            None,
            CREATE_VIRTUAL_DISK_FLAG_NONE,
            0,
            &params,
            None,
            &mut handle,
        )
    };
    if rc.is_err() {
        let hresult = 0x8007_0000u32 | (rc.0 as u32 & 0xFFFF);
        return Err(SolonError::new(
            ErrorCode::DataDiskError,
            format!(
                "CreateVirtualDisk({}) : erreur Win32 {}",
                path.display(),
                rc.0
            ),
        )
        .with_hresult(hresult));
    }
    unsafe {
        let _ = CloseHandle(handle);
    }
    tracing::info!(path = %path.display(), max_size_gib, "disque de données créé");
    Ok(true)
}
