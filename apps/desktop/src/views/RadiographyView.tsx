// Radiographie : qui appelle qui, en direct, dans la machine.
//
// Deux lectures du même flux. La carte répond à « comment ce projet est-il branché » : un cercle
// de services, un trait par relation, rouge quand des appels échouent. Le journal répond à « que
// s'est-il passé à 14 h 32 » : un échange par ligne, le plus récent en haut, dépliable pour voir
// les en-têtes.
//
// Deux sources, et elles ne voient pas la même chose. Les sondes du noyau voient toutes les
// connexions, mais pas leur contenu. Le mandataire des domaines locaux, lui, voit le contenu en
// clair — c'est lui qui termine le TLS — mais seulement ce qui entre par `*.solon.local`.
//
// La barre de lecture fait de l'écran un magnétoscope : on fige, on recule, on rejoue au ralenti.
// Tout l'écran — carte comme journal — montre alors la machine telle qu'elle était à l'instant du
// curseur. Rien n'est perdu pendant ce temps : les échanges continuent d'arriver, simplement
// « dans le futur » du curseur.
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { trace, type TraceConnection, type TraceEvent, type TraceHttp, type TracePeer } from "../api";
import { EmptyState, hueOf, IconPulse, PageHeader } from "../components/ui";
import { TraceTimeline, type Speed, type TimelinePoint } from "../components/TraceTimeline";

/** Échanges gardés à l'écran. Plus qu'il n'en faut pour afficher : la relecture remonte loin. */
const SHOWN = 5000;
/** Un trait reste « allumé » sur la carte tant que son dernier échange date de moins que ça, en
 *  temps de la relecture : au ralenti ×0,25, un appel brille donc huit vraies secondes. */
const HOT_MS = 2000;
/** Pas de la relecture. 20 images par seconde suffisent pour une carte et un journal. */
const TICK_MS = 50;
/** Au-delà, la carte devient un plat de spaghettis : on bascule sur le journal seul. */
const MAX_NODES = 24;
/** Nœud du navigateur sur la carte : tout ce qui entre par le mandataire part de là. */
const BROWSER = "\u0000browser";

type Fact = ({ id: number } & TraceConnection) | ({ id: number } & TraceHttp);

/** Clé stable d'une extrémité : le conteneur s'il est connu, sinon l'adresse. */
function keyOf(p: TracePeer): string {
  return p.container_id ?? p.ip;
}

function labelOf(p: TracePeer): string {
  return p.service ?? p.name ?? p.ip;
}

function projectOf(f: Fact): string | null {
  return f.kind === "http" ? (f.dst.project ?? null) : (f.src.project ?? f.dst.project ?? null);
}

