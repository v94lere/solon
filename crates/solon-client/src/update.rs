//! Mise à jour de Solon. C'est la seule requête sortante de Solon, et elle part toujours d'un geste
//! de l'utilisateur (`solon update`, ou l'entrée de menu de l'icône) : rien n'est envoyé sur lui ni
//! sur son installation, la requête ne porte que l'adresse du dépôt.
//!
//! Trois étapes, chacune refusable :
//! 1. lecture de la dernière version publiée sur GitHub (API des releases) ;
//! 2. téléchargement de l'installateur, obligatoirement sous `github.com/v94lere/solon/releases/download/`,
//!    et vérification de son SHA-256 contre le `SHA256SUMS.txt` joint à la même version : un fichier qui
//!    ne correspond pas est effacé, jamais lancé ;
//! 3. lancement de l'installateur en silencieux (`/S`), en mode mise à jour de Tauri (`/UPDATE` : pas de
//!    désinstallation préalable) et avec relance de l'icône à la fin (`/R`). Windows demande l'élévation.
//!    L'installateur arrête le service et le moteur, remplace les fichiers, redémarre le service, qui
//!    relance ce qui tournait.
//!
//! Le fichier est posé dans `%LOCALAPPDATA%\Solon\updates` ; [`cleanup`] efface les restes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

const RELEASE_DOWNLOADS: &str = "https://github.com/v94lere/solon/releases/download/";
const LATEST_API: &str = "https://api.github.com/repos/v94lere/solon/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/v94lere/solon/releases";

