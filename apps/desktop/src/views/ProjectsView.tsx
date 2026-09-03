import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { useQuery } from "@tanstack/react-query";
import { compose, containers } from "../api";
import { forgetProject, loadRecentProjects, projectBaseName, projectDirOf, rememberProject } from "../projects";
import { IconTrash } from "../components/Icons";

interface Row {
  dir: string;
  name: string;
  total: number;
  running: number;
  recent: boolean;
}

/** Liste des projets Compose : ouverts récemment et détectés depuis les conteneurs présents. */
export function ProjectsView({ onOpen }: { onOpen: (dir: string) => void }) {
  const { t } = useTranslation();
  const [recent, setRecent] = useState<string[]>(loadRecentProjects);
  const [error, setError] = useState<string | null>(null);
  const query = useQuery({ queryKey: ["containers", true], queryFn: () => containers.list(true) });

  const rows = useMemo<Row[]>(() => {
    const byDir = new Map<string, Row>();
    for (const dir of recent) byDir.set(dir.toLowerCase(), { dir, name: projectBaseName(dir), total: 0, running: 0, recent: true });
    for (const c of query.data ?? []) {
      const dir = projectDirOf(c);
      if (!dir) continue;
      const k = dir.toLowerCase();
      const row = byDir.get(k) ?? { dir, name: projectBaseName(dir), total: 0, running: 0, recent: false };
      row.total += 1;
      if (c.State === "running") row.running += 1;
      byDir.set(k, row);
    }
    return [...byDir.values()].sort((a, b) => b.running - a.running || a.name.localeCompare(b.name));
  }, [recent, query.data]);

  async function pick() {
    setError(null);
    const chosen = (await open({ directory: true, multiple: false, title: t("compose.pick") })) as string | null;
    if (!chosen) return;
    try {
      const p = await compose.detect(chosen);
      if (!p) {
        setError(t("compose.not_found", { dir: chosen }));
        return;
      }
      rememberProject(chosen);
      setRecent(loadRecentProjects());
      onOpen(chosen);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold">{t("projects.title")}</h1>
        <span className="kbd-hint">{t("projects.count", { count: rows.length })}</span>
        <div className="flex-1" />
        <button type="button" className="btn btn-primary" onClick={() => void pick()}>{t("compose.open")}</button>
      </div>
      {error && <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>}
      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {rows.length === 0 ? (
          <p className="p-6 text-center" style={{ color: "var(--ink-2)" }}>{t("projects.empty")}</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("projects.columns.name")}</th>
                <th>{t("projects.columns.folder")}</th>
                <th>{t("projects.columns.services")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.dir} tabIndex={0} onDoubleClick={() => onOpen(r.dir)} onKeyDown={(e) => { if (e.key === "Enter") onOpen(r.dir); }}>
                  <td>
                    <button type="button" className="font-medium hover:underline" style={{ color: "var(--ink)" }} onClick={() => onOpen(r.dir)}>{r.name}</button>
                  </td>
                  <td className="mono max-w-[420px] truncate" title={r.dir}>{r.dir}</td>
                  <td>
                    {r.total === 0 ? (
                      <span className="kbd-hint">{t("projects.not_started")}</span>
                    ) : (
                      <span className="flex items-center gap-2">
                        <span className={`pill pill-dot ${r.running > 0 ? "pill-ok" : "pill-muted"}`} aria-hidden="true" />
                        {t("projects.running_count", { running: r.running, total: r.total })}
                      </span>
                    )}
                  </td>
                  <td>
                    <div className="flex justify-end gap-0.5">
                      <button type="button" className="btn btn-sm" onClick={() => onOpen(r.dir)}>{t("projects.open")}</button>
                      {r.recent && r.total === 0 && (
                        <button type="button" className="icon-btn" title={t("projects.forget")} aria-label={t("projects.forget")} onClick={() => { forgetProject(r.dir); setRecent(loadRecentProjects()); }}>
                          <IconTrash />
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
