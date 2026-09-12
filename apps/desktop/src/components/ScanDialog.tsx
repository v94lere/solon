// « Chercher les projets sur ce PC » : le service lit la table des fichiers des disques NTFS (une à trois
// secondes par disque) et renvoie les dossiers qui contiennent un fichier Compose, un Dockerfile ou un
// devcontainer.json. On coche ceux à garder : ils deviennent des cartes « Pas encore démarré » ;
// un dossier sans Compose s'ouvre dans la galerie pour lui proposer un environnement.
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { scan, stacks, type FoundProject, type Probe, type ScanReport } from "../api";
import { loadRecentProjects, rememberProject, samePath } from "../projects";

export function ScanDialog({ open, onClose, onAdded, onSetup }: { open: boolean; onClose: () => void; onAdded: (dirs: string[]) => void; onSetup: (probe: Probe) => void }) {
  const { t } = useTranslation();
  const [report, setReport] = useState<ScanReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");

  useEffect(() => {
    if (!open) return;
    setReport(null);
    setError(null);
    setFilter("");
    setBusy(true);
    let alive = true;
    scan
      .run()
      .then((r) => {
        if (!alive) return;
        setReport(r);
        // Pré-cochés : les projets Compose pas encore connus de l'accueil.
        const known = loadRecentProjects();
        setChecked(new Set(r.projects.filter((p) => p.kinds.includes("compose") && !known.some((k) => samePath(k, p.dir))).map((p) => p.dir)));
      })
      .catch((e: unknown) => alive && setError(String(e)))
      .finally(() => alive && setBusy(false));
    return () => {
      alive = false;
    };
  }, [open]);

  const known = useMemo(() => loadRecentProjects(), [report]);
  const rows = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return (report?.projects ?? []).filter((p) => !f || p.dir.toLowerCase().includes(f));
  }, [report, filter]);

  function toggle(dir: string) {
    setChecked((s) => {
      const n = new Set(s);
      if (n.has(dir)) n.delete(dir);
      else n.add(dir);
      return n;
    });
  }

  async function setup(p: FoundProject) {
    try {
      const probe = await stacks.probe(p.dir);
      onSetup(probe);
    } catch (e) {
      setError(String(e));
    }
  }

  if (!open) return null;
  const kindLabel = (k: string) => t(`scan.kind_${k}`, { defaultValue: k });
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal modal-stacks" role="dialog" aria-modal="true" aria-label={t("scan.title")}>
        <div className="flex items-center gap-3 border-b px-4 py-3" style={{ borderColor: "var(--line)" }}>
          <span className="font-semibold">{t("scan.title")}</span>
          {report && <span className="kbd-hint">{t("scan.summary", { count: report.projects.length, drives: report.drives.join(", "), seconds: (report.ms / 1000).toFixed(1) })}</span>}
          <span className="flex-1" />
          {report && <input type="search" className="input w-56" placeholder={t("scan.filter")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("scan.filter")} />}
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>{t("common.close")}</button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {busy && <p style={{ color: "var(--ink-2)" }}>{t("scan.running")}</p>}
          {error && <p role="alert" style={{ color: "var(--bad)" }}>{error}</p>}
          {report && report.notes.length > 0 && <p className="kbd-hint mb-2">{report.notes.join(" · ")}</p>}
          {report && rows.length === 0 && !busy && <p style={{ color: "var(--ink-2)" }}>{t("scan.none")}</p>}
          {rows.length > 0 && (
            <table className="table scan-table">
              <thead>
                <tr>
                  <th style={{ width: 28 }} />
                  <th>{t("scan.col_folder")}</th>
                  <th>{t("scan.col_found")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {rows.map((p) => {
                  const compose = p.kinds.includes("compose");
                  const already = known.some((k) => samePath(k, p.dir));
                  return (
                    <tr key={p.dir}>
                      <td>{compose && !already && <input type="checkbox" checked={checked.has(p.dir)} onChange={() => toggle(p.dir)} aria-label={p.dir} />}</td>
                      <td className="mono scan-dir" title={p.dir}>{p.dir}{p.git && <span className="branch-badge mono">⎇ git</span>}</td>
                      <td>
                        <span className="flex flex-wrap gap-1">
                          {p.kinds.map((k) => <span key={k} className={`pill ${k === "compose" ? "pill-ok" : "pill-muted"}`}>{kindLabel(k)}</span>)}
                          {already && <span className="kbd-hint">{t("scan.already")}</span>}
                        </span>
                      </td>
                      <td className="text-right">
                        {!compose && <button type="button" className="btn btn-sm" onClick={() => void setup(p)}>{t("scan.setup")}</button>}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
        <div className="flex items-center gap-2 border-t px-4 py-3" style={{ borderColor: "var(--line)" }}>
          <span className="kbd-hint">{t("scan.privacy")}</span>
          <span className="flex-1" />
          <button type="button" className="btn btn-primary btn-sm" disabled={checked.size === 0} onClick={() => { const dirs = [...checked]; dirs.forEach(rememberProject); onAdded(dirs); }}>{t("scan.add", { count: checked.size })}</button>
        </div>
      </div>
    </div>
  );
}
