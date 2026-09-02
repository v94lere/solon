import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { containers, type LogChunk } from "../api";

const MAX_LINES = 5000;

/** Journaux en flux (Channel Tauri), fermés au démontage. */
export function LogsPanel({ id }: { id: string }) {
  const { t } = useTranslation();
  const [lines, setLines] = useState<LogChunk[]>([]);
  const [follow, setFollow] = useState(true);
  const [timestamps, setTimestamps] = useState(false);
  const [ended, setEnded] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);

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
    if (follow) bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines, follow]);

  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex items-center gap-4 border-b px-3 py-2" style={{ borderColor: "var(--line)" }}>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} /> {t("detail.logs.follow")}
        </label>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={timestamps} onChange={(e) => setTimestamps(e.target.checked)} /> {t("detail.logs.timestamps")}
        </label>
        <div className="flex-1" />
        {ended && <span className="kbd-hint">{t("detail.terminal.disconnected")}</span>}
        <button type="button" className="btn btn-sm" onClick={() => setLines([])}>
          {t("detail.logs.clear")}
        </button>
      </div>
      <div
        ref={boxRef}
        className="mono min-h-0 flex-1 overflow-auto p-3 text-xs leading-5 whitespace-pre-wrap"
        onWheel={(e) => {
          // Seul un défilement volontaire vers le haut suspend le suivi (pas l'auto-défilement).
          if (e.deltaY < 0 && follow) setFollow(false);
        }}
        aria-live="polite"
      >
        {lines.length === 0 ? (
          <span style={{ color: "var(--ink-3)" }}>{t("detail.logs.empty")}</span>
        ) : (
          lines.map((l, i) => (
            <span key={i} style={{ color: l.stream === "stderr" ? "var(--warn)" : l.stream === "error" ? "var(--bad)" : undefined }}>
              {l.text}
            </span>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
