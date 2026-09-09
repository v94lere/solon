import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { IconLogs, IconPlay, IconRestart, IconStop, IconTrash } from "../components/Icons";
import { compose, containers, engine, formatBytes, stacks, type ContainerSummary, type Probe, type StatSample } from "../api";
import { StackDialog } from "../components/StackDialog";
import { markUserAction } from "../engine";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PortLinks } from "../components/PortLinks";
import { Avatar, EmptyState, IconBox, IconFolderOpen, IconGlobe, PageHeader, SkeletonRows, Spark, imageBase, pushHistory } from "../components/ui";
import { loadRecentProjects, projectBaseName, projectDirOf, rememberProject } from "../projects";
import { useEngine } from "../engine";

const COMPOSE_LABEL = "com.docker.compose.project";

const IconMoon = () => (
  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z" />
  </svg>
);
const HELLO_IMAGE = "public.ecr.aws/docker/library/hello-world";

/** Nom court d'une image : dernier segment du dépôt, tag conservé (`…/library/busybox:1.36` → `busybox:1.36`). */
function shortImage(image: string | undefined): string {
  if (!image) return "";
  const at = image.indexOf("@");
  const base = at >= 0 ? image.slice(0, at) : image;
  return base.slice(base.lastIndexOf("/") + 1);
}

function stateClass(state: string) {
  switch (state) {
    case "running":
      return "pill-ok";
    case "paused":
    case "restarting":
      return "pill-warn";
    case "dead":
      return "pill-bad";
    default:
      return "pill-muted";
  }
}

