import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { useQueryClient } from "@tanstack/react-query";
import { compose, type ComposeProject, type ComposeResult } from "../api";

const RECENT_KEY = "solon.compose.recent";

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as string[];
  } catch {
    return [];
  }
}

/** Projets Compose : choix d'un dossier, détection du fichier, up / down, sortie de la commande. */
export function ComposePanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [recent, setRecent] = useState<string[]>(loadRecent);
  const [project, setProject] = useState<ComposeProject | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<ComposeResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recent.slice(0, 8)));
    } catch {
      /* ignoré */
    }
  }, [recent]);

  async function pick(dir?: string) {
    setError(null);
    setResult(null);
    const chosen = dir ?? ((await open({ directory: true, multiple: false, title: t("compose.pick") })) as string | null);
    if (!chosen) return;
    try {
      const p = await compose.detect(chosen);
      if (!p) {
        setProject(null);
        setError(t("compose.not_found", { dir: chosen }));
        return;
      }
      setProject(p);
      setRecent((r) => [chosen, ...r.filter((x) => x !== chosen)]);
    } catch (e) {
      setError(String(e));
    }
  }

  async function run(label: string, args: string[]) {
    if (!project) return;
    setBusy(label);
    setError(null);
    try {
      const r = await compose.run(project.dir, args);
      setResult(r);
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="card mx-4 mb-3 p-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="chip">{t("compose.title")}</span>
        <button type="button" className="btn btn-primary" onClick={() => void pick()}>{t("compose.open")}</button>
        {recent.length > 0 && (
          <select className="input max-w-72" value="" onChange={(e) => { if (e.target.value) void pick(e.target.value); }} aria-label={t("compose.recent")}>
            <option value="">{t("compose.recent")}</option>
            {recent.map((r) => <option key={r} value={r}>{r}</option>)}
          </select>
        )}
        {project && (
          <>
            <span className="mono kbd-hint truncate max-w-[360px]" title={project.dir}>{project.dir}\{project.file}</span>
            <span className="pill pill-muted">{project.name}</span>
            <div className="flex-1" />
            <button type="button" className="btn btn-primary btn-sm" disabled={busy !== null} onClick={() => void run("up", ["up", "-d", "--remove-orphans"])}>{busy === "up" ? t("compose.running") : t("compose.up")}</button>
            <button type="button" className="btn btn-sm" disabled={busy !== null} onClick={() => void run("down", ["down"])}>{busy === "down" ? t("compose.running") : t("compose.down")}</button>
            <button type="button" className="btn btn-ghost btn-sm" disabled={busy !== null} onClick={() => void run("ps", ["ps"])}>{t("compose.ps")}</button>
          </>
        )}
      </div>
      {error && <p className="mt-2" role="alert" style={{ color: "var(--bad)" }}>{error}</p>}
      {result && (
        <pre className="mono mt-2 max-h-48 overflow-auto rounded p-2 text-xs leading-5 whitespace-pre-wrap" style={{ background: "var(--surface-3)", color: result.code === 0 ? undefined : "var(--warn)" }}>
          {result.output.trim() || t("compose.no_output")}{"\n"}[{t("compose.exit", { code: result.code ?? "?", seconds: (result.ms / 1000).toFixed(1) })}]
        </pre>
      )}
    </div>
  );
}
