//! Ce que l'application de bureau faisait et que le CLI reprend : mise à jour, recherche des projets
//! sur le PC, shell dans le moteur, copie des volumes d'un projet, import depuis Docker Desktop.

use std::io::Write;

use solon_client::control::call;
use solon_client::update::{self, Progress};
use solon_core::ipc::{FoundProject, ServiceCommand};

use crate::text::tr;
use crate::util::{self, Args, fail};

/// `solon update [--check]` : la dernière version publiée ; sans `--check`, la télécharge, vérifie son
/// SHA-256 et lance l'installateur (Windows demande l'élévation).
pub async fn update(args: &Args) {
    if update::packaged() {
        out!(
            "{}",
            tr(
                "This Solon comes from the Microsoft Store: updates come from the Store.",
                "Ce Solon vient du Microsoft Store : les mises à jour viennent du Store."
            )
        );
        return;
    }
    let current = env!("CARGO_PKG_VERSION");
    out!(
        "{}",
        tr(
            "Asking GitHub for the latest version…",
            "Demande de la dernière version à GitHub…"
        )
    );
    let release = update::latest().await.unwrap_or_else(|e| fail(e));
    if args.json() {
        out!(
            "{}",
            serde_json::to_string_pretty(&release).unwrap_or_default()
        );
        return;
    }
    if !update::is_newer(&release.version, current) {
        out!(
            "{} ({current}).",
            tr("Solon is up to date", "Solon est à jour")
        );
        return;
    }
    out!(
        "{} {} ({} {current}). {}",
        tr("New version:", "Nouvelle version :"),
        release.version,
        tr("installed:", "installée :"),
        release.page
    );
    let notes: Vec<&str> = release.notes.lines().take(12).collect();
    if !notes.is_empty() {
        out!();
        for l in &notes {
            out!("  {l}");
        }
        if release.notes.lines().count() > 12 {
            out!("  …");
        }
        out!();
    }
    if args.flag("--check") {
        out!(
            "{}",
            tr(
                "Run `solon update` to install it.",
                "Lancez `solon update` pour l'installer."
            )
        );
        return;
    }
    let path = update::download(&release, |p| {
        let mut o = std::io::stdout().lock();
        match p {
            Progress::Downloading { received, total } => {
                let _ = match total {
                    Some(t) if t > 0 => write!(
                        o,
                        "\r{} {} / {} ({} %)   ",
                        tr("Downloading", "Téléchargement"),
                        util::bytes(received),
                        util::bytes(t),
                        received * 100 / t
                    ),
                    _ => write!(
                        o,
                        "\r{} {}   ",
                        tr("Downloading", "Téléchargement"),
                        util::bytes(received)
                    ),
                };
            }
            Progress::Verifying => {
                let _ = write!(
                    o,
                    "\n{}",
                    tr("Verifying the SHA-256…", "Vérification du SHA-256…")
                );
            }
        }
        let _ = o.flush();
    })
    .await
    .unwrap_or_else(|e| fail(e));
    out!(" {}", tr("ok", "ok"));
    out!(
        "{}",
        tr(
            "Installing: Windows will ask for permission. The engine stops, Solon is replaced, the service restarts what was running.",
            "Installation : Windows va demander l'autorisation. Le moteur s'arrête, Solon est remplacé, le service relance ce qui tournait."
        )
    );
    update::install(&path).unwrap_or_else(|e| fail(e));
}

/// `solon scan [--json]` : les projets Docker trouvés sur les disques internes (recherche faite par le
/// service, qui a les droits de lire la table des fichiers).
pub async fn scan(args: &Args) {
    out!(
        "{}",
        tr(
            "Searching the internal drives (a few seconds)…",
            "Recherche sur les disques internes (quelques secondes)…"
        )
    );
    let v = call(ServiceCommand::ScanProjects)
        .await
        .unwrap_or_else(|e| fail(e));
    if args.json() {
        out!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return;
    }
    let found: Vec<FoundProject> = v
        .get("projects")
        .cloned()
        .and_then(|p| serde_json::from_value(p).ok())
        .or_else(|| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    if found.is_empty() {
        out!("{}", tr("No project found.", "Aucun projet trouvé."));
        return;
    }
    let mut rows = vec![vec![
        tr("FOLDER", "DOSSIER").to_owned(),
        tr("KIND", "TYPE").to_owned(),
        "GIT".to_owned(),
    ]];
    for p in &found {
        rows.push(vec![
            p.dir.clone(),
            p.kinds.join(", "),
            if p.git { tr("yes", "oui") } else { "-" }.to_owned(),
        ]);
    }
    util::table(&rows);
    out!();
    out!(
        "{}",
        tr(
            "Start one with: cd <folder> && docker compose up -d",
            "Lancer un projet : cd <dossier> && docker compose up -d"
        )
    );
}

/// `solon shell` : un shell root dans le moteur, par un conteneur privilégié qui entre dans les
/// espaces de noms du PID 1. Passe par le `docker` livré à côté de `solon`.
pub fn shell(_args: &Args) {
    let docker = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("docker.exe")))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "docker".to_owned());
    out!(
        "{}",
        tr(
            "Root shell inside the engine (exit to leave).",
            "Shell root dans le moteur (exit pour sortir)."
        )
    );
    let status = std::process::Command::new(docker)
        .env("DOCKER_HOST", "npipe:////./pipe/solon")
        .args([
            "run",
            "--rm",
            "-it",
            "--privileged",
            "--pid=host",
            "--net=host",
            "public.ecr.aws/docker/library/busybox:1.37",
            "nsenter",
            "-t",
            "1",
            "-m",
            "-u",
            "-n",
            "-i",
            "sh",
        ])
        .status();
    match status {
        Ok(s) => util::exit(s.code().unwrap_or(1)),
        Err(e) => fail(format!("docker: {e}")),
    }
}

