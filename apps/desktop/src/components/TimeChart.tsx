import { useEffect, useMemo, useRef, useState } from "react";

/** Un point horodaté (ms depuis l'époque, valeur dans l'unité de la série). */
export interface Pt {
  t: number;
  v: number;
}

export interface ChartSeries {
  id: string;
  name: string;
  /** Teinte stable (celle de l'avatar) ; `null` pour la série de fond (le moteur), en gris. */
  hue: number | null;
  points: Pt[];
  /** Aire remplie sous la courbe (série de fond). */
  area?: boolean;
}

export interface ChartLimit {
  v: number;
  label: string;
}

const ML = 52;
const MR = 12;
const MT = 10;
const MB = 22;
const H = 220;

/** Borne haute « ronde » juste au-dessus de `v` : 1, 2, 5 × 10^k (ou multiples binaires pour les octets). */
export function niceMax(v: number, binary: boolean, floor: number): number {
  const target = Math.max(v * 1.08, floor, 1e-9);
  if (binary) {
    let m = 1;
    while (m < target) m *= 2;
    return m;
  }
  const exp = Math.floor(Math.log10(target));
  const base = Math.pow(10, exp);
  for (const k of [1, 2, 2.5, 5, 10]) if (k * base >= target) return k * base;
  return 10 * base;
}

function clockLabel(t: number, windowMs: number): string {
  const d = new Date(t);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  if (windowMs <= 120_000) return `${hh}:${mm}:${String(d.getSeconds()).padStart(2, "0")}`;
  return `${hh}:${mm}`;
}

/** Point le plus proche de `t` (les points sont triés par temps). */
function nearest(points: Pt[], t: number): Pt | null {
  if (points.length === 0) return null;
  let lo = 0;
  let hi = points.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (points[mid].t < t) lo = mid + 1;
    else hi = mid;
  }
  const a = points[lo];
  const b = points[lo - 1];
  return b && Math.abs(b.t - t) < Math.abs(a.t - t) ? b : a;
}

export function seriesColor(hue: number | null): string {
  return hue === null ? "var(--chart-bg-line)" : `hsl(${hue} 62% var(--chart-l))`;
}

/**
 * Courbes dans le temps : fenêtre glissante, axes gradués, survol avec valeurs exactes, légende
 * cliquable (nom = isoler, pastille = masquer), ligne pointillée de limite.
 */
