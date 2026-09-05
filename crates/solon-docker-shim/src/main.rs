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

/// Vrai si les **options globales** (celles placées avant la sous-commande) désignent un hôte ou un
/// contexte. On s'arrête à la sous-commande : un `-c` de `sh -c` dans `docker run … sh -c …` ne compte pas.
fn targets_engine_explicitly(args: &[OsString]) -> bool {
    // Options globales du CLI Docker qui prennent une valeur.
    const WITH_VALUE: &[&str] = &[
        "--config",
        "-c",
        "--context",
        "-H",
        "--host",
        "-l",
        "--log-level",
        "--tlscacert",
        "--tlscert",
        "--tlskey",
    ];
    let mut i = 0;
    while i < args.len() {
        let a = args[i].to_string_lossy().into_owned();
        if !a.starts_with('-') {
            return false; // sous-commande atteinte
        }
        if a == "-H"
            || a == "--host"
            || a.starts_with("--host=")
            || a == "-c"
            || a == "--context"
            || a.starts_with("--context=")
        {
            return true;
        }
        if WITH_VALUE.contains(&a.as_str()) {
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

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
    let explicit_target = targets_engine_explicitly(&args);

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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn options_globales_seulement() {
        assert!(targets_engine_explicitly(&v(&["-H", "tcp://x", "ps"])));
        assert!(targets_engine_explicitly(&v(&[
            "--context",
            "desktop-linux",
            "ps"
        ])));
        assert!(targets_engine_explicitly(&v(&[
            "--context=desktop-linux",
            "ps"
        ])));
        assert!(!targets_engine_explicitly(&v(&[
            "run", "--rm", "busybox", "sh", "-c", "echo"
        ])));
        assert!(!targets_engine_explicitly(&v(&["-D", "ps"])));
        assert!(!targets_engine_explicitly(&v(&[
            "--log-level",
            "debug",
            "run",
            "-H",
            "x"
        ])));
        assert!(!targets_engine_explicitly(&v(&[])));
    }
}
