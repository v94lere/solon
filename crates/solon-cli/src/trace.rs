//! La radiographie en ligne de commande : le journal des échanges (`trace`), l'explication en
//! langage courant de ce qui ne va pas (`why`), l'oubli (`clear`).

use std::collections::BTreeMap;
use std::time::Duration;

use solon_client::control::{call, stream};
use solon_core::ipc::ServiceCommand;
use solon_core::protocol::{ConnectionOutcome, TraceConnection, TraceEvent, TraceHttp, TracePeer};

use crate::text::tr;
use crate::util::{self, Args, fail};

/// Après ce silence, l'historique est passé : ce qui suit est le direct.
const BACKLOG_IDLE: Duration = Duration::from_millis(600);

/// Ce que les sondes ont dit d'elles-mêmes, puis les faits.
pub struct Collected {
    pub active: Option<(bool, String)>,
    pub events: Vec<TraceEvent>,
}

/// L'historique gardé par le service (jusqu'à 20 000 faits), sans le direct.
pub async fn collect() -> Result<Collected, String> {
    let mut c = Collected {
        active: None,
        events: Vec::new(),
    };
    stream(ServiceCommand::TraceSubscribe, Some(BACKLOG_IDLE), |v| {
        if let Ok(ev) = serde_json::from_value::<TraceEvent>(v) {
            match ev {
                TraceEvent::Status { active, detail } => c.active = Some((active, detail)),
                other => c.events.push(other),
            }
        }
        true
    })
    .await?;
    Ok(c)
}

fn peer_matches(p: &TracePeer, needle: &str) -> bool {
    [&p.project, &p.service, &p.name]
        .into_iter()
        .flatten()
        .any(|s| s.eq_ignore_ascii_case(needle))
        || p.ip == needle
}

/// Le fait touche-t-il `needle` (projet, service, conteneur, hôte ou adresse) ?
fn matches(ev: &TraceEvent, needle: Option<&str>) -> bool {
    let Some(n) = needle else { return true };
    match ev {
        TraceEvent::Connection(c) => peer_matches(&c.src, n) || peer_matches(&c.dst, n),
        TraceEvent::Http(h) => {
            peer_matches(&h.dst, n) || h.host.eq_ignore_ascii_case(n) || h.host.contains(n)
        }
        TraceEvent::Status { .. } => false,
    }
}

fn ts(ev: &TraceEvent) -> u64 {
    match ev {
        TraceEvent::Connection(c) => c.ts_ms,
        TraceEvent::Http(h) => h.ts_ms,
        TraceEvent::Status { .. } => 0,
    }
}

fn label(p: &TracePeer) -> String {
    match (&p.project, &p.service) {
        (Some(pr), Some(s)) => format!("{s} ({pr})"),
        _ => p.label().to_owned(),
    }
}

fn line(ev: &TraceEvent) -> String {
    match ev {
        TraceEvent::Connection(c) => {
            let outcome = match c.outcome {
                ConnectionOutcome::Established => tr("open", "ouverte").to_owned(),
                ConnectionOutcome::Closed => format!(
                    "{} {}",
                    tr("closed after", "fermée après"),
                    util::dur(c.duration_ms.unwrap_or(0))
                ),
                ConnectionOutcome::Failed => {
                    tr("FAILED, no answer", "ÉCHEC, pas de réponse").to_owned()
                }
            };
            format!(
                "{}  {} -> {}:{}  {outcome}  [{}]",
                util::clock(c.ts_ms),
                label(&c.src),
                label(&c.dst),
                c.dport,
                c.comm
            )
        }
        TraceEvent::Http(h) => format!(
            "{}  {} {}://{}{}  -> {} {} {}  ({})",
            util::clock(h.ts_ms),
            h.method,
            h.scheme,
            h.host,
            h.path,
            h.status,
            tr("in", "en"),
            util::dur(h.duration_ms),
            label(&h.dst)
        ),
        TraceEvent::Status { active, detail } => format!("status active={active} {detail}"),
    }
}

fn explain_inactive(detail: &str) {
    out!(
        "{}",
        tr(
            "Radiography is not active: the kernel probes could not be loaded.",
            "La radiographie n'est pas active : les sondes du noyau n'ont pas pu être chargées."
        )
    );
    if !detail.is_empty() {
        out!("  {detail}");
    }
}