export function RadiographyView() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<{ active: boolean; detail: string } | null>(null);
  const [facts, setFacts] = useState<Fact[]>([]);
  const [project, setProject] = useState<string>("");
  const [open, setOpen] = useState<number | null>(null);
  // Lecture : en direct, ou figée / rejouée à partir d'un instant du passé.
  const [live, setLive] = useState(true);
  const [cursor, setCursor] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState<Speed>(1);
  const now = useNow(500);
  const nextId = useRef(0);

  useEffect(() => {
    let alive = true;
    const channel = trace.subscribe((ev: TraceEvent) => {
      if (!alive) return;
      if (ev.kind === "status") {
        setStatus({ active: ev.active, detail: ev.detail });
        return;
      }
      const fact = { ...ev, id: nextId.current++ } as Fact;
      setFacts((prev) => insertByTime(prev, fact).slice(0, SHOWN));
    });
    return () => {
      alive = false;
      channel.onmessage = () => {};
    };
  }, []);

  // Relecture : le curseur avance à la vitesse choisie. Rattraper le présent rend la main au
  // direct — sinon, à ×4, on finirait par rejouer un futur qui n'existe pas encore.
  useEffect(() => {
    if (live || !playing) return;
    const id = setInterval(() => {
      setCursor((c) => {
        const next = c + TICK_MS * speed;
        if (next >= Date.now()) {
          setLive(true);
          setPlaying(false);
        }
        return next;
      });
    }, TICK_MS);
    return () => clearInterval(id);
  }, [live, playing, speed]);

  const at = live ? now : cursor;

  function seek(ts: number) {
    setLive(false);
    setCursor(Math.min(ts, Date.now()));
  }

  const projects = useMemo(() => {
    const names = new Set<string>();
    for (const f of facts) {
      const p = projectOf(f);
      if (p) names.add(p);
    }
    return [...names].sort((a, b) => a.localeCompare(b));
  }, [facts]);

  const rows = useMemo(
    () => (project ? facts.filter((f) => projectOf(f) === project) : facts),
    [facts, project],
  );
  // Ce qui « a déjà eu lieu » à l'instant du curseur. En direct, tout.
  const visible = useMemo(() => (live ? rows : rows.filter((f) => f.ts_ms <= cursor)), [rows, live, cursor]);

  const graph = useMemo(() => buildGraph(visible, t("radiography.browser")), [visible, t]);
  const order = useMemo(() => buildGraph(rows, "").nodes.map((n) => n.key), [rows]);

  const points = useMemo<TimelinePoint[]>(
    () => rows.filter((f) => f.kind === "http" || f.outcome !== "closed").map((f) => ({ ts: f.ts_ms, severity: severity(f) })),
    [rows],
  );
  // La chronologie couvre tout l'historique gardé, et au moins une minute pour rester lisible.
  const start = Math.min(rows.length > 0 ? rows[rows.length - 1].ts_ms : now, now - 60_000);

  function playPause() {
    if (live) {
      // Mettre en pause le direct, c'est figer l'écran à cet instant.
      seek(Date.now());
      setPlaying(false);
    } else {
      setPlaying((p) => !p);
    }
  }

  /** Saute à l'échange précédent ou suivant : le pas à pas d'un débogueur. */
  function step(direction: -1 | 1) {
    // `rows` va du plus récent au plus ancien.
    const target =
      direction < 0
        ? rows.find((f) => f.ts_ms < at - 1)?.ts_ms
        : [...rows].reverse().find((f) => f.ts_ms > at + 1)?.ts_ms;
    if (target != null) {
      seek(target);
      setPlaying(false);
    }
  }

  function goLive() {
    setLive(true);
    setPlaying(false);
  }

  async function clear() {
    setFacts([]);
    setOpen(null);
    goLive();
    await trace.clear();
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("radiography.title")}
        count={t("radiography.count", { count: visible.length })}
        actions={<button type="button" className="btn btn-sm" onClick={() => void clear()}>{t("radiography.clear")}</button>}
        search={
          <select className="input w-56" value={project} onChange={(e) => setProject(e.target.value)} aria-label={t("radiography.project")}>
            <option value="">{t("radiography.all_projects")}</option>
            {projects.map((p) => <option key={p} value={p}>{p}</option>)}
          </select>
        }
      />
      {status && !status.active && (
        <div className="mx-4 mb-2 rounded px-3 py-2" role="status" style={{ background: "var(--warn-soft)", color: "var(--warn)" }}>
          {t("radiography.inactive", { detail: status.detail })}
        </div>
      )}
      {rows.length > 0 && (
        <TraceTimeline
          points={points}
          start={start}
          end={now}
          at={at}
          live={live}
          playing={playing}
          speed={speed}
          onSeek={(ts) => {
            seek(ts);
            setPlaying(false);
          }}
          onPlayPause={playPause}
          onSpeed={setSpeed}
          onLive={goLive}
          onStep={step}
          onBack={(ms) => seek(at - ms)}
        />
      )}
      {rows.length === 0 ? (
        <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
          <EmptyState
            icon={<IconPulse />}
            title={status?.active ? t("radiography.empty") : t("radiography.waiting")}
            hint={status?.active ? t("radiography.empty_hint") : undefined}
          />
        </div>
      ) : (
        <div className="mx-4 mb-4 flex min-h-0 flex-1 gap-4">
          <div className="card flex min-w-0 flex-1 items-center justify-center overflow-hidden p-2">
            {order.length > MAX_NODES ? (
              <p className="px-6 text-center text-[13px]" style={{ color: "var(--ink-2)" }}>
                {t("radiography.too_many", { count: order.length })}
              </p>
            ) : (
              <CallMap graph={graph} order={order} at={at} />
            )}
          </div>
          <div className="card list-card min-h-0 w-[30rem] shrink-0 overflow-auto">
            <table className="w-full text-[13px]">
              <tbody>
                {visible.slice(0, 200).map((r) => (
                  <Row key={r.id} fact={r} open={open === r.id} onToggle={() => setOpen(open === r.id ? null : r.id)} />
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}

function Row({ fact, open, onToggle }: { fact: Fact; open: boolean; onToggle: () => void }) {
  const { t } = useTranslation();
  if (fact.kind === "connection") {
    return (
      <tr>
        <td className="w-16 px-2 py-1.5 align-top tabular-nums" style={{ color: "var(--ink-3)" }}>{clock(fact.ts_ms)}</td>
        <td className="px-2 py-1.5">
          <div style={{ color: fact.outcome === "failed" ? "var(--bad)" : "var(--ink)" }}>
            {labelOf(fact.src)} <span style={{ color: "var(--ink-3)" }}>→</span> {labelOf(fact.dst)}:{fact.dport}
          </div>
          <div className="text-[12px]" style={{ color: "var(--ink-3)" }}>
            {t(`radiography.outcome.${fact.outcome}`)}
            {" · "}<code>{fact.comm}</code>
            {fact.duration_ms != null && ` · ${formatDuration(fact.duration_ms)}`}
          </div>
        </td>
      </tr>
    );
  }
  const color = fact.status >= 500 ? "var(--bad)" : fact.status >= 400 ? "var(--warn)" : "var(--ok)";
  return (
    <>
      <tr className="cursor-pointer" onClick={onToggle}>
        <td className="w-16 px-2 py-1.5 align-top tabular-nums" style={{ color: "var(--ink-3)" }}>{clock(fact.ts_ms)}</td>
        <td className="px-2 py-1.5">
          <div className="flex items-baseline gap-2">
            <span className="tabular-nums" style={{ color, fontWeight: 600 }}>{fact.status || "—"}</span>
            <span style={{ color: "var(--ink-2)" }}>{fact.method}</span>
            <span className="min-w-0 truncate" style={{ color: "var(--ink)" }}>{fact.path}</span>
          </div>
          <div className="text-[12px]" style={{ color: "var(--ink-3)" }}>
            {labelOf(fact.dst)} · {formatDuration(fact.duration_ms)}
            {fact.response_bytes > 0 && ` · ${formatBytes(fact.response_bytes)}`}
            {" · "}{open ? t("radiography.hide_headers") : t("radiography.show_headers")}
          </div>
        </td>
      </tr>
      {open && (
        <tr>
          <td />
          <td className="px-2 pb-3">
            <Headers title={t("radiography.request")} headers={fact.request_headers} />
            <Headers title={t("radiography.response")} headers={fact.response_headers} />
          </td>
        </tr>
      )}
    </>
  );
}

function Headers({ title, headers }: { title: string; headers: { name: string; value: string }[] }) {
  return (
    <div className="mt-2 rounded px-2 py-1.5" style={{ background: "var(--surface-2)" }}>
      <div className="mb-1 text-[11px] uppercase tracking-wide" style={{ color: "var(--ink-3)" }}>{title}</div>
      {headers.map((h, i) => (
        <div key={`${h.name}-${i}`} className="text-[12px] leading-5">
          <span style={{ color: "var(--ink-3)" }}>{h.name}: </span>
          <span style={{ color: "var(--ink-2)", wordBreak: "break-all" }}>{h.value}</span>
        </div>
      ))}
    </div>
  );
}

interface Node { key: string; label: string; project: string | null; inside: boolean; }
interface Edge { from: string; to: string; total: number; failed: number; ports: Set<number>; last: number; }
interface Graph { nodes: Node[]; edges: Edge[]; }

/** Agrège les échanges en un graphe : un nœud par extrémité, une arête par relation observée. */
function buildGraph(facts: Fact[], browserLabel: string): Graph {
  const nodes = new Map<string, Node>();
  const edges = new Map<string, Edge>();
  const node = (key: string, label: string, project: string | null, inside: boolean) => {
    if (!nodes.has(key)) nodes.set(key, { key, label, project, inside });
    return key;
  };
  const peer = (p: TracePeer) => node(keyOf(p), labelOf(p), p.project ?? null, p.container_id != null);
  for (const f of facts) {
    let from: string;
    let port: number;
    let failed: boolean;
    if (f.kind === "http") {
      from = node(BROWSER, browserLabel, null, false);
      port = f.scheme === "https" ? 443 : 80;
      failed = f.status >= 500 || f.status === 0;
    } else {
      from = peer(f.src);
      port = f.dport;
      failed = f.outcome === "failed";
    }
    const to = peer(f.dst);
    if (from === to) continue; // un service qui s'appelle lui-même n'apprend rien sur la carte
    const id = `${from}\u0000${to}`;
    let edge = edges.get(id);
    if (!edge) {
      edge = { from, to, total: 0, failed: 0, ports: new Set(), last: 0 };
      edges.set(id, edge);
    }
    // Une connexion comptée à l'ouverture puis à la fermeture compterait double.
    if (f.kind === "http" || f.outcome !== "closed") edge.total += 1;
    if (failed) edge.failed += 1;
    edge.ports.add(port);
    edge.last = Math.max(edge.last, f.ts_ms);
  }
  // Deux projets ont souvent un service du même nom — `db`, `web`, `cache`. Deux cercles
  // identiques sur la carte ne veulent plus rien dire : on ajoute le projet, mais seulement à
  // ceux qui se marchent dessus, pour ne pas alourdir les autres.
  const seen = new Map<string, number>();
  for (const n of nodes.values()) seen.set(n.label, (seen.get(n.label) ?? 0) + 1);
  for (const n of nodes.values()) {
    if ((seen.get(n.label) ?? 0) > 1 && n.project) n.label = `${n.label} · ${n.project}`;
  }
  // Les conteneurs d'abord : la carte garde la même forme quand un appel extérieur apparaît.
  const ordered = [...nodes.values()].sort(
    (a, b) => Number(b.inside) - Number(a.inside) || a.label.localeCompare(b.label),
  );
  return { nodes: ordered, edges: [...edges.values()] };
}

const SIZE = 520;
const RADIUS = 185;
const NODE_R = 22;

/** Disposition en cercle : stable, sans simulation, lisible jusqu'à une vingtaine de nœuds.
 *
 *  Les places sont attribuées d'après `order` — tous les nœuds de l'historique, pas seulement
 *  ceux déjà apparus à l'instant du curseur. En relecture, un service qui entre en scène prend la
 *  place qui l'attendait au lieu de bousculer les autres. */
function CallMap({ graph, order, at }: { graph: Graph; order: string[]; at: number }) {
  const { t } = useTranslation();
  const n = order.length;
  const pos = (key: string) => {
    // Le premier nœud en haut, puis dans le sens horaire.
    const i = Math.max(0, order.indexOf(key));
    const angle = (i / Math.max(n, 1)) * 2 * Math.PI - Math.PI / 2;
    return { x: SIZE / 2 + RADIUS * Math.cos(angle), y: SIZE / 2 + RADIUS * Math.sin(angle) };
  };
  const busiest = Math.max(1, ...graph.edges.map((e) => e.total));
  const labels = new Map(graph.nodes.map((node) => [node.key, node.label]));
  // Ce qui vient de se passer à l'instant du curseur : c'est ce qui s'allume en relecture.
  const hot = (e: Edge) => e.last <= at && at - e.last < HOT_MS;
  const hotNodes = new Set(graph.edges.filter(hot).flatMap((e) => [e.from, e.to]));

  return (
    <svg viewBox={`0 0 ${SIZE} ${SIZE}`} className="h-full w-full" role="img" aria-label={t("radiography.map")}>
      <defs>
        {/* La pointe est à l'extrémité du trait (refX = largeur du dessin), et le trait s'arrête
            déjà au bord du cercle visé : la flèche se pose exactement contre lui. */}
        {[["arrow", "var(--accent)"], ["arrow-bad", "var(--bad)"]].map(([id, color]) => (
          <marker key={id} id={id} viewBox="0 0 10 10" refX="10" refY="5" markerWidth="5" markerHeight="5" orient="auto">
            <path d="M0 0 L10 5 L0 10 z" fill={color} />
          </marker>
        ))}
      </defs>
      {graph.edges.map((e) => {
        const seg = trim(pos(e.from), pos(e.to), NODE_R + 4);
        const failing = e.failed > 0;
        const lit = hot(e);
        const ports = [...e.ports].sort((x, y) => x - y).join(", ");
        return (
          <g key={`${e.from}-${e.to}`}>
            <title>{`${labels.get(e.from) ?? e.from} → ${labels.get(e.to) ?? e.to} : ${t("radiography.calls", { count: e.total })}${failing ? ` · ${t("radiography.failed_count", { count: e.failed })}` : ""} · ${ports}`}</title>
            <line
              x1={seg.x1} y1={seg.y1} x2={seg.x2} y2={seg.y2}
              stroke={failing ? "var(--bad)" : "var(--accent)"}
              strokeWidth={1.5 + 3 * (e.total / busiest) + (lit ? 2 : 0)}
              strokeOpacity={failing || lit ? 1 : 0.35}
              // L'échec ne se distingue pas que par la couleur : le trait est aussi discontinu.
              strokeDasharray={failing ? "7 5" : undefined}
              markerEnd={failing ? "url(#arrow-bad)" : "url(#arrow)"}
            />
          </g>
        );
      })}
      {graph.nodes.map((node) => {
        const { x, y } = pos(node.key);
        const hue = hueOf(node.key);
        return (
          <g key={node.key}>
            <circle
              cx={x} cy={y} r={NODE_R}
              fill={node.inside ? `hsl(${hue} 70% 55% / 0.22)` : "var(--surface-2)"}
              stroke={node.inside ? `hsl(${hue} 60% 50%)` : "var(--line-strong)"}
              strokeWidth={hotNodes.has(node.key) ? 3.5 : 1.5}
              strokeDasharray={node.inside ? undefined : "3 3"}
            />
            <text x={x} y={y + 5} textAnchor="middle" fontSize="14" fontWeight="600" fill={node.inside ? `hsl(${hue} 70% 72%)` : "var(--ink-3)"}>
              {initials(node.label)}
            </text>
            <text x={x} y={y + NODE_R + 18} textAnchor="middle" fontSize="12" fill="var(--ink)">
              {node.label.length > 24 ? `${node.label.slice(0, 23)}…` : node.label}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

/** Raccourcit un segment aux deux bouts, pour qu'il parte du bord d'un cercle et s'arrête au bord
 *  de l'autre au lieu de les traverser. */
function trim(a: { x: number; y: number }, b: { x: number; y: number }, by: number) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy) || 1;
  const ux = (dx / len) * by;
  const uy = (dy / len) * by;
  return { x1: a.x + ux, y1: a.y + uy, x2: b.x - ux, y2: b.y - uy };
}

/** Deux caractères au plus, pour habiller le cercle sans le surcharger. Une adresse donne son
 *  dernier nombre, qui est ce qui distingue deux voisins. */
function initials(full: string): string {
  // Le projet ajouté pour départager deux homonymes (« db · odoo18 ») n'entre pas dans les
  // initiales : le cercle reste celui du service.
  const label = full.split(" · ")[0];
  if (/^[\d.]+$/.test(label)) return label.split(".").pop() ?? "";
  const parts = label.split(/[-_. ]/).filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return label.slice(0, 2).toUpperCase();
}

/** Range un échange à sa place, du plus récent au plus ancien.
 *
 *  L'ordre d'arrivée ne suffit pas : un échange HTTP est daté de sa requête mais n'est connu
 *  qu'à sa réponse, donc après des connexions plus récentes que lui. La relecture, elle, a besoin
 *  de l'ordre réel. Presque tout arrive en tête : la recherche part de là. */
function insertByTime(list: Fact[], fact: Fact): Fact[] {
  let i = 0;
  while (i < list.length && list[i].ts_ms > fact.ts_ms) i++;
  return [...list.slice(0, i), fact, ...list.slice(i)];
}

/** Gravité d'un échange pour la chronologie : 2 échec, 1 à surveiller, 0 rien à signaler. */
function severity(f: Fact): 0 | 1 | 2 {
  if (f.kind === "connection") return f.outcome === "failed" ? 2 : 0;
  if (f.status >= 500 || f.status === 0) return 2;
  return f.status >= 400 ? 1 : 0;
}

/** L'heure courante, rafraîchie à intervalle fixe : la carte éteint les traits refroidis et la
 *  chronologie avance, même quand rien n'arrive. */
function useNow(period: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), period);
    return () => clearInterval(id);
  }, [period]);
  return now;
}

/** Heure à la seconde près : à cette échelle, la date n'apporte rien. */
function clock(ms: number): string {
  const d = new Date(ms);
  const pad = (x: number) => String(x).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.round(ms / 60_000)} min`;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} o`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} Ko`;
  return `${(n / (1024 * 1024)).toFixed(1)} Mo`;
}
