//! Le moteur : état, démarrage, arrêt, prérequis, réglages, journaux, diagnostic, exécution.

use std::time::Duration;

use serde_json::Value;
use solon_client::control::{call, exec, stream};
use solon_core::ipc::{EngineSnapshot, EngineState, PrereqReport, ServiceCommand, Settings};

use crate::text::tr;
use crate::util::{self, Args, fail};

fn from_value<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("unreadable answer from the service: {e}"))
}

pub async fn snapshot() -> Result<EngineSnapshot, String> {
    from_value(call(ServiceCommand::Status).await?)
}

/// Un conteneur tel que `docker ps` le décrit, avec ses étiquettes Compose.
#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub project: Option<String>,
    pub service: Option<String>,
    pub state: String,
    pub status: String,
    pub ports: String,
}

impl Container {
    /// Les domaines locaux du conteneur : `service.projet.solon.local` pour un service Compose,
    /// `nom.solon.local` toujours.
    pub fn domains(&self) -> Vec<String> {
        let mut d = Vec::new();
        if let (Some(p), Some(s)) = (&self.project, &self.service) {
            d.push(format!("{s}.{p}.solon.local"));
        }
        d.push(format!("{}.solon.local", self.name));
        d
    }
}

/// Tous les conteneurs (`docker ps -a`), lus dans la machine.
pub async fn containers() -> Result<Vec<Container>, String> {
    let (code, out, err) = exec(
        "docker ps -a --no-trunc --format '{{.ID}}|{{.Names}}|{{.Label \"com.docker.compose.project\"}}|{{.Label \"com.docker.compose.service\"}}|{{.State}}|{{.Status}}|{{.Ports}}'".into(),
        30,
    )
    .await?;
    if code != 0 {
        return Err(format!("docker ps : {}", err.trim()));
    }
    let opt = |s: &str| {
        if s.is_empty() {
            None
        } else {
            Some(s.to_owned())
        }
    };
    Ok(out
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(7, '|').collect();
            (f.len() == 7).then(|| Container {
                id: f[0].to_owned(),
                name: f[1].to_owned(),
                project: opt(f[2]),
                service: opt(f[3]),
                state: f[4].to_owned(),
                status: f[5].to_owned(),
                ports: f[6].to_owned(),
            })
        })
        .collect())
}

fn state_word(s: Option<EngineState>) -> &'static str {
    match s {
        Some(EngineState::Ready) => tr("ready", "prêt"),
        Some(EngineState::Starting) => tr("starting", "en démarrage"),
        Some(EngineState::Stopping) => tr("stopping", "en arrêt"),
        Some(EngineState::Degraded) => tr("degraded", "dégradé"),
        Some(EngineState::Failed) => tr("failed", "en échec"),
        Some(EngineState::Stopped) | None => tr("stopped", "arrêté"),
    }
}

