//! Traduction des HRESULT de HCS (et de quelques HRESULT Win32 génériques) en erreurs Solon
//! à codes stables. Les constantes sont recopiées depuis `windows::Win32::Foundation`
//! (préfixe `HCS_E_`, plage `0x80370100`–`0x80370121`).

use solon_core::{ErrorCode, SolonError};

pub const HCS_E_TERMINATED_DURING_START: u32 = 0x8037_0100;
pub const HCS_E_IMAGE_MISMATCH: u32 = 0x8037_0101;
pub const HCS_E_HYPERV_NOT_INSTALLED: u32 = 0x8037_0102;
pub const HCS_E_INVALID_STATE: u32 = 0x8037_0105;
pub const HCS_E_UNEXPECTED_EXIT: u32 = 0x8037_0106;
pub const HCS_E_TERMINATED: u32 = 0x8037_0107;
pub const HCS_E_CONNECT_FAILED: u32 = 0x8037_0108;
pub const HCS_E_CONNECTION_TIMEOUT: u32 = 0x8037_0109;
pub const HCS_E_CONNECTION_CLOSED: u32 = 0x8037_010A;
pub const HCS_E_INVALID_JSON: u32 = 0x8037_010D;
pub const HCS_E_SYSTEM_NOT_FOUND: u32 = 0x8037_010E;
pub const HCS_E_SYSTEM_ALREADY_EXISTS: u32 = 0x8037_010F;
pub const HCS_E_SYSTEM_ALREADY_STOPPED: u32 = 0x8037_0110;
pub const HCS_E_WINDOWS_INSIDER_REQUIRED: u32 = 0x8037_0113;
pub const HCS_E_SERVICE_NOT_AVAILABLE: u32 = 0x8037_0114;
pub const HCS_E_OPERATION_TIMEOUT: u32 = 0x8037_0118;
pub const HCS_E_ACCESS_DENIED: u32 = 0x8037_011B;
pub const HCS_E_GUEST_CRITICAL_ERROR: u32 = 0x8037_011C;
pub const HCS_E_SERVICE_DISCONNECT: u32 = 0x8037_011E;

/// HRESULT Win32 génériques rencontrés autour de HCS.
pub const E_ACCESSDENIED: u32 = 0x8007_0005;
pub const E_INVALIDARG: u32 = 0x8007_0057;
pub const RPC_S_SERVER_UNAVAILABLE: u32 = 0x8007_06BA;
pub const ERROR_FILE_NOT_FOUND: u32 = 0x8007_0002;
pub const ERROR_PATH_NOT_FOUND: u32 = 0x8007_0003;
/// « Le service de virtualisation n'est pas en cours d'exécution » (hyperviseur non lancé).
pub const ERROR_VIRTUALIZATION_NOT_RUNNING: u32 = 0x8007_1D18;

/// Classe un HRESULT en code d'erreur Solon.
pub fn classify(hresult: u32) -> ErrorCode {
    match hresult {
        HCS_E_HYPERV_NOT_INSTALLED | ERROR_VIRTUALIZATION_NOT_RUNNING => {
            ErrorCode::HypervisorNotRunning
        }
        HCS_E_SERVICE_NOT_AVAILABLE | HCS_E_SERVICE_DISCONNECT | RPC_S_SERVER_UNAVAILABLE => {
            ErrorCode::HostComputeServiceUnavailable
        }
        HCS_E_ACCESS_DENIED | E_ACCESSDENIED => ErrorCode::InsufficientPrivileges,
        HCS_E_INVALID_JSON
        | E_INVALIDARG
        | HCS_E_WINDOWS_INSIDER_REQUIRED
        | HCS_E_IMAGE_MISMATCH => ErrorCode::VmConfigurationRejected,
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => ErrorCode::ImageCorrupted,
        HCS_E_OPERATION_TIMEOUT | HCS_E_CONNECTION_TIMEOUT => ErrorCode::VmBootTimeout,
        _ => ErrorCode::HcsError,
    }
}

/// Construit une erreur Solon à partir d'une erreur `windows` et du document de résultat
/// éventuellement renvoyé par HCS (JSON décrivant l'échec).
pub fn from_windows(
    context: &str,
    error: &windows::core::Error,
    result_document: Option<&str>,
) -> SolonError {
    let hresult = error.code().0 as u32;
    let mut message = format!("{context} : {} (0x{hresult:08X})", error.message());
    if let Some(doc) = result_document.filter(|d| !d.trim().is_empty()) {
        message.push_str(" — ");
        message.push_str(summarize_result_document(doc).as_str());
    }
    SolonError::new(classify(hresult), message).with_hresult(hresult)
}

/// Extrait le champ le plus parlant d'un document d'erreur HCS, sinon renvoie le JSON tronqué.
fn summarize_result_document(doc: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(doc) {
        if let Some(msg) = value.get("ErrorMessage").and_then(|v| v.as_str()) {
            return msg.to_owned();
        }
        if let Some(events) = value.get("ErrorEvents").and_then(|v| v.as_array()) {
            let messages: Vec<&str> = events
                .iter()
                .filter_map(|e| e.get("Message").and_then(|m| m.as_str()))
                .collect();
            if !messages.is_empty() {
                return messages.join(" | ");
            }
        }
    }
    let mut short: String = doc.chars().take(400).collect();
    if short.len() < doc.len() {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classe_les_codes_connus() {
        assert_eq!(
            classify(HCS_E_HYPERV_NOT_INSTALLED),
            ErrorCode::HypervisorNotRunning
        );
        assert_eq!(classify(E_ACCESSDENIED), ErrorCode::InsufficientPrivileges);
        assert_eq!(
            classify(HCS_E_INVALID_JSON),
            ErrorCode::VmConfigurationRejected
        );
        assert_eq!(
            classify(RPC_S_SERVER_UNAVAILABLE),
            ErrorCode::HostComputeServiceUnavailable
        );
        assert_eq!(classify(0x8037_0FFF), ErrorCode::HcsError);
    }

    #[test]
    fn resume_un_document_d_erreur() {
        let doc = r#"{"Error":-2143878653,"ErrorMessage":"Le fichier spécifié est introuvable.","ErrorEvents":[]}"#;
        assert_eq!(
            summarize_result_document(doc),
            "Le fichier spécifié est introuvable."
        );
        let doc2 = r#"{"ErrorEvents":[{"Message":"a"},{"Message":"b"}]}"#;
        assert_eq!(summarize_result_document(doc2), "a | b");
        assert_eq!(summarize_result_document("pas du json"), "pas du json");
    }
}
