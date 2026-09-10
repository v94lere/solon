import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { containers, engine, formatBytes, type ContainerSummary, type MachineMetrics, type StatSample } from "../api";
import { EmptyState, IconPulse, PageHeader, Spark, hueOf, pushHistory } from "../components/ui";
import { TimeChart, type ChartSeries, type Pt } from "../components/TimeChart";

const ENGINE_PERIOD_MS = 2000;
/** Historique conservé en mémoire : la plus grande fenêtre affichable, plus un peu de marge. */
const RETENTION_MS = 3_600_000 + 20_000;
const WINDOWS = [60_000, 600_000, 3_600_000] as const;
type WindowMs = (typeof WINDOWS)[number];
type Metric = "cpu" | "mem" | "net";
const ENGINE_ID = "__engine";

interface EngineNow {
  cpuPct: number;
  memUsed: number;
  memTotal: number;
  diskUsed: number;
  diskTotal: number;
  rxRate: number;
  txRate: number;
  load1: number;
  cpus: number;
}

interface ContainerNow {
  sample: StatSample;
  rxRate: number;
  txRate: number;
  cpuHistory: number[];
}

interface History {
  cpu: Pt[];
  mem: Pt[];
  net: Pt[];
}

type SortKey = "name" | "cpu" | "mem" | "net";

function push(list: Pt[], p: Pt): Pt[] {
  const cutoff = p.t - RETENTION_MS;
  let i = 0;
  while (i < list.length && list[i].t < cutoff) i++;
  const next = i > 0 ? list.slice(i) : list.slice();
  next.push(p);
  return next;
}

const emptyHistory = (): History => ({ cpu: [], mem: [], net: [] });

