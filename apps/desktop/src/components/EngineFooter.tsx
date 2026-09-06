import { useState } from "react";
import { useTranslation } from "react-i18next";
import { engine } from "../api";
import { useEngine } from "../engine";
import { IconPlay, IconRestart, IconStop } from "./Icons";

/** Pied de la barre latérale : état du moteur (point + libellé) et commandes démarrer / redémarrer / arrêter.
 *  En mode compact (barre repliée), seul le point d'état reste visible ; l'infobulle porte le libellé. */
export function EngineFooter({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation();
  const { snapshot, serviceAvailable } = useEngine();
  const [busy, setBusy] = useState(false);
  const state = !serviceAvailable ? "service_unavailable" : (snapshot?.state ?? "stopped");
  const tone = state === "ready" ? "pill-ok" : state === "starting" || state === "stopping" || state === "degraded" ? "pill-warn" : state === "failed" || state === "service_unavailable" ? "pill-bad" : "pill-muted";
  // Libellé court : le pied de la barre latérale n'a pas la place de « Moteur en marche ».
  const label = t(`engine.short.${state}`, { defaultValue: t(`engine.state.${state}`) });
  const boot = snapshot?.state === "ready" && snapshot.last_boot_ms != null ? t("engine.boot_time", { seconds: (snapshot.last_boot_ms / 1000).toFixed(1) }) : "";

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
    <div className={`engine-footer ${compact ? "is-compact" : ""}`} role="status" aria-live="polite" title={boot ? `${t(`engine.state.${state}`)} — ${boot}` : t(`engine.state.${state}`)}>
      <span className={`pill pill-dot ${tone}`} aria-hidden="true" />
      {compact && <span className="sr-only">{label}</span>}
      {!compact && <span className="engine-footer-label">{label}</span>}
      <span className="flex-1" />
      {!compact && serviceAvailable && (state === "stopped" || state === "failed") && (
        <button type="button" className="icon-btn" title={t("engine.start")} aria-label={t("engine.start")} disabled={busy} onClick={() => run(() => engine.start())}>
          <IconPlay />
        </button>
      )}
      {!compact && serviceAvailable && (state === "ready" || state === "degraded") && (
        <>
          <button type="button" className="icon-btn" title={t("engine.restart")} aria-label={t("engine.restart")} disabled={busy} onClick={() => run(() => engine.restart())}>
            <IconRestart />
          </button>
          <button type="button" className="icon-btn" title={t("engine.stop")} aria-label={t("engine.stop")} disabled={busy} onClick={() => run(() => engine.stop())}>
            <IconStop />
          </button>
        </>
      )}
    </div>
  );
}
