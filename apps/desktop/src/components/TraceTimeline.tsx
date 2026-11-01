// Barre de lecture de la radiographie : une chronologie des échanges, un curseur, et les
// commandes d'un magnétoscope.
//
// La chronologie est un histogramme plutôt qu'un trait par échange : une heure d'historique peut
// en compter des milliers, et ce qu'on y cherche d'un coup d'œil, ce sont les rafales et les
// échecs. Chaque barre prend la couleur du pire échange qu'elle contient — une erreur au milieu
// de cent succès doit rester visible.
import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

/** Ce que la barre a besoin de savoir d'un échange. */
export interface TimelinePoint {
  ts: number;
  /** 0 : rien à signaler, 1 : à surveiller (4xx), 2 : échec (refus, 5xx). */
  severity: 0 | 1 | 2;
}

export const SPEEDS = [0.25, 1, 4] as const;
export type Speed = (typeof SPEEDS)[number];

const BINS = 240;
const WIDTH = 1000;
const HEIGHT = 36;

interface Props {
  points: TimelinePoint[];
  start: number;
  end: number;
  /** Position du curseur ; égale à `end` en direct. */
  at: number;
  live: boolean;
  playing: boolean;
  speed: Speed;
  onSeek: (ts: number) => void;
  onPlayPause: () => void;
  onSpeed: (s: Speed) => void;
  onLive: () => void;
  onStep: (direction: -1 | 1) => void;
  onBack: (ms: number) => void;
}

export function TraceTimeline(p: Props) {
  const { t } = useTranslation();
  const bar = useRef<SVGSVGElement>(null);
  const span = Math.max(1, p.end - p.start);
  const x = (ts: number) => ((ts - p.start) / span) * WIDTH;

  const bins = Array.from({ length: BINS }, () => ({ count: 0, worst: 0 }));
  for (const pt of p.points) {
    if (pt.ts < p.start || pt.ts > p.end) continue;
    const i = Math.min(BINS - 1, Math.floor(((pt.ts - p.start) / span) * BINS));
    bins[i].count += 1;
    bins[i].worst = Math.max(bins[i].worst, pt.severity);
  }
  // Échelle logarithmique : une rafale de 500 appels ne doit pas écraser un appel isolé.
  const top = Math.log1p(Math.max(1, ...bins.map((b) => b.count)));

  function seekFrom(e: PointerEvent<SVGSVGElement>) {
    const rect = bar.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return;
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    p.onSeek(p.start + ratio * span);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === " ") p.onPlayPause();
    else if (e.key === "ArrowLeft") p.onStep(-1);
    else if (e.key === "ArrowRight") p.onStep(1);
    else return;
    e.preventDefault();
  }

  const behind = Math.max(0, Math.round((p.end - p.at) / 1000));

  return (
    <div className="card mx-4 mb-3 px-3 py-2" tabIndex={0} onKeyDown={onKey} aria-label={t("radiography.timeline")}>
      <svg
        ref={bar}
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        className="block h-9 w-full cursor-pointer"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          seekFrom(e);
        }}
        onPointerMove={(e) => {
          if (e.buttons === 1) seekFrom(e);
        }}
        role="slider"
        aria-valuemin={p.start}
        aria-valuemax={p.end}
        aria-valuenow={p.at}
        aria-valuetext={clockMs(p.at)}
      >
        <rect x={0} y={0} width={WIDTH} height={HEIGHT} fill="var(--surface-2)" />
        {bins.map((b, i) => {
          if (b.count === 0) return null;
          const h = Math.max(3, (Math.log1p(b.count) / top) * (HEIGHT - 4));
          const color = b.worst === 2 ? "var(--bad)" : b.worst === 1 ? "var(--warn)" : "var(--accent)";
          return (
            <rect
              key={i}
              x={(i / BINS) * WIDTH + 0.5}
              y={HEIGHT - h}
              width={WIDTH / BINS - 1}
              height={h}
              fill={color}
              opacity={b.worst === 0 ? 0.55 : 1}
            />
          );
        })}
        {/* Tout ce qui est à droite du curseur n'a « pas encore eu lieu » dans la relecture. */}
        {!p.live && <rect x={x(p.at)} y={0} width={WIDTH - x(p.at)} height={HEIGHT} fill="var(--bg)" opacity={0.55} />}
        <line x1={x(p.at)} x2={x(p.at)} y1={0} y2={HEIGHT} stroke="var(--ink)" strokeWidth={2} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="mt-2 flex items-center gap-2 text-[13px]">
        <button type="button" className="btn btn-sm" onClick={() => p.onBack(10_000)} title={t("radiography.back_10s")}>−10 s</button>
        <button type="button" className="btn btn-sm" onClick={() => p.onStep(-1)} title={t("radiography.prev_event")} aria-label={t("radiography.prev_event")}>⟨</button>
        <button type="button" className="btn btn-sm min-w-[5.5rem]" onClick={p.onPlayPause}>
          {p.live || p.playing ? t("radiography.pause") : t("radiography.play")}
        </button>
        <button type="button" className="btn btn-sm" onClick={() => p.onStep(1)} title={t("radiography.next_event")} aria-label={t("radiography.next_event")}>⟩</button>
        <div className="ml-2 flex gap-1" role="group" aria-label={t("radiography.speed")}>
          {SPEEDS.map((s) => (
            <button
              key={s}
              type="button"
              className="btn btn-sm tabular-nums"
              aria-pressed={p.speed === s}
              style={p.speed === s ? { background: "var(--accent-soft)", color: "var(--accent-ink)" } : undefined}
              onClick={() => p.onSpeed(s)}
            >
              {s === 0.25 ? "×0,25" : `×${s}`}
            </button>
          ))}
        </div>
        <span className="ml-auto tabular-nums" style={{ color: "var(--ink-2)" }}>
          {clockMs(p.at)}
          {!p.live && behind > 0 && <span style={{ color: "var(--ink-3)" }}> · {t("radiography.behind", { s: behind })}</span>}
        </span>
        <button
          type="button"
          className="btn btn-sm"
          onClick={p.onLive}
          disabled={p.live}
          style={p.live ? { color: "var(--ok)" } : undefined}
        >
          ● {t("radiography.live")}
        </button>
      </div>
    </div>
  );
}

/** Heure au dixième de seconde : en relecture lente, la seconde est trop grossière. */
function clockMs(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${Math.floor(d.getMilliseconds() / 100)}`;
}
