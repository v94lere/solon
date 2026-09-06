import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { containers, engine, formatBytes, type ContainerSummary, type MachineMetrics, type StatSample } from "../api";

const ENGINE_PERIOD_MS = 2000;
const HISTORY = 60;

/** Petite courbe glissante : `values` de gauche (ancien) à droite (récent), bornée par `max`. */
function Spark({ values, max, className }: { values: number[]; max: number; className?: string }) {
  const w = 240;
  const h = 40;
  if (values.length < 2) return <svg className={`spark ${className ?? ""}`} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden="true" />;
  const m = Math.max(max, 1e-9);
  const step = w / (HISTORY - 1);
  const x0 = w - (values.length - 1) * step;
  const pts = values.map((v, i) => `${(x0 + i * step).toFixed(1)},${(h - 2 - (Math.min(v, m) / m) * (h - 4)).toFixed(1)}`);
  const area = `${x0.toFixed(1)},${h} ${pts.join(" ")} ${w},${h}`;
  return (
    <svg className={`spark ${className ?? ""}`} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden="true">
      <polygon className="spark-area" points={area} />
      <polyline className="spark-line" points={pts.join(" ")} />
    </svg>
  );
}

function pushHistory(list: number[], v: number): number[] {
  const next = list.length >= HISTORY ? list.slice(list.length - HISTORY + 1) : list.slice();
  next.push(v);
  return next;
}

interface EngineSeries {
  cpu: number[];
  mem: number[];
  net: number[];
}

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

type SortKey = "name" | "cpu" | "mem" | "net";

/** Activité : mesures en temps réel du moteur (processeur, mémoire, stockage, réseau) et des conteneurs. */
export function ActivityView({ onOpenContainer }: { onOpenContainer: (id: string) => void }) {
  const { t } = useTranslation();
  const [now, setNow] = useState<EngineNow | null>(null);
  const [series, setSeries] = useState<EngineSeries>({ cpu: [], mem: [], net: [] });
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const prevRef = useRef<{ m: MachineMetrics; at: number } | null>(null);
  const [perContainer, setPerContainer] = useState<Record<string, ContainerNow>>({});
  const [sort, setSort] = useState<{ key: SortKey; desc: boolean }>({ key: "cpu", desc: true });
  const running = useQuery({ queryKey: ["containers", false], queryFn: () => containers.list(false), refetchInterval: 5000 });
  const settings = useQuery({ queryKey: ["settings"], queryFn: () => engine.settingsGet() });

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
          setSeries((s) => ({ cpu: pushHistory(s.cpu, cpuPct), mem: pushHistory(s.mem, memUsed), net: pushHistory(s.net, rxRate + txRate) }));
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

  const toggleSort = (key: SortKey) => setSort((s) => (s.key === key ? { key, desc: !s.desc } : { key, desc: key !== "name" }));
  const arrow = (key: SortKey) => (sort.key === key ? (sort.desc ? " ↓" : " ↑") : "");
  const reserved = settings.data ? settings.data.memory_mb * 1024 * 1024 : 0;
  const pct = (a: number, b: number) => (b > 0 ? Math.min(100, (100 * a) / b) : 0);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold">{t("activity.title")}</h1>
        <span className="kbd-hint">{t("activity.subtitle")}</span>
      </div>

      {unavailable && (
        <div className="mx-4 mb-3 rounded px-3 py-2" role="status" style={{ background: "var(--warn-soft)", color: "var(--warn)" }}>
          {t("activity.unavailable")}
        </div>
      )}

      <div className="mx-4 mb-3 grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(230px, 1fr))" }}>
        <section className="card metric">
          <div className="metric-head">
            <span className="metric-label">{t("activity.cpu")}</span>
            <span className="metric-sub">{now ? t("activity.cpus", { count: now.cpus }) : ""}</span>
          </div>
          <div className="metric-value">{now && series.cpu.length ? `${now.cpuPct.toFixed(0)} %` : "—"}</div>
          <div className="metric-sub">{now ? t("activity.load", { value: now.load1.toFixed(2) }) : t("activity.waiting")}</div>
          <Spark values={series.cpu} max={100} />
        </section>
        <section className="card metric">
          <div className="metric-head">
            <span className="metric-label">{t("activity.memory")}</span>
            <span className="metric-sub">{reserved ? t("activity.reserved", { value: formatBytes(reserved) }) : ""}</span>
          </div>
          <div className="metric-value">{now ? formatBytes(now.memUsed) : "—"}</div>
          <div className="metric-sub">{now ? t("activity.of", { total: formatBytes(now.memTotal) }) : t("activity.waiting")}</div>
          <Spark values={series.mem} max={now?.memTotal ?? 1} />
        </section>
        <section className="card metric">
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
        <section className="card metric">
          <div className="metric-head">
            <span className="metric-label">{t("activity.network")}</span>
            <span className="metric-sub">eth0</span>
          </div>
          <div className="metric-value">{now && series.net.length ? `↓ ${formatBytes(now.rxRate)}/s` : "—"}</div>
          <div className="metric-sub">{now && series.net.length ? `↑ ${formatBytes(now.txRate)}/s` : t("activity.waiting")}</div>
          <Spark values={series.net} max={Math.max(...series.net, 1)} />
        </section>
      </div>

      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {rows.length === 0 ? (
          <p className="p-6 text-center" style={{ color: "var(--ink-2)" }}>
            {t("activity.empty")}
          </p>
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
                      {name}
                    </button>
                  </td>
                  <td className="mono text-right whitespace-nowrap">{st ? `${st.sample.cpu_percent.toFixed(1)} %` : "—"}</td>
                  <td className="mono text-right whitespace-nowrap">{st ? `${formatBytes(st.sample.mem_usage)}${st.sample.mem_limit ? ` / ${formatBytes(st.sample.mem_limit)}` : ""}` : "—"}</td>
                  <td className="mono text-right whitespace-nowrap">{st ? `↓ ${formatBytes(st.rxRate)}/s  ↑ ${formatBytes(st.txRate)}/s` : "—"}</td>
                  <td className="w-40">
                    <Spark values={st?.cpuHistory ?? []} max={Math.max(100, ...(st?.cpuHistory ?? [0]))} className="spark-row" />
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
