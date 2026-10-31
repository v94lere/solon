//! `solon` : Solon en ligne de commande.
//!
//! Tout passe par le canal de contrôle du service (`\\.\pipe\solon-control`), comme l'application ;
//! les questions à Docker sont posées dans la machine, par `Exec`. Aucune élévation nécessaire,
//! aucune requête sortante. Les messages sont en anglais, ou en français si Windows l'est
//! (`SOLON_LANG=fr` force).

/// `println!` qui se tait au lieu de paniquer quand la sortie est fermée (`solon status | head`).
macro_rules! out {
    ($($t:tt)*) => {{
        use std::io::Write;
        let mut o = std::io::stdout().lock();
        if writeln!(o, $($t)*).is_err() {
            crate::util::exit(0);
        }
    }};
}

/// `print!` avec la même tolérance.
macro_rules! outp {
    ($($t:tt)*) => {{
        use std::io::Write;
        let mut o = std::io::stdout().lock();
        if write!(o, $($t)*).is_err() {
            crate::util::exit(0);
        }
    }};
}

mod engine;
mod more;
mod project;
mod text;
mod trace;
mod util;

use text::tr;
use util::Args;

fn help() {
    let fr = text::french();
    let lines: &[(&str, &str, &str)] = &[
        (
            "status",
            "Engine state, containers and their addresses (default)",
            "État du moteur, conteneurs et adresses (par défaut)",
        ),
        (
            "start | stop [--force] | restart",
            "Start or stop the engine, showing the steps",
            "Démarrer ou arrêter le moteur, étapes affichées",
        ),
        (
            "projects",
            "Compose projects known to the engine",
            "Projets Compose connus du moteur",
        ),
        (
            "domains",
            "Every *.solon.local address answering right now",
            "Toutes les adresses *.solon.local qui répondent",
        ),
        (
            "why [name]",
            "What is wrong, in plain words: refused calls, server errors, slow paths",
            "Ce qui ne va pas, en clair : appels refusés, erreurs de serveur, lenteurs",
        ),
        (
            "trace [name] [-n N|--all] [-f]",
            "The journal of exchanges (Radiography); -f follows live",
            "Le journal des échanges (Radiographie) ; -f suit le direct",
        ),
        (
            "clear [project]",
            "Forget the observed history",
            "Oublier l'historique observé",
        ),
        (
            "backup [folder] [file.zip] [--force]",
            "Save a project with its volumes into one zip",
            "Sauvegarder un projet et ses volumes en un zip",
        ),
        (
            "restore <file.zip> [folder]",
            "Restore such a zip",
            "Restaurer un tel zip",
        ),
        (
            "settings [key=value…]",
            "Show or change the settings (memory, cpus, sleep, idle…)",
            "Voir ou changer les réglages (memory, cpus, sleep, idle…)",
        ),
        (
            "prereq",
            "Check the Windows prerequisites",
            "Vérifier les prérequis Windows",
        ),
        (
            "logs [-n N] [-f]",
            "The service log",
            "Le journal du service",
        ),
        (
            "diag [file.zip]",
            "Export everything useful for a bug report",
            "Exporter tout ce qui sert à un signalement",
        ),
        (
            "exec <command…>",
            "Run a command inside the engine (root); one quoted word is a full shell line",
            "Lancer une commande dans le moteur (root) ; un seul mot entre guillemets = une ligne shell",
        ),
        (
            "watch [--json]",
            "Engine events as they happen",
            "Les événements du moteur au fil de l'eau",
        ),
        (
            "update [--check]",
            "Install the latest published version (asks Windows for permission)",
            "Installer la dernière version publiée (Windows demande l'autorisation)",
        ),
        (
            "scan",
            "Find the Docker projects on this PC",
            "Trouver les projets Docker de ce PC",
        ),
        (
            "shell",
            "A root shell inside the engine",
            "Un shell root dans le moteur",
        ),
        (
            "clone <from> <to>",
            "Copy the volumes of a project to another name (one environment per branch)",
            "Copier les volumes d'un projet sous un autre nom (un environnement par branche)",
        ),
        (
            "import [--all | --volumes a,b --images x]",
            "Copy volumes and images from Docker Desktop, without touching it",
            "Copier volumes et images depuis Docker Desktop, sans y toucher",
        ),
        (
            "version",
            "Versions of the CLI, the service and the image",
            "Versions du CLI, du service et de l'image",
        ),
    ];
    out!(
        "{}",
        tr(
            "solon — Docker on Windows, quietly. Usage: solon <command> [options]",
            "solon — Docker sur Windows, en silence. Usage : solon <commande> [options]"
        )
    );
    out!();
    let width = lines.iter().map(|l| l.0.len()).max().unwrap_or(0);
    for (cmd, en, fr_text) in lines {
        out!("  {cmd:width$}  {}", if fr { fr_text } else { en });
    }
    out!();
    out!(
        "{}",
        tr(
            "`--json` on status, projects, domains, trace, settings, prereq and watch prints machine-readable output.\n`docker` and `docker compose` are the regular ones: Solon only runs the engine.",
            "`--json` sur status, projects, domains, trace, settings, prereq et watch donne une sortie lisible par un programme.\n`docker` et `docker compose` sont les vrais : Solon ne fait que faire tourner le moteur."
        )
    );
}

fn main() {
    text::utf8_console();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let cmd = raw.first().map(String::as_str).unwrap_or("status");
    let args = Args::parse(raw.get(1..).unwrap_or(&[]));
    if matches!(cmd, "help" | "--help" | "-h") || (cmd != "exec" && args.flag("--help")) {
        help();
        return;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        match cmd {
            "status" | "st" => engine::status(&args).await,
            "start" | "up" => engine::start(&args).await,
            "stop" | "down" => engine::stop(&args).await,
            "restart" => engine::restart(&args).await,
            "watch" => engine::watch(&args).await,
            "prereq" => engine::prereq(&args).await,
            "settings" | "config" => engine::settings(&args).await,
            "version" | "--version" | "-V" => engine::version(&args).await,
            "exec" => engine::exec_cmd(raw.get(1..).unwrap_or(&[])).await,
            "logs" | "log" => engine::logs(&args).await,
            "diag" | "diagnostic" => engine::diag(&args).await,
            "projects" | "ls" => project::projects(&args).await,
            "domains" | "urls" => project::domains(&args).await,
            "backup" => project::backup(&args).await,
            "restore" => project::restore(&args).await,
            "trace" => trace::trace(&args).await,
            "why" => trace::why(&args).await,
            "clear" => trace::clear(&args).await,
            "update" => more::update(&args).await,
            "scan" => more::scan(&args).await,
            "shell" => more::shell(&args),
            "clone" => more::clone(&args).await,
            "import" => more::import(&args).await,
            other => {
                eprintln!(
                    "{} {other}. {}",
                    tr("Unknown command:", "Commande inconnue :"),
                    tr("See `solon help`.", "Voir `solon help`.")
                );
                crate::util::exit(2);
            }
        }
    });
    text::restore_console();
}