#[derive(Debug, Clone, Serialize)]
pub struct Release {
    /// `0.2.0` (sans le `v` de l'étiquette).
    pub version: String,
    pub page: String,
    pub installer_url: Option<String>,
    pub sums_url: Option<String>,
    /// Notes de version (Markdown brut).
    pub notes: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Progress {
    Downloading { received: u64, total: Option<u64> },
    Verifying,
}

fn updates_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is not set".to_string())?;
    let dir = base.join("Solon").join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

fn check_url(url: &str) -> Result<(), String> {
    if url.starts_with(RELEASE_DOWNLOADS) {
        Ok(())
    } else {
        Err(format!(
            "address refused (outside Solon's GitHub releases): {url}"
        ))
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("Solon/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

/// Numéros comparés partie par partie (`0.1.18` < `0.2.0`) ; un suffixe non numérique vaut 0.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let (a, b) = (parts(candidate), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// La dernière version publiée.
pub async fn latest() -> Result<Release, String> {
    let v: serde_json::Value = client()?
        .get(LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("GitHub: {e}"))?
        .json()
        .await
        .map_err(|e| format!("GitHub: {e}"))?;
    let version = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or("GitHub: no tag_name")?
        .trim_start_matches('v')
        .to_owned();
    let installer_name = format!("Solon_{version}_x64-setup.exe");
    let mut installer_url = None;
    let mut sums_url = None;
    if let Some(assets) = v.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let url = a
                .get("browser_download_url")
                .and_then(|u| u.as_str())
                .map(str::to_owned);
            if name == installer_name {
                installer_url = url;
            } else if name == "SHA256SUMS.txt" {
                sums_url = url;
            }
        }
    }
    Ok(Release {
        version,
        page: v
            .get("html_url")
            .and_then(|u| u.as_str())
            .unwrap_or(RELEASES_PAGE)
            .to_owned(),
        installer_url,
        sums_url,
        notes: v
            .get("body")
            .and_then(|b| b.as_str())
            .unwrap_or("")
            .to_owned(),
    })
}

/// Lit `SHA256SUMS.txt` (lignes `<sha256>  <nom>` ou `<sha256> *<nom>`) et renvoie l'empreinte attendue.
async fn expected_hash(
    client: &reqwest::Client,
    sums_url: &str,
    file_name: &str,
) -> Result<String, String> {
    let text = client
        .get(sums_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("SHA256SUMS.txt: {e}"))?
        .text()
        .await
        .map_err(|e| format!("SHA256SUMS.txt: {e}"))?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == file_name && hash.len() == 64 {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    Err(format!("{file_name} is not listed in SHA256SUMS.txt"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn hash_file(path: &Path) -> Result<String, String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
        Ok(hex(&hasher.finalize()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Télécharge et vérifie l'installateur de `release`. Renvoie le chemin du fichier vérifié.
pub async fn download<F: FnMut(Progress)>(
    release: &Release,
    mut progress: F,
) -> Result<PathBuf, String> {
    let installer_url = release
        .installer_url
        .as_deref()
        .ok_or("this release has no installer attached")?;
    let sums_url = release
        .sums_url
        .as_deref()
        .ok_or("this release has no SHA256SUMS.txt: no install from here")?;
    check_url(installer_url)?;
    check_url(sums_url)?;
    let version = &release.version;
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return Err(format!("unexpected version number: {version}"));
    }
    let file_name = format!("Solon_{version}_x64-setup.exe");
    if !installer_url.ends_with(&format!("/{file_name}")) {
        return Err(format!(
            "the published installer does not match version {version}"
        ));
    }

    let client = client()?;
    let expected = expected_hash(&client, sums_url, &file_name).await?;
    let dir = updates_dir()?;
    let path = dir.join(&file_name);

    // Déjà téléchargé (élévation refusée la fois précédente, par exemple) : on revérifie au lieu de retélécharger.
    if path.is_file() {
        progress(Progress::Verifying);
        if hash_file(&path).await? == expected {
            return Ok(path);
        }
        let _ = tokio::fs::remove_file(&path).await;
    }

    let part = dir.join(format!("{file_name}.part"));
    let response = client
        .get(installer_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download: {e}"))?;
    let total = response.content_length();
    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|e| format!("{}: {e}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last_report = Instant::now();
    let mut stream = response.bytes_stream();
    progress(Progress::Downloading { received, total });
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write: {e}"))?;
        received += chunk.len() as u64;
        if last_report.elapsed() >= Duration::from_millis(150) {
            progress(Progress::Downloading { received, total });
            last_report = Instant::now();
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    progress(Progress::Downloading {
        received,
        total: Some(received),
    });
    progress(Progress::Verifying);

    let actual = hex(&hasher.finalize());
    if actual != expected {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(
            "the SHA-256 of the downloaded file does not match SHA256SUMS.txt: file discarded (corrupted or altered download)".to_string(),
        );
    }
    tokio::fs::rename(&part, &path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// Lance l'installateur vérifié (Windows demande l'élévation). L'appelant quitte ensuite : au terme
/// de l'installation, l'installateur relance l'icône de Solon.
pub fn install(path: &Path) -> Result<(), String> {
    let dir = updates_dir()?;
    if path.parent() != Some(dir.as_path()) || !path.is_file() {
        return Err("installer not found: download it again".to_string());
    }
    launch_elevated(path, "/S /UPDATE /R")
}

/// `ShellExecuteW` avec le verbe `runas` : Windows affiche la demande d'élévation. Renvoie une erreur
/// lisible si l'utilisateur refuse (SE_ERR_ACCESSDENIED, 5).
pub fn launch_elevated(path: &Path, args: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR};

    let operation = HSTRING::from("runas");
    let file = HSTRING::from(path.as_os_str());
    let params = HSTRING::from(args);
    // SAFETY : chaînes valides le temps de l'appel ; ShellExecuteW ne garde pas de pointeur.
    let handle = unsafe {
        ShellExecuteW(
            None,
            &operation,
            &file,
            &params,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Valeur > 32 : lancé ; sinon un code d'erreur SE_ERR_*.
    match handle.0 as isize {
        code if code > 32 => Ok(()),
        5 => Err("Windows did not allow the installation (request refused)".to_string()),
        code => Err(format!("installer launch refused (code {code})")),
    }
}

/// Efface les installateurs téléchargés lors des mises à jour précédentes. Un fichier encore ouvert
/// (l'installateur qui vient de nous relancer) est simplement laissé pour la fois suivante.
pub fn cleanup() {
    let Ok(dir) = updates_dir() else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Vrai quand le programme tourne depuis un paquet MSIX (Microsoft Store) : c'est le Store qui met à
/// jour, pas nous.
pub fn packaged() -> bool {
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len: u32 = 0;
    // SAFETY : sans tampon, l'appel renvoie ERROR_INSUFFICIENT_BUFFER si le processus a une identité
    // de paquet, et APPMODEL_ERROR_NO_PACKAGE sinon ; il n'écrit rien.
    let err = unsafe { GetCurrentPackageFullName(&mut len, None) };
    const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
    err.0 != APPMODEL_ERROR_NO_PACKAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparaison_de_versions() {
        assert!(is_newer("0.2.0", "0.1.18"));
        assert!(is_newer("v0.1.19", "0.1.18"));
        assert!(!is_newer("0.1.18", "0.1.18"));
        assert!(!is_newer("0.1.9", "0.1.18"));
        assert!(is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn adresses_hors_releases_refusees() {
        assert!(
            check_url("https://github.com/v94lere/solon/releases/download/v0.2.0/x.exe").is_ok()
        );
        assert!(check_url("https://example.com/x.exe").is_err());
        assert!(
            check_url("https://github.com/autre/solon/releases/download/v0.2.0/x.exe").is_err()
        );
    }
}