/// `solon clone <from-project> <to-project>` : copie les volumes d'un projet vers un autre nom.
pub async fn clone(args: &Args) {
    let (Some(from), Some(to)) = (args.pos(0), args.pos(1)) else {
        fail(tr(
            "usage: solon clone <from-project> <to-project>",
            "usage : solon clone <projet-source> <projet-cible>",
        ));
    };
    out!(
        "{} {from} → {to}…",
        tr("Copying the volumes of", "Copie des volumes de")
    );
    let n = solon_client::volumes::clone_project(from, to)
        .await
        .unwrap_or_else(|e| fail(e));
    out!(
        "{n} {}. {}",
        tr("volumes copied", "volumes copiés"),
        if crate::text::french() {
            format!("Lancez le projet avec : docker compose -p {to} up -d")
        } else {
            format!("Start the project with: docker compose -p {to} up -d")
        }
    );
}

/// `solon import` liste ce que Docker Desktop contient ; `--volumes a,b --images x,y` ou `--all` copie.
pub async fn import(args: &Args) {
    let source = solon_client::import::probe().await;
    if !source.available {
        fail(format!(
            "{} {}",
            tr(
                "Docker Desktop does not answer (is it installed and running?):",
                "Docker Desktop ne répond pas (est-il installé et démarré ?) :"
            ),
            source.reason.unwrap_or_default()
        ));
    }
    let list = |v: Option<&str>| -> Vec<String> {
        v.map(|s| {
            s.split(',')
                .map(|x| x.trim().to_owned())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default()
    };
    let (mut volumes, mut images) = (list(args.value("--volumes")), list(args.value("--images")));
    if args.flag("--all") {
        volumes = source
            .volumes
            .iter()
            .filter(|v| !v.exists_here)
            .map(|v| v.name.clone())
            .collect();
        images = source
            .images
            .iter()
            .filter(|i| !i.exists_here)
            .map(|i| i.name.clone())
            .collect();
    }
    if volumes.is_empty() && images.is_empty() {
        if args.json() {
            out!(
                "{}",
                serde_json::to_string_pretty(&source).unwrap_or_default()
            );
            return;
        }
        out!(
            "Docker Desktop {}",
            source.version.as_deref().unwrap_or("?")
        );
        let size = |s: Option<i64>| {
            s.map(|b| util::bytes(b as u64))
                .unwrap_or_else(|| "-".into())
        };
        let mark = |here: bool, used: bool| {
            let mut m = String::new();
            if here {
                m.push_str(tr("already in Solon", "déjà dans Solon"));
            }
            if used {
                if !m.is_empty() {
                    m.push_str(", ");
                }
                m.push_str(tr("in use", "utilisé"));
            }
            m
        };
        if !source.volumes.is_empty() {
            out!();
            let mut rows = vec![vec![
                "VOLUME".to_owned(),
                tr("SIZE", "TAILLE").to_owned(),
                String::new(),
            ]];
            for v in &source.volumes {
                rows.push(vec![
                    v.name.clone(),
                    size(v.size),
                    mark(v.exists_here, v.in_use),
                ]);
            }
            util::table(&rows);
        }
        if !source.images.is_empty() {
            out!();
            let mut rows = vec![vec![
                "IMAGE".to_owned(),
                tr("SIZE", "TAILLE").to_owned(),
                String::new(),
            ]];
            for i in &source.images {
                rows.push(vec![
                    i.name.clone(),
                    size(i.size),
                    mark(i.exists_here, i.in_use),
                ]);
            }
            util::table(&rows);
        }
        out!();
        out!(
            "{}",
            tr(
                "Copy with: solon import --volumes a,b --images x:tag   or   solon import --all\nDocker Desktop is never modified. Containers are not copied: recreate them with docker compose up.",
                "Copier : solon import --volumes a,b --images x:tag   ou   solon import --all\nDocker Desktop n'est jamais modifié. Les conteneurs ne sont pas copiés : recréez-les avec docker compose up."
            )
        );
        return;
    }
    let report = solon_client::import::run(&volumes, &images, |label, done, total| {
        if !label.is_empty() {
            out!("[{}/{total}] {label}", done + 1);
        }
    })
    .await
    .unwrap_or_else(|e| fail(e));
    out!();
    out!(
        "{} {} {}, {} {}.",
        tr("Imported:", "Importés :"),
        report.volumes_imported.len(),
        tr("volumes", "volumes"),
        report.images_imported.len(),
        tr("images", "images")
    );
    for (name, why) in &report.skipped {
        out!("  {} {name}: {why}", tr("skipped", "ignoré"));
    }
}
