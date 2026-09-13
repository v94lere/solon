// Accueil : une carte par projet Compose (nom, état, adresse principale en évidence, dernière activité,
// Ouvrir dans le navigateur, Up / Stop), les conteneurs isolés dans
// un groupe « Divers », les dossiers ouverts récemment mais sans conteneur, et l'accueil en trois choix
// quand il n'y a encore rien.
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { backup, compose, containers, engine, stacks, type ContainerSummary, type Probe } from "../api";
import { StackDialog } from "../components/StackDialog";
import { ScanDialog } from "../components/ScanDialog";
import { Avatar, EmptyState, IconBox, IconFolderOpen, IconGlobe, PageHeader, imageBase } from "../components/ui";
import { IconPlay, IconStop } from "../components/Icons";
import { useEngine, markUserAction } from "../engine";
import { domainOf, forgetProject, lastActivity, loadRecentProjects, primaryAddress, projectBaseName, projectDirOf, projectNameOf, rememberProject, samePath, withSleeping } from "../projects";
import { branchEnvEnabled, branchOfProjectName } from "../branches";

const HELLO_IMAGE = "public.ecr.aws/docker/library/hello-world";

const IconLock = () => (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="4" y="11" width="16" height="10" rx="2" />
    <path d="M8 11V7a4 4 0 0 1 8 0v4" />
  </svg>
);
const IconCopy = () => (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V6a2 2 0 0 1 2-2h9" />
  </svg>
);
const IconExternal = () => (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M14 4h6v6M20 4l-9 9M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
  </svg>
);

interface Group {
  name: string;
  dir: string | null;
  list: ContainerSummary[];
}

/** Adresse principale, grande, copiable, avec cadenas quand c'est du HTTPS. */
export function AddressLine({ url, host, big = false }: { url: string; host: string; big?: boolean }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const tls = url.startsWith("https://");
  return (
    <span className={`addr${big ? " addr-big" : ""}`}>
      {tls && <span className="addr-lock" title="HTTPS"><IconLock /></span>}
      <button type="button" className="addr-host mono" title={t("projects.open_browser", { url })} onClick={() => void openUrl(url)}>{host}</button>
      <button
        type="button"
        className="icon-btn addr-copy"
        title={copied ? t("projects.address_copied") : t("projects.address_copy")}
        aria-label={t("projects.address_copy")}
        onClick={() => {
          void navigator.clipboard.writeText(url).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1500);
          });
        }}
      >
        <IconCopy />
      </button>
      {copied && <span className="kbd-hint">{t("projects.address_copied")}</span>}
    </span>
  );
}