/// `solon trace [name] [--last N | --all] [-f] [--json]`
pub async fn trace(args: &Args) {
    let needle = args.pos(0);
    let json = args.json();
    let follow = args.flag("-f") || args.flag("--follow");
    let c = collect().await.unwrap_or_else(|e| fail(e));
    if let Some((false, detail)) = &c.active
        && !json
    {
        explain_inactive(detail);
    }
    let mut shown: Vec<&TraceEvent> = c.events.iter().filter(|e| matches(e, needle)).collect();
    if !args.flag("--all") {
        let n = args.last(50);
        let skip = shown.len().saturating_sub(n);
        shown.drain(..skip);
    }
    if shown.is_empty() && !json && !follow {
        out!(
            "{}",
            tr(
                "Nothing observed yet. Make a request to one of your services, then try again.",
                "Rien d'observé pour l'instant. Faites une requête vers un de vos services, puis réessayez."
            )
        );
        return;
    }
    let mut last_ts = 0;
    for ev in shown {
        if json {
            out!("{}", serde_json::to_string(ev).unwrap_or_default());
        } else {
            out!("{}", line(ev));
        }
        last_ts = last_ts.max(ts(ev));
    }
    if !follow {
        return;
    }
    // Le direct : un second abonnement renvoie d'abord l'historique, qu'on saute.
    let r = stream(ServiceCommand::TraceSubscribe, None, |v| {
        if let Ok(ev) = serde_json::from_value::<TraceEvent>(v)
            && !matches!(ev, TraceEvent::Status { .. })
            && ts(&ev) > last_ts
            && matches(&ev, needle)
        {
            if json {
                out!("{}", serde_json::to_string(&ev).unwrap_or_default());
            } else {
                out!("{}", line(&ev));
            }
        }
        true
    })
    .await;
    if let Err(e) = r {
        fail(e);
    }
}

/// `solon clear [project]`
pub async fn clear(args: &Args) {
    let project = args.pos(0).map(str::to_owned);
    call(ServiceCommand::TraceClear {
        project: project.clone(),
    })
    .await
    .unwrap_or_else(|e| fail(e));
    match project {
        Some(p) => out!(
            "{} {p}.",
            tr("Forgot everything about", "Oublié tout ce qui concerne")
        ),
        None => out!("{}", tr("History cleared.", "Historique effacé.")),
    }
}

#[derive(Default)]
struct Failures {
    count: usize,
    first: u64,
    last: u64,
    comm: String,
    dst_has_container: bool,
    dst_name: String,
    dst_id: Option<String>,
}

#[derive(Default)]
struct HostStats {
    total: usize,
    server_errors: usize,
    denied: usize,
    worst_error: Option<(String, u16)>,
    durations: Vec<u64>,
    slowest: Option<(String, u64)>,
}

