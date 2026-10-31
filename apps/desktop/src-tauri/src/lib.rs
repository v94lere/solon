//! Solon dans la barre des tâches. Pas de fenêtre : une icône en bas à droite, un menu pour voir
//! l'état du moteur et relancer un projet ou un conteneur en deux clics, des notifications quand
//! quelque chose se passe. Tout le reste est dans la commande `solon` (dossier `bin`, dans le PATH).
//!
//! - [`service`] : canal de contrôle du service Windows, abonnement aux événements ;
//! - [`tray`] : l'icône, son menu, les notifications ;
//! - [`lang`] : anglais ou français selon Windows.

mod lang;
mod service;
mod tray;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();
    let app = tauri::Builder::default()
        // Une seule icône : un second lancement (raccourci, menu Démarrer, fin d'installation) ne fait
        // rien de plus que signaler que Solon est déjà là. Doit être le premier plugin enregistré.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::notify(
                app,
                "Solon",
                lang::tr(
                    "Solon is already running: its icon is in the taskbar corner.",
                    "Solon tourne déjà : son icône est dans le coin de la barre des tâches.",
                ),
            );
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            tray::setup(app.handle())?;
            // Restes des mises à jour précédentes, effacés sans se presser.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(90));
                solon_client::update::cleanup();
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erreur au lancement de Solon");
    app.run(|_app, event| {
        // Sans fenêtre, Tauri demanderait à quitter aussitôt : on reste tant que « Quitter » n'a
        // pas été choisi (qui passe par `app.exit(0)`, avec un code).
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event
            && code.is_none()
        {
            api.prevent_exit();
        }
    });
}
