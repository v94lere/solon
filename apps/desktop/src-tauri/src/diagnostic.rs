//! Export de diagnostic : une archive zip avec tout ce qu'il faut pour comprendre un problème chez
//! quelqu'un d'autre (journaux du service et de l'installation, état, réglages, prérequis, versions,
//! `docker info` si le moteur tourne). Aucune donnée d'identifiants ; les chemins de dossiers
//! partagés apparaissent dans les journaux, c'est dit à l'utilisateur.

use std::io::Write;
use std::path::{Path, PathBuf};

use monodon_core::ipc::ServiceCommand;
use serde_json::Value;
use zip::write::SimpleFileOptions;

use crate::service;

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Monodon")
}

fn add_file<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    name: &str,
    path: &Path,
) -> Result<(), String> {
    let Ok(data) = std::fs::read(path) else {
        return Ok(());
    };
    zip.start_file(name, SimpleFileOptions::default())
        .map_err(|e| e.to_string())?;
    zip.write_all(&data).map_err(|e| e.to_string())
}

fn add_text<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    name: &str,
    text: &str,
) -> Result<(), String> {
    zip.start_file(name, SimpleFileOptions::default())
        .map_err(|e| e.to_string())?;
    zip.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

async fn call_or_error(cmd: ServiceCommand) -> String {
    match service::call(cmd).await {
        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
        Err(e) => format!("{{\"error\": {} }}", Value::String(e)),
    }
}

/// Écrit l'archive à `dest` et renvoie le nombre de fichiers inclus.
#[tauri::command]
pub async fn diagnostic_export(app: tauri::AppHandle, dest: String) -> Result<u32, String> {
    let status = call_or_error(ServiceCommand::Status).await;
    let prereq = call_or_error(ServiceCommand::Prerequisites).await;
    let settings = call_or_error(ServiceCommand::GetSettings).await;
    let shares = call_or_error(ServiceCommand::ListShares).await;
    let version = call_or_error(ServiceCommand::Version).await;
    let engine_ready = status.contains("\"ready\"");
    let docker_info = if engine_ready {
        call_or_error(ServiceCommand::Exec {
            command: "docker version 2>&1; echo; docker info 2>&1; echo; df -h /var/lib/monodon 2>&1; echo; free -m; echo; uname -a; echo; mount | grep -E '9p|/var/lib' ".into(),
            timeout_s: Some(30),
        })
        .await
    } else {
        "moteur arrêté : pas de docker info".to_owned()
    };
    let windows = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    let app_version = app.package_info().version.to_string();

    let root = program_data();
    let dest_path = PathBuf::from(&dest);
    let file =
        std::fs::File::create(&dest_path).map_err(|e| format!("création de {dest} : {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let mut count = 0u32;

    add_text(
        &mut zip,
        "LISEZMOI.txt",
        &format!(
            "Diagnostic Monodon\nApplication : {app_version}\nWindows : {windows}\nDate : {}\n\nContenu : journaux du service et de l'installation, état, réglages, prérequis, versions, docker info.\nAucun identifiant n'est inclus ; les journaux peuvent contenir les chemins des dossiers partagés.\n",
            chrono_like_now()
        ),
    )?;
    count += 1;
    for (name, text) in [
        ("status.json", &status),
        ("prereq.json", &prereq),
        ("settings.json", &settings),
        ("shares.json", &shares),
        ("service-version.json", &version),
        ("docker-info.txt", &docker_info),
    ] {
        add_text(&mut zip, name, text)?;
        count += 1;
    }
    for name in ["state.json", "settings.json"] {
        let p = root.join(name);
        if p.is_file() {
            add_file(&mut zip, &format!("programdata/{name}"), &p)?;
            count += 1;
        }
    }
    // Journaux : les 5 fichiers les plus récents.
    if let Ok(entries) = std::fs::read_dir(root.join("logs")) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        for p in files.iter().rev().take(5) {
            if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                add_file(&mut zip, &format!("logs/{n}"), p)?;
                count += 1;
            }
        }
    }
    let hosts =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32\\drivers\\etc\\hosts");
    if let Ok(text) = std::fs::read_to_string(&hosts) {
        let block: Vec<&str> = text
            .lines()
            .skip_while(|l| !l.contains("monodon-begin"))
            .take_while(|l| !l.contains("monodon-end"))
            .collect();
        add_text(&mut zip, "hosts-monodon.txt", &block.join("\n"))?;
        count += 1;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(count)
}

/// Horodatage lisible sans dépendance supplémentaire.
fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs} (secondes Unix)")
}
