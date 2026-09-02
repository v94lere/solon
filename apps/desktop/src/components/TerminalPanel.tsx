import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { base64ToBytes, bytesToBase64, containers } from "../api";

/** Terminal exec hijacké : xterm.js ↔ Channel Tauri, redimensionnement du TTY. */
export function TerminalPanel({ id, running }: { id: string; running: boolean }) {
  const { t } = useTranslation();
  const hostRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !hostRef.current) return;
    const dark = matchMedia("(prefers-color-scheme: dark)").matches || document.documentElement.dataset.theme === "dark";
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: '"Cascadia Mono", Consolas, monospace',
      fontSize: 13,
      theme: dark ? { background: "#1e1e1e", foreground: "#f0f0f0" } : { background: "#ffffff", foreground: "#1b1b1b", cursor: "#1b1b1b" },
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(hostRef.current);
    fit.fit();
    let execId: number | null = null;
    let closed = false;
    const encoder = new TextEncoder();

    containers
      .execOpen(id, ["/bin/sh", "-c", "command -v bash >/dev/null 2>&1 && exec bash || exec sh"], term.cols, term.rows, (out) => {
        if (out.kind === "data" && out.data) term.write(base64ToBytes(out.data));
        else if (out.kind === "end") {
          setStatus(t("detail.terminal.disconnected"));
          term.write("\r\n");
        } else if (out.kind === "error") setStatus(out.message ?? "error");
      })
      .then((eid) => {
        if (closed) void containers.execClose(eid);
        else {
          execId = eid;
          term.focus();
        }
      })
      .catch((e: unknown) => setStatus(String(e)));

    const onData = term.onData((data) => {
      if (execId !== null) void containers.execInput(execId, bytesToBase64(encoder.encode(data)));
    });
    const onBinary = term.onBinary((data) => {
      if (execId !== null) {
        const bytes = Uint8Array.from(data, (ch) => ch.charCodeAt(0));
        void containers.execInput(execId, bytesToBase64(bytes));
      }
    });
    const ro = new ResizeObserver(() => {
      fit.fit();
      if (execId !== null) void containers.execResize(execId, term.cols, term.rows);
    });
    ro.observe(hostRef.current);

    return () => {
      closed = true;
      ro.disconnect();
      onData.dispose();
      onBinary.dispose();
      if (execId !== null) void containers.execClose(execId);
      term.dispose();
    };
  }, [open, id, t]);

  if (!running) {
    return (
      <div className="card flex h-full items-center justify-center" style={{ color: "var(--ink-2)" }}>
        {t("detail.terminal.stopped")}
      </div>
    );
  }
  if (!open) {
    return (
      <div className="card flex h-full items-center justify-center">
        <button type="button" className="btn btn-primary" onClick={() => setOpen(true)}>
          {t("detail.terminal.connect")}
        </button>
      </div>
    );
  }
  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div ref={hostRef} className="min-h-0 flex-1" style={{ background: "var(--surface)" }} />
      {status && (
        <div className="border-t px-3 py-1 text-xs" style={{ borderColor: "var(--line)", color: "var(--ink-2)" }}>
          {status}
        </div>
      )}
    </div>
  );
}
