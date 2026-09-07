import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { base64ToBytes, bytesToBase64, machineShell } from "../api";

/** Shell de débogage : boîte à outils lancée dans les espaces de noms du conteneur (processus, réseau,
 *  volumes), même si l'image n'a pas de shell. Passe par le terminal de la machine (`solon-debug`). */
export function DebugPanel({ id, running }: { id: string; running: boolean }) {
  const { t } = useTranslation();
  const hostRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    if (!running || !hostRef.current) return;
    setStatus(null);
    const dark = document.documentElement.dataset.theme === "dark" || (!document.documentElement.dataset.theme && matchMedia("(prefers-color-scheme: dark)").matches);
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: '"Cascadia Mono", Consolas, monospace',
      fontSize: 13,
      theme: dark ? { background: "#121212", foreground: "#ffffff", cursor: "#3c82c3" } : { background: "#ffffff", foreground: "#0a0a0a", cursor: "#3c82c3" },
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.attachCustomKeyEventHandler((e) => {
      if (!e.ctrlKey || e.altKey) return true;
      const k = e.key.toLowerCase();
      return !(k === "k" || k === "b" || e.key === "`" || e.code === "Backquote" || /^[1-7]$/.test(e.key));
    });
    term.open(hostRef.current);
    fit.fit();
    let sessionId: number | null = null;
    let closed = false;
    const encoder = new TextEncoder();

    machineShell
      .open(
        term.cols,
        term.rows,
        (out) => {
          if (out.kind === "data" && out.data) term.write(base64ToBytes(out.data));
          else if (out.kind === "end") {
            setStatus(t("detail.terminal.disconnected"));
            term.write("\r\n");
          } else if (out.kind === "error") setStatus(out.message ?? "error");
        },
        `exec solon-debug ${id}`,
      )
      .then((sid) => {
        if (closed) void machineShell.close(sid);
        else {
          sessionId = sid;
          term.focus();
        }
      })
      .catch((e: unknown) => setStatus(String(e)));

    const onData = term.onData((data) => {
      if (sessionId !== null) void machineShell.input(sessionId, bytesToBase64(encoder.encode(data)));
    });
    const onBinary = term.onBinary((data) => {
      if (sessionId !== null) void machineShell.input(sessionId, bytesToBase64(Uint8Array.from(data, (ch) => ch.charCodeAt(0))));
    });
    const ro = new ResizeObserver(() => {
      if (!hostRef.current || hostRef.current.clientWidth === 0) return;
      fit.fit();
      if (sessionId !== null) void machineShell.resize(sessionId, term.cols, term.rows);
    });
    ro.observe(hostRef.current);

    return () => {
      closed = true;
      ro.disconnect();
      onData.dispose();
      onBinary.dispose();
      if (sessionId !== null) void machineShell.close(sessionId);
      term.dispose();
    };
  }, [id, running, generation, t]);

  if (!running) {
    return (
      <div className="card flex h-full items-center justify-center p-6" style={{ color: "var(--ink-2)" }}>
        {t("detail.debug.stopped")}
      </div>
    );
  }
  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex items-center gap-3 border-b px-3 py-2" style={{ borderColor: "var(--line)" }}>
        <span className="kbd-hint">{t("detail.debug.hint")}</span>
        <span className="flex-1" />
        {status && <span className="kbd-hint">{status}</span>}
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => setGeneration((g) => g + 1)}>
          {t("machine.new_session")}
        </button>
      </div>
      <div ref={hostRef} className="min-h-0 flex-1 p-1" style={{ background: "var(--surface)" }} />
    </div>
  );
}