/** Activité : mesures en temps réel du moteur (processeur, mémoire, stockage, réseau) et des conteneurs. */
export function ActivityView({ onOpenContainer }: { onOpenContainer: (id: string) => void }) {
  const { t } = useTranslation();
  const [now, setNow] = useState<EngineNow | null>(null);
  const [engineHist, setEngineHist] = useState<History>(emptyHistory);
  const [contHist, setContHist] = useState<Record<string, History>>({});
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const prevRef = useRef<{ m: MachineMetrics; at: number } | null>(null);
  const cpusRef = useRef(1);
  const [perContainer, setPerContainer] = useState<Record<string, ContainerNow>>({});
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "cpu", desc: true });
  const [windowMs, setWindowMs] = useState<WindowMs>(60_000);
  const [metric, setMetric] = useState<Metric>("cpu");
  // Le moteur est masqué par défaut sur la mémoire : il écraserait l'échelle des conteneurs.
  const [hidden, setHidden] = useState<Record<Metric, Set<string>>>({ cpu: new Set(), mem: new Set([ENGINE_ID]), net: new Set() });
  const [clock, setClock] = useState(() => Date.now());
  const running = useQuery({ queryKey: ["containers", false], queryFn: () => containers.list(false), refetchInterval: 5000 });
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => engine.settingsGet() });

  // L'horloge du graphique avance toutes les secondes, même sans nouvelle mesure.
  useEffect(() => {
    const id = window.setInterval(() => setClock(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  // Compteurs du moteur toutes les 2 s ; pourcentages et débits par différence avec le relevé précédent.
  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const m = await engine.metrics();
        if (cancelled) return;
        setUnavailable(null);
        const at = performance.now();
        const prev = prevRef.current;
        prevRef.current = { m, at };
        cpusRef.current = Math.max(1, m.cpus);
        const memUsed = (m.mem_total_kb - m.mem_available_kb) * 1024;
        const memTotal = m.mem_total_kb * 1024;
        let cpuPct = 0;
        let rxRate = 0;
        let txRate = 0;
        if (prev) {
          const dt = Math.max((at - prev.at) / 1000, 0.1);
          const dTotal = m.cpu_total_ticks - prev.m.cpu_total_ticks;
          const dBusy = m.cpu_busy_ticks - prev.m.cpu_busy_ticks;
          cpuPct = dTotal > 0 ? Math.min(100, Math.max(0, (100 * dBusy) / dTotal)) : 0;
          rxRate = Math.max(0, m.net_rx_bytes - prev.m.net_rx_bytes) / dt;
          txRate = Math.max(0, m.net_tx_bytes - prev.m.net_tx_bytes) / dt;
          const ts = Date.now();
          // Le processeur du moteur est exprimé comme les conteneurs : 100 % = un cœur entier.
          setEngineHist((h) => ({ cpu: push(h.cpu, { t: ts, v: cpuPct * Math.max(1, m.cpus) }), mem: push(h.mem, { t: ts, v: memUsed }), net: push(h.net, { t: ts, v: rxRate + txRate }) }));
        }
        setNow({ cpuPct, memUsed, memTotal, diskUsed: m.disk_used_bytes, diskTotal: m.disk_total_bytes, rxRate, txRate, load1: m.load1, cpus: m.cpus });
      } catch (e) {
        if (!cancelled) setUnavailable(String(e));
      }
      if (!cancelled) timer = window.setTimeout(() => void tick(), ENGINE_PERIOD_MS);
    };
    void tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  // Statistiques des conteneurs en flux (un canal pour tous), débits réseau par différence.
  useEffect(() => {
    let streamId: number | null = null;
    let cancelled = false;
    const last: Record<string, { s: StatSample; at: number }> = {};
    containers
      .statsOpen((s) => {
        const at = performance.now();
        const prev = last[s.id];
        last[s.id] = { s, at };
        let rxRate = 0;
        let txRate = 0;
        if (prev) {
          const dt = Math.max((at - prev.at) / 1000, 0.1);
          rxRate = Math.max(0, s.rx_bytes - prev.s.rx_bytes) / dt;
          txRate = Math.max(0, s.tx_bytes - prev.s.tx_bytes) / dt;
        }
        setPerContainer((p) => ({ ...p, [s.id]: { sample: s, rxRate, txRate, cpuHistory: pushHistory(p[s.id]?.cpuHistory ?? [], s.cpu_percent) } }));
        const ts = Date.now();
        setContHist((h) => {
          const cur = h[s.id] ?? emptyHistory();
          return { ...h, [s.id]: { cpu: push(cur.cpu, { t: ts, v: s.cpu_percent }), mem: push(cur.mem, { t: ts, v: s.mem_usage }), net: prev ? push(cur.net, { t: ts, v: rxRate + txRate }) : cur.net } };
        });
      })
      .then((id) => {
        if (cancelled) void containers.streamClose(id);
        else streamId = id;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      if (streamId !== null) void containers.streamClose(streamId);
    };
  }, []);

  const rows = useMemo(() => {
    const list = (running.data ?? []).map((c: ContainerSummary) => ({ c, name: (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, ""), st: perContainer[c.Id] }));
    const dir = sort.desc ? -1 : 1;
    return list.sort((a, b) => {
      switch (sort.key) {
        case "name":
          return dir * a.name.localeCompare(b.name);
        case "cpu":
          return dir * ((a.st?.sample.cpu_percent ?? -1) - (b.st?.sample.cpu_percent ?? -1));
        case "mem":
          return dir * ((a.st?.sample.mem_usage ?? -1) - (b.st?.sample.mem_usage ?? -1));
        default:
          return dir * ((a.st ? a.st.rxRate + a.st.txRate : -1) - (b.st ? b.st.rxRate + b.st.txRate : -1));
      }
    });
  }, [running.data, perContainer, sort]);

  // Séries du graphique : le moteur en fond, puis un conteneur par courbe (couleur de son avatar).
  const chartSeries = useMemo<ChartSeries[]>(() => {
    const out: ChartSeries[] = [{ id: ENGINE_ID, name: t("activity.engine"), hue: null, points: engineHist[metric], area: true }];
    const byName = (running.data ?? []).map((c) => ({ id: c.Id, name: (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "") })).sort((a, b) => a.name.localeCompare(b.name));
    for (const { id, name } of byName) out.push({ id, name, hue: hueOf(name), points: contHist[id]?.[metric] ?? [] });
    return out;
  }, [running.data, contHist, engineHist, metric, t]);

  // Limite mémoire en pointillé quand un seul conteneur est isolé et qu'il en a une (sinon Docker
  // renvoie la mémoire totale du moteur, qui n'est pas une limite).
  const limit = useMemo(() => {
    if (metric !== "mem") return null;
    const shown = chartSeries.filter((s) => !hidden.mem.has(s.id));
    if (shown.length !== 1 || shown[0].id === ENGINE_ID) return null;
    const l = perContainer[shown[0].id]?.sample.mem_limit ?? 0;
    if (!l || (now && l >= now.memTotal * 0.95)) return null;
    return { v: l, label: t("activity.limit") };
  }, [metric, chartSeries, hidden, perContainer, now, t]);

  const toggleSort = (key: SortKey) => setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }));
  const arrow = (key: SortKey) => (sort.key === key ? (sort.desc ? " ↓" : " ↑") : "");
  const reserved = settings.data ? settings.data.memory_mb * 1024 * 1024 : 0;
  const pct = (a: number, b: number) => (b > 0 ? Math.min(100, (100 * a) / b) : 0);
  const fmtPct = (v: number) => `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} %`;
  const fmtRate = (v: number) => `${formatBytes(v)}/s`;
  const windowLabel = (w: WindowMs) => (w === 60_000 ? t("activity.window.m1") : w === 600_000 ? t("activity.window.m10") : t("activity.window.h1"));

  return (
    <div className="flex h-full flex-col">
      <PageHeader title={t("activity.title")}>
        <span className="kbd-hint">{t("activity.subtitle")}</span>
        <div className="segmented" role="radiogroup" aria-label={t("activity.window.label")}>
          {WINDOWS.map((w) => (
            <button key={w} type="button" role="radio" aria-checked={windowMs === w} className={windowMs === w ? "is-on" : ""} onClick={() => setWindowMs(w)}>{windowLabel(w)}</button>
          ))}
        </div>
      </PageHeader>

      {unavailable && (
        <div className="mx-4 mb-3 rounded px-3 py-2" role="status" style={{ background: "var(--warn-soft)", color: "var(--warn)" }}>
          {t("activity.unavailable")}
        </div>
      )}

      <div className="mx-4 mb-3 grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
        <section className="card metric metric-compact">
          <div className="metric-head">
            <span className="metric-label">{t("activity.cpu")}</span>
            <span className="metric-sub">{now ? t("activity.cpus", { count: now.cpus }) : ""}</span>
          </div>
          <div className="metric-value">{now && engineHist.cpu.length ? `${now.cpuPct.toFixed(0)} %` : "—"}</div>
          <div className="metric-sub">{now ? t("activity.load", { value: now.load1.toFixed(2) }) : t("activity.waiting")}</div>
        </section>
        <section className="card metric metric-compact">
          <div className="metric-head">
            <span className="metric-label">{t("activity.memory")}</span>
            <span className="metric-sub">{reserved ? t("activity.reserved", { value: formatBytes(reserved) }) : ""}</span>
          </div>
          <div className="metric-value">{now ? formatBytes(now.memUsed) : "—"}</div>
          <div className="metric-sub">{now ? t("activity.of", { total: formatBytes(now.memTotal) }) : t("activity.waiting")}</div>
        </section>
        <section className="card metric metric-compact">
          <div className="metric-head">
            <span className="metric-label">{t("activity.disk")}</span>
            <span className="metric-sub">{now && now.diskTotal ? `${pct(now.diskUsed, now.diskTotal).toFixed(0)} %` : ""}</span>
          </div>
          <div className="metric-value">{now ? formatBytes(now.diskUsed) : "—"}</div>
          <div className="metric-sub">{now ? t("activity.of", { total: formatBytes(now.diskTotal) }) : t("activity.waiting")}</div>
          <div className="meter" role="progressbar" aria-valuenow={now ? Math.round(pct(now.diskUsed, now.diskTotal)) : 0} aria-valuemin={0} aria-valuemax={100}>
            <div className={`meter-fill ${now && pct(now.diskUsed, now.diskTotal) >= 90 ? "is-bad" : ""}`} style={{ width: `${now ? pct(now.diskUsed, now.diskTotal) : 0}%` }} />
          </div>
        </section>
        <section className="card metric metric-compact">
          <div className="metric-head">
            <span className="metric-label">{t("activity.network")}</span>
            <span className="metric-sub">eth0</span>
          </div>
          <div className="metric-value">{now && engineHist.net.length ? `↓ ${formatBytes(now.rxRate)}/s` : "—"}</div>
          <div className="metric-sub">{now && engineHist.net.length ? `↑ ${formatBytes(now.txRate)}/s` : t("activity.waiting")}</div>
        </section>
      </div>

      <div className="card mx-4 mb-3">
        <div className="flex items-center gap-3 border-b px-3" style={{ borderColor: "var(--line)" }}>
          <div role="tablist" className="tabs" style={{ border: 0 }}>
            {(["cpu", "mem", "net"] as Metric[]).map((m) => (
              <button key={m} role="tab" type="button" aria-selected={metric === m} onClick={() => setMetric(m)} className="tab">{t(`activity.${m === "mem" ? "memory" : m === "net" ? "network" : "cpu"}`)}</button>
            ))}
          </div>
          <span className="kbd-hint">{t(`activity.unit.${metric}`)}</span>
          <span className="flex-1" />
          <span className="kbd-hint">{t("activity.legend_hint")}</span>
        </div>
        <TimeChart
          series={chartSeries}
          windowMs={windowMs}
          now={clock}
          format={metric === "cpu" ? fmtPct : metric === "mem" ? formatBytes : fmtRate}
          minMax={metric === "cpu" ? 10 : metric === "mem" ? 64 * 1024 * 1024 : 10 * 1024}
          binary={metric === "mem"}
          limit={limit}
          hidden={hidden[metric]}
          onHiddenChange={(next) => setHidden((h) => ({ ...h, [metric]: next }))}
          emptyLabel={t("activity.waiting")}
          unitLabel={t(`activity.unit.${metric}`)}
        />
      </div>

      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {rows.length === 0 ? (
          <EmptyState icon={<IconPulse />} title={t("activity.empty")} hint={t("activity.empty_hint")} />
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th><button type="button" className="th-sort" onClick={() => toggleSort("name")}>{t("activity.columns.name")}{arrow("name")}</button></th>
                <th className="text-right"><button type="button" className="th-sort" onClick={() => toggleSort("cpu")}>{t("activity.columns.cpu")}{arrow("cpu")}</button></th>
                <th className="text-right"><button type="button" className="th-sort" onClick={() => toggleSort("mem")}>{t("activity.columns.memory")}{arrow("mem")}</button></th>
                <th className="text-right"><button type="button" className="th-sort" onClick={() => toggleSort("net")}>{t("activity.columns.network")}{arrow("net")}</button></th>
                <th>{t("activity.columns.trend")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(({ c, name, st }) => (
                <tr key={c.Id}>
                  <td>
                    <button type="button" className="font-medium hover:underline" onClick={() => onOpenContainer(c.Id)} style={{ color: "var(--ink)" }}>
                      <i className="chart-swatch chart-swatch-inline" style={{ background: `hsl(${hueOf(name)} 62% var(--chart-l))` }} />
                      {name}
                    </button>
                  </td>
                  <td className="mono text-right whitespace-nowrap">{st ? `${st.sample.cpu_percent.toFixed(1)} %` : "—"}</td>
                  <td className="mono text-right whitespace-nowrap">{st ? `${formatBytes(st.sample.mem_usage)}${st.sample.mem_limit && now && st.sample.mem_limit < now.memTotal * 0.95 ? ` / ${formatBytes(st.sample.mem_limit)}` : ""}` : "—"}</td>
                  <td className="mono text-right whitespace-nowrap">{st ? `↓ ${formatBytes(st.rxRate)}/s  ↑ ${formatBytes(st.txRate)}/s` : "—"}</td>
                  <td className="w-40">
                    <Spark values={st?.cpuHistory ?? []} max={Math.max(100, ...(st?.cpuHistory ?? [0]))} className="spark-row" warn={70} bad={90} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
