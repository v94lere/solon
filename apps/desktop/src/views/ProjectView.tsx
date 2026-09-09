import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openPath } from "@tauri-apps/plugin-opener";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { compose, containers, system, type ComposeProject, type ContainerSummary } from "../api";
import { markUserAction } from "../engine";
import { projectBaseName, projectDirOf, projectNameOf, rememberProject, samePath, serviceNameOf } from "../projects";
import { MultiLogsPanel } from "../components/MultiLogsPanel";
import { PortLinks } from "../components/PortLinks";
import { IconLogs, IconPlay, IconRestart, IconStop } from "../components/Icons";

function stateClass(state: string) {
  switch (state) {
    case "running": return "pill-ok";
    case "restarting": case "paused": return "pill-warn";
    case "dead": return "pill-bad";
    default: return "pill-muted";
  }
}

/** Un projet Compose : ses services, ses journaux mêlés, Up / Down / Rebuild, ouverture du dossier. */
export function ProjectView({ dir, autoUp = false, onBack, onOpenContainer }: { dir: string; autoUp?: boolean; onBack: () => void; onOpenContainer: (id: string) => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [project, setProject] = useState<ComposeProject | null | undefined>(undefined);
  const [busy, setBusy] = useState<string | null>(null);
  const [output, setOutput] = useState<{ kind: string; text: string }[]>([]);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const outputRef = useRef<HTMLPreElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [showOutput, setShowOutput] = useState(false);
  const query = useQuery({ queryKey: ["containers", true], queryFn: () => containers.list(true), refetchInterval: 5000 });

  const autoStarted = useRef(false);
  useEffect(() => {
    setProject(undefined);
    autoStarted.current = false;
    compose.detect(dir).then((p) => {
      setProject(p);
      if (p) rememberProject(dir);
      // Projet créé depuis la galerie : démarrage immédiat, une seule fois.
      if (p && autoUp && !autoStarted.current) {
        autoStarted.current = true;
        void run("up", ["up", "-d"]);
      }
    }).catch((e: unknown) => { setProject(null); setError(String(e)); });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dir, autoUp]);

  const services = useMemo<ContainerSummary[]>(() => {
    const list = (query.data ?? []).filter((c) => samePath(projectDirOf(c), dir) || (project?.name && projectNameOf(c) === project.name && !projectDirOf(c)));
    return list.sort((a, b) => serviceNameOf(a).localeCompare(serviceNameOf(b)));
  }, [query.data, dir, project]);
  const running = services.filter((c) => c.State === "running").length;
  const logSources = useMemo(() => services.map((c) => ({ id: c.Id, name: serviceNameOf(c) })), [services]);

  async function run(label: string, args: string[]) {
    setBusy(label);
    setError(null);
    setShowOutput(true);
    setOutput([]);
    setExitCode(null);
    try {
      const code = await compose.stream(dir, args, (c) => {
        if (c.kind === "exit") return;
        setOutput((prev) => (prev.length > 4000 ? prev.slice(prev.length - 4000) : prev).concat({ kind: c.kind, text: c.text }));
      });
      setExitCode(code);
      if (code !== 0) setError(t("project.exit_code", { code }));
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  useEffect(() => {
    outputRef.current?.scrollTo({ top: outputRef.current.scrollHeight });
  }, [output]);

  async function act(id: string, action: () => Promise<void>) {
    markUserAction(id);
    setBusy(id);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const name = project?.name || projectBaseName(dir);
  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 px-4 pt-4 pb-2">
        <button type="button" className="btn btn-ghost btn-sm" onClick={onBack}>← {t("project.back")}</button>
        <h1 className="text-lg font-semibold">{name}</h1>
        <span className={`pill pill-dot ${running > 0 ? "pill-ok" : "pill-muted"}`} aria-hidden="true" />
        <span className="kbd-hint">{t("projects.running_count", { running, total: services.length })}</span>
        <div className="flex-1" />
        <button type="button" className="btn btn-primary btn-sm" disabled={busy !== null || project === null} onClick={() => void run("up", ["up", "-d", "--remove-orphans"])}>{busy === "up" ? t("compose.running") : t("compose.up")}</button>
        <button type="button" className="btn btn-sm" disabled={busy !== null || project === null} onClick={() => void run("rebuild", ["up", "-d", "--build", "--remove-orphans"])}>{busy === "rebuild" ? t("compose.running") : t("project.rebuild")}</button>
        <button type="button" className="btn btn-sm" disabled={busy !== null || project === null} onClick={() => void run("down", ["down"])}>{busy === "down" ? t("compose.running") : t("compose.down")}</button>
        <span className="mx-1" />
        <button type="button" className="btn btn-sm" onClick={() => void openPath(dir)}>{t("project.explorer")}</button>
        <button type="button" className="btn btn-sm" onClick={() => void system.openInVsCode(dir).catch((e: unknown) => setError(String(e)))}>{t("project.vscode")}</button>
      </div>
      <div className="px-4 pb-2">
        <span className="mono kbd-hint" title={dir}>{dir}{project ? `\\${project.file}` : ""}</span>
        {project === null && <span className="ml-3" style={{ color: "var(--warn)" }}>{t("compose.not_found", { dir })}</span>}
      </div>
      {error && <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>}

      <div className="card mx-4 mb-3 overflow-auto" style={{ maxHeight: "40%" }}>
        {services.length === 0 ? (
          <p className="p-4" style={{ color: "var(--ink-2)" }}>{t("project.no_services")}</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("project.columns.service")}</th>
                <th>{t("project.columns.container")}</th>
                <th>{t("containers.columns.image")}</th>
                <th>{t("containers.columns.status")}</th>
                <th>{t("containers.columns.ports")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {services.map((c) => {
                const isRunning = c.State === "running";
                const cname = (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
                return (
                  <tr key={c.Id}>
                    <td className="font-medium">{serviceNameOf(c)}</td>
                    <td className="mono">{cname}</td>
                    <td className="mono max-w-[220px] truncate" title={c.Image}>{c.Image.slice(c.Image.lastIndexOf("/") + 1)}</td>
                    <td>
                      <span className={`pill pill-dot ${stateClass(c.State)}`} role="img" title={c.Status} aria-label={t(`containers.state.${c.State}`, { defaultValue: c.State })} />
                    </td>
                    <td className="mono"><PortLinks c={c} running={isRunning} /></td>
                    <td>
                      <div className="flex justify-end gap-0.5">
                        {isRunning ? (
                          <>
                            <button type="button" className="icon-btn" title={t("containers.actions.stop")} aria-label={t("containers.actions.stop")} disabled={busy !== null} onClick={() => void act(c.Id, () => containers.stop(c.Id))}><IconStop /></button>
                            <button type="button" className="icon-btn" title={t("containers.actions.restart")} aria-label={t("containers.actions.restart")} disabled={busy !== null} onClick={() => void act(c.Id, () => containers.restart(c.Id))}><IconRestart /></button>
                          </>
                        ) : (
                          <button type="button" className="icon-btn" title={t("containers.actions.start")} aria-label={t("containers.actions.start")} disabled={busy !== null} onClick={() => void act(c.Id, () => containers.start(c.Id))}><IconPlay /></button>
                        )}
                        <button type="button" className="icon-btn" title={t("containers.actions.logs")} aria-label={t("containers.actions.logs")} onClick={() => onOpenContainer(c.Id)}><IconLogs /></button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {showOutput && (
        <div className="card mx-4 mb-3">
          <div className="flex items-center gap-3 border-b px-3 py-1.5" style={{ borderColor: "var(--line)" }}>
            <span className="font-semibold text-xs uppercase tracking-wide" style={{ color: "var(--ink-2)" }}>{t("project.output")}</span>
            {busy && <span className="kbd-hint">{t("compose.running")}</span>}
            <span className="flex-1" />
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setShowOutput(false)}>{t("common.close")}</button>
          </div>
          <pre ref={outputRef} className="mono max-h-48 overflow-auto p-2 text-xs leading-5 whitespace-pre-wrap">
            {output.length === 0 && busy ? t("compose.running") : null}
            {output.map((c, i) => (
              <span key={i} style={{ color: c.kind === "stderr" ? "var(--ink-2)" : undefined }}>{c.text}</span>
            ))}
            {exitCode !== null && <span style={{ color: exitCode === 0 ? "var(--ok)" : "var(--bad)" }}>{`\n[${t("project.exit_code", { code: exitCode })}]`}</span>}
          </pre>
        </div>
      )}

      <div className="mx-4 mb-4 min-h-0 flex-1">
        <MultiLogsPanel sources={logSources} />
      </div>
    </div>
  );
}
