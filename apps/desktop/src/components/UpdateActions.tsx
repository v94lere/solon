// Boutons d'une nouvelle version : « Installer la mise à jour » (téléchargement, vérification SHA-256,
// lancement de l'installateur après confirmation), notes de version, et « Plus tard » quand il y a lieu.
// Partagé par le bandeau du haut (Notices) et la carte Mises à jour des Réglages.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { formatBytes } from "../api";
import { canInstall, downloadUpdate, installUpdate, type UpdateInfo, type UpdateProgress } from "../updates";
import { ConfirmDialog } from "./ConfirmDialog";

type Phase =
  | { kind: "idle" }
  | { kind: "confirm" }
  | { kind: "downloading"; received: number; total: number | null }
  | { kind: "verifying" }
  | { kind: "installing" }
  | { kind: "error"; detail: string };

export function UpdateActions({ info, onLater }: { info: UpdateInfo; onLater?: () => void }) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  async function run() {
    setPhase({ kind: "downloading", received: 0, total: null });
    try {
      const path = await downloadUpdate(info, (p: UpdateProgress) => {
        setPhase(p.kind === "verifying" ? { kind: "verifying" } : { kind: "downloading", received: p.received, total: p.total });
      });
      setPhase({ kind: "installing" });
      await installUpdate(path);
    } catch (e) {
      setPhase({ kind: "error", detail: String(e instanceof Error ? e.message : e) });
    }
  }

  const busy = phase.kind === "downloading" || phase.kind === "verifying" || phase.kind === "installing";
  const pct = phase.kind === "downloading" && phase.total ? Math.min(100, Math.round((phase.received / phase.total) * 100)) : null;

  return (
    <>
      {phase.kind === "idle" || phase.kind === "confirm" || phase.kind === "error" ? (
        <>
          {phase.kind === "error" && <span style={{ color: "var(--bad)" }}>{t("updates.install_failed", { detail: phase.detail })}</span>}
          {canInstall(info) ? (
            <button type="button" className="btn btn-primary btn-sm" onClick={() => setPhase({ kind: "confirm" })}>
              {phase.kind === "error" ? t("updates.retry") : t("updates.install")}
            </button>
          ) : null}
          <button type="button" className={`btn btn-sm ${canInstall(info) ? "btn-ghost" : "btn-primary"}`} onClick={() => void openUrl(info.installer ?? info.page)}>
            {t("updates.download")}
          </button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => void openUrl(info.page)}>{t("updates.notes")}</button>
          {onLater && phase.kind !== "error" && (
            <button type="button" className="btn btn-ghost btn-sm" onClick={onLater}>{t("updates.later")}</button>
          )}
        </>
      ) : (
        <span className="update-progress" role="status" aria-live="polite">
          <span>
            {phase.kind === "downloading" &&
              (phase.total
                ? t("updates.downloading_size", { done: formatBytes(phase.received), total: formatBytes(phase.total) })
                : t("updates.downloading"))}
            {phase.kind === "verifying" && t("updates.verifying")}
            {phase.kind === "installing" && t("updates.installing")}
          </span>
          {busy && (
            <span className="mini-meter" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct ?? undefined}>
              <i className={pct === null ? "is-indeterminate" : ""} style={{ width: `${pct ?? 100}%` }} />
            </span>
          )}
        </span>
      )}
      <ConfirmDialog
        open={phase.kind === "confirm"}
        title={t("updates.install_title", { version: info.latest })}
        confirmLabel={t("updates.install_confirm")}
        cancelLabel={t("common.cancel")}
        onConfirm={() => void run()}
        onCancel={() => setPhase({ kind: "idle" })}
      >
        <p>{t("updates.install_body")}</p>
        <p className="mt-2" style={{ color: "var(--ink-2)" }}>{t("updates.install_keeps")}</p>
      </ConfirmDialog>
    </>
  );
}