export function ProjectsView({ onOpenProject, onOpenContainer }: { onOpenProject: (dir: string, autoUp?: boolean) => void; onOpenContainer: (id: string, tab?: "logs") => void }) {
  const { t } = useTranslation();
  const { snapshot } = useEngine();
  const queryClient = useQueryClient();
  const tls = !!snapshot?.local_domains_tls;
  const domainsOn = !!snapshot?.local_domains;
  const query = useQuery({ queryKey: ["containers", true], queryFn: () => containers.list(true), refetchInterval: 5000 });
  const [recent, setRecent] = useState<string[]>(loadRecentProjects);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hello, setHello] = useState<"idle" | "running">("idle");
  const [stackDialog, setStackDialog] = useState<{ open: boolean; probe: Probe | null }>({ open: false, probe: null });
  const [restoring, setRestoring] = useState<string | null>(null);
  const [scanOpen, setScanOpen] = useState(false);

  /** Restaurer une sauvegarde : le zip, un aperçu, le dossier de destination, puis la page du projet. */
  async function restoreBackup() {
    setError(null);
    const zip = (await openDialog({ multiple: false, title: t("projects.restore_pick_zip"), filters: [{ name: "Solon backup", extensions: ["zip"] }] })) as string | null;
    if (!zip) return;
    try {
      const info = await backup.info(zip);
      const target = (await openDialog({ directory: true, multiple: false, title: t("projects.restore_pick_dir", { project: info.project, volumes: info.volumes.length }) })) as string | null;
      if (!target) return;
      setRestoring(t("projects.restoring", { project: info.project }));
      const r = await backup.restore(zip, target);
      setRestoring(null);
      rememberProject(r.dir);
      setRecent(loadRecentProjects());
      onOpenProject(r.dir);
    } catch (e) {
      setRestoring(null);
      setError(String(e));
    }
  }

  useEffect(() => {
    // Un projet ouvert depuis une autre vue peut avoir été ajouté aux récents.
    setRecent(loadRecentProjects());
  }, [query.data]);

  // Un dossier récent qui n'existe plus (supprimé, disque débranché) disparaît de lui-même.
  useEffect(() => {
    let alive = true;
    void Promise.all(recent.map((dir) => compose.detect(dir).then(() => true).catch(() => false))).then((ok) => {
      if (!alive) return;
      const gone = recent.filter((_, i) => !ok[i]);
      if (gone.length > 0) {
        gone.forEach(forgetProject);
        setRecent(loadRecentProjects());
      }
    });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recent.join("|")]);

  const { groups, misc } = useMemo(() => {
    const map = new Map<string, Group>();
    const misc: ContainerSummary[] = [];
    for (const c of withSleeping(query.data ?? [], snapshot?.sleeping ?? [])) {
      const name = projectNameOf(c);
      if (!name) {
        misc.push(c);
        continue;
      }
      const g = map.get(name) ?? { name, dir: null, list: [] };
      g.list.push(c);
      g.dir = g.dir ?? projectDirOf(c);
      map.set(name, g);
    }
    const groups = [...map.values()].sort((a, b) => {
      const ra = a.list.some((c) => c.State === "running") ? 0 : 1;
      const rb = b.list.some((c) => c.State === "running") ? 0 : 1;
      return ra - rb || a.name.localeCompare(b.name);
    });
    misc.sort((a, b) => (a.State === "running" ? 0 : 1) - (b.State === "running" ? 0 : 1) || (a.Names?.[0] ?? "").localeCompare(b.Names?.[0] ?? ""));
    return { groups, misc };
  }, [query.data, snapshot?.sleeping]);

  // Dossiers ouverts récemment qui n'ont aucun conteneur : proposés à la reprise.
  const dormant = useMemo(() => recent.filter((dir) => !groups.some((g) => samePath(g.dir, dir))), [recent, groups]);


  async function run(g: Group, args: string[]) {
    if (!g.dir) return;
    setBusy(g.name);
    setError(null);
    for (const c of g.list) markUserAction(c.Id);
    try {
      const code = await compose.stream(g.dir, args, () => {});
      if (code !== 0) setError(t("projects.command_failed", { name: g.name, code }));
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function actContainer(c: ContainerSummary, action: () => Promise<void>) {
    markUserAction(c.Id);
    setBusy(c.Id);
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

  async function runHello() {
    setHello("running");
    setError(null);
    try {
      const r = await engine.exec(`docker rm -f hello-world >/dev/null 2>&1; docker pull -q ${HELLO_IMAGE} >/dev/null && docker run --name hello-world ${HELLO_IMAGE}`, 180);
      if (r.code !== 0) throw new Error(r.stderr.trim() || r.stdout.trim() || `exit ${r.code}`);
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
      const all = await containers.list(true);
      const created = all.find((c) => (c.Names ?? []).some((n) => n.replace(/^\//, "") === "hello-world"));
      if (created) onOpenContainer(created.Id, "logs");
    } catch (e) {
      setError(String(e));
    } finally {
      setHello("idle");
    }
  }

  const empty = !query.isLoading && groups.length === 0 && misc.length === 0 && dormant.length === 0;

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("projects.title")}
        count={groups.length > 0 ? t("projects.count", { count: groups.length }) : undefined}
        actions={
          <>
            <button type="button" className="btn btn-sm" onClick={() => void pickProject()}><IconFolderOpen />{t("compose.open")}</button>
            <button type="button" className="btn btn-sm" onClick={() => setStackDialog({ open: true, probe: null })}><IconGlobe />{t("stacks.new")}</button>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setScanOpen(true)}>{t("scan.button")}</button>
            <button type="button" className="btn btn-ghost btn-sm" disabled={restoring !== null} onClick={() => void restoreBackup()}>{restoring ?? t("projects.restore")}</button>
          </>
        }
      />
      {error && <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>}

      <div className="min-h-0 flex-1 overflow-auto px-4 pb-4">
        {empty ? (
          <div className="card p-6">
            <EmptyState icon={<IconBox />} title={t("projects.welcome_title")} hint={t("projects.welcome_hint")}>
              <div className="start-grid">
                <div className="start-card">
                  <div className="start-icon"><IconFolderOpen /></div>
                  <h4>{t("projects.welcome_open_title")}</h4>
                  <p>{t("projects.welcome_open_body")}</p>
                  <button type="button" className="btn btn-primary btn-sm" onClick={() => void pickProject()}>{t("compose.open")}</button>
                </div>
                <div className="start-card">
                  <div className="start-icon"><IconGlobe /></div>
                  <h4>{t("projects.welcome_stack_title")}</h4>
                  <p>{t("projects.welcome_stack_body")}</p>
                  <button type="button" className="btn btn-sm" onClick={() => setStackDialog({ open: true, probe: null })}>{t("stacks.new")}</button>
                </div>
                <div className="start-card">
                  <div className="start-icon"><IconFolderOpen /></div>
                  <h4>{t("scan.welcome_title")}</h4>
                  <p>{t("scan.welcome_body")}</p>
                  <button type="button" className="btn btn-sm" onClick={() => setScanOpen(true)}>{t("scan.button")}</button>
                </div>
                <div className="start-card">
                  <div className="start-icon"><IconBox /></div>
                  <h4>{t("projects.welcome_hello_title")}</h4>
                  <p>{t("projects.welcome_hello_body")}</p>
                  <button type="button" className="btn btn-sm" disabled={hello !== "idle"} onClick={() => void runHello()}>{hello === "running" ? t("containers.start.hello_running") : t("containers.start.hello_action")}</button>
                </div>
              </div>
            </EmptyState>
          </div>
        ) : (
          <>
            <div className="project-grid">
              {groups.map((g) => {
                const running = g.list.filter((c) => c.State === "running").length;
                const addr = domainsOn ? primaryAddress(g.list, tls) : null;
                const activity = lastActivity(g.list);
                const isBusy = busy === g.name;
                return (
                  <article key={g.name} className={`project-card${running > 0 ? " is-running" : ""}`}>
                    <header className="project-head">
                      <Avatar label={g.name} seed={g.name} size={34} />
                      <div className="min-w-0 flex-1">
                        <button type="button" className="project-name" disabled={!g.dir} title={g.dir ?? t("projects.no_dir")} onClick={() => g.dir && onOpenProject(g.dir)}>
                          {g.name}
                          {g.dir && branchEnvEnabled(g.dir) && branchOfProjectName(projectBaseName(g.dir).toLowerCase().replace(/[^a-z0-9_-]/g, ""), g.name) && (
                            <span className="branch-badge mono">⎇ {branchOfProjectName(projectBaseName(g.dir).toLowerCase().replace(/[^a-z0-9_-]/g, ""), g.name)}</span>
                          )}
                        </button>
                        <div className="project-meta">
                          <span className={`pill pill-dot ${running > 0 ? "pill-ok" : "pill-muted"}`} aria-hidden="true" />
                          <span>{t("projects.running_count", { running, total: g.list.length })}</span>
                          {activity.running ? <span>· {activity.text}</span> : activity.created > 0 ? <span>· {t("projects.stopped_since", { date: new Date(activity.created * 1000).toLocaleDateString() })}</span> : null}
                        </div>
                      </div>
                    </header>
                    {addr ? (
                      <AddressLine url={addr.url} host={addr.host} big />
                    ) : (
                      <p className="kbd-hint project-noaddr">{running > 0 ? t("projects.no_address") : t("projects.address_when_running")}</p>
                    )}
                    <ul className="project-services">
                      {g.list.map((c) => {
                        const d = domainOf(c);
                        return (
                          <li key={c.Id}>
                            <span className={`pill pill-dot ${c.State === "running" ? "pill-ok" : "pill-muted"}`} aria-hidden="true" />
                            <button type="button" className="project-service" onClick={() => onOpenContainer(c.Id)} title={imageBase(c.Image)}>{c.Labels?.["com.docker.compose.service"] ?? (c.Names?.[0] ?? "").replace(/^\//, "")}</button>
                            {d && c.State === "running" && domainsOn && d !== addr?.host && <span className="mono kbd-hint truncate">{d}</span>}
                          </li>
                        );
                      })}
                    </ul>
                    <footer className="project-actions">
                      {addr && running > 0 && (
                        <button type="button" className="btn btn-primary btn-sm" onClick={() => void openUrl(addr.url)}><IconExternal />{t("projects.open")}</button>
                      )}
                      {running > 0 ? (
                        <button type="button" className="btn btn-sm" disabled={isBusy || !g.dir} onClick={() => void run(g, ["stop"])}><IconStop />{isBusy ? t("compose.running") : t("projects.stop")}</button>
                      ) : (
                        <button type="button" className="btn btn-primary btn-sm" disabled={isBusy || !g.dir} onClick={() => void run(g, ["up", "-d"])}><IconPlay />{isBusy ? t("compose.running") : t("compose.up")}</button>
                      )}
                      {g.dir && <button type="button" className="btn btn-ghost btn-sm" onClick={() => onOpenProject(g.dir as string)}>{t("projects.details")}</button>}
                    </footer>
                  </article>
                );
              })}
              {dormant.map((dir) => (
                <article key={dir} className="project-card is-dormant">
                  <header className="project-head">
                    <Avatar label={projectBaseName(dir)} seed={projectBaseName(dir)} size={34} />
                    <div className="min-w-0 flex-1">
                      <button type="button" className="project-name" title={dir} onClick={() => onOpenProject(dir)}>{projectBaseName(dir)}</button>
                      <div className="project-meta"><span className="pill pill-dot pill-muted" aria-hidden="true" /><span>{t("projects.not_started")}</span></div>
                    </div>
                  </header>
                  <p className="kbd-hint mono truncate" title={dir}>{dir}</p>
                  <footer className="project-actions">
                    <button type="button" className="btn btn-primary btn-sm" onClick={() => onOpenProject(dir, true)}><IconPlay />{t("compose.up")}</button>
                    <button type="button" className="btn btn-ghost btn-sm" onClick={() => onOpenProject(dir)}>{t("projects.details")}</button>
                    <span className="flex-1" />
                    <button type="button" className="btn btn-ghost btn-sm" onClick={() => { forgetProject(dir); setRecent(loadRecentProjects()); }}>{t("projects.forget")}</button>
                  </footer>
                </article>
              ))}
            </div>

            {misc.length > 0 && (
              <section className="mt-5">
                <h2 className="projects-misc-title">{t("projects.misc")} <span className="kbd-hint">{t("projects.misc_hint")}</span></h2>
                <div className="card list-card">
                  <table className="table">
                    <tbody>
                      {misc.map((c) => {
                        const name = (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
                        const running = c.State === "running";
                        const d = running && domainsOn ? domainOf(c) : null;
                        return (
                          <tr key={c.Id}>
                            <td>
                              <span className="flex items-center gap-2.5">
                                <Avatar label={imageBase(c.Image)} seed={imageBase(c.Image)} title={c.Image} />
                                <button type="button" className="truncate text-left font-medium hover:underline" onClick={() => onOpenContainer(c.Id)} style={{ color: running ? "var(--ink)" : "var(--ink-2)" }}>{name}</button>
                              </span>
                            </td>
                            <td className="col-status"><span className={`pill pill-dot ${running ? "pill-ok" : "pill-muted"}`} role="img" title={c.Status} aria-label={c.State} /></td>
                            <td className="col-ports">{d ? <AddressLine url={`${tls ? "https" : "http"}://${d}/`} host={d} /> : <span className="kbd-hint">{c.Status}</span>}</td>
                            <td className="col-actions-wide">
                              <div className="flex justify-end gap-0.5">
                                {running ? (
                                  <button type="button" className="icon-btn" title={t("containers.actions.stop")} aria-label={t("containers.actions.stop")} disabled={busy === c.Id} onClick={() => void actContainer(c, () => containers.stop(c.Id))}><IconStop /></button>
                                ) : (
                                  <button type="button" className="icon-btn" title={t("containers.actions.start")} aria-label={t("containers.actions.start")} disabled={busy === c.Id} onClick={() => void actContainer(c, () => containers.start(c.Id))}><IconPlay /></button>
                                )}
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </section>
            )}
          </>
        )}
      </div>

      <ScanDialog
        open={scanOpen}
        onClose={() => setScanOpen(false)}
        onAdded={() => { setScanOpen(false); setRecent(loadRecentProjects()); }}
        onSetup={(probe) => { setScanOpen(false); setStackDialog({ open: true, probe }); }}
      />
      <StackDialog
        open={stackDialog.open}
        probe={stackDialog.probe}
        onClose={() => setStackDialog({ open: false, probe: null })}
        onCreated={(dir, autoUp) => {
          setStackDialog({ open: false, probe: null });
          onOpenProject(dir, autoUp);
        }}
      />
    </div>
  );
}
