import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { containers, type LogChunk } from "../api";

const MAX_LINES = 5000;
const ERROR_RE = /\b(error|erreur|exception|fatal|panic|traceback|critical)\b/i;
const WARN_RE = /\b(warn|warning|avertissement|deprecated)\b/i;

export type Level = "all" | "warn" | "error";

/** Niveau d'une ligne : erreur (mot-clé ou flux d'erreur du canal), avertissement (sortie d'erreur ou mot-clé), info. */
export function levelOf(l: LogChunk): "error" | "warn" | "info" {
  if (l.stream === "error" || ERROR_RE.test(l.text)) return "error";
  if (l.stream === "stderr" || WARN_RE.test(l.text)) return "warn";
  return "info";
}

function toneOf(l: LogChunk): string | undefined {
  const lv = levelOf(l);
  return lv === "error" ? "var(--bad)" : lv === "warn" ? "var(--warn)" : undefined;
}

/** Trois pastilles Tout / Avertissements / Erreurs, avec le compte des lignes concernées. */
export function LevelChips({ level, onChange, counts }: { level: Level; onChange: (l: Level) => void; counts: { warn: number; error: number } }) {
  const { t } = useTranslation();
  return (
    <span className="segmented log-levels" role="tablist" aria-label={t("detail.logs.level")}>
      <button type="button" role="tab" aria-selected={level === "all"} onClick={() => onChange("all")}>{t("detail.logs.level_all")}</button>
      <button type="button" role="tab" aria-selected={level === "warn"} onClick={() => onChange("warn")} className={counts.warn > 0 ? "has-warn" : ""}>{t("detail.logs.level_warn")} {counts.warn > 0 && <b>{counts.warn}</b>}</button>
      <button type="button" role="tab" aria-selected={level === "error"} onClick={() => onChange("error")} className={counts.error > 0 ? "has-error" : ""}>{t("detail.logs.level_error")} {counts.error > 0 && <b>{counts.error}</b>}</button>
    </span>
  );
}

export function matchesLevel(l: LogChunk, level: Level): boolean {
  if (level === "all") return true;
  const lv = levelOf(l);
  return level === "error" ? lv === "error" : lv !== "info";
}

/** Journaux en flux (Channel Tauri) : suivi, horodatage, recherche, retour à la ligne, couleurs
 *  (erreurs en rouge, avertissements et sortie d'erreur en ambre). */
export function LogsPanel({ id }: { id: string }) {
  const { t } = useTranslation();
  const [lines, setLines] = useState<LogChunk[]>([]);
  const [follow, setFollow] = useState(true);
  const [timestamps, setTimestamps] = useState(false);
  const [wrap, setWrap] = useState(true);
  const [search, setSearch] = useState("");
  const [level, setLevel] = useState<Level>("all");
  const [ended, setEnded] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setLines([]);
    setEnded(false);
    let streamId: number | null = null;
    let cancelled = false;
    let pending: LogChunk[] = [];
    let raf = 0;
    const flush = () => {
      raf = 0;
      const batch = pending;
      pending = [];
      setLines((prev) => {
        const next = prev.concat(batch);
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    };
    containers
      .logsOpen(id, 500, timestamps, (chunk) => {
        if (chunk.stream === "end") {
          setEnded(true);
          return;
        }
        pending.push(chunk);
        if (!raf) raf = requestAnimationFrame(flush);
      })
      .then((sid) => {
        if (cancelled) void containers.streamClose(sid);
        else streamId = sid;
      })
      .catch((e: unknown) => setLines([{ stream: "error", text: String(e) }]));
    return () => {
      cancelled = true;
      if (raf) cancelAnimationFrame(raf);
      if (streamId !== null) void containers.streamClose(streamId);
    };
  }, [id, timestamps]);

  useEffect(() => {
    if (follow && !search && level === "all") bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines, follow, search, level]);

  const counts = useMemo(() => {
    let warn = 0;
    let error = 0;
    for (const l of lines) {
      const lv = levelOf(l);
      if (lv === "error") error++;
      else if (lv === "warn") warn++;
    }
    return { warn, error };
  }, [lines]);
  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return lines.filter((l) => matchesLevel(l, level) && (!q || l.text.toLowerCase().includes(q)));
  }, [lines, search, level]);

  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b px-3 py-2" style={{ borderColor: "var(--line)" }}>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} /> {t("detail.logs.follow")}
        </label>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={timestamps} onChange={(e) => setTimestamps(e.target.checked)} /> {t("detail.logs.timestamps")}
        </label>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={wrap} onChange={(e) => setWrap(e.target.checked)} /> {t("detail.logs.wrap")}
        </label>
        <LevelChips level={level} onChange={setLevel} counts={counts} />
        <div className="flex-1" />
        <input type="search" className="input w-60" placeholder={t("detail.logs.search")} value={search} onChange={(e) => setSearch(e.target.value)} aria-label={t("detail.logs.search")} />
        {search && <span className="kbd-hint">{t("detail.logs.matches", { count: shown.length })}</span>}
        {ended && <span className="kbd-hint">{t("detail.terminal.disconnected")}</span>}
        <button type="button" className="btn btn-sm" onClick={() => setLines([])}>
          {t("detail.logs.clear")}
        </button>
      </div>
      <div
        className={`mono min-h-0 flex-1 overflow-auto p-3 text-xs leading-5 ${wrap ? "whitespace-pre-wrap" : "whitespace-pre"}`}
        onWheel={(e) => {
          // Seul un défilement volontaire vers le haut suspend le suivi (pas l'auto-défilement).
          if (e.deltaY < 0 && follow) setFollow(false);
        }}
        aria-live="polite"
      >
        {shown.length === 0 ? (
          <span style={{ color: "var(--ink-3)" }}>{search || level !== "all" ? t("detail.logs.no_match") : t("detail.logs.empty")}</span>
        ) : (
          shown.map((l, i) => (
            <span key={i} style={{ color: toneOf(l) }}>
              {l.text}
            </span>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
