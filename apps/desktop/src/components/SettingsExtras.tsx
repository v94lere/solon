// Cartes des Réglages : nouvelles versions (vérification volontaire) et disque (place occupée par le
// moteur, place libre sur le lecteur Windows, bouton « Récupérer l'espace »).
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { containers, engine, formatBytes, host, images, type HostDiskInfo, type MachineMetrics, type ReclaimReport } from "../api";
import { reclaimSpace } from "../reclaim";
import { useEngine } from "../engine";
import { imageUsage } from "../usage";
import { checkForUpdate, lastCheck, RELEASES_URL, setUpdateCheckEnabled, updateCheckEnabled, type UpdateInfo } from "../updates";
import { ConfirmDialog } from "./ConfirmDialog";
import { MiniMeter } from "./ui";
import { lowDiskThreshold } from "./Notices";

export function UpdatesCard() {
  const { t, i18n } = useTranslation();
  const [enabled, setEnabled] = useState(updateCheckEnabled());
  const [info, setInfo] = useState<UpdateInfo | null>(lastCheck());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function check() {
    setBusy(true);
    setError(null);
    try {
      setInfo(await checkForUpdate());
    } catch (e) {
      setError(t("updates.failed", { detail: String(e instanceof Error ? e.message : e) }));
    } finally {
      setBusy(false);
    }
  }

  const checkedAt = info ? new Date(info.checkedAt).toLocaleString(i18n.language.startsWith("fr") ? "fr-FR" : "en-GB", { dateStyle: "medium", timeStyle: "short" }) : null;
  return (
    <section className="card mt-4 p-4">
      <h2 className="font-semibold">{t("updates.title")}</h2>
      <label className="mt-2 flex items-center gap-2">
        <input type="checkbox" checked={enabled} onChange={(e) => { setEnabled(e.target.checked); setUpdateCheckEnabled(e.target.checked); }} />
        {t("updates.check_at_launch")}
      </label>
      <p className="mt-1" style={{ color: "var(--ink-2)" }}>{t("updates.privacy")}</p>
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <button type="button" className="btn" disabled={busy} onClick={() => void check()}>{busy ? t("updates.checking") : t("updates.check_now")}</button>
        {info && !info.available && <span style={{ color: "var(--ok)" }}>{t("updates.up_to_date", { version: info.current })}</span>}
        {info?.available && (
          <>
            <span>{t("updates.available", { version: info.latest, current: info.current })}</span>
            <button type="button" className="btn btn-primary btn-sm" onClick={() => void openUrl(info.installer ?? info.page)}>{t("updates.download")}</button>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => void openUrl(info.page)}>{t("updates.notes")}</button>
          </>
        )}
        {error && <span style={{ color: "var(--bad)" }}>{error}</span>}
      </div>
      {checkedAt && <p className="mt-2 kbd-hint">{t("updates.last_check", { date: checkedAt })} · <button type="button" className="link-btn" onClick={() => void openUrl(RELEASES_URL)}>{t("updates.all_releases")}</button></p>}
    </section>
  );
}

