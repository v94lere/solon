// Bandeaux en haut de la fenêtre : nouvelle version disponible, disque Windows presque plein.
// Discrets : chacun se ferme d'un clic et ne revient pas pour la même version / la même session.
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { formatBytes, host, type HostDiskInfo } from "../api";
import { useEngine } from "../engine";
import { checkForUpdate, skipVersion, skippedVersion, updateCheckEnabled, type UpdateInfo } from "../updates";

/** Sous ce seuil (5 % du lecteur, au moins 5 Gio), Solon prévient : le disque de données grossit
 *  dans ce lecteur et Windows lui-même se dégrade quand il est plein. */
export function lowDiskThreshold(info: HostDiskInfo): number {
  return Math.max(5 * 1024 ** 3, Math.floor(info.drive_total_bytes * 0.05));
}

export function Notices({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const { ready } = useEngine();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [disk, setDisk] = useState<HostDiskInfo | null>(null);
  const [diskDismissed, setDiskDismissed] = useState(false);

  // Une vérification par lancement, dix secondes après l'ouverture, si l'utilisateur l'accepte.
  useEffect(() => {
    if (!updateCheckEnabled()) return;
    const timer = window.setTimeout(() => {
      checkForUpdate()
        .then((info) => {
          if (info.available && skippedVersion() !== info.latest) setUpdate(info);
        })
        .catch(() => {});
    }, 10_000);
    return () => window.clearTimeout(timer);
  }, []);

  // Place sur le lecteur Windows : au lancement puis toutes les dix minutes, quand le moteur tourne.
  useEffect(() => {
    if (!ready) return;
    let alive = true;
    const probe = () => {
      host
        .diskInfo()
        .then((info) => {
          if (alive) setDisk(info);
        })
        .catch(() => {});
    };
    probe();
    const id = window.setInterval(probe, 10 * 60_000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [ready]);

  const lowDisk = disk && !diskDismissed && disk.drive_free_bytes < lowDiskThreshold(disk) ? disk : null;
  if (!update && !lowDisk) return null;
  return (
    <div className="notices">
      {update && (
        <div className="notice notice-bar" role="status">
          <span>{t("updates.available", { version: update.latest, current: update.current })}</span>
          <button type="button" className="btn btn-primary btn-sm" onClick={() => void openUrl(update.installer ?? update.page)}>{t("updates.download")}</button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => void openUrl(update.page)}>{t("updates.notes")}</button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => { skipVersion(update.latest); setUpdate(null); }}>{t("updates.later")}</button>
        </div>
      )}
      {lowDisk && (
        <div className="notice notice-bar is-warn" role="alert">
          <span>{t("disk.low", { drive: lowDisk.drive, free: formatBytes(lowDisk.drive_free_bytes), engine: formatBytes(lowDisk.data_disk_bytes) })}</span>
          <button type="button" className="btn btn-sm" onClick={onOpenSettings}>{t("disk.reclaim")}</button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => setDiskDismissed(true)}>{t("common.close")}</button>
        </div>
      )}
    </div>
  );
}
