//! Les projets Compose : liste, domaines, sauvegarde et restauration.

use std::path::{Path, PathBuf};

use solon_client::control::exec;

use crate::engine::containers;
use crate::text::tr;
use crate::util::{self, Args, fail};

/// `solon projects [--json]` : ce que `docker compose ls -a` connaît, avec les adresses.
pub async fn projects(args: &Args) {
    let (code, out, err) = exec("docker compose ls -a --format json".into(), 30)
        .await
        .unwrap_or_else(|e| fail(e));
    if code != 0 {
        fail(format!("docker compose ls : {}", err.trim()));
    }
    if args.json() {
        outp!("{out}");
        return;
    }
    let list: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap_or_default();
    if list.is_empty() {
        out!(
            "{}",
            tr(
                "No Compose project known to the engine. Run `docker compose up -d` in a project folder.",
                "Aucun projet Compose connu du moteur. Lancez `docker compose up -d` dans un dossier de projet."
            )
        );
        return;
    }
    let all = containers().await.unwrap_or_default();
    let mut rows = vec![vec![
        tr("PROJECT", "PROJET").to_owned(),
        tr("STATUS", "ÉTAT").to_owned(),
        tr("FOLDER", "DOSSIER").to_owned(),
        tr("ADDRESSES", "ADRESSES").to_owned(),
    ]];
    for p in list {
        let name = p
            .get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_owned();
        let status = p
            .get("Status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let folder = p
            .get("ConfigFiles")
            .and_then(|v| v.as_str())
            .and_then(|f| f.split(',').next())
            .map(|f| {
                let host = util::host_path(f.trim());
                Path::new(&host)
                    .parent()
                    .map(|d| d.to_string_lossy().into_owned())
                    .unwrap_or(host)
            })
            .unwrap_or_default();
        let mut addresses: Vec<String> = all
            .iter()
            .filter(|c| c.running() && c.project.as_deref() == Some(&name))
            .map(|c| c.domains()[0].clone())
            .collect();
        addresses.sort();
        rows.push(vec![name, status, folder, addresses.join(", ")]);
    }
    util::table(&rows);
}

/// `solon domains` : toutes les adresses `*.solon.local` qui répondent en ce moment.
pub async fn domains(args: &Args) {
    let all = containers().await.unwrap_or_else(|e| fail(e));
    let snap = crate::engine::snapshot().await.unwrap_or_default();
    let scheme = if snap.local_domains_tls {
        "https"
    } else {
        "http"
    };
    let mut rows: Vec<Vec<String>> = Vec::new();
    for c in all.iter().filter(|c| c.running()) {
        for d in c.domains() {
            rows.push(vec![format!("{scheme}://{d}"), c.name.clone()]);
        }
    }
    if args.json() {
        let v: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| serde_json::json!({ "url": r[0], "container": r[1] }))
            .collect();
        out!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return;
    }
    if !snap.local_domains {
        out!(
            "{}",
            tr(
                "Local domains are off: the engine is not running, or port 80 on this PC is taken by another program.",
                "Les domaines locaux sont désactivés : le moteur est arrêté, ou le port 80 de ce PC est pris par un autre programme."
            )
        );
    }
    if rows.is_empty() {
        out!(
            "{}",
            tr("No container running.", "Aucun conteneur en marche.")
        );
        return;
    }
    rows.sort();
    rows.insert(
        0,
        vec![
            tr("ADDRESS", "ADRESSE").to_owned(),
            tr("CONTAINER", "CONTENEUR").to_owned(),
        ],
    );
    util::table(&rows);
}

fn absolute(p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|d| d.join(&path))
            .unwrap_or(path)
    }
}

/// Nom de projet d'un dossier : `--name`, sinon la clé `name:` du fichier Compose, sinon le dossier.
fn project_of(dir: &Path, args: &Args) -> Result<(String, String), String> {
    let dir_s = dir.to_string_lossy().into_owned();
    let detected = solon_client::compose::detect(&dir_s)?.ok_or_else(|| {
        format!(
            "{dir_s} : {}",
            tr(
                "no Compose file in this folder",
                "aucun fichier Compose dans ce dossier"
            )
        )
    })?;
    if let Some(n) = args.value("--name") {
        return Ok((n.to_owned(), detected.file));
    }
    let text = std::fs::read_to_string(dir.join(&detected.file)).unwrap_or_default();
    Ok((
        solon_client::compose::project_name(dir, &text),
        detected.file,
    ))
}

/// `solon backup [folder] [file.zip] [--name project]`
pub async fn backup(args: &Args) {
    let dir = absolute(args.pos(0).unwrap_or("."));
    let (project, _) = project_of(&dir, args).unwrap_or_else(|e| fail(e));
    let dest = args
        .pos(1)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{project}-{}.zip", util::file_stamp()));
    out!(
        "{} {project} ({})…",
        tr("Backing up", "Sauvegarde de"),
        dir.display()
    );
    let report = solon_client::backup::backup(
        &dir.to_string_lossy(),
        &project,
        &dest,
        env!("CARGO_PKG_VERSION"),
        args.flag("--force"),
    )
    .await
    .unwrap_or_else(|e| fail(e));
    out!(
        "{} {} ({} {}, {})",
        tr("Written:", "Écrit :"),
        report.path,
        report.volumes,
        tr("volumes", "volumes"),
        util::bytes(report.bytes)
    );
}

/// `solon restore <file.zip> [folder]`
pub async fn restore(args: &Args) {
    let Some(zip) = args.pos(0) else {
        fail(tr(
            "usage: solon restore <backup.zip> [folder]",
            "usage : solon restore <sauvegarde.zip> [dossier]",
        ));
    };
    let info = solon_client::backup::info(zip).unwrap_or_else(|e| fail(e));
    let project = info
        .get("project")
        .and_then(|v| v.as_str())
        .unwrap_or("project")
        .to_owned();
    let target = args
        .pos(1)
        .map(absolute)
        .unwrap_or_else(|| absolute(&project));
    let volumes = info
        .get("volumes")
        .and_then(|v| v.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    out!(
        "{} {project} ({} {}) → {}",
        tr("Restoring", "Restauration de"),
        volumes,
        tr("volumes", "volumes"),
        target.display()
    );
    let report = solon_client::backup::restore(zip, &target.to_string_lossy())
        .await
        .unwrap_or_else(|e| fail(e));
    out!(
        "{} {} {}. {}",
        tr("Done:", "Terminé :"),
        report.volumes,
        tr("volumes restored", "volumes restaurés"),
        if crate::text::french() {
            format!(
                "Pour lancer le projet : cd \"{}\" && docker compose up -d",
                report.dir
            )
        } else {
            format!("To start it: cd \"{}\" && docker compose up -d", report.dir)
        }
    );
}