pub async fn status(args: &Args) {
    let snap = match snapshot().await {
        Ok(s) => s,
        Err(e) => {
            if args.json() {
                out!("{}", serde_json::json!({ "error": e }));
            } else {
                out!(
                    "{}",
                    tr(
                        "Solon service not reachable: is Solon installed and the SolonService service running?",
                        "Service Solon injoignable : Solon est-il installé, et le service SolonService démarré ?"
                    )
                );
                out!("  {e}");
            }
            crate::util::exit(3);
        }
    };
    if args.json() {
        out!(
            "{}",
            serde_json::to_string_pretty(&snap).unwrap_or_default()
        );
        return;
    }
    let mut head = format!(
        "{} {} · {} {}",
        "Solon",
        env!("CARGO_PKG_VERSION"),
        tr("engine", "moteur"),
        state_word(snap.state)
    );
    if let Some(since) = snap.ready_since_unix_ms
        && snap.state == Some(EngineState::Ready)
    {
        head.push_str(&format!(
            " {} {} ({})",
            tr("since", "depuis"),
            util::clock(since),
            util::dur(util::now_ms().saturating_sub(since))
        ));
        if let Some(b) = snap.last_boot_ms {
            head.push_str(&format!(
                ", {} {}",
                tr("started in", "démarré en"),
                util::dur(b)
            ));
        }
    }
    if let Some(step) = &snap.step
        && snap.state == Some(EngineState::Starting)
    {
        head.push_str(&format!(" ({})", step_label(step)));
    }
    out!("{head}");
    if let Some(e) = &snap.error {
        out!("  [{:?}] {}", e.code, e.message);
        out!(
            "  {}",
            tr(
                "See `solon logs`, or `solon diag` to export everything for a bug report.",
                "Voir `solon logs`, ou `solon diag` pour tout exporter en vue d'un signalement."
            )
        );
    }
    if snap.state != Some(EngineState::Ready) {
        if snap.state.is_none() || snap.state == Some(EngineState::Stopped) {
            out!(
                "  {}",
                tr("Start it with: solon start", "Démarrer : solon start")
            );
        }
        return;
    }
    out!(
        "  {} {} · {} {}",
        tr("image", "image"),
        snap.image_version.as_deref().unwrap_or("?"),
        tr("address", "adresse"),
        snap.guest_address.as_deref().unwrap_or("?")
    );
    let domains = match (snap.local_domains, snap.local_domains_tls) {
        (true, true) => tr("http and https", "http et https"),
        (true, false) => tr("http only", "http seulement"),
        _ => tr("off", "désactivés"),
    };
    out!(
        "  {} *.solon.local : {domains}",
        tr("local domains", "domaines locaux")
    );
    let list = match containers().await {
        Ok(l) => l,
        Err(e) => {
            out!("  {e}");
            return;
        }
    };
    let running: Vec<&Container> = list.iter().filter(|c| c.state == "running").collect();
    let sleeping = snap.sleeping.len();
    out!();
    if running.is_empty() {
        out!(
            "{}",
            tr("No container running.", "Aucun conteneur en marche.")
        );
        return;
    }
    out!(
        "{} {}{}",
        running.len(),
        tr("containers running", "conteneurs en marche"),
        if sleeping > 0 {
            format!(
                ", {sleeping} {}",
                if sleeping == 1 {
                    tr("asleep (woken on demand)", "endormi (réveil à la demande)")
                } else {
                    tr("asleep (woken on demand)", "endormis (réveil à la demande)")
                }
            )
        } else {
            String::new()
        }
    );
    let scheme = if snap.local_domains_tls {
        "https"
    } else {
        "http"
    };
    let mut rows = vec![vec![
        tr("CONTAINER", "CONTENEUR").to_owned(),
        tr("PROJECT", "PROJET").to_owned(),
        tr("STATUS", "ÉTAT").to_owned(),
        tr("ADDRESS", "ADRESSE").to_owned(),
    ]];
    let mut sorted = running.clone();
    sorted.sort_by(|a, b| a.project.cmp(&b.project).then(a.name.cmp(&b.name)));
    for c in sorted {
        let asleep = snap
            .sleeping
            .iter()
            .any(|id| c.id.starts_with(id) || id.starts_with(&c.id));
        let status = if asleep {
            tr("asleep", "endormi").to_owned()
        } else {
            c.status.clone()
        };
        let address = if snap.local_domains {
            format!("{scheme}://{}", c.domains()[0])
        } else {
            c.ports.clone()
        };
        rows.push(vec![
            c.name.clone(),
            c.project.clone().unwrap_or_else(|| "-".into()),
            status,
            address,
        ]);
    }
    util::table(&rows);
}

fn step_label(step: &solon_core::ipc::ProvisionStep) -> &'static str {
    use solon_core::ipc::ProvisionStep as S;
    match step {
        S::CheckingPrerequisites => tr("checking prerequisites", "vérification des prérequis"),
        S::VerifyingImage => tr("verifying the image", "vérification de l'image"),
        S::PreparingDataDisk => tr(
            "preparing the data disk",
            "préparation du disque de données",
        ),
        S::CleaningOrphans => tr("cleaning up", "nettoyage"),
        S::CreatingNetwork => tr("creating the network", "création du réseau"),
        S::CreatingMachine => tr("creating the engine", "création du moteur"),
        S::Booting => tr("booting", "amorçage"),
        S::WaitingAgent => tr("waiting for the engine", "attente du moteur"),
        S::ConfiguringNetwork => tr("configuring the network", "configuration du réseau"),
        S::WaitingEngine => tr("waiting for Docker", "attente de Docker"),
        S::Ready => tr("ready", "prêt"),
    }
}

