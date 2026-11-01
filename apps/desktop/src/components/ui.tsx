// Briques visuelles partagées : en-tête de page, état vide, squelettes de chargement, pastille
// d'initiale colorée, courbe glissante, mini-jauge.
import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Usage } from "../usage";

/** En-tête commun à tous les écrans : titre, compteur, actions au centre, recherche à droite. */
export function PageHeader({ title, count, actions, search, children }: { title: string; count?: string; actions?: ReactNode; search?: ReactNode; children?: ReactNode }) {
  return (
    <div className="page-head">
      <h1 className="page-title">{title}</h1>
      {count && <span className="count-chip">{count}</span>}
      {actions && <div className="page-actions">{actions}</div>}
      <span className="flex-1" />
      {children}
      {search}
    </div>
  );
}

/** État vide : pictogramme, titre, phrase d'aide et une action qui fait quelque chose. */
export function EmptyState({ icon, title, hint, action, children }: { icon?: ReactNode; title: string; hint?: string; action?: ReactNode; children?: ReactNode }) {
  return (
    <div className="empty" role="status">
      {icon ?? <IconBox />}
      <h3>{title}</h3>
      {hint && <p>{hint}</p>}
      {action && <div className="mt-1 flex flex-wrap justify-center gap-2">{action}</div>}
      {children}
    </div>
  );
}

/** Lignes grises animées le temps du chargement d'un tableau. */
export function SkeletonRows({ rows = 6, cols = 4 }: { rows?: number; cols?: number }) {
  const widths = [38, 22, 10, 30, 12, 12, 8];
  return (
    <div className="p-3" aria-busy="true" aria-live="polite">
      {Array.from({ length: rows }, (_, r) => (
        <div key={r} className="skel-row">
          {Array.from({ length: cols }, (_, c) => (
            <span key={c} className="skel" style={{ width: `${widths[c % widths.length]}%`, opacity: 1 - r * 0.1 }} />
          ))}
        </div>
      ))}
    </div>
  );
}

/** Teinte stable (0-359) dérivée d'un texte : la même image donne toujours la même couleur. */
export function hueOf(seed: string): number {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h) % 360;
}

/** Nom court d'une image sans registre, dépôt intermédiaire ni tag (`…/library/postgres:16` → `postgres`). */
export function imageBase(image: string | undefined): string {
  if (!image) return "?";
  const at = image.indexOf("@");
  const noDigest = at >= 0 ? image.slice(0, at) : image;
  const last = noDigest.slice(noDigest.lastIndexOf("/") + 1);
  const colon = last.lastIndexOf(":");
  return (colon > 0 ? last.slice(0, colon) : last) || "?";
}

/** Pastille colorée avec l'initiale : couleur calculée d'après `seed` (le nom de l'image). */
export function Avatar({ label, seed, size = 26, title }: { label: string; seed?: string; size?: number; title?: string }) {
  const s = seed ?? label;
  const initial = (label.trim()[0] ?? "?").toUpperCase();
  return (
    <span className="avatar" title={title ?? label} aria-hidden="true" style={{ ["--h" as string]: hueOf(s), width: size, height: size, fontSize: Math.round(size * 0.46) }}>
      {initial}
    </span>
  );
}

export const SPARK_HISTORY = 60;

/** Courbe glissante : `values` de gauche (ancien) à droite (récent), bornée par `max`.
 *  `warn` / `bad` : seuils (dans l'unité de `values`) qui changent la couleur de la courbe. */
export function Spark({ values, max, className, warn, bad }: { values: number[]; max: number; className?: string; warn?: number; bad?: number; history?: number }) {
  const id = useId();
  const w = 240;
  const h = 40;
  const last = values[values.length - 1] ?? 0;
  const tone = bad !== undefined && last >= bad ? "is-bad" : warn !== undefined && last >= warn ? "is-warn" : "";
  if (values.length < 2) return <svg className={`spark ${tone} ${className ?? ""}`} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden="true" />;
  const m = Math.max(max, 1e-9);
  // Tant que l'historique n'est pas plein, les points s'étalent sur toute la largeur.
  const step = w / Math.max(values.length - 1, 1);
  const x0 = 0;
  const pts = values.map((v, i) => `${(x0 + i * step).toFixed(1)},${(h - 2 - (Math.min(v, m) / m) * (h - 4)).toFixed(1)}`);
  const area = `${x0.toFixed(1)},${h} ${pts.join(" ")} ${w},${h}`;
  return (
    <svg className={`spark ${tone} ${className ?? ""}`} viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" className="spark-stop-a" />
          <stop offset="100%" className="spark-stop-b" />
        </linearGradient>
      </defs>
      <polygon className="spark-area" points={area} fill={`url(#${id})`} />
      <polyline className="spark-line" points={pts.join(" ")} />
    </svg>
  );
}

export function pushHistory(list: number[], v: number, history = SPARK_HISTORY): number[] {
  const next = list.length >= history ? list.slice(list.length - history + 1) : list.slice();
  next.push(v);
  return next;
}

/** Mini-jauge horizontale (0-100), colorée au-delà des seuils. */
export function MiniMeter({ pct, label }: { pct: number; label: string }) {
  const p = Math.max(0, Math.min(100, pct));
  const tone = p >= 90 ? "is-bad" : p >= 70 ? "is-warn" : "";
  return (
    <span className="mini-meter" role="progressbar" aria-label={label} aria-valuenow={Math.round(p)} aria-valuemin={0} aria-valuemax={100} title={`${label} ${Math.round(p)} %`}>
      <i className={tone} style={{ width: `${p}%` }} />
    </span>
  );
}

// Pictogrammes des états vides (traits 1.6, 40 px).
const svgProps = { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
export const IconBox = () => (
  <svg {...svgProps}>
    <path d="M12 3 3.5 7.5v9L12 21l8.5-4.5v-9Z" />
    <path d="M3.5 7.5 12 12l8.5-4.5M12 12v9" />
  </svg>
);
export const IconLayers = () => (
  <svg {...svgProps}>
    <path d="M12 3 3 8l9 5 9-5-9-5Z" />
    <path d="m3 12 9 5 9-5M3 16l9 5 9-5" />
  </svg>
);
export const IconDisk = () => (
  <svg {...svgProps}>
    <ellipse cx="12" cy="6" rx="8" ry="3" />
    <path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
  </svg>
);
export const IconGlobe = () => (
  <svg {...svgProps}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3c2.8 3 2.8 15 0 18M12 3c-2.8 3-2.8 15 0 18" />
  </svg>
);
export const IconPulse = () => (
  <svg {...svgProps}>
    <path d="M3 12h4l3-8 4 16 3-8h4" />
  </svg>
);
export const IconFolderOpen = () => (
  <svg {...svgProps}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
    <path d="M3 11h18" />
  </svg>
);

/** « Utilisé » ou non par des conteneurs : vert si au moins un tourne, neutre s'ils sont tous arrêtés, gris sinon. */
export function UsagePill({ usage }: { usage: Usage }) {
  const { t } = useTranslation();
  if (usage.total === 0) return <span className="pill pill-muted usage-pill">{t("usage.unused")}</span>;
  const title = t("usage.used_by", { names: usage.names.join(", ") });
  if (usage.running > 0) return <span className="pill pill-ok usage-pill" title={title}>{t("usage.in_use", { count: usage.running })}</span>;
  return <span className="pill pill-warn usage-pill" title={title}>{t("usage.stopped", { count: usage.total })}</span>;
}