export function DiskCard() {
  const { t } = useTranslation();
  const { ready } = useEngine();
  const queryClient = useQueryClient();
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ReclaimReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hostInfo, setHostInfo] = useState<HostDiskInfo | null>(null);
  const metrics = useQuery<MachineMetrics>({ queryKey: ["metrics-disk"], queryFn: engine.metrics, enabled: ready, refetchInterval: 15_000 });
  const imageList = useQuery({ queryKey: ["images"], queryFn: images.list, enabled: ready });
  const all = useQuery({ queryKey: ["containers", true], queryFn: () => containers.list(true), enabled: ready });

  useEffect(() => {
    let alive = true;
    host.diskInfo().then((i) => { if (alive) setHostInfo(i); }).catch(() => {});
    return () => { alive = false; };
  }, [report]);

  // Estimation de ce que le nettoyage libère : la taille des images qu'aucun conteneur n'utilise
  // (les couches partagées entre images sont comptées plusieurs fois : c'est un ordre de grandeur).
  const unused = useMemo(() => {
    const list = imageList.data ?? [];
    const cs = all.data ?? [];
    const u = list.filter((img) => imageUsage(cs, img).total === 0);
    return { count: u.length, bytes: u.reduce((s, i) => s + (i.Size ?? 0), 0) };
  }, [imageList.data, all.data]);

  async function reclaim() {
    setConfirm(false);
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      const r = await reclaimSpace();
      setReport(r);
      await queryClient.invalidateQueries({ queryKey: ["images"] });
      await queryClient.invalidateQueries({ queryKey: ["metrics-disk"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const m = metrics.data;
  const usedPct = m && m.disk_total_bytes > 0 ? (m.disk_used_bytes / m.disk_total_bytes) * 100 : 0;
  const drivePct = hostInfo && hostInfo.drive_total_bytes > 0 ? ((hostInfo.drive_total_bytes - hostInfo.drive_free_bytes) / hostInfo.drive_total_bytes) * 100 : 0;
  const low = hostInfo ? hostInfo.drive_free_bytes < lowDiskThreshold(hostInfo) : false;

  return (
    <section className="card mt-4 p-4">
      <h2 className="font-semibold">{t("disk.title")}</h2>
      <div className="mt-3 grid gap-3">
        <div className="disk-row">
          <span className="disk-label">{t("disk.engine")}</span>
          <MiniMeter pct={usedPct} label={t("disk.engine")} />
          <span className="disk-value">{m ? t("disk.used_of", { used: formatBytes(m.disk_used_bytes), total: formatBytes(m.disk_total_bytes) }) : ready ? t("common.loading") : t("disk.engine_stopped")}</span>
        </div>
        <div className="disk-row">
          <span className="disk-label">{hostInfo ? t("disk.drive", { drive: hostInfo.drive }) : t("disk.drive", { drive: "C:" })}</span>
          <MiniMeter pct={drivePct} label={t("disk.drive", { drive: hostInfo?.drive ?? "C:" })} />
          <span className="disk-value" style={low ? { color: "var(--bad)" } : undefined}>{hostInfo ? t("disk.free_of", { free: formatBytes(hostInfo.drive_free_bytes), total: formatBytes(hostInfo.drive_total_bytes) }) : "—"}</span>
        </div>
        {hostInfo && <p className="kbd-hint">{t("disk.file", { size: formatBytes(hostInfo.data_disk_bytes), path: hostInfo.data_disk_path })}</p>}
        {low && <p style={{ color: "var(--bad)" }}>{t("disk.low_short")}</p>}
      </div>
      <p className="mt-3" style={{ color: "var(--ink-2)" }}>{t("disk.reclaim_help")}</p>
      <div className="mt-2 flex flex-wrap items-center gap-3">
        <button type="button" className="btn" disabled={!ready || busy} onClick={() => setConfirm(true)}>{busy ? t("disk.reclaiming") : t("disk.reclaim")}</button>
        {ready && unused.count > 0 && !report && <span style={{ color: "var(--ink-2)" }}>{t("disk.estimate", { count: unused.count, size: formatBytes(unused.bytes) })}</span>}
        {ready && unused.count === 0 && !report && imageList.data && <span style={{ color: "var(--ink-2)" }}>{t("disk.nothing_unused")}</span>}
        {report && <span style={{ color: "var(--ok)" }}>{t("disk.reclaimed", { images: report.images_removed, cache: report.build_cache_removed, size: formatBytes(report.space_reclaimed) })}</span>}
        {error && <span style={{ color: "var(--bad)" }}>{error}</span>}
      </div>
      <ConfirmDialog open={confirm} title={t("disk.confirm_title")} confirmLabel={t("disk.reclaim")} cancelLabel={t("common.cancel")} onConfirm={() => void reclaim()} onCancel={() => setConfirm(false)}>
        <p>{t("disk.confirm_body", { count: unused.count, size: formatBytes(unused.bytes) })}</p>
        <p className="mt-2" style={{ color: "var(--ink-2)" }}>{t("disk.confirm_keeps")}</p>
      </ConfirmDialog>
    </section>
  );
}