/// `solon why [name]` : ce qui ne va pas, en langage courant.
pub async fn why(args: &Args) {
    let needle = args.pos(0);
    let c = collect().await.unwrap_or_else(|e| fail(e));
    if let Some((false, detail)) = &c.active {
        explain_inactive(detail);
        crate::util::exit(1);
    }
    let events: Vec<&TraceEvent> = c.events.iter().filter(|e| matches(e, needle)).collect();
    if events.is_empty() {
        out!(
            "{}",
            match needle {
                Some(n) => format!(
                    "{} {n}. {}",
                    tr("Nothing observed about", "Rien d'observé au sujet de"),
                    tr(
                        "Is it the name of a project, a service or a container? `solon status` lists them.",
                        "Est-ce bien le nom d'un projet, d'un service ou d'un conteneur ? `solon status` les liste."
                    )
                ),
                None => tr(
                    "Nothing observed yet. Make a request to one of your services, then try again.",
                    "Rien d'observé pour l'instant. Faites une requête vers un de vos services, puis réessayez."
                )
                .to_owned(),
            }
        );
        return;
    }
    let sleeping: Vec<String> = crate::engine::snapshot()
        .await
        .map(|s| s.sleeping)
        .unwrap_or_default();

    let mut failures: BTreeMap<(String, String, u16), Failures> = BTreeMap::new();
    let mut hosts: BTreeMap<String, HostStats> = BTreeMap::new();
    let mut connections = 0usize;
    let mut requests = 0usize;
    let mut first_ts = u64::MAX;
    for ev in &events {
        first_ts = first_ts.min(ts(ev));
        match ev {
            TraceEvent::Connection(cn) => {
                connections += 1;
                if cn.outcome == ConnectionOutcome::Failed {
                    note_failure(&mut failures, cn);
                }
            }
            TraceEvent::Http(h) => {
                requests += 1;
                note_http(&mut hosts, h);
            }
            TraceEvent::Status { .. } => {}
        }
    }

    let mut findings: Vec<String> = Vec::new();
    let mut failed: Vec<(&(String, String, u16), &Failures)> = failures.iter().collect();
    failed.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    for ((src, dst, port), f) in failed.into_iter().take(5) {
        let mut text = if crate::text::french() {
            format!(
                "{src} a essayé {} fois de joindre {dst} sur le port {port} sans jamais obtenir de réponse (la dernière à {}, depuis le programme « {} »).",
                f.count,
                util::clock(f.last),
                f.comm
            )
        } else {
            format!(
                "{src} tried {} times to reach {dst} on port {port} and never got an answer (last at {}, from the program \"{}\").",
                f.count,
                util::clock(f.last),
                f.comm
            )
        };
        let asleep = f.dst_id.as_ref().is_some_and(|id| {
            sleeping
                .iter()
                .any(|s| id.starts_with(s) || s.starts_with(id))
        });
        text.push(' ');
        text.push_str(if asleep {
            tr(
                "That container is asleep: Solon paused it after inactivity. It wakes when something reaches it through its solon.local address or a published port; a direct call from another container does not wake it. Wake it with `docker unpause`, or list it under `solon settings never=…`.",
                "Ce conteneur est endormi : Solon l'a mis en pause après inactivité. Il se réveille quand on le joint par son adresse solon.local ou un port publié ; un appel direct d'un autre conteneur ne le réveille pas. Réveillez-le avec `docker unpause`, ou inscrivez-le dans `solon settings never=…`."
            )
        } else if f.dst_has_container {
            tr(
                "Nothing was listening there. Is the container up, and does it listen on that port? `docker logs` on it usually says why.",
                "Rien n'écoutait là. Le conteneur est-il démarré, et écoute-t-il bien ce port ? `docker logs` dessus dit en général pourquoi."
            )
        } else {
            tr(
                "That address is outside the engine: a network, DNS or firewall problem, or the remote service is down.",
                "Cette adresse est hors du moteur : un problème de réseau, de DNS ou de pare-feu, ou le service distant est arrêté."
            )
        });
        findings.push(text);
    }
    for (host, st) in &hosts {
        if st.server_errors > 0 {
            let (path, status) = st.worst_error.clone().unwrap_or_default();
            findings.push(if crate::text::french() {
                format!(
                    "{host} a répondu {} erreurs de serveur (5xx) sur {} requêtes ; la dernière : {path} → {status}. C'est le service qui plante, pas le réseau : ses journaux ont la trace.",
                    st.server_errors, st.total
                )
            } else {
                format!(
                    "{host} answered {} server errors (5xx) out of {} requests; the last one: {path} -> {status}. The service itself is failing, not the network: its logs have the stack trace.",
                    st.server_errors, st.total
                )
            });
        }
        if st.denied > 0 && st.denied * 4 >= st.total {
            findings.push(if crate::text::french() {
                format!(
                    "{host} a refusé {} requêtes sur {} (401/403) : un jeton ou un cookie manquant ou périmé, plutôt qu'une panne.",
                    st.denied, st.total
                )
            } else {
                format!(
                    "{host} denied {} of {} requests (401/403): a missing or expired token or cookie, rather than an outage.",
                    st.denied, st.total
                )
            });
        }
        if let Some((path, ms)) = &st.slowest
            && *ms >= 1000
        {
            let mut d = st.durations.clone();
            d.sort_unstable();
            let median = d[d.len() / 2];
            findings.push(if crate::text::french() {
                format!(
                    "{host} : la requête la plus lente, {path}, a pris {} ; la moitié des requêtes répondent en moins de {}. Si c'est toujours le même chemin, c'est lui qu'il faut regarder.",
                    util::dur(*ms),
                    util::dur(median)
                )
            } else {
                format!(
                    "{host}: the slowest request, {path}, took {}; half of the requests answer in under {}. If it is always the same path, that is the one to look at.",
                    util::dur(*ms),
                    util::dur(median)
                )
            });
        }
    }

    let since = util::clock(first_ts);
    if findings.is_empty() {
        out!(
            "{}",
            if crate::text::french() {
                format!(
                    "Rien d'anormal depuis {since} : {connections} connexions et {requests} requêtes HTTP, aucun échec, aucune erreur de serveur, rien au-dessus d'une seconde."
                )
            } else {
                format!(
                    "Nothing wrong since {since}: {connections} connections and {requests} HTTP requests, no failure, no server error, nothing above one second."
                )
            }
        );
        return;
    }
    out!(
        "{}",
        if crate::text::french() {
            format!(
                "Depuis {since}, {connections} connexions et {requests} requêtes HTTP observées. Ce qui ressort :"
            )
        } else {
            format!(
                "Since {since}, {connections} connections and {requests} HTTP requests observed. What stands out:"
            )
        }
    );
    for f in findings {
        out!();
        out!("- {f}");
    }
    out!();
    out!(
        "{} solon trace{}",
        tr("Detail:", "Le détail :"),
        needle.map(|n| format!(" {n}")).unwrap_or_default()
    );
}

