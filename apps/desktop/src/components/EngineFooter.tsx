import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { engine } from "../api";
import { useEngine } from "../engine";
import { useEngineLoad } from "../metrics";
import { IconPlay, IconRestart, IconStop } from "./Icons";
import { MiniMeter } from "./ui";

function formatUptime(sinceMs: number, now: number): string {
  const s = Math.max(0, Math.floor((now - sinceMs) / 1000));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m.toString().padStart(2, "0")}`;
  return `${m} min`;
}

/** Bloc « moteur » en bas de la barre latérale : état, temps de fonctionnement, mini-jauges processeur
 *  et mémoire, commandes démarrer / redémarrer / arrêter. En mode compact (barre repliée), seul le
 *  point d'état reste, le libellé en infobulle. */
export function EngineFooter({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation();
  const { snapshot, serviceAvailable, ready } = useEngine();
  const [busy, setBusy] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const load = useEngineLoad(ready && !compact, 3000);
  const state = !serviceAvailable ? "service_unavailable" : (snapshot?.state ?? "stopped");
  const tone = state === "ready" ? "pill-ok" : state === "starting" || state === "stopping" || state === "degraded" ? "pill-warn" : state === "failed" || state === "service_unavailable" ? "pill-bad" : "pill-muted";
  const label = t(`engine.short.${state}`, { defaultValue: t(`engine.state.${state}`) });
  const since = snapshot?.state === "ready" ? snapshot.ready_since_unix_ms : null;

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(id);
  }, []);

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

  const fullLabel = t(`engine.state.${state}`);
  if (compact) {
    return (
      <div className="engine-footer is-compact" role="status" aria-live="polite" title={fullLabel}>
        <span className={`pill pill-dot ${tone}`} aria-hidden="true" />
        <span className="sr-only">{label}</span>
      </div>
    );
  }

  return (
    <div className="engine-block" role="status" aria-live="polite">
      <div className="engine-row">
        <span className={`pill pill-dot ${tone}`} aria-hidden="true" />
        <span className="engine-footer-label font-medium" style={{ color: "var(--ink)" }}>{label}</span>
        <span className="flex-1" />
        {serviceAvailable && (state === "stopped" || state === "failed") && (
          <button type="button" className="icon-btn" title={t("engine.start")} aria-label={t("engine.start")} disabled={busy} onClick={() => run(() => engine.start())}>
            <IconPlay />
          </button>
        )}
        {serviceAvailable && (state === "ready" || state === "degraded") && (
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
      {ready && (
        <>
          <div className="engine-row engine-meters">
            <span className="engine-meter-label">CPU</span>
            <MiniMeter pct={load?.cpuPct ?? 0} label={t("activity.cpu")} />
            <span className="engine-meter-value">{load ? `${load.cpuPct.toFixed(0)} %` : "—"}</span>
          </div>
          <div className="engine-row engine-meters">
            <span className="engine-meter-label">RAM</span>
            <MiniMeter pct={load?.memPct ?? 0} label={t("activity.memory")} />
            <span className="engine-meter-value">{load ? `${load.memPct.toFixed(0)} %` : "—"}</span>
          </div>
          {since != null && <div className="engine-uptime">{t("engine.uptime", { value: formatUptime(since, now) })}</div>}
        </>
      )}
    </div>
  );
}
