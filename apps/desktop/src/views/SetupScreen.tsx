// Écran de première installation / démarrage : progression par étapes, erreurs actionnables.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { openPath } from "@tauri-apps/plugin-opener";
import { engine, type ProvisionStep } from "../api";
import { useEngine } from "../engine";

const STEPS: ProvisionStep[] = [
  "checking_prerequisites",
  "verifying_image",
  "preparing_data_disk",
  "cleaning_orphans",
  "creating_network",
  "creating_machine",
  "booting",
  "waiting_agent",
  "configuring_network",
  "waiting_engine",
];

export function SetupScreen() {
  const { t } = useTranslation();
  const { snapshot, serviceAvailable, serviceDetail } = useEngine();
  const [showDetails, setShowDetails] = useState(false);
  const state = snapshot?.state ?? "stopped";
  const currentIndex = snapshot?.step ? STEPS.indexOf(snapshot.step) : -1;

  return (
    <div className="mx-auto flex h-full max-w-xl flex-col justify-center gap-5 p-8">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">{t("app.name")}</h1>
        <p style={{ color: "var(--ink-2)" }}>{serviceAvailable ? t(`engine.state.${state}`) : t("engine.state.service_unavailable")}</p>
      </div>

      {!serviceAvailable && (
        <div className="card p-4" role="alert">
          <p className="font-semibold">{t("error.title")}</p>
          <p className="mt-1">{t("engine.service_hint")}</p>
          {serviceDetail && <p className="mono mt-2 text-xs" style={{ color: "var(--ink-3)" }}>{serviceDetail}</p>}
        </div>
      )}

      {serviceAvailable && (state === "starting" || state === "stopping") && (
        <ol className="card divide-y p-2" style={{ borderColor: "var(--line)" }} aria-label={t("engine.state.starting")}>
          {STEPS.map((step, i) => {
            const done = i < currentIndex;
            const active = i === currentIndex;
            return (
              <li key={step} className="flex items-center gap-3 px-2 py-1.5" aria-current={active ? "step" : undefined} style={{ opacity: done || active ? 1 : 0.5 }}>
                <span
                  aria-hidden
                  className="inline-block h-2.5 w-2.5 rounded-full"
                  style={{ background: done ? "var(--ok)" : active ? "var(--accent)" : "var(--line-strong)", animation: active ? "pulse 1.2s ease-in-out infinite" : undefined }}
                />
                <span style={{ fontWeight: active ? 600 : 400 }}>{t(`engine.step.${step}`)}</span>
              </li>
            );
          })}
        </ol>
      )}

      {serviceAvailable && state === "failed" && snapshot?.error && (
        <div className="card p-4" role="alert" style={{ borderColor: "var(--bad)" }}>
          <p className="font-semibold" style={{ color: "var(--bad)" }}>
            {t("error.title")}
          </p>
          <p className="mt-1">{t(`error.${snapshot.error.code}`, { defaultValue: t("error.INTERNAL") })}</p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button type="button" className="btn btn-primary" onClick={() => void engine.start()}>
              {t("engine.retry")}
            </button>
            <button type="button" className="btn" onClick={() => setShowDetails((v) => !v)} aria-expanded={showDetails}>
              {t("error.details")}
            </button>
            <button type="button" className="btn btn-ghost" onClick={() => void engine.logsDir().then((p) => openPath(p))}>
              {t("error.open_logs")}
            </button>
          </div>
          {showDetails && (
            <pre className="mono mt-3 max-h-40 overflow-auto rounded p-2 text-xs" style={{ background: "var(--surface-3)" }}>
              {snapshot.error.code}
              {"\n"}
              {snapshot.error.message}
              {snapshot.error.hresult != null ? `\nHRESULT 0x${snapshot.error.hresult.toString(16).toUpperCase().padStart(8, "0")}` : ""}
            </pre>
          )}
        </div>
      )}

      {serviceAvailable && state === "stopped" && (
        <div className="flex items-center gap-3">
          <button type="button" className="btn btn-primary" onClick={() => void engine.start()}>
            {t("engine.start")}
          </button>
          {snapshot?.recovered_from_crash && <span style={{ color: "var(--warn)" }}>{t("engine.recovered")}</span>}
        </div>
      )}

      <style>{`@keyframes pulse { 0%,100% { opacity: 1 } 50% { opacity: .35 } }`}</style>
    </div>
  );
}
