import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { compose } from "../api";
import { loadRecentProjects, projectBaseName, rememberProject } from "../projects";

/** Raccourci vers les projets Compose depuis la liste des conteneurs : ouvrir un dossier ou un projet récent. */
export function ComposePanel({ onOpenProject }: { onOpenProject: (dir: string) => void }) {
  const { t } = useTranslation();
  const [recent] = useState<string[]>(loadRecentProjects);
  const [error, setError] = useState<string | null>(null);

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
      onOpenProject(chosen);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="card mx-4 mb-3 p-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="chip">{t("compose.title")}</span>
        <button type="button" className="btn btn-primary" onClick={() => void pick()}>{t("compose.open")}</button>
        {recent.length > 0 && (
          <select className="input max-w-72" value="" onChange={(e) => { if (e.target.value) onOpenProject(e.target.value); }} aria-label={t("compose.recent")}>
            <option value="">{t("compose.recent")}</option>
            {recent.map((r) => <option key={r} value={r}>{projectBaseName(r)} — {r}</option>)}
          </select>
        )}
      </div>
      {error && <p className="mt-2" role="alert" style={{ color: "var(--bad)" }}>{error}</p>}
    </div>
  );
}