/// Suit les événements jusqu'à un état final ; imprime chaque étape une fois.
async fn follow(done: &[EngineState]) -> Result<EngineState, String> {
    let mut last_step: Option<String> = None;
    let mut final_state: Option<EngineState> = None;
    let mut error: Option<String> = None;
    stream(ServiceCommand::Subscribe, None, |ev| {
        match ev.get("event").and_then(|e| e.as_str()) {
            Some("state") => {
                let Ok(snap) = serde_json::from_value::<EngineSnapshot>(ev.clone()) else {
                    return true;
                };
                if let Some(step) = &snap.step {
                    let label = step_label(step).to_owned();
                    if snap.state == Some(EngineState::Starting)
                        && last_step.as_ref() != Some(&label)
                    {
                        out!("  {label}…");
                        last_step = Some(label);
                    }
                }
                if let Some(e) = &snap.error {
                    error = Some(format!("[{:?}] {}", e.code, e.message));
                }
                if let Some(s) = snap.state
                    && done.contains(&s)
                {
                    final_state = Some(s);
                    return false;
                }
            }
            Some("log") => {
                let level = ev.get("level").and_then(|l| l.as_str()).unwrap_or("");
                if matches!(level, "warn" | "error") {
                    out!(
                        "  {level}: {}",
                        ev.get("message").and_then(|m| m.as_str()).unwrap_or("")
                    );
                }
            }
            _ => {}
        }
        true
    })
    .await?;
    if let Some(e) = error
        && final_state == Some(EngineState::Failed)
    {
        return Err(e);
    }
    final_state.ok_or_else(|| "flux interrompu".to_owned())
}

pub async fn start(_args: &Args) {
    let snap = snapshot().await.unwrap_or_else(|e| fail(e));
    if snap.state == Some(EngineState::Ready) {
        out!(
            "{}",
            tr("The engine is already running.", "Le moteur tourne déjà.")
        );
        return;
    }
    call(ServiceCommand::Start)
        .await
        .unwrap_or_else(|e| fail(e));
    out!("{}", tr("Starting the engine…", "Démarrage du moteur…"));
    match follow(&[
        EngineState::Ready,
        EngineState::Failed,
        EngineState::Stopped,
    ])
    .await
    {
        Ok(EngineState::Ready) => {
            let snap = snapshot().await.unwrap_or_default();
            out!(
                "{} {}",
                tr("Engine ready in", "Moteur prêt en"),
                util::dur(snap.last_boot_ms.unwrap_or(0))
            );
        }
        Ok(s) => fail(format!(
            "{} : {}",
            tr("engine", "moteur"),
            state_word(Some(s))
        )),
        Err(e) => fail(e),
    }
}

pub async fn stop(args: &Args) {
    let force = args.flag("--force");
    if force {
        out!(
            "{}",
            tr(
                "Forced stop: containers are cut without a clean shutdown.",
                "Arrêt forcé : les conteneurs sont coupés sans arrêt propre."
            )
        );
    }
    call(ServiceCommand::Stop { force })
        .await
        .unwrap_or_else(|e| fail(e));
    out!("{}", tr("Stopping the engine…", "Arrêt du moteur…"));
    match follow(&[EngineState::Stopped, EngineState::Failed]).await {
        Ok(EngineState::Stopped) => out!("{}", tr("Engine stopped.", "Moteur arrêté.")),
        Ok(s) => fail(state_word(Some(s))),
        Err(e) => fail(e),
    }
}

pub async fn restart(_args: &Args) {
    call(ServiceCommand::Restart)
        .await
        .unwrap_or_else(|e| fail(e));
    out!("{}", tr("Restarting the engine…", "Redémarrage du moteur…"));
    match follow(&[EngineState::Ready, EngineState::Failed]).await {
        Ok(EngineState::Ready) => out!("{}", tr("Engine ready.", "Moteur prêt.")),
        Ok(s) => fail(state_word(Some(s))),
        Err(e) => fail(e),
    }
}

