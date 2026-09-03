//! `docker.exe` livré par Solon (installé dans `<dossier>\bin`, ajouté au PATH par l'installeur).
//!
//! Il lance le CLI Docker officiel (`docker-cli.exe`, à côté de lui) en le dirigeant vers le moteur
//! Solon (`npipe:////./pipe/solon`) **sauf** si l'utilisateur a choisi explicitement un hôte ou un
//! contexte (`-H`, `--host`, `-c`, `--context`, ou les variables `DOCKER_HOST` / `DOCKER_CONTEXT`).
//! Le plugin Compose livré avec Solon (`bin\cli-plugins\docker-compose.exe`) est rendu visible par
//! `DOCKER_CLI_PLUGIN_EXTRA_DIRS`. Le code de sortie du CLI est propagé tel quel.

use std::ffi::OsString;
use std::process::{Command, exit};

const SOLON_HOST: &str = "npipe:////./pipe/solon";

fn main() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("solon docker : chemin de l'exécutable inconnu : {e}");
            exit(127);
        }
    };
    let dir = exe.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let real = dir.join("docker-cli.exe");
    if !real.is_file() {
        eprintln!(
            "solon docker : CLI Docker introuvable ({}). Réinstallez Solon.",
            real.display()
        );
        exit(127);
    }

    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let explicit_target = args.iter().any(|a| {
        let s = a.to_string_lossy();
        s == "-H"
            || s == "--host"
            || s.starts_with("--host=")
            || s == "-c"
            || s == "--context"
            || s.starts_with("--context=")
    });

    let mut cmd = Command::new(&real);
    cmd.args(&args);
    if !explicit_target
        && std::env::var_os("DOCKER_HOST").is_none()
        && std::env::var_os("DOCKER_CONTEXT").is_none()
    {
        cmd.env("DOCKER_HOST", SOLON_HOST);
    }
    let mut plugin_dirs = dir.join("cli-plugins").into_os_string();
    if let Some(prev) = std::env::var_os("DOCKER_CLI_PLUGIN_EXTRA_DIRS") {
        plugin_dirs.push(";");
        plugin_dirs.push(prev);
    }
    cmd.env("DOCKER_CLI_PLUGIN_EXTRA_DIRS", plugin_dirs);

    match cmd.status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!(
                "solon docker : impossible de lancer {} : {e}",
                real.display()
            );
            exit(127);
        }
    }
}
