//! Export de diagnostic : la mécanique est dans `solon_client::diag`, partagée avec le CLI.

/// Écrit l'archive à `dest` et renvoie le nombre de fichiers inclus.
#[tauri::command]
pub async fn diagnostic_export(app: tauri::AppHandle, dest: String) -> Result<u32, String> {
    let version = app.package_info().version.to_string();
    solon_client::diag::export(&version, &dest).await
}
