import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { containers, type LogChunk } from "../api";

const MAX_LINES = 5000;
const ERROR_RE = /\b(error|erreur|exception|fatal|panic|traceback|critical)\b/i;
const WARN_RE = /\b(warn|warning|avertissement|deprecated)\b/i;

function toneOf(l: LogChunk): string | undefined {
  if (l.stream === "error") return "var(--bad)";
  if (ERROR_RE.test(l.text)) return "var(--bad)";
  if (l.stream === "stderr" || WARN_RE.test(l.text)) return "var(--warn)";
  return undefined;
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
    if (follow && !search) bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines, follow, search]);

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? lines.filter((l) => l.text.toLowerCase().includes(q)) : lines;
  }, [lines, search]);

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
          <span style={{ color: "var(--ink-3)" }}>{search ? t("detail.logs.no_match") : t("detail.logs.empty")}</span>
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
