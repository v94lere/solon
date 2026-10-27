// Radiographie : qui appelle qui, en direct, dans la machine.
//
// Deux lectures du même flux. La carte répond à « comment ce projet est-il branché » : un cercle
// de services, un trait par relation, rouge quand des appels échouent. La liste répond à « que
// s'est-il passé à 14 h 32 » : un appel par ligne, le plus récent en haut.
//
// Les connexions entrantes ne sont pas rapportées séparément par les sondes : entre deux
// conteneurs, elles sont l'autre extrémité d'une sortante déjà vue.
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { trace, type TraceConnection, type TraceEvent, type TracePeer } from "../api";
import { EmptyState, hueOf, IconPulse, PageHeader } from "../components/ui";

/** Appels gardés à l'écran. Le service en garde bien davantage ; inutile d'en afficher plus. */
const SHOWN = 1000;
/** Au-delà, la carte devient un plat de spaghettis : on bascule sur la liste seule. */
const MAX_NODES = 24;

interface Fact extends TraceConnection {
  id: number;
}

/** Clé stable d'une extrémité : le conteneur s'il est connu, sinon l'adresse. */
function keyOf(p: TracePeer): string {
  return p.container_id ?? p.ip;
}

function labelOf(p: TracePeer): string {
  return p.service ?? p.name ?? p.ip;
}

