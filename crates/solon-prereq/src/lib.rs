//! Détection des prérequis Windows. Aucune modification du système ici : l'activation des
//! fonctionnalités (DISM, redémarrage) appartient à l'installeur. Le service ne fait que constater
//! et renvoyer des codes stables ; l'interface les traduit en messages actionnables.
//!
//! Ordre des vérifications : édition Windows, virtualisation matérielle, service HCS (`vmcompute`)
//! et réseau (`hns`), API HCS réellement utilisable (c'est le test décisif : il détecte à la fois
//! l'hyperviseur non lancé, la fonctionnalité absente et les droits insuffisants), mémoire.

#![cfg(windows)]

use solon_core::ErrorCode;
use solon_core::ipc::{PrereqItem, PrereqReport};
use windows::Win32::System::SystemInformation::{
    GetProductInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, OS_PRODUCT_TYPE,
};
use windows::Win32::System::Threading::{
    IsProcessorFeaturePresent, PF_SECOND_LEVEL_ADDRESS_TRANSLATION, PF_VIRT_FIRMWARE_ENABLED,
};

/// SKU « Famille » (Core) : pas de Hyper-V complet, donc pas de partage 9P (voir ARCHITECTURE §13 R2).
const HOME_SKUS: &[u32] = &[
    0x65, // PRODUCT_CORE
    0x62, // PRODUCT_CORE_N
    0x63, // PRODUCT_CORE_COUNTRYSPECIFIC
    0x64, // PRODUCT_CORE_SINGLELANGUAGE
    0x98, // PRODUCT_CORE_ARM
    0xC5, // PRODUCT_CLOUDEDITION (Windows 11 SE) — même limitation
];

fn item(
    id: &str,
    ok: bool,
    blocking: bool,
    detail: impl Into<String>,
    code: Option<ErrorCode>,
) -> PrereqItem {
    PrereqItem {
        id: id.into(),
        ok,
        blocking,
        detail: detail.into(),
        code: if ok { None } else { code },
    }
}

pub fn product_sku() -> u32 {
    let mut sku = OS_PRODUCT_TYPE::default();
    // Version 10.0 : GetProductInfo lit la vraie édition quel que soit le manifeste de compatibilité.
    unsafe {
        let _ = GetProductInfo(10, 0, 0, 0, &mut sku);
    }
    sku.0
}

fn check_edition() -> PrereqItem {
    let sku = product_sku();
    let home = HOME_SKUS.contains(&sku);
    item(
        "windows_edition",
        !home,
        true,
        format!(
            "SKU Windows 0x{sku:X}{}",
            if home { " (édition Famille)" } else { "" }
        ),
        Some(ErrorCode::UnsupportedWindowsEdition),
    )
}

fn check_virtualization_firmware() -> PrereqItem {
    // Quand un hyperviseur tourne déjà (Hyper-V actif), Windows ne voit plus les extensions
    // directement ; PF_VIRT_FIRMWARE_ENABLED reste vrai sur les machines où la virtualisation
    // est activée et faux quand le BIOS la désactive.
    let firmware = unsafe { IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED) }.as_bool();
    let slat = unsafe { IsProcessorFeaturePresent(PF_SECOND_LEVEL_ADDRESS_TRANSLATION) }.as_bool();
    let ok = firmware || hypervisor_present();
    item(
        "virtualization_firmware",
        ok,
        true,
        format!(
            "VT-x/AMD-V dans le firmware : {firmware}, SLAT : {slat}, hyperviseur présent : {}",
            hypervisor_present()
        ),
        Some(ErrorCode::VirtualizationDisabledInFirmware),
    )
}

/// Un hyperviseur est présent si HCS répond ou si le noyau Windows tourne sous Hyper-V.
fn hypervisor_present() -> bool {
    // Bit 31 de CPUID.1:ECX = « hypervisor present ».
    #[cfg(target_arch = "x86_64")]
    {
        let r = std::arch::x86_64::__cpuid(1);
        (r.ecx >> 31) & 1 == 1
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn service_state(name: &str) -> Result<String, String> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| e.to_string())?;
    let service = manager
        .open_service(name, ServiceAccess::QUERY_STATUS)
        .map_err(|e| format!("absent ({e})"))?;
    let status = service.query_status().map_err(|e| e.to_string())?;
    Ok(format!("{:?}", status.current_state))
}

fn check_service(name: &str, id: &str) -> PrereqItem {
    match service_state(name) {
        Ok(state) => {
            // Les services HCS démarrent à la demande : « Stopped » n'est pas une erreur, « absent » l'est.
            item(id, true, true, format!("service {name} : {state}"), None)
        }
        Err(e) => item(
            id,
            false,
            true,
            format!("service {name} : {e}"),
            Some(ErrorCode::HostComputeServiceUnavailable),
        ),
    }
}

/// Le test décisif : l'API HCS répond-elle à un appel anodin ?
fn check_hcs() -> PrereqItem {
    match solon_vm_hcs::HcsVm::list_owned() {
        Ok(list) => item(
            "hcs_api",
            true,
            true,
            format!("HCS répond ({} machine(s) Solon)", list.len()),
            None,
        ),
        Err(e) => {
            let code = e.code;
            item("hcs_api", false, true, e.message.clone(), Some(code))
        }
    }
}

fn check_memory() -> PrereqItem {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) }.is_ok();
    let total_mb = status.ullTotalPhys / (1024 * 1024);
    let avail_mb = status.ullAvailPhys / (1024 * 1024);
    // Non bloquant : on préfère démarrer avec moins de mémoire allouée.
    item(
        "memory",
        ok && total_mb >= 4096,
        false,
        format!("RAM totale {total_mb} Mo, disponible {avail_mb} Mo"),
        None,
    )
}

/// Exécute toutes les vérifications. `ok` est vrai si aucun élément bloquant n'échoue.
pub fn check() -> PrereqReport {
    let items = vec![
        check_edition(),
        check_virtualization_firmware(),
        check_service("vmcompute", "service_vmcompute"),
        check_service("hns", "service_hns"),
        check_hcs(),
        check_memory(),
    ];
    let ok = items.iter().all(|i| i.ok || !i.blocking);
    PrereqReport { ok, items }
}

/// Mémoire physique totale de l'hôte, en Mo (pour dimensionner la machine par défaut).
pub fn total_memory_mb() -> u64 {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GlobalMemoryStatusEx(&mut status);
    }
    status.ullTotalPhys / (1024 * 1024)
}
