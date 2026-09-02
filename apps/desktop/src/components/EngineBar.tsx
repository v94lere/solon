import { useState } from "react";
import { useTranslation } from "react-i18next";
import { engine } from "../api";
import { useEngine } from "../engine";

export function EngineBar() {
  const { t } = useTranslation();
  const { snapshot, serviceAvailable } = useEngine();
  const [busy, setBusy] = useState(false);
  const state = !serviceAvailable ? "service_unavailable" : (snapshot?.state ?? "stopped");
  const pill = state === "ready" ? "pill-ok" : state === "starting" || state === "stopping" || state === "degraded" ? "pill-warn" : state === "failed" || state === "service_unavailable" ? "pill-bad" : "pill-muted";

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } catch (e) {
      console.error(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4" style={{ borderColor: "var(--line)", background: "var(--surface)" }} role="banner">
      <span className={`pill ${pill}`} aria-live="polite">
        {t(`engine.state.${state}`)}
      </span>
      {snapshot?.state === "ready" && snapshot.last_boot_ms != null && (
        <span className="kbd-hint">{t("engine.boot_time", { seconds: (snapshot.last_boot_ms / 1000).toFixed(1) })}</span>
      )}
      <div className="flex-1" />
      {serviceAvailable && (state === "stopped" || state === "failed") && (
        <button type="button" className="btn btn-primary btn-sm" disabled={busy} onClick={() => run(() => engine.start())}>
          {t("engine.start")}
        </button>
      )}
      {serviceAvailable && (state === "ready" || state === "degraded") && (
        <>
          <button type="button" className="btn btn-sm" disabled={busy} onClick={() => run(() => engine.restart())}>
            {t("engine.restart")}
          </button>
          <button type="button" className="btn btn-sm" disabled={busy} onClick={() => run(() => engine.stop())}>
            {t("engine.stop")}
          </button>
        </>
      )}
    </header>
  );
}