/// Événements du service en continu, jusqu'à Ctrl+C.
pub async fn watch(args: &Args) {
    let json = args.json();
    let r = stream(ServiceCommand::Subscribe, None, |ev| {
        if json {
            out!("{ev}");
            return true;
        }
        let now = util::clock(util::now_ms());
        match ev.get("event").and_then(|e| e.as_str()) {
            Some("state") => {
                let snap: EngineSnapshot = serde_json::from_value(ev.clone()).unwrap_or_default();
                let mut line = format!(
                    "{now}  {} {}",
                    tr("engine", "moteur"),
                    state_word(snap.state)
                );
                if let Some(step) = &snap.step
                    && snap.state == Some(EngineState::Starting)
                {
                    line.push_str(&format!(" ({})", step_label(step)));
                }
                if let Some(e) = &snap.error {
                    line.push_str(&format!(" [{:?}] {}", e.code, e.message));
                }
                out!("{line}");
            }
            Some("container") => out!(
                "{now}  {} {} {}",
                tr("container", "conteneur"),
                ev.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                ev.get("action").and_then(|v| v.as_str()).unwrap_or("?")
            ),
            Some("ports") => out!(
                "{now}  {} {}",
                tr("published ports:", "ports publiés :"),
                ev.get("bindings")
                    .and_then(|b| b.as_array())
                    .map(Vec::len)
                    .unwrap_or(0)
            ),
            Some("log") => out!(
                "{now}  {}: {}",
                ev.get("level").and_then(|v| v.as_str()).unwrap_or("log"),
                ev.get("message").and_then(|v| v.as_str()).unwrap_or("")
            ),
            Some("disk_pressure") => out!(
                "{now}  {} {} % ({} {} MB {})",
                tr("data disk at", "disque de données à"),
                ev.get("used_pct").and_then(|v| v.as_u64()).unwrap_or(0),
                tr("only", "seulement"),
                ev.get("free_mb").and_then(|v| v.as_u64()).unwrap_or(0),
                tr("free", "libres")
            ),
            Some("resumed") => out!(
                "{now}  {} {:?}",
                tr("projects resumed:", "projets relancés :"),
                ev.get("projects").cloned().unwrap_or(Value::Null)
            ),
            _ => out!("{now}  {ev}"),
        }
        true
    })
    .await;
    if let Err(e) = r {
        fail(e);
    }
}

