import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { base64ToBytes, bytesToBase64, machineShell } from "../api";

/** Terminal dans la machine Linux du moteur (shell root), dans une fenêtre modale. */
export function MachineTerminal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const hostRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !hostRef.current) return;
    setStatus(null);
    const dark = document.documentElement.dataset.theme === "dark" || (!document.documentElement.dataset.theme && matchMedia("(prefers-color-scheme: dark)").matches);
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: '"Cascadia Mono", Consolas, monospace',
      fontSize: 13,
      theme: dark ? { background: "#222222", foreground: "#f1f1f1", cursor: "#ff7a47" } : { background: "#ffffff", foreground: "#222222", cursor: "#ff7a47" },
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
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
    const ro = new ResizeObserver(() => {
      fit.fit();
      if (sessionId !== null) void machineShell.resize(sessionId, term.cols, term.rows);
    });
    ro.observe(hostRef.current);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);

    return () => {
      closed = true;
      window.removeEventListener("keydown", onKey);
      ro.disconnect();
      onData.dispose();
      onBinary.dispose();
      if (sessionId !== null) void machineShell.close(sessionId);
      term.dispose();
    };
  }, [open, onClose, t]);

  if (!open) return null;
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal modal-terminal" role="dialog" aria-modal="true" aria-label={t("machine.terminal")}>
        <div className="flex items-center gap-3 border-b px-3 py-2" style={{ borderColor: "var(--line)" }}>
          <span className="font-semibold">{t("machine.terminal")}</span>
          <span className="kbd-hint">{t("machine.terminal_hint")}</span>
          <span className="flex-1" />
          {status && <span className="kbd-hint">{status}</span>}
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>{t("common.close")}</button>
        </div>
        <div ref={hostRef} className="min-h-0 flex-1 p-1" style={{ background: "var(--surface)" }} />
      </div>
    </div>
  );
}
