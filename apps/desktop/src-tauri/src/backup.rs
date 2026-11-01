//! Sauvegarde et restauration d'un projet : la mécanique est dans `solon_client::backup`,
//! partagée avec le CLI ; ici seulement les commandes Tauri.

use solon_client::backup::{BackupReport, RestoreReport};

/// Sauvegarde `dir` (projet Compose nommé `project`) dans le zip `dest`.
#[tauri::command]
pub async fn project_backup(
    app: tauri::AppHandle,
    dir: String,
    project: String,
    dest: String,
) -> Result<BackupReport, String> {
    let version = app.package_info().version.to_string();
    // La boîte « Enregistrer sous » de Windows a déjà demandé avant d'écraser.
    solon_client::backup::backup(&dir, &project, &dest, &version, true).await
}

/// Lit le manifeste d'un zip de sauvegarde (aperçu avant restauration).
#[tauri::command]
pub fn project_backup_info(zip_path: String) -> Result<serde_json::Value, String> {
    solon_client::backup::info(&zip_path)
}

/// Restaure le zip `zip_path` dans `target_dir`.
#[tauri::command]
pub async fn project_restore(
    zip_path: String,
    target_dir: String,
) -> Result<RestoreReport, String> {
    solon_client::backup::restore(&zip_path, &target_dir).await
}
