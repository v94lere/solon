// « Importer depuis Docker Desktop » : on liste ses volumes et ses images, on coche, on copie.
// Rien n'est touché du côté de Docker Desktop, et ce qui porte déjà le même nom dans Solon n'est
// jamais écrasé. Les conteneurs ne sont pas importés : ils se recréent depuis le fichier Compose.
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatBytes, importer, type ImportItem, type ImportProgress, type ImportReport, type ImportSource } from "../api";

export function ImportDialog({ open, onClose, onDone }: { open: boolean; onClose: () => void; onDone: () => void }) {
  const { t } = useTranslation();
  const [source, setSource] = useState<ImportSource | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [volumes, setVolumes] = useState<Set<string>>(new Set());
  const [images, setImages] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!open) return;
    setSource(null);
    setError(null);
    setReport(null);
    setProgress(null);
    setBusy(true);
    let alive = true;
    importer
      .probe()
      .then((s) => {
        if (!alive) return;
        setSource(s);
        // Pré-cochés : les volumes, qui sont la seule chose qu'on ne peut pas refaire. Pas les
        // images : elles se retéléchargent, et elles pèsent lourd.
        setVolumes(new Set(s.volumes.filter((v) => !v.exists_here).map((v) => v.name)));
      })
      .catch((e: unknown) => alive && setError(String(e)))
      .finally(() => alive && setBusy(false));
    return () => {
      alive = false;
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose, busy]);

  const selectedBytes = useMemo(() => {
    const sum = (items: ImportItem[], set: Set<string>) => items.filter((i) => set.has(i.name)).reduce((s, i) => s + (i.size ?? 0), 0);
    return sum(source?.volumes ?? [], volumes) + sum(source?.images ?? [], images);
  }, [source, volumes, images]);

  function toggle(set: Set<string>, apply: (s: Set<string>) => void, name: string) {
    const next = new Set(set);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    apply(next);
  }

  async function run() {
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      const r = await importer.run([...volumes], [...images], setProgress);
      setReport(r);
      onDone();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  if (!open) return null;

  const rows = (items: ImportItem[], set: Set<string>, apply: (s: Set<string>) => void) =>
    items.map((i) => (
      <tr key={i.name}>
        <td>{!i.exists_here && <input type="checkbox" checked={set.has(i.name)} disabled={busy} onChange={() => toggle(set, apply, i.name)} aria-label={i.name} />}</td>
        <td className="mono scan-dir" title={i.name}>{i.name}</td>
        <td className="disk-value">{i.size === null ? "—" : formatBytes(i.size)}</td>
        <td>
          {i.exists_here && <span className="kbd-hint">{t("import.exists")}</span>}
          {!i.exists_here && i.in_use && <span className="pill pill-ok">{t("import.in_use")}</span>}
        </td>
      </tr>
    ));

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget && !busy) onClose(); }}>
      <div className="modal modal-stacks" role="dialog" aria-modal="true" aria-label={t("import.title")}>
        <div className="flex items-center gap-3 border-b px-4 py-3" style={{ borderColor: "var(--line)" }}>
          <span className="font-semibold">{t("import.title")}</span>
          {source?.available && <span className="kbd-hint">{t("import.found", { version: source.version ?? "?" })}</span>}
          <span className="flex-1" />
          <button type="button" className="btn btn-ghost btn-sm" disabled={busy} onClick={onClose}>{t("common.close")}</button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          {busy && !progress && <p style={{ color: "var(--ink-2)" }}>{t("import.looking")}</p>}
          {error && <p role="alert" style={{ color: "var(--bad)" }}>{error}</p>}

          {source && !source.available && (
            <div>
              <p>{t("import.unavailable")}</p>
              <p className="kbd-hint mt-2 mono">{source.reason}</p>
            </div>
          )}

          {report && (
            <div className="grid gap-2">
              <p style={{ color: "var(--ok)" }}>{t("import.done", { volumes: report.volumes_imported.length, images: report.images_imported.length })}</p>
              {report.volumes_imported.length > 0 && <p className="mono text-xs">{report.volumes_imported.join(", ")}</p>}
              {report.skipped.length > 0 && (
                <>
                  <p className="mt-2">{t("import.skipped")}</p>
                  <ul className="grid gap-1">
                    {report.skipped.map(([name, why]) => (
                      <li key={name} className="text-xs"><span className="mono">{name}</span> — <span style={{ color: "var(--ink-2)" }}>{why}</span></li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          )}

          {source?.available && !report && (
            <div className="grid gap-4">
              <p style={{ color: "var(--ink-2)" }}>{t("import.help")}</p>

              <section>
                <h3 className="font-semibold">{t("import.volumes")}</h3>
                {source.volumes.length === 0 ? (
                  <p className="kbd-hint">{t("import.no_volumes")}</p>
                ) : (
                  <table className="table scan-table">
                    <thead><tr><th style={{ width: 28 }} /><th>{t("import.col_name")}</th><th>{t("import.col_size")}</th><th /></tr></thead>
                    <tbody>{rows(source.volumes, volumes, setVolumes)}</tbody>
                  </table>
                )}
              </section>

              <section>
                <h3 className="font-semibold">{t("import.images")}</h3>
                <p className="kbd-hint">{t("import.images_hint")}</p>
                {source.images.length === 0 ? (
                  <p className="kbd-hint">{t("import.no_images")}</p>
                ) : (
                  <table className="table scan-table">
                    <thead><tr><th style={{ width: 28 }} /><th>{t("import.col_name")}</th><th>{t("import.col_size")}</th><th /></tr></thead>
                    <tbody>{rows(source.images, images, setImages)}</tbody>
                  </table>
                )}
              </section>
            </div>
          )}
        </div>

        <div className="flex items-center gap-3 border-t px-4 py-3" style={{ borderColor: "var(--line)" }}>
          {progress && progress.total > 0 ? (
            <>
              <span className="kbd-hint">{progress.label ? t("import.copying", { name: progress.label }) : t("import.finishing")}</span>
              <span className="mini-meter" style={{ maxWidth: 160 }} role="progressbar" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.done}>
                <i style={{ width: `${Math.round((progress.done / progress.total) * 100)}%` }} />
              </span>
              <span className="disk-value">{progress.done}/{progress.total}</span>
            </>
          ) : (
            <span className="kbd-hint">{t("import.safe")}</span>
          )}
          <span className="flex-1" />
          {source?.available && !report && (
            <button type="button" className="btn btn-primary btn-sm" disabled={busy || volumes.size + images.size === 0} onClick={() => void run()}>
              {t("import.run", { count: volumes.size + images.size, size: formatBytes(selectedBytes) })}
            </button>
          )}
          {report && <button type="button" className="btn btn-primary btn-sm" onClick={onClose}>{t("common.close")}</button>}
        </div>
      </div>
    </div>
  );
}