pub async fn prereq(args: &Args) {
    let v = call(ServiceCommand::Prerequisites)
        .await
        .unwrap_or_else(|e| fail(e));
    if args.json() {
        out!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return;
    }
    let report: PrereqReport = from_value(v).unwrap_or_else(|e| fail(e));
    for item in &report.items {
        let mark = if item.ok {
            "ok "
        } else if item.blocking {
            "!! "
        } else {
            "-- "
        };
        out!("{mark}{}  {}", item.id, item.detail);
    }
    out!(
        "{}",
        if report.ok {
            tr(
                "All prerequisites are met.",
                "Tous les prérequis sont réunis.",
            )
        } else {
            tr(
                "A blocking prerequisite is missing (lines marked !!).",
                "Un prérequis bloquant manque (lignes marquées !!).",
            )
        }
    );
    if !report.ok {
        crate::util::exit(1);
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

fn parse_switch(v: &str) -> Result<bool, String> {
    match v.to_lowercase().as_str() {
        "on" | "true" | "yes" | "oui" | "1" => Ok(true),
        "off" | "false" | "no" | "non" | "0" => Ok(false),
        _ => Err(format!("{v} : on/off")),
    }
}

fn parse_list(v: &str) -> Vec<String> {
    if v.eq_ignore_ascii_case("none") || v.eq_ignore_ascii_case("aucun") || v.is_empty() {
        return Vec::new();
    }
    v.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `solon settings` montre ; `solon settings memory=4096 sleep=off` modifie.
pub async fn settings(args: &Args) {
    let mut s: Settings = from_value(
        call(ServiceCommand::GetSettings)
            .await
            .unwrap_or_else(|e| fail(e)),
    )
    .unwrap_or_else(|e| fail(e));
    if args.positional.is_empty() {
        if args.json() {
            out!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
            return;
        }
        let rows = vec![
            vec![
                "memory".to_owned(),
                format!("{} MB", s.memory_mb),
                tr(
                    "memory of the engine (next start)",
                    "mémoire du moteur (prochain démarrage)",
                )
                .to_owned(),
            ],
            vec![
                "cpus".to_owned(),
                s.processors.to_string(),
                tr(
                    "processors (next start)",
                    "processeurs (prochain démarrage)",
                )
                .to_owned(),
            ],
            vec![
                "disk".to_owned(),
                format!("{} GiB", s.data_disk_gib),
                tr(
                    "data disk, grows only (next start)",
                    "disque de données, ne fait que grandir (prochain démarrage)",
                )
                .to_owned(),
            ],
            vec![
                "autostart".to_owned(),
                on_off(s.autostart).to_owned(),
                tr(
                    "start the engine at sign-in",
                    "démarrer le moteur à l'ouverture de session",
                )
                .to_owned(),
            ],
            vec![
                "resume".to_owned(),
                on_off(s.resume_running).to_owned(),
                tr(
                    "restart what was running when the engine starts",
                    "relancer ce qui tournait au démarrage du moteur",
                )
                .to_owned(),
            ],
            vec![
                "sleep".to_owned(),
                on_off(s.sleep_enabled).to_owned(),
                tr(
                    "pause idle containers, wake on demand",
                    "endormir les conteneurs inactifs, réveil à la demande",
                )
                .to_owned(),
            ],
            vec![
                "idle".to_owned(),
                format!("{} min", s.sleep_idle_minutes),
                tr(
                    "minutes before sleep",
                    "minutes d'inactivité avant la veille",
                )
                .to_owned(),
            ],
            vec![
                "never".to_owned(),
                if s.sleep_never.is_empty() {
                    "none".to_owned()
                } else {
                    s.sleep_never.join(",")
                },
                tr(
                    "containers never put to sleep",
                    "conteneurs jamais endormis",
                )
                .to_owned(),
            ],
            vec![
                "bodies".to_owned(),
                if s.trace_bodies.is_empty() {
                    "none".to_owned()
                } else {
                    s.trace_bodies.join(",")
                },
                tr(
                    "projects whose HTTP bodies Radiography keeps",
                    "projets dont la Radiographie garde les corps HTTP",
                )
                .to_owned(),
            ],
            vec![
                "legacy-9p".to_owned(),
                on_off(s.legacy_file_sharing).to_owned(),
                tr(
                    "share Windows folders with Windows 9P instead of solonfs",
                    "partager les dossiers Windows en 9P plutôt qu'en solonfs",
                )
                .to_owned(),
            ],
        ];
        util::table(&rows);
        out!();
        out!(
            "{}",
            tr(
                "Change one with: solon settings key=value [key=value…]",
                "Modifier : solon settings clé=valeur [clé=valeur…]"
            )
        );
        return;
    }
    let mut needs_restart = false;
    for kv in &args.positional {
        let Some((k, v)) = kv.split_once('=') else {
            fail(format!(
                "{kv} : {}",
                tr("expected key=value", "attendu clé=valeur")
            ));
        };
        let num = |what: &str| -> u64 {
            v.parse::<u64>().unwrap_or_else(|_| {
                fail(format!(
                    "{what}={v} : {}",
                    tr("a whole number", "un nombre entier")
                ))
            })
        };
        match k {
            "memory" => {
                s.memory_mb = num(k);
                needs_restart = true;
            }
            "cpus" => {
                s.processors = num(k) as u32;
                needs_restart = true;
            }
            "disk" => {
                s.data_disk_gib = num(k);
                needs_restart = true;
            }
            "autostart" => s.autostart = parse_switch(v).unwrap_or_else(|e| fail(e)),
            "resume" => s.resume_running = parse_switch(v).unwrap_or_else(|e| fail(e)),
            "sleep" => s.sleep_enabled = parse_switch(v).unwrap_or_else(|e| fail(e)),
            "idle" => s.sleep_idle_minutes = num(k) as u32,
            "never" => s.sleep_never = parse_list(v),
            "bodies" => s.trace_bodies = parse_list(v),
            "legacy-9p" => {
                s.legacy_file_sharing = parse_switch(v).unwrap_or_else(|e| fail(e));
                needs_restart = true;
            }
            _ => fail(format!(
                "{k} : {}",
                tr(
                    "unknown setting (memory, cpus, disk, autostart, resume, sleep, idle, never, bodies, legacy-9p)",
                    "réglage inconnu (memory, cpus, disk, autostart, resume, sleep, idle, never, bodies, legacy-9p)"
                )
            )),
        }
    }
    call(ServiceCommand::SetSettings(s))
        .await
        .unwrap_or_else(|e| fail(e));
    out!("{}", tr("Saved.", "Enregistré."));
    if needs_restart {
        out!(
            "{}",
            tr(
                "Memory, processors, disk and file sharing apply at the next engine start: solon restart",
                "Mémoire, processeurs, disque et partage de fichiers s'appliquent au prochain démarrage du moteur : solon restart"
            )
        );
    }
}

pub async fn version(_args: &Args) {
    out!("solon {}", env!("CARGO_PKG_VERSION"));
    match call(ServiceCommand::Version).await {
        Ok(v) => out!(
            "{} {}",
            tr("service", "service"),
            v.get("service")
                .or_else(|| v.get("version"))
                .and_then(|s| s.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        ),
        Err(e) => out!("{} {e}", tr("service:", "service :")),
    }
    if let Ok(s) = snapshot().await
        && let Some(img) = s.image_version
    {
        out!("{} {img}", tr("image", "image"));
    }
}

/// Une commande shell dans la machine (root). Sortie et code propagés. Les mots sont passés tels
/// quels (pas d'analyse d'options : `uname -r` garde son `-r`).
pub async fn exec_cmd(words: &[String]) {
    if words.is_empty() {
        fail(tr(
            "usage: solon exec <command…>",
            "usage : solon exec <commande…>",
        ));
    }
    // Un seul mot : passé brut à `sh` (une ligne complète avec ses tubes). Plusieurs : chaque mot
    // qui contient un blanc ou une quote est requoté, sinon `ls "/un dossier"` ferait deux arguments.
    let line = if words.len() == 1 {
        words[0].clone()
    } else {
        words
            .iter()
            .map(|w| {
                if w.chars()
                    .any(|c| c.is_whitespace() || c == '\'' || c == '"')
                {
                    solon_client::control::shell_quote(w)
                } else {
                    w.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (code, out, err) = exec(line, 600).await.unwrap_or_else(|e| fail(e));
    outp!("{out}");
    eprint!("{err}");
    crate::util::exit(code);
}

/// Journal du service : les dernières lignes, et `-f` pour suivre.
pub async fn logs(args: &Args) {
    let Some(path) = solon_client::diag::recent_logs(1).into_iter().next() else {
        fail(tr(
            "no service log found under %ProgramData%\\Solon\\logs",
            "aucun journal du service sous %ProgramData%\\Solon\\logs",
        ));
    };
    let n = args.last(50);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| fail(format!("{} : {e}", path.display())));
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    for l in &lines[start..] {
        out!("{l}");
    }
    if !(args.flag("-f") || args.flag("--follow")) {
        return;
    }
    let mut offset = text.len() as u64;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.len() < offset {
            offset = 0; // fichier recréé
        }
        if meta.len() > offset {
            use std::io::{Read, Seek};
            if let Ok(mut f) = std::fs::File::open(&path)
                && f.seek(std::io::SeekFrom::Start(offset)).is_ok()
            {
                let mut buf = String::new();
                if f.read_to_string(&mut buf).is_ok() {
                    outp!("{buf}");
                    offset = meta.len();
                }
            }
        }
    }
}

/// Export de diagnostic (zip) pour un signalement.
pub async fn diag(args: &Args) {
    let dest = args
        .pos(0)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("solon-diagnostic-{}.zip", util::file_stamp()));
    let count = solon_client::diag::export(env!("CARGO_PKG_VERSION"), &dest)
        .await
        .unwrap_or_else(|e| fail(e));
    out!(
        "{} {dest} ({count} {})",
        tr("Diagnostic written to", "Diagnostic écrit dans"),
        tr("files", "fichiers")
    );
    out!(
        "{}",
        tr(
            "It contains no credentials; the logs may show the paths of shared folders.",
            "Il ne contient aucun identifiant ; les journaux peuvent montrer les chemins des dossiers partagés."
        )
    );
}
