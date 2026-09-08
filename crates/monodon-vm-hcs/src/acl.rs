//! Droits d'accès pour le processus de la machine virtuelle.
//!
//! Les disques (et, par prudence, le noyau et l'initrd) sont ouverts par le processus `vmwp`
//! sous un compte de machine virtuelle membre du groupe **« NT VIRTUAL MACHINE\Virtual Machines »**
//! (SID `S-1-5-83-0`). Hyper-V Manager ajoute l'ACE correspondante lui-même ; HCS ne le fait pas,
//! et `HcsStartComputeSystem` échoue alors avec `E_ACCESSDENIED` (constaté au bloc 1). On imite
//! `hcsshim` (`security.GrantVmGroupAccess`) : une ACE sur le fichier et une ACE de traversée sur
//! son dossier parent, sans héritage.

use std::ffi::c_void;
use std::path::Path;

use monodon_core::{ErrorCode, MonodonError, Result};
use windows::Win32::Foundation::{
    GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE, HLOCAL, LocalFree, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP,
    TRUSTEE_W,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
};
use windows::core::{HSTRING, PWSTR};

/// Groupe « Virtual Machines ».
pub const VM_GROUP_SID: &str = "S-1-5-83-0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    ReadWrite,
}

fn win32_err(context: &str, path: &Path, code: WIN32_ERROR) -> MonodonError {
    let hresult = 0x8007_0000u32 | (code.0 & 0xFFFF);
    MonodonError::new(
        if code.0 == 5 {
            ErrorCode::InsufficientPrivileges
        } else {
            ErrorCode::Io
        },
        format!("{context} sur {} : erreur Win32 {}", path.display(), code.0),
    )
    .with_hresult(hresult)
}

/// Ajoute au DACL de `path` une ACE accordant `mask` au groupe Virtual Machines (sans héritage).
fn grant(path: &Path, mask: u32) -> Result<()> {
    let name = HSTRING::from(path.as_os_str());
    let mut sid = PSID::default();
    unsafe { ConvertStringSidToSidW(&HSTRING::from(VM_GROUP_SID), &mut sid) }
        .map_err(|e| MonodonError::internal(format!("ConvertStringSidToSidW : {e}")))?;

    let mut old_dacl: *mut ACL = std::ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let rc = unsafe {
        GetNamedSecurityInfoW(
            &name,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut old_dacl),
            None,
            &mut sd,
        )
    };
    if rc.0 != 0 {
        unsafe { LocalFree(Some(HLOCAL(sid.0))) };
        return Err(win32_err("GetNamedSecurityInfoW", path, rc));
    }

    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: mask,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: Default::default(),
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
            ptstrName: PWSTR(sid.0 as *mut u16),
        },
    };
    let mut new_dacl: *mut ACL = std::ptr::null_mut();
    let rc = unsafe { SetEntriesInAclW(Some(&[ea]), Some(old_dacl as *const ACL), &mut new_dacl) };
    let result = if rc.0 != 0 {
        Err(win32_err("SetEntriesInAclW", path, rc))
    } else {
        let rc = unsafe {
            SetNamedSecurityInfoW(
                &name,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(new_dacl as *const ACL),
                None,
            )
        };
        if rc.0 != 0 {
            Err(win32_err("SetNamedSecurityInfoW", path, rc))
        } else {
            Ok(())
        }
    };
    unsafe {
        if !new_dacl.is_null() {
            LocalFree(Some(HLOCAL(new_dacl as *mut c_void)));
        }
        if !sd.0.is_null() {
            LocalFree(Some(HLOCAL(sd.0)));
        }
        LocalFree(Some(HLOCAL(sid.0)));
    }
    result
}

/// Rend `path` accessible au processus de la machine : lecture (ou lecture-écriture) sur le
/// fichier, traversée sur le dossier parent.
pub fn grant_vm_access(path: &Path, access: Access) -> Result<()> {
    let mask = match access {
        Access::Read => GENERIC_READ.0,
        Access::ReadWrite => GENERIC_READ.0 | GENERIC_WRITE.0,
    };
    grant(path, mask)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        grant(parent, GENERIC_READ.0 | GENERIC_EXECUTE.0)?;
    }
    tracing::debug!(path = %path.display(), ?access, "accès accordé au groupe Virtual Machines");
    Ok(())
}