export function RadiographyView() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<{ active: boolean; detail: string } | null>(null);
  const [facts, setFacts] = useState<Fact[]>([]);
  const [project, setProject] = useState<string>("");
  const [paused, setPaused] = useState(false);
  // Le flux continue d'arriver pendant une pause : on l'accumule plutôt que de le perdre.
  const held = useRef<Fact[]>([]);
  const pausedRef = useRef(paused);
  const nextId = useRef(0);

  useEffect(() => {
    pausedRef.current = paused;
    if (!paused && held.current.length > 0) {
      const pending = held.current.reverse();
      held.current = [];
      setFacts((prev) => [...pending, ...prev].slice(0, SHOWN));
    }
  }, [paused]);

  useEffect(() => {
    let alive = true;
    const channel = trace.subscribe((ev: TraceEvent) => {
      if (!alive) return;
      if (ev.kind === "status") {
        setStatus({ active: ev.active, detail: ev.detail });
        return;
      }
      const fact: Fact = { ...ev, id: nextId.current++ };
      if (pausedRef.current) held.current.push(fact);
      else setFacts((prev) => [fact, ...prev].slice(0, SHOWN));
    });
    return () => {
      alive = false;
      channel.onmessage = () => {};
    };
  }, []);

  const projects = useMemo(() => {
    const names = new Set<string>();
    for (const f of facts) {
      if (f.src.project) names.add(f.src.project);
      if (f.dst.project) names.add(f.dst.project);
    }
    return [...names].sort((a, b) => a.localeCompare(b));
  }, [facts]);

  const rows = useMemo(
    () => (project ? facts.filter((f) => f.src.project === project || f.dst.project === project) : facts),
    [facts, project],
  );

  const graph = useMemo(() => buildGraph(rows), [rows]);

  async function clear() {
    held.current = [];
    setFacts([]);
    await trace.clear();
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("radiography.title")}
        count={t("radiography.count", { count: rows.length })}
        actions={
          <div className="flex gap-2">
            <button type="button" className="btn btn-sm" onClick={() => setPaused((p) => !p)}>
              {paused ? t("radiography.resume") : t("radiography.pause")}
            </button>
            <button type="button" className="btn btn-sm" onClick={() => void clear()}>{t("radiography.clear")}</button>
          </div>
        }
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
      {paused && (
        <div className="mx-4 mb-2 rounded px-3 py-2 text-[13px]" role="status" style={{ background: "var(--surface-2)", color: "var(--ink-2)" }}>
          {t("radiography.held", { count: held.current.length })}
        </div>
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
            {graph.nodes.length > MAX_NODES ? (
              <p className="px-6 text-center text-[13px]" style={{ color: "var(--ink-2)" }}>
                {t("radiography.too_many", { count: graph.nodes.length })}
              </p>
            ) : (
              <CallMap graph={graph} />
            )}
          </div>
          <div className="card list-card min-h-0 w-[26rem] shrink-0 overflow-auto">
            <table className="w-full text-[13px]">
              <tbody>
                {rows.slice(0, 200).map((r) => (
                  <tr key={r.id}>
                    <td className="w-16 px-2 py-1.5 tabular-nums align-top" style={{ color: "var(--ink-3)" }}>{clock(r.ts_ms)}</td>
                    <td className="px-2 py-1.5">
                      <div style={{ color: r.outcome === "failed" ? "var(--bad)" : "var(--ink)" }}>
                        {labelOf(r.src)} <span style={{ color: "var(--ink-3)" }}>→</span> {labelOf(r.dst)}:{r.dport}
                      </div>
                      <div className="text-[12px]" style={{ color: "var(--ink-3)" }}>
                        {t(`radiography.outcome.${r.outcome}`)}
                        {" · "}<code>{r.comm}</code>
                        {r.duration_ms != null && ` · ${formatDuration(r.duration_ms)}`}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}

interface Node { key: string; label: string; inside: boolean; }
interface Edge { from: string; to: string; total: number; failed: number; ports: Set<number>; }
interface Graph { nodes: Node[]; edges: Edge[]; }

/** Agrège les appels en un graphe : un nœud par extrémité, une arête par relation observée. */
function buildGraph(facts: Fact[]): Graph {
  const nodes = new Map<string, Node>();
  const edges = new Map<string, Edge>();
  for (const f of facts) {
    for (const p of [f.src, f.dst]) {
      const key = keyOf(p);
      if (!nodes.has(key)) nodes.set(key, { key, label: labelOf(p), inside: p.container_id != null });
    }
    const from = keyOf(f.src);
    const to = keyOf(f.dst);
    if (from === to) continue; // un service qui s'appelle lui-même n'apprend rien sur la carte
    const id = `${from}\u0000${to}`;
    let edge = edges.get(id);
    if (!edge) {
      edge = { from, to, total: 0, failed: 0, ports: new Set() };
      edges.set(id, edge);
    }
    // Une connexion comptée à l'ouverture puis à la fermeture compterait double.
    if (f.outcome !== "closed") edge.total += 1;
    if (f.outcome === "failed") edge.failed += 1;
    edge.ports.add(f.dport);
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

/** Disposition en cercle : stable, sans simulation, lisible jusqu'à une vingtaine de nœuds. */
function CallMap({ graph }: { graph: Graph }) {
  const { t } = useTranslation();
  const n = graph.nodes.length;
  const at = (i: number) => {
    // Le premier nœud en haut, puis dans le sens horaire.
    const angle = (i / Math.max(n, 1)) * 2 * Math.PI - Math.PI / 2;
    return { x: SIZE / 2 + RADIUS * Math.cos(angle), y: SIZE / 2 + RADIUS * Math.sin(angle) };
  };
  const index = new Map(graph.nodes.map((node, i) => [node.key, i]));
  const busiest = Math.max(1, ...graph.edges.map((e) => e.total));

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
        const seg = trim(at(index.get(e.from) ?? 0), at(index.get(e.to) ?? 0), NODE_R + 4);
        const failing = e.failed > 0;
        const ports = [...e.ports].sort((x, y) => x - y).join(", ");
        return (
          <g key={`${e.from}-${e.to}`}>
            <title>{`${e.from} → ${e.to} : ${t("radiography.calls", { count: e.total })}${failing ? ` · ${t("radiography.failed_count", { count: e.failed })}` : ""} · ${ports}`}</title>
            <line
              x1={seg.x1} y1={seg.y1} x2={seg.x2} y2={seg.y2}
              stroke={failing ? "var(--bad)" : "var(--accent)"}
              strokeWidth={1.5 + 3 * (e.total / busiest)}
              strokeOpacity={failing ? 1 : 0.5}
              // L'échec ne se distingue pas que par la couleur : le trait est aussi discontinu.
              strokeDasharray={failing ? "7 5" : undefined}
              markerEnd={failing ? "url(#arrow-bad)" : "url(#arrow)"}
            />
          </g>
        );
      })}
      {graph.nodes.map((node, i) => {
        const { x, y } = at(i);
        const hue = hueOf(node.key);
        return (
          <g key={node.key}>
            <circle
              cx={x} cy={y} r={NODE_R}
              fill={node.inside ? `hsl(${hue} 70% 55% / 0.22)` : "var(--surface-2)"}
              stroke={node.inside ? `hsl(${hue} 60% 50%)` : "var(--line-strong)"}
              strokeWidth={1.5}
              strokeDasharray={node.inside ? undefined : "3 3"}
            />
            <text x={x} y={y + 5} textAnchor="middle" fontSize="14" fontWeight="600" fill={node.inside ? `hsl(${hue} 70% 72%)` : "var(--ink-3)"}>
              {initials(node.label)}
            </text>
            <text x={x} y={y + NODE_R + 18} textAnchor="middle" fontSize="12" fill="var(--ink)">
              {node.label.length > 18 ? `${node.label.slice(0, 17)}…` : node.label}
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
function initials(label: string): string {
  if (/^[\d.]+$/.test(label)) return label.split(".").pop() ?? "";
  const parts = label.split(/[-_. ]/).filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return label.slice(0, 2).toUpperCase();
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