fn note_failure(map: &mut BTreeMap<(String, String, u16), Failures>, cn: &TraceConnection) {
    let key = (label(&cn.src), label(&cn.dst), cn.dport);
    let f = map.entry(key).or_default();
    if f.count == 0 {
        f.first = cn.ts_ms;
        f.comm = cn.comm.clone();
        f.dst_has_container = cn.dst.container_id.is_some();
        f.dst_name = cn.dst.label().to_owned();
        f.dst_id = cn.dst.container_id.clone();
    }
    f.count += 1;
    f.last = f.last.max(cn.ts_ms);
}

fn note_http(map: &mut BTreeMap<String, HostStats>, h: &TraceHttp) {
    let st = map.entry(h.host.clone()).or_default();
    st.total += 1;
    st.durations.push(h.duration_ms);
    let label = format!("{} {}", h.method, h.path);
    if h.status >= 500 {
        st.server_errors += 1;
        st.worst_error = Some((label.clone(), h.status));
    }
    if h.status == 401 || h.status == 403 {
        st.denied += 1;
    }
    if st
        .slowest
        .as_ref()
        .is_none_or(|(_, ms)| h.duration_ms > *ms)
    {
        st.slowest = Some((label, h.duration_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(project: &str, service: &str, id: Option<&str>) -> TracePeer {
        TracePeer {
            ip: "172.18.0.2".into(),
            container_id: id.map(str::to_owned),
            name: Some(format!("{project}-{service}-1")),
            project: Some(project.into()),
            service: Some(service.into()),
        }
    }

    #[test]
    fn filtre_par_nom() {
        let ev = TraceEvent::Connection(Box::new(TraceConnection {
            ts_ms: 1,
            outcome: ConnectionOutcome::Failed,
            src: peer("blog", "web", Some("a")),
            dst: peer("blog", "db", Some("b")),
            sport: 1,
            dport: 5432,
            comm: "php-fpm".into(),
            pid: 1,
            duration_ms: None,
        }));
        assert!(matches(&ev, Some("blog")));
        assert!(matches(&ev, Some("db")));
        assert!(matches(&ev, Some("blog-web-1")));
        assert!(!matches(&ev, Some("api")));
        assert!(matches(&ev, None));
    }

    #[test]
    fn regroupe_les_echecs() {
        let mut map = BTreeMap::new();
        for i in 0..3 {
            note_failure(
                &mut map,
                &TraceConnection {
                    ts_ms: 100 + i,
                    outcome: ConnectionOutcome::Failed,
                    src: peer("blog", "web", Some("a")),
                    dst: peer("blog", "db", Some("b")),
                    sport: 1,
                    dport: 5432,
                    comm: "php-fpm".into(),
                    pid: 1,
                    duration_ms: None,
                },
            );
        }
        let f = map.values().next().unwrap();
        assert_eq!(f.count, 3);
        assert_eq!(f.last, 102);
        assert!(f.dst_has_container);
    }
}
