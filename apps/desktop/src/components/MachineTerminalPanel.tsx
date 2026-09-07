import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { base64ToBytes, bytesToBase64, machineShell } from "../api";

/** Terminal dans la machine Linux du moteur (shell root), en section plein écran. La session reste
 *  ouverte tant que la section est montée ; « Nouvelle session » relance un shell. */
export function MachineTerminalPanel() {
  const { t } = useTranslation();
  const hostRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    if (!hostRef.current) return;
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
    // Les raccourcis de l'application passent devant le shell : Ctrl+K, Ctrl+B, Ctrl+`, Ctrl+1…7.
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
      .open(term.cols, term.rows, (out) => {
        if (out.kind === "data" && out.data) term.write(base64ToBytes(out.data));
        else if (out.kind === "end") {
          setStatus(t("detail.terminal.disconnected"));
          term.write("\r\n");
        } else if (out.kind === "error") setStatus(out.message ?? "error");
      })
      .then((id) => {
        if (closed) void machineShell.close(id);
        else {
          sessionId = id;
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
    // Suivi de la taille : la section peut être masquée puis réaffichée (Ctrl+`), le terminal se réajuste.
    const ro = new ResizeObserver(() => {
      if (!hostRef.current || hostRef.current.clientWidth === 0) {
        // Section masquée : rendre le focus au reste de l'application.
        term.blur();
        return;
      }
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
  }, [t, generation]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold">{t("machine.terminal")}</h1>
        <span className="kbd-hint">{t("machine.terminal_hint")}</span>
        <span className="flex-1" />
        {status && <span className="kbd-hint">{status}</span>}
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => setGeneration((g) => g + 1)}>
          {t("machine.new_session")}
        </button>
      </div>
      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-hidden">
        <div ref={hostRef} className="h-full p-1" style={{ background: "var(--surface)" }} />
      </div>
    </div>
  );
}
