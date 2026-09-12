import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openPath } from "@tauri-apps/plugin-opener";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { compose, containers, host, system, type ComposeProject, type ContainerSummary } from "../api";
import { portConflictIn } from "../ports";
import { markUserAction, useEngine } from "../engine";
import { EnvPanel } from "../components/EnvPanel";
import { AddressLine } from "./ProjectsView";
import { primaryAddress, projectBaseName, projectDirOf, projectNameOf, rememberProject, samePath, serviceNameOf } from "../projects";
import { MultiLogsPanel } from "../components/MultiLogsPanel";
import { PortLinks } from "../components/PortLinks";
import { IconFile, IconLogs, IconPencil, IconPlay, IconRestart, IconStop } from "../components/Icons";

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
  const [tab, setTab] = useState<"logs" | "compose" | "env">("logs");
  const { snapshot } = useEngine();
  const [yaml, setYaml] = useState("");
  const [savedYaml, setSavedYaml] = useState("");
  const [saving, setSaving] = useState(false);
  const dirty = yaml !== savedYaml;

  // Le fichier Compose est lu dès que le projet est reconnu (et relu à la demande).
  async function loadYaml() {
    try {
      const text = await compose.read(dir);
      setYaml(text);
      setSavedYaml(text);
    } catch (e) {
      setError(String(e));
    }
  }
  useEffect(() => {
    if (project) void loadYaml();
    else { setYaml(""); setSavedYaml(""); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project]);

  async function saveYaml(): Promise<boolean> {
    setSaving(true);
    setError(null);
    try {
      await compose.write(dir, yaml);
      setSavedYaml(yaml);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setSaving(false);
    }
  }

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
  const addr = snapshot?.local_domains ? primaryAddress(services, !!snapshot?.local_domains_tls) : null;
  const ownPorts = useMemo(() => [...new Set(services.flatMap((c) => (c.Ports ?? []).map((p) => p.PublicPort ?? 0).filter((p) => p > 0)))], [services]);
  const logSources = useMemo(() => services.map((c) => ({ id: c.Id, name: serviceNameOf(c) })), [services]);

  async function run(label: string, args: string[]) {
    setBusy(label);
    setError(null);
    setShowOutput(true);
    setOutput([]);
    setExitCode(null);
    const collected: string[] = [];
    try {
      const code = await compose.stream(dir, args, (c) => {
        if (c.kind === "exit") return;
        if (collected.length < 2000) collected.push(c.text);
        setOutput((prev) => (prev.length > 4000 ? prev.slice(prev.length - 4000) : prev).concat({ kind: c.kind, text: c.text }));
      });
      setExitCode(code);
      if (code !== 0) {
        // Port hôte déjà pris : la cause la plus fréquente d'un Up qui échoue ; on nomme le port,
        // on propose le suivant libre et on indique où le changer.
        const port = portConflictIn(collected.join(""));
        if (port) {
          const probe = await host.portsProbe([port]).catch(() => []);
          const next = probe[0]?.suggestion ?? port + 1;
          setError(t("project.port_conflict", { port, next }));
        } else {
          setError(t("project.exit_code", { code }));
        }
      }
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
      {addr && running > 0 && (
        <div className="px-4 pb-3">
          <AddressLine url={addr.url} host={addr.host} big />
        </div>
      )}
      {error && <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>}

      <div className="card list-card mx-4 mb-3 overflow-auto" style={{ maxHeight: "40%" }}>
        {services.length === 0 ? (
          <p className="p-4" style={{ color: "var(--ink-2)" }}>{t("project.no_services")}</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("project.columns.service")}</th>
                <th className="col-container">{t("project.columns.container")}</th>
                <th className="col-image">{t("containers.columns.image")}</th>
                <th className="col-status">{t("containers.columns.status")}</th>
                <th className="col-ports">{t("containers.columns.ports")}</th>
                <th className="col-actions" />
              </tr>
            </thead>
            <tbody>
              {services.map((c) => {
                const isRunning = c.State === "running";
                const cname = (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
                return (
                  <tr key={c.Id}>
                    <td className="font-medium">{serviceNameOf(c)}</td>
                    <td className="col-container mono" title={cname}>{cname}</td>
                    <td className="col-image mono" title={c.Image}>{c.Image.slice(c.Image.lastIndexOf("/") + 1)}</td>
                    <td>
                      <span className={`pill pill-dot ${stateClass(c.State)}`} role="img" title={c.Status} aria-label={t(`containers.state.${c.State}`, { defaultValue: c.State })} />
                    </td>
                    <td className="col-ports mono"><PortLinks c={c} running={isRunning} /></td>
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

      <div role="tablist" className="tabs mx-4 mb-2">
        <button role="tab" type="button" aria-selected={tab === "logs"} onClick={() => setTab("logs")} className="tab"><IconLogs />{t("project.tabs.logs")}</button>
        <button role="tab" type="button" aria-selected={tab === "compose"} onClick={() => setTab("compose")} className="tab">
          <IconFile />{t("project.tabs.compose")}
          {dirty && <span className="pill pill-warn" style={{ marginLeft: 4 }}>{t("project.unsaved")}</span>}
        </button>
        <button role="tab" type="button" aria-selected={tab === "env"} onClick={() => setTab("env")} className="tab"><IconPencil />{t("project.tabs.env")}</button>
      </div>
      {tab === "env" && (
        <EnvPanel dir={dir} composeFile={project?.file ?? null} address={addr?.url ?? null} ownPorts={ownPorts} busy={busy !== null} onUp={() => run("up", ["up", "-d", "--remove-orphans"])} onFilesChanged={() => void loadYaml()} />
      )}
      <div className="mx-4 mb-4 min-h-0 flex-1" hidden={tab !== "logs"}>
        <MultiLogsPanel sources={logSources} />
      </div>
      {tab === "compose" && (
        <div className="card mx-4 mb-4 flex min-h-0 flex-1 flex-col">
          <div className="flex items-center gap-2 border-b px-3 py-1.5" style={{ borderColor: "var(--line)" }}>
            <span className="mono text-xs" style={{ color: "var(--ink-2)" }}>{project?.file ?? "compose.yaml"}</span>
            <span className="kbd-hint">{dirty ? t("project.unsaved") : t("project.saved")}</span>
            <span className="flex-1" />
            <button type="button" className="btn btn-ghost btn-sm" disabled={saving || !project} onClick={() => void loadYaml()}>{t("project.reload")}</button>
            <button type="button" className="btn btn-sm" disabled={saving || !dirty || !project} onClick={() => void saveYaml()}>{t("project.save")}</button>
            <button type="button" className="btn btn-primary btn-sm" disabled={saving || busy !== null || !project} onClick={() => void (async () => { if (await saveYaml()) await run("up", ["up", "-d", "--remove-orphans"]); })()}>{t("project.save_up")}</button>
          </div>
          <textarea
            className="mono min-h-0 flex-1 resize-none p-3 text-xs leading-5"
            style={{ background: "transparent", color: "var(--ink)", border: 0, outline: "none", userSelect: "text" }}
            spellCheck={false}
            value={yaml}
            disabled={!project}
            aria-label={project?.file ?? "compose.yaml"}
            onChange={(e) => setYaml(e.target.value)}
            onKeyDown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") { e.preventDefault(); if (dirty) void saveYaml(); } }}
          />
        </div>
      )}
    </div>
  );
}
