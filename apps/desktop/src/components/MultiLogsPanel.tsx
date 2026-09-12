import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { containers, type LogChunk } from "../api";
import { LevelChips, levelOf, matchesLevel, type Level } from "./LogsPanel";

const MAX_LINES = 8000;
const COLORS = ["#c8258a", "#1f8a3a", "#3c82c3", "#8e44ad", "#c2185b", "#00838f", "#6d4c41", "#737373"];

export interface LogSource {
  id: string;
  name: string;
}
type Line = LogChunk & { source: number };

/** Journaux de plusieurs conteneurs mêlés dans un seul flux, chaque ligne préfixée du nom du service. */
export function MultiLogsPanel({ sources }: { sources: LogSource[] }) {
  const { t } = useTranslation();
  const [lines, setLines] = useState<Line[]>([]);
  const [follow, setFollow] = useState(true);
  const [level, setLevel] = useState<Level>("all");
  const [search, setSearch] = useState("");
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const bottomRef = useRef<HTMLDivElement>(null);
  const key = useMemo(() => sources.map((s) => s.id).join("|"), [sources]);

  useEffect(() => {
    setLines([]);
    const streams: number[] = [];
    let cancelled = false;
    let pending: Line[] = [];
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
    sources.forEach((src, index) => {
      containers
        .logsOpen(src.id, 200, false, (chunk) => {
          if (chunk.stream === "end") return;
          pending.push({ ...chunk, source: index });
          if (!raf) raf = requestAnimationFrame(flush);
        })
        .then((sid) => {
          if (cancelled) void containers.streamClose(sid);
          else streams.push(sid);
        })
        .catch((e: unknown) => {
          pending.push({ stream: "error", text: `${src.name}: ${String(e)}\n`, source: index });
          if (!raf) raf = requestAnimationFrame(flush);
        });
    });
    return () => {
      cancelled = true;
      if (raf) cancelAnimationFrame(raf);
      streams.forEach((sid) => void containers.streamClose(sid));
    };
    // `key` résume la liste des identifiants : rouvrir les flux seulement quand elle change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  useEffect(() => {
    if (follow && level === "all" && !search) bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines, follow, level, search]);

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
    return lines.filter((l) => !hidden.has(sources[l.source]?.id ?? "") && matchesLevel(l, level) && (!q || l.text.toLowerCase().includes(q)));
  }, [lines, hidden, level, search, sources]);
  const toggleSource = (id: string) => setHidden((h) => { const n = new Set(h); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const width = Math.max(4, ...sources.map((s) => s.name.length));
  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b px-3 py-2" style={{ borderColor: "var(--line)" }}>
        <span className="font-semibold">{t("project.logs")}</span>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} /> {t("detail.logs.follow")}
        </label>
        <LevelChips level={level} onChange={setLevel} counts={counts} />
        {sources.length > 1 && (
          <span className="log-sources" aria-label={t("detail.logs.services")}>
            {sources.map((s, i) => (
              <button key={s.id} type="button" className={`log-source${hidden.has(s.id) ? " is-off" : ""}`} style={{ borderColor: COLORS[i % COLORS.length], color: hidden.has(s.id) ? undefined : COLORS[i % COLORS.length] }} aria-pressed={!hidden.has(s.id)} onClick={() => toggleSource(s.id)}>{s.name}</button>
            ))}
          </span>
        )}
        <div className="flex-1" />
        <input type="search" className="input input-sm w-52" placeholder={t("detail.logs.search")} value={search} onChange={(e) => setSearch(e.target.value)} aria-label={t("detail.logs.search")} />
        <button type="button" className="btn btn-sm" onClick={() => setLines([])}>{t("detail.logs.clear")}</button>
      </div>
      <div
        className="mono min-h-0 flex-1 overflow-auto p-3 text-xs leading-5 whitespace-pre-wrap"
        onWheel={(e) => { if (e.deltaY < 0 && follow) setFollow(false); }}
        aria-live="polite"
      >
        {sources.length === 0 ? (
          <span style={{ color: "var(--ink-3)" }}>{t("project.no_services")}</span>
        ) : shown.length === 0 ? (
          <span style={{ color: "var(--ink-3)" }}>{lines.length === 0 ? t("detail.logs.empty") : t("detail.logs.no_match")}</span>
        ) : (
          shown.map((l, i) => (
            <span key={i} style={{ color: l.stream === "stderr" ? "var(--warn)" : l.stream === "error" ? "var(--bad)" : undefined }}>
              <span style={{ color: COLORS[l.source % COLORS.length], fontWeight: 600 }}>{(sources[l.source]?.name ?? "?").padEnd(width)} │ </span>
              {l.text}
            </span>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