export function TimeChart({ series, windowMs, now, format, minMax, binary = false, limit, hidden, onHiddenChange, emptyLabel, unitLabel }: {
  series: ChartSeries[];
  windowMs: number;
  now: number;
  format: (v: number) => string;
  /** Plancher de l'axe vertical, pour ne pas zoomer sur du bruit. */
  minMax: number;
  binary?: boolean;
  limit?: ChartLimit | null;
  hidden: Set<string>;
  onHiddenChange: (next: Set<string>) => void;
  emptyLabel: string;
  unitLabel: string;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(600);
  const [hover, setHover] = useState<number | null>(null); // temps survolé (ms)

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const cw = entries[0]?.contentRect.width;
      if (cw && cw > 50) setW(Math.round(cw));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const t0 = now - windowMs;
  const plotW = Math.max(w - ML - MR, 10);
  const plotH = H - MT - MB;
  const x = (t: number) => ML + ((t - t0) / windowMs) * plotW;

  const visible = useMemo(() => series.filter((s) => !hidden.has(s.id)), [series, hidden]);
  // Pour chaque série visible : points de la fenêtre, plus un point juste avant pour ne pas couper la courbe.
  const clipped = useMemo(
    () =>
      visible.map((s) => {
        let i = s.points.findIndex((p) => p.t >= t0);
        if (i < 0) i = s.points.length;
        return { s, pts: s.points.slice(Math.max(0, i - 1)) };
      }),
    [visible, t0],
  );
  const maxV = useMemo(() => {
    let m = 0;
    for (const { pts } of clipped) for (const p of pts) if (p.v > m) m = p.v;
    if (limit && clipped.length === 1) m = Math.max(m, limit.v);
    return niceMax(m, binary, minMax);
  }, [clipped, limit, binary, minMax]);
  const y = (v: number) => MT + plotH - (Math.min(v, maxV) / maxV) * plotH;

  const hasData = clipped.some(({ pts }) => pts.length >= 2);
  const ticks = [0, 0.25, 0.5, 0.75, 1];
  const hoverRows = hover === null ? [] : clipped.map(({ s, pts }) => ({ s, p: nearest(pts, hover) })).filter((r) => r.p && Math.abs(r.p.t - hover) < windowMs / 20) as { s: ChartSeries; p: Pt }[];
  const hoverX = hover === null ? 0 : x(hover);

  function toggleHidden(id: string) {
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    onHiddenChange(next);
  }
  function solo(id: string) {
    const others = series.filter((s) => s.id !== id).map((s) => s.id);
    const isSolo = !hidden.has(id) && others.every((o) => hidden.has(o)) && others.length > 0;
    onHiddenChange(isSolo ? new Set() : new Set(others));
  }

  return (
    <div className="chart">
      <div ref={wrap} className="chart-plot" onMouseLeave={() => setHover(null)} onMouseMove={(e) => {
        const r = wrap.current?.getBoundingClientRect();
        if (!r) return;
        const px = e.clientX - r.left;
        if (px < ML || px > w - MR) { setHover(null); return; }
        setHover(t0 + ((px - ML) / plotW) * windowMs);
      }}>
        <svg width={w} height={H} viewBox={`0 0 ${w} ${H}`} role="img" aria-label={unitLabel}>
          {ticks.map((k) => (
            <g key={k}>
              <line className="chart-grid" x1={ML} x2={w - MR} y1={y(k * maxV)} y2={y(k * maxV)} />
              {(k === 0 || k === 0.5 || k === 1) && (
                <text className="chart-tick" x={ML - 6} y={y(k * maxV) + 3.5} textAnchor="end">{format(k * maxV)}</text>
              )}
            </g>
          ))}
          {(plotW < 420 ? [0, 0.5, 1] : [0, 0.25, 0.5, 0.75, 1]).map((k) => {
            const t = t0 + k * windowMs;
            const xx = x(t);
            return (
              <g key={k}>
                <line className="chart-grid" x1={xx} x2={xx} y1={MT} y2={MT + plotH} />
                <text className="chart-tick" x={xx} y={H - 6} textAnchor={k === 0 ? "start" : k === 1 ? "end" : "middle"}>{clockLabel(t, windowMs)}</text>
              </g>
            );
          })}
          <clipPath id="chart-clip"><rect x={ML} y={MT - 2} width={plotW} height={plotH + 2} /></clipPath>
          <g clipPath="url(#chart-clip)">
            {clipped.map(({ s, pts }) => {
              if (pts.length < 2) return null;
              const d = pts.map((p) => `${x(p.t).toFixed(1)},${y(p.v).toFixed(1)}`).join(" ");
              const color = seriesColor(s.hue);
              return (
                <g key={s.id}>
                  {s.area && <polygon className="chart-area" points={`${x(pts[0].t).toFixed(1)},${MT + plotH} ${d} ${x(pts[pts.length - 1].t).toFixed(1)},${MT + plotH}`} fill={color} />}
                  <polyline className="chart-line" points={d} stroke={color} />
                </g>
              );
            })}
            {limit && clipped.length === 1 && limit.v <= maxV && (
              <g>
                <line className="chart-limit" x1={ML} x2={w - MR} y1={y(limit.v)} y2={y(limit.v)} />
                <text className="chart-tick chart-limit-label" x={w - MR - 4} y={y(limit.v) - 4} textAnchor="end">{limit.label} {format(limit.v)}</text>
              </g>
            )}
          </g>
          {hover !== null && hoverRows.length > 0 && (
            <g>
              <line className="chart-cursor" x1={hoverX} x2={hoverX} y1={MT} y2={MT + plotH} />
              {hoverRows.map(({ s, p }) => (
                <circle key={s.id} cx={x(p.t)} cy={y(p.v)} r={3} fill={seriesColor(s.hue)} className="chart-dot" />
              ))}
            </g>
          )}
          {!hasData && <text className="chart-empty" x={ML + plotW / 2} y={MT + plotH / 2} textAnchor="middle">{emptyLabel}</text>}
        </svg>
        {hover !== null && hoverRows.length > 0 && (
          <div className="chart-tip" style={hoverX > w * 0.6 ? { right: w - hoverX + 10 } : { left: hoverX + 10 }}>
            <div className="chart-tip-time">{clockLabel(hoverRows[0].p.t, 0)}</div>
            {hoverRows
              .slice()
              .sort((a, b) => b.p.v - a.p.v)
              .map(({ s, p }) => (
                <div key={s.id} className="chart-tip-row">
                  <i style={{ background: seriesColor(s.hue) }} />
                  <span className="truncate">{s.name}</span>
                  <b className="mono">{format(p.v)}</b>
                </div>
              ))}
          </div>
        )}
      </div>
      <div className="chart-legend" role="list">
        {series.map((s) => {
          const off = hidden.has(s.id);
          const last = s.points[s.points.length - 1];
          return (
            <div key={s.id} role="listitem" className={`chart-legend-item ${off ? "is-off" : ""}`}>
              <button type="button" className="chart-swatch" style={{ background: off ? "transparent" : seriesColor(s.hue), borderColor: seriesColor(s.hue) }} title={off ? "Show" : "Hide"} aria-pressed={!off} onClick={() => toggleHidden(s.id)} />
              <button type="button" className="chart-legend-name truncate" title={s.name} onClick={() => solo(s.id)}>{s.name}</button>
              <span className="mono chart-legend-value">{last ? format(last.v) : "—"}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
