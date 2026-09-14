//! Mise à jour depuis l'application. Trois étapes, chacune refusable :
//!
//! 1. téléchargement de l'installateur de la version publiée sur GitHub (adresse tirée de l'API des
//!    releases, obligatoirement sous `github.com/v94lere/solon/releases/download/`) ;
//! 2. vérification de son SHA-256 contre le `SHA256SUMS.txt` joint à la même version : un fichier qui ne
//!    correspond pas est effacé, jamais lancé ;
//! 3. lancement de l'installateur en silencieux (`/S` : pas de sélecteur de langue ni d'assistant), en mode
//!    mise à jour de Tauri (`/UPDATE` : pas de désinstallation préalable, raccourcis et WebView2 laissés en
//!    place) et avec relance de Solon à la fin (`/R`, sous le compte de l'utilisateur). Windows demande
//!    l'élévation ; l'application, elle, ne l'est jamais. L'installateur arrête le service et le moteur,
//!    remplace les fichiers, redémarre le service, qui relance ce qui tournait.
//!
//! Le fichier est posé dans `%LOCALAPPDATA%\Solon\updates` ; les restes sont nettoyés au lancement suivant.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tokio::io::AsyncWriteExt;

const RELEASE_DOWNLOADS: &str = "https://github.com/v94lere/solon/releases/download/";

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateProgress {
    Downloading { received: u64, total: Option<u64> },
    Verifying,
}

fn updates_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA n'est pas défini".to_string())?;
    let dir = base.join("Solon").join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    Ok(dir)
}

fn check_url(url: &str) -> Result<(), String> {
    if url.starts_with(RELEASE_DOWNLOADS) {
        Ok(())
    } else {
        Err(format!(
            "adresse refusée (hors des releases GitHub de Solon) : {url}"
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
        .map_err(|e| format!("SHA256SUMS.txt : {e}"))?
        .text()
        .await
        .map_err(|e| format!("SHA256SUMS.txt : {e}"))?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == file_name && hash.len() == 64 {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    Err(format!("{file_name} n'apparaît pas dans SHA256SUMS.txt"))
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

/// Télécharge et vérifie l'installateur de `version`. Renvoie le chemin du fichier vérifié.
#[tauri::command]
pub async fn update_download(
    version: String,
    installer_url: String,
    sums_url: String,
    progress: Channel<UpdateProgress>,
) -> Result<String, String> {
    check_url(&installer_url)?;
    check_url(&sums_url)?;
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return Err(format!("numéro de version inattendu : {version}"));
    }
    let file_name = format!("Solon_{version}_x64-setup.exe");
    if !installer_url.ends_with(&format!("/{file_name}")) {
        return Err(format!(
            "l'installateur publié ne correspond pas à la version {version}"
        ));
    }

    let client = client()?;
    let expected = expected_hash(&client, &sums_url, &file_name).await?;
    let dir = updates_dir()?;
    let path = dir.join(&file_name);

    // Déjà téléchargé (élévation refusée la fois précédente, par exemple) : on revérifie au lieu de retélécharger.
    if path.is_file() {
        let _ = progress.send(UpdateProgress::Verifying);
        if hash_file(&path).await? == expected {
            return Ok(path.to_string_lossy().into_owned());
        }
        let _ = tokio::fs::remove_file(&path).await;
    }

    let part = dir.join(format!("{file_name}.part"));
    let response = client
        .get(&installer_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("téléchargement : {e}"))?;
    let total = response.content_length();
    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|e| format!("{} : {e}", part.display()))?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last_report = Instant::now();
    let mut stream = response.bytes_stream();
    let _ = progress.send(UpdateProgress::Downloading { received, total });
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("téléchargement interrompu : {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("écriture : {e}"))?;
        received += chunk.len() as u64;
        if last_report.elapsed() >= Duration::from_millis(150) {
            let _ = progress.send(UpdateProgress::Downloading { received, total });
            last_report = Instant::now();
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    let _ = progress.send(UpdateProgress::Downloading {
        received,
        total: Some(received),
    });
    let _ = progress.send(UpdateProgress::Verifying);

    let actual = hex(&hasher.finalize());
    if actual != expected {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(
            "le SHA-256 du fichier téléchargé ne correspond pas à SHA256SUMS.txt : fichier écarté (téléchargement corrompu ou altéré)".to_string(),
        );
    }
    tokio::fs::rename(&part, &path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Lance l'installateur vérifié, puis quitte : l'installateur relance Solon une fois terminé.
#[tauri::command]
pub async fn update_install(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    let dir = updates_dir()?;
    if path.parent() != Some(dir.as_path()) || !path.is_file() {
        return Err("installateur introuvable : relancez le téléchargement".to_string());
    }
    launch_elevated(&path, "/S /UPDATE /R")?;
    tracing::info!(
        "mise à jour : installateur lancé ({}), fermeture de l'application",
        path.display()
    );
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        app.exit(0);
    });
    Ok(())
}

/// `ShellExecuteW` avec le verbe `runas` : Windows affiche la demande d'élévation. Renvoie une erreur
/// lisible si l'utilisateur refuse (SE_ERR_ACCESSDENIED, 5).
fn launch_elevated(path: &Path, args: &str) -> Result<(), String> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR};

    let operation = HSTRING::from("runas");
    let file = HSTRING::from(path.as_os_str());
    let params = HSTRING::from(args);
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
        5 => Err("Windows n'a pas autorisé l'installation (demande refusée)".to_string()),
        code => Err(format!("lancement de l'installateur refusé (code {code})")),
    }
}

/// Efface les installateurs téléchargés lors des mises à jour précédentes. Un fichier encore ouvert
/// (l'installateur qui vient de nous relancer) est simplement laissé pour la fois suivante.
pub fn cleanup_later() {
    tauri::async_runtime::spawn(async {
        tokio::time::sleep(Duration::from_secs(90)).await;
        let Ok(dir) = updates_dir() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && std::fs::remove_file(&path).is_ok() {
                tracing::info!("mise à jour : {} supprimé", path.display());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Réseau : télécharge l'installateur 0.1.12 publié et vérifie son SHA-256 contre le SHA256SUMS.txt
    /// de la release. `cargo test -p solon -- --ignored downloads_published_release`.
    #[tokio::test]
    #[ignore]
    async fn downloads_published_release() {
        let base = "https://github.com/v94lere/solon/releases/download/v0.1.12/";
        let progress = Channel::new(|_| Ok(()));
        let path = update_download(
            "0.1.12".into(),
            format!("{base}Solon_0.1.12_x64-setup.exe"),
            format!("{base}SHA256SUMS.txt"),
            progress,
        )
        .await
        .expect("téléchargement et vérification");
        assert!(std::path::Path::new(&path).is_file());
        std::fs::remove_file(&path).unwrap();
    }

    #[tokio::test]
    async fn refuses_foreign_urls() {
        let progress = Channel::new(|_| Ok(()));
        let err = update_download(
            "0.1.12".into(),
            "https://example.com/Solon_0.1.12_x64-setup.exe".into(),
            "https://example.com/SHA256SUMS.txt".into(),
            progress,
        )
        .await
        .unwrap_err();
        assert!(err.contains("adresse refusée"), "{err}");
    }
}