export function ContainersView({ onOpen, onOpenProject }: { onOpen: (id: string) => void; onOpenProject: (dir: string, autoUp?: boolean) => void }) {
  const { t } = useTranslation();
  const { snapshot } = useEngine();
  const sleeping = snapshot?.sleeping ?? [];
  const queryClient = useQueryClient();
  const [showStopped, setShowStopped] = useState(true);
  const [filter, setFilter] = useState("");
  const [stats, setStats] = useState<Record<string, StatSample>>({});
  const [history, setHistory] = useState<Record<string, number[]>>({});
  const [removing, setRemoving] = useState<ContainerSummary | null>(null);
  const [removeVolumes, setRemoveVolumes] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recent] = useState<string[]>(loadRecentProjects);
  const [hello, setHello] = useState<"idle" | "running" | "done">("idle");
  const [stackDialog, setStackDialog] = useState<{ open: boolean; probe: Probe | null }>({ open: false, probe: null });

  const query = useQuery({ queryKey: ["containers", showStopped], queryFn: () => containers.list(showStopped) });

  // Statistiques en flux : un seul canal pour tous les conteneurs en marche ; historique court pour la courbe.
  useEffect(() => {
    let streamId: number | null = null;
    let cancelled = false;
    containers
      .statsOpen((s) => {
        setStats((prev) => ({ ...prev, [s.id]: s }));
        setHistory((prev) => ({ ...prev, [s.id]: pushHistory(prev[s.id] ?? [], s.cpu_percent, 30) }));
      })
      .then((id) => {
        if (cancelled) void containers.streamClose(id);
        else streamId = id;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      if (streamId !== null) void containers.streamClose(streamId);
    };
  }, []);

  const rows = useMemo(() => {
    const list = query.data ?? [];
    const f = filter.trim().toLowerCase();
    const filtered = f
      ? list.filter((c) => (c.Names ?? []).join(" ").toLowerCase().includes(f) || c.Image.toLowerCase().includes(f) || c.State.toLowerCase().includes(f) || c.Status.toLowerCase().includes(f))
      : list;
    return [...filtered].sort((a, b) => (a.State === "running" ? 0 : 1) - (b.State === "running" ? 0 : 1) || (a.Names?.[0] ?? "").localeCompare(b.Names?.[0] ?? ""));
  }, [query.data, filter]);

  // Regroupement Compose : projet → conteneurs (les autres dans un groupe sans nom).
  const groups = useMemo(() => {
    const map = new Map<string, ContainerSummary[]>();
    for (const c of rows) {
      const key = c.Labels?.[COMPOSE_LABEL] ?? "";
      map.set(key, [...(map.get(key) ?? []), c]);
    }
    return [...map.entries()].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
  }, [rows]);

  async function act(id: string, action: () => Promise<void>) {
    markUserAction(id);
    setBusy(id);
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function pickProject() {
    setError(null);
    const chosen = (await openDialog({ directory: true, multiple: false, title: t("compose.pick") })) as string | null;
    if (!chosen) return;
    try {
      const p = await compose.detect(chosen);
      if (!p) {
        // Pas de fichier Compose : on regarde ce que contient le dossier et on propose un environnement.
        const probe = await stacks.probe(chosen);
        setStackDialog({ open: true, probe });
        return;
      }
      rememberProject(chosen);
      onOpenProject(chosen);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Premier conteneur : hello-world tiré du miroir public, lancé dans la machine. */
  async function runHello() {
    setHello("running");
    setError(null);
    try {
      const r = await engine.exec(`docker pull -q ${HELLO_IMAGE} >/dev/null && docker run --name hello-world ${HELLO_IMAGE}`, 180);
      if (r.code !== 0) throw new Error(r.stderr.trim() || r.stdout.trim() || `exit ${r.code}`);
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
      setHello("done");
    } catch (e) {
      setError(String(e));
      setHello("idle");
    }
  }

  // Écran de démarrage seulement quand il n'existe vraiment aucun conteneur (arrêtés compris).
  const noContainersAtAll = !query.isLoading && showStopped && (query.data ?? []).length === 0 && !filter.trim();
  const noRunning = !query.isLoading && !showStopped && (query.data ?? []).length === 0 && !filter.trim();

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("containers.title")}
        count={t("containers.count", { count: rows.length })}
        actions={
          <>
            <button type="button" className="btn btn-sm" onClick={() => void pickProject()}>
              <IconFolderOpen />
              {t("compose.open")}
            </button>
            <button type="button" className="btn btn-sm" onClick={() => setStackDialog({ open: true, probe: null })}>
              <IconGlobe />
              {t("stacks.new")}
            </button>
            {recent.length > 0 && (
              <select className="input input-sm max-w-64" value="" onChange={(e) => { if (e.target.value) onOpenProject(e.target.value); }} aria-label={t("compose.recent")}>
                <option value="">{t("compose.recent")}</option>
                {recent.map((r) => <option key={r} value={r}>{projectBaseName(r)}</option>)}
              </select>
            )}
          </>
        }
        search={<input type="search" className="input w-56" placeholder={t("containers.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("containers.search")} />}
      >
        <label className="flex items-center gap-2 whitespace-nowrap text-[13px]" style={{ color: "var(--ink-2)" }}>
          <input type="checkbox" checked={showStopped} onChange={(e) => setShowStopped(e.target.checked)} />
          {t("containers.show_stopped")}
        </label>
      </PageHeader>
      {error && (
        <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>
          {error}
        </div>
      )}
      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {query.isLoading ? (
          <SkeletonRows rows={5} cols={6} />
        ) : noContainersAtAll ? (
          <EmptyState icon={<IconBox />} title={t("containers.start.title")} hint={t("containers.start.hint")}>
            <div className="start-grid">
              <div className="start-card">
                <div className="start-icon"><IconBox /></div>
                <h4>{t("containers.start.hello_title")}</h4>
                <p>{t("containers.start.hello_body")}</p>
                <button type="button" className="btn btn-primary btn-sm" disabled={hello !== "idle"} onClick={() => void runHello()}>
                  {hello === "running" ? t("containers.start.hello_running") : t("containers.start.hello_action")}
                </button>
              </div>
              <div className="start-card">
                <div className="start-icon"><IconFolderOpen /></div>
                <h4>{t("containers.start.compose_title")}</h4>
                <p>{t("containers.start.compose_body")}</p>
                <button type="button" className="btn btn-sm" onClick={() => void pickProject()}>{t("compose.open")}</button>
              </div>
              <div className="start-card">
                <div className="start-icon"><IconGlobe /></div>
                <h4>{t("containers.start.stack_title")}</h4>
                <p>{t("containers.start.stack_body")}</p>
                <button type="button" className="btn btn-sm" onClick={() => setStackDialog({ open: true, probe: null })}>{t("stacks.new")}</button>
              </div>
              <div className="start-card">
                <div className="start-icon"><IconGlobe /></div>
                <h4>{t("containers.start.cli_title")}</h4>
                <p>{t("containers.start.cli_body")}</p>
                <code className="start-code">docker run -d --name web nginx</code>
                <p className="kbd-hint">{t("containers.start.cli_hint")}</p>
              </div>
            </div>
          </EmptyState>
        ) : noRunning ? (
          <EmptyState icon={<IconBox />} title={t("containers.none_running")} hint={t("containers.none_running_hint")} action={<button type="button" className="btn btn-sm" onClick={() => setShowStopped(true)}>{t("containers.show_stopped")}</button>} />
        ) : rows.length === 0 ? (
          <EmptyState title={t("containers.no_match")} hint={t("containers.no_match_hint")} action={<button type="button" className="btn btn-sm" onClick={() => setFilter("")}>{t("containers.clear_filter")}</button>} />
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("containers.columns.name")}</th>
                <th>{t("containers.columns.image")}</th>
                <th>{t("containers.columns.status")}</th>
                <th>{t("containers.columns.ports")}</th>
                <th className="text-right">{t("containers.columns.cpu")}</th>
                <th className="text-right">{t("containers.columns.memory")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {groups.map(([project, list]) => (
                <GroupRows
                  key={project || "__none"}
                  project={project}
                  list={list}
                  stats={stats}
                  history={history}
                  sleeping={sleeping}
                  busy={busy}
                  onOpen={onOpen}
                  onOpenProject={onOpenProject}
                  onAct={act}
                  onRemove={(c) => {
                    setRemoveVolumes(false);
                    setRemoving(c);
                  }}
                />
              ))}
            </tbody>
          </table>
        )}
      </div>

      <StackDialog
        open={stackDialog.open}
        probe={stackDialog.probe}
        onClose={() => setStackDialog({ open: false, probe: null })}
        onCreated={(dir, autoUp) => {
          setStackDialog({ open: false, probe: null });
          onOpenProject(dir, autoUp);
        }}
      />
      <ConfirmDialog
        open={removing !== null}
        title={t("containers.remove_confirm.title", { name: removing?.Names?.[0]?.replace(/^\//, "") ?? "" })}
        confirmLabel={t("containers.remove_confirm.confirm")}
        cancelLabel={t("containers.remove_confirm.cancel")}
        danger
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          const c = removing;
          setRemoving(null);
          if (c) void act(c.Id, () => containers.remove(c.Id, true, removeVolumes));
        }}
      >
        <p>{t("containers.remove_confirm.body")}</p>
        <label className="mt-3 flex items-center gap-2">
          <input type="checkbox" checked={removeVolumes} onChange={(e) => setRemoveVolumes(e.target.checked)} />
          {t("containers.remove_confirm.with_volumes")}
        </label>
      </ConfirmDialog>
    </div>
  );
}

function GroupRows({
  project,
  list,
  stats,
  history,
  sleeping,
  busy,
  onOpen,
  onAct,
  onRemove,
  onOpenProject,
}: {
  project: string;
  list: ContainerSummary[];
  stats: Record<string, StatSample>;
  history: Record<string, number[]>;
  sleeping: string[];
  busy: string | null;
  onOpen: (id: string) => void;
  onAct: (id: string, action: () => Promise<void>) => Promise<void>;
  onRemove: (c: ContainerSummary) => void;
  onOpenProject: (dir: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      {project && (
        <tr className="group-row">
          <td colSpan={7}>
            {(() => {
              const dir = list.map(projectDirOf).find((d) => d);
              return dir ? (
                <button type="button" className="group-link" title={dir} onClick={() => onOpenProject(dir)}>
                  {t("containers.compose")} · {project} →
                </button>
              ) : (
                <>{t("containers.compose")} · {project}</>
              );
            })()}
          </td>
        </tr>
      )}
      {list.map((c) => {
        const name = (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
        const s = stats[c.Id];
        const asleep = c.State === "paused" && sleeping.includes(c.Id);
        const running = c.State === "running";
        const reachable = running || asleep;
        const cpuHist = history[c.Id] ?? [];
        return (
          <tr
            key={c.Id}
            tabIndex={0}
            onDoubleClick={() => onOpen(c.Id)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onOpen(c.Id);
            }}
          >
            <td>
              <span className="flex items-center gap-2.5">
                <Avatar label={imageBase(c.Image)} seed={imageBase(c.Image)} title={c.Image} />
                <button type="button" className="font-medium hover:underline" onClick={() => onOpen(c.Id)} style={{ color: running ? "var(--ink)" : "var(--ink-2)" }}>
                  {name}
                </button>
              </span>
            </td>
            <td className="mono max-w-[220px] truncate" title={c.Image} style={{ color: "var(--ink-2)" }}>
              {shortImage(c.Image)}
            </td>
            <td>
              {asleep ? (
                <span className="pill pill-sleep" title={t("containers.sleeping_hint")}>
                  <IconMoon />
                  {t("containers.sleeping")}
                </span>
              ) : (
                <span
                  className={`pill pill-dot ${stateClass(c.State)}`}
                  role="img"
                  title={`${t(`containers.state.${c.State}`, { defaultValue: c.State })} — ${c.Status ?? ""}`}
                  aria-label={t(`containers.state.${c.State}`, { defaultValue: c.State })}
                />
              )}
            </td>
            <td className="mono">
              <PortLinks c={c} running={reachable} />
            </td>
            <td className="text-right whitespace-nowrap">
              {running && s ? (
                <span className="inline-flex items-center justify-end gap-2">
                  <Spark values={cpuHist} max={Math.max(100, ...cpuHist)} className="spark-row spark-cell" warn={70} bad={90} history={30} />
                  <span className="mono">{s.cpu_percent.toFixed(1)} %</span>
                </span>
              ) : (
                <span className="mono">—</span>
              )}
            </td>
            <td className="mono text-right whitespace-nowrap">{running && s ? formatBytes(s.mem_usage) : "—"}</td>
            <td>
              <div className="flex justify-end gap-0.5">
                {running || asleep ? (
                  <>
                    <button type="button" className="icon-btn" title={t("containers.actions.stop")} aria-label={t("containers.actions.stop")} disabled={busy === c.Id} onClick={() => void onAct(c.Id, () => containers.stop(c.Id))}>
                      <IconStop />
                    </button>
                    <button type="button" className="icon-btn" title={t("containers.actions.restart")} aria-label={t("containers.actions.restart")} disabled={busy === c.Id} onClick={() => void onAct(c.Id, () => containers.restart(c.Id))}>
                      <IconRestart />
                    </button>
                  </>
                ) : (
                  <button type="button" className="icon-btn" title={t("containers.actions.start")} aria-label={t("containers.actions.start")} disabled={busy === c.Id} onClick={() => void onAct(c.Id, () => containers.start(c.Id))}>
                    <IconPlay />
                  </button>
                )}
                <button type="button" className="icon-btn" title={t("containers.actions.logs")} aria-label={t("containers.actions.logs")} onClick={() => onOpen(c.Id)}>
                  <IconLogs />
                </button>
                <button type="button" className="icon-btn icon-btn-danger" title={t("containers.actions.remove")} aria-label={t("containers.actions.remove")} disabled={busy === c.Id} onClick={() => onRemove(c)}>
                  <IconTrash />
                </button>
              </div>
            </td>
          </tr>
        );
      })}
    </>
  );
}
