import { useState, type JSX } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { containers } from "../api";
import { LABEL_PROJECT, LABEL_WORKDIR, hostPathFromGuest } from "../projects";
import { LogsPanel } from "../components/LogsPanel";
import { TerminalPanel } from "../components/TerminalPanel";
import { DebugPanel } from "../components/DebugPanel";
import { FilesPanel } from "../components/FilesPanel";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { IconLogs, IconPencil, IconPlay, IconRestart, IconStop, IconTerminal, IconTrash } from "../components/Icons";
import { Avatar, imageBase } from "../components/ui";
import { markUserAction } from "../engine";
import { useQueryClient } from "@tanstack/react-query";
import { engine } from "../api";
import { useEngine } from "../engine";

type Tab = "overview" | "logs" | "files" | "terminal" | "debug" | "inspect";

interface Inspect {
  Id?: string;
  Name?: string;
  Created?: string;
  Path?: string;
  Args?: string[];
  Platform?: string;
  RestartCount?: number;
  State?: { Status?: string; Running?: boolean; StartedAt?: string; FinishedAt?: string; ExitCode?: number; Pid?: number; Health?: { Status?: string } };
  Config?: { Image?: string; Hostname?: string; User?: string; WorkingDir?: string; Env?: string[]; Cmd?: string[]; Entrypoint?: string[]; Labels?: Record<string, string>; ExposedPorts?: Record<string, unknown> };
  HostConfig?: { RestartPolicy?: { Name?: string; MaximumRetryCount?: number }; Memory?: number; NanoCpus?: number; Privileged?: boolean; NetworkMode?: string };
  Mounts?: { Type?: string; Name?: string; Source?: string; Destination?: string; Mode?: string; RW?: boolean }[];
  NetworkSettings?: {
    Ports?: Record<string, { HostIp?: string; HostPort?: string }[] | null>;
    Networks?: Record<string, { IPAddress?: string; Gateway?: string; MacAddress?: string; Aliases?: string[] | null; NetworkID?: string }>;
  };
}

const TAB_ICONS: Record<Tab, JSX.Element> = {
  overview: (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></svg>
  ),
  logs: <IconLogs />,
  files: (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" /></svg>
  ),
  terminal: <IconTerminal />,
  debug: (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M8 9a4 4 0 0 1 8 0v5a4 4 0 0 1-8 0Z" /><path d="M4 13h4M16 13h4M5 19l3-2M19 19l-3-2M5 7l3 2M19 7l-3 2M10 5l2-2 2 2" /></svg>
  ),
  inspect: (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m8 8-4 4 4 4M16 8l4 4-4 4M14 5l-4 14" /></svg>
  ),
};

export function ContainerDetail({ id, onBack, onOpenProject }: { id: string; onBack: () => void; onOpenProject?: (dir: string) => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inspect = useQuery({ queryKey: ["container", id], queryFn: () => containers.inspect(id) as Promise<Inspect>, refetchInterval: 5000 });
  const name = inspect.data?.Name?.replace(/^\//, "") ?? id.slice(0, 12);
  const running = inspect.data?.State?.Running ?? false;
  const image = inspect.data?.Config?.Image;
  const projectDir = hostPathFromGuest(inspect.data?.Config?.Labels?.[LABEL_WORKDIR]);
  const [renaming, setRenaming] = useState<string | null>(null);
  async function doRename() {
    if (renaming === null) return;
    const next = renaming.trim();
    if (!next || next === name) { setRenaming(null); return; }
    setBusy(true);
    setError(null);
    try {
      await containers.rename(id, next);
      setRenaming(null);
      await queryClient.invalidateQueries({ queryKey: ["container", id] });
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const tabs: Tab[] = ["overview", "logs", "files", "terminal", "debug", "inspect"];
  const { snapshot } = useEngine();
  const asleep = inspect.data?.State?.Status === "paused" && (snapshot?.sleeping ?? []).includes(id);
  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: () => engine.settingsGet() });
  const keepAwake = (settingsQuery.data?.sleep_never ?? []).includes(name);
  async function toggleKeepAwake() {
    const current = settingsQuery.data ?? (await engine.settingsGet());
    const never = new Set(current.sleep_never ?? []);
    if (never.has(name)) never.delete(name);
    else never.add(name);
    await engine.settingsSet({ ...current, sleep_never: [...never] });
    await queryClient.invalidateQueries({ queryKey: ["settings"] });
  }

  async function act(action: () => Promise<void>) {
    markUserAction(id);
    setBusy(true);
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["container", id] });
      await queryClient.invalidateQueries({ queryKey: ["containers"] });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-3 pb-2">
        <button type="button" className="btn btn-ghost btn-sm" onClick={onBack}>
          ← {t("detail.back")}
        </button>
        <Avatar label={imageBase(image)} seed={imageBase(image)} size={30} title={image} />
        {renaming !== null ? (
          <form className="flex items-center gap-1" onSubmit={(e) => { e.preventDefault(); void doRename(); }}>
            <input
              className="input input-sm mono"
              autoFocus
              value={renaming}
              pattern="[a-zA-Z0-9][a-zA-Z0-9_.-]*"
              title={t("detail.rename_hint")}
              aria-label={t("detail.rename")}
              onChange={(e) => setRenaming(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Escape") setRenaming(null); }}
              style={{ width: Math.max(14, renaming.length + 4) + "ch" }}
            />
            <button type="submit" className="btn btn-sm btn-primary" disabled={busy || !renaming.trim() || renaming === name}>{t("common.ok")}</button>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setRenaming(null)}>{t("common.cancel")}</button>
          </form>
        ) : (
          <button type="button" className="title-edit" title={t("detail.rename")} onClick={() => setRenaming(name)}>
            <h1 className="text-base font-semibold">{name}</h1>
            <IconPencil />
          </button>
        )}
        {image && <span className="mono kbd-hint max-w-[320px] truncate" title={image}>{image}</span>}
        {asleep ? (
          <span className="pill pill-sleep" title={t("containers.sleeping_hint")}>{t("containers.sleeping")}</span>
        ) : (
          <span className={`pill ${running ? "pill-ok" : "pill-muted"}`}>{t(`containers.state.${inspect.data?.State?.Status ?? "created"}`, { defaultValue: inspect.data?.State?.Status })}</span>
        )}
        {projectDir && onOpenProject && (
          <button type="button" className="btn btn-ghost btn-sm" title={projectDir} onClick={() => onOpenProject(projectDir)}>
            {t("detail.project")} · {inspect.data?.Config?.Labels?.[LABEL_PROJECT]}
          </button>
        )}
        <span className="flex-1" />
        <button type="button" className={`btn btn-ghost btn-sm ${keepAwake ? "is-on" : ""}`} title={t("detail.keep_awake_help")} aria-pressed={keepAwake} onClick={() => void toggleKeepAwake()}>
          {keepAwake ? t("detail.keep_awake_on") : t("detail.keep_awake")}
        </button>
        <div className="flex items-center gap-0.5">
          {running || asleep ? (
            <>
              <button type="button" className="icon-btn" title={t("containers.actions.stop")} aria-label={t("containers.actions.stop")} disabled={busy} onClick={() => void act(() => containers.stop(id))}><IconStop /></button>
              <button type="button" className="icon-btn" title={t("containers.actions.restart")} aria-label={t("containers.actions.restart")} disabled={busy} onClick={() => void act(() => containers.restart(id))}><IconRestart /></button>
            </>
          ) : (
            <button type="button" className="icon-btn" title={t("containers.actions.start")} aria-label={t("containers.actions.start")} disabled={busy} onClick={() => void act(() => containers.start(id))}><IconPlay /></button>
          )}
          <button type="button" className="icon-btn icon-btn-danger" title={t("containers.actions.remove")} aria-label={t("containers.actions.remove")} disabled={busy} onClick={() => setRemoving(true)}><IconTrash /></button>
        </div>
      </div>
      {error && (
        <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>
      )}
      <div role="tablist" className="tabs px-4">
        {tabs.map((tb) => (
          <button key={tb} role="tab" type="button" aria-selected={tab === tb} onClick={() => setTab(tb)} className="tab">
            {TAB_ICONS[tb]}
            {t(`detail.tabs.${tb}`)}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 p-4" role="tabpanel">
        {tab === "overview" && <OverviewPanel id={id} data={inspect.data} running={running} />}
        {tab === "logs" && <LogsPanel id={id} />}
        {tab === "files" && <FilesPanel target={{ kind: "container", id }} />}
        {tab === "terminal" && <TerminalPanel id={id} running={running} />}
        {tab === "debug" && <DebugPanel id={id} running={running} />}
        {tab === "inspect" && <InspectPanel data={inspect.data} />}
      </div>
      <ConfirmDialog
        open={removing}
        title={t("containers.remove_confirm.title", { name })}
        confirmLabel={t("containers.remove_confirm.confirm")}
        cancelLabel={t("containers.remove_confirm.cancel")}
        danger
        onCancel={() => setRemoving(false)}
        onConfirm={() => {
          setRemoving(false);
          void act(() => containers.remove(id, true, false)).then(onBack);
        }}
      >
        <p>{t("containers.remove_confirm.body")}</p>
      </ConfirmDialog>
    </div>
  );
}

function fmtDate(s: string | undefined): string {
  if (!s || s.startsWith("0001-")) return "—";
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? s : d.toLocaleString();
}

function quote(args: string[] | undefined): string {
  if (!args || args.length === 0) return "—";
  return args.map((a) => (/[\s"']/.test(a) ? JSON.stringify(a) : a)).join(" ");
}

function Section({ title, children, aside }: { title: string; children: React.ReactNode; aside?: React.ReactNode }) {
  return (
    <section className="card p-3">
      <div className="mb-2 flex items-center gap-2">
        <h2 className="text-[12px] font-semibold uppercase tracking-wide" style={{ color: "var(--ink-3)" }}>{title}</h2>
        <span className="flex-1" />
        {aside}
      </div>
      {children}
    </section>
  );
}

function KV({ rows }: { rows: [string, React.ReactNode][] }) {
  return (
    <dl className="kv">
      {rows.map(([k, v]) => (
        <div key={k} className="kv-row">
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="btn btn-ghost btn-sm"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? t("common.copied") : label}
    </button>
  );
}

/** Fiche : identité, état, réseau, ports, montages, variables d'environnement, étiquettes, copie de fichiers. */
function OverviewPanel({ id, data, running }: { id: string; data: Inspect | undefined; running: boolean }) {
  const { t } = useTranslation();
  const [fromPath, setFromPath] = useState("/");
  const [toPath, setToPath] = useState("/tmp");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string; open?: string } | null>(null);
  if (!data) {
    return (
      <p style={{ color: "var(--ink-2)" }}>{t("common.loading")}</p>
    );
  }
  const st = data.State ?? {};
  const cfg = data.Config ?? {};
  const hc = data.HostConfig ?? {};
  const nets = Object.entries(data.NetworkSettings?.Networks ?? {});
  const ports = Object.entries(data.NetworkSettings?.Ports ?? {}).sort(([a], [b]) => a.localeCompare(b));
  const mounts = data.Mounts ?? [];
  const env = (cfg.Env ?? []).map((e) => {
    const i = e.indexOf("=");
    return i < 0 ? [e, ""] : [e.slice(0, i), e.slice(i + 1)];
  });
  const labels = Object.entries(cfg.Labels ?? {}).sort(([a], [b]) => a.localeCompare(b));
  const restart = hc.RestartPolicy?.Name || "no";

  async function copyFrom() {
    setResult(null);
    const dest = (await openDialog({ directory: true, multiple: false, title: t("detail.files.pick_dest") })) as string | null;
    if (!dest) return;
    setBusy(true);
    try {
      const out = await containers.copyFrom(id, fromPath, dest);
      setResult({ ok: true, text: t("detail.files.copied_from", { path: fromPath, dest: out }), open: out });
    } catch (e) {
      setResult({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  }
  async function copyTo(directory: boolean) {
    setResult(null);
    const src = (await openDialog({ directory, multiple: false, title: t("detail.files.pick_source") })) as string | null;
    if (!src) return;
    setBusy(true);
    try {
      await containers.copyTo(id, src, toPath);
      setResult({ ok: true, text: t("detail.files.copied_to", { src, dest: toPath }) });
    } catch (e) {
      setResult({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="grid h-full gap-3 overflow-auto pr-1" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))", alignContent: "start" }}>
      <Section title={t("detail.overview.general")}>
        <KV
          rows={[
            [t("detail.overview.id"), <span className="mono select-text">{data.Id?.slice(0, 12)}</span>],
            [t("detail.overview.image"), <span className="mono select-text">{cfg.Image ?? "—"}</span>],
            [t("detail.overview.created"), fmtDate(data.Created)],
            [t("detail.overview.started"), fmtDate(st.StartedAt)],
            ...(running ? [] : [[t("detail.overview.finished"), `${fmtDate(st.FinishedAt)}${st.ExitCode != null ? ` · ${t("detail.overview.exit_code", { code: st.ExitCode })}` : ""}`] as [string, React.ReactNode]]),
            ...(st.Health?.Status ? [[t("detail.overview.health"), st.Health.Status] as [string, React.ReactNode]] : []),
            [t("detail.overview.command"), <span className="mono select-text">{quote([...(cfg.Entrypoint ?? []), ...(cfg.Cmd ?? [])])}</span>],
            [t("detail.overview.workdir"), <span className="mono">{cfg.WorkingDir || "/"}</span>],
            [t("detail.overview.user"), <span className="mono">{cfg.User || "root"}</span>],
            [t("detail.overview.hostname"), <span className="mono">{cfg.Hostname ?? "—"}</span>],
            [t("detail.overview.restart"), `${restart}${hc.RestartPolicy?.MaximumRetryCount ? ` (${hc.RestartPolicy.MaximumRetryCount})` : ""}${data.RestartCount ? ` · ${t("detail.overview.restarts", { count: data.RestartCount })}` : ""}`],
            ...(hc.Privileged ? [[t("detail.overview.privileged"), "yes"] as [string, React.ReactNode]] : []),
          ]}
        />
      </Section>

      <Section title={t("detail.overview.network")}>
        {nets.length === 0 ? (
          <p style={{ color: "var(--ink-2)" }}>—</p>
        ) : (
          nets.map(([n, v]) => (
            <div key={n} className="mb-2">
              <div className="font-medium">{n}</div>
              <KV
                rows={[
                  [t("detail.overview.ip"), <span className="mono select-text">{v.IPAddress || "—"}</span>],
                  [t("detail.overview.gateway"), <span className="mono">{v.Gateway || "—"}</span>],
                  [t("detail.overview.mac"), <span className="mono">{v.MacAddress || "—"}</span>],
                  ...(v.Aliases && v.Aliases.length ? [[t("detail.overview.aliases"), <span className="mono">{v.Aliases.join(", ")}</span>] as [string, React.ReactNode]] : []),
                ]}
              />
            </div>
          ))
        )}
        {ports.length > 0 && (
          <>
            <div className="mt-2 mb-1 text-[12px] font-semibold" style={{ color: "var(--ink-3)" }}>{t("detail.overview.ports")}</div>
            <ul className="mono text-[12.5px] leading-6">
              {ports.map(([spec, bindings]) => {
                const b = bindings?.find((x) => x.HostPort);
                const local = b?.HostPort ? `http://localhost:${b.HostPort}/` : null;
                return (
                  <li key={spec}>
                    {spec}
                    {b?.HostPort ? (
                      <>
                        {" → "}
                        <button type="button" className="port-link" onClick={() => void openUrl(local!)}>
                          localhost:{b.HostPort}
                        </button>
                      </>
                    ) : (
                      <span style={{ color: "var(--ink-3)" }}> · {t("detail.overview.not_published")}</span>
                    )}
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </Section>

      <Section title={t("detail.overview.mounts")}>
        {mounts.length === 0 ? (
          <p style={{ color: "var(--ink-2)" }}>{t("detail.overview.no_mounts")}</p>
        ) : (
          <ul className="mono text-[12.5px] leading-6">
            {mounts.map((m, i) => (
              <li key={i} className="truncate" title={`${m.Source ?? m.Name ?? ""} → ${m.Destination ?? ""}`}>
                <span className="chip-mini">{m.Type ?? "?"}</span> {m.Type === "volume" ? m.Name : m.Source} → {m.Destination}
                {m.RW === false && <span style={{ color: "var(--ink-3)" }}> (ro)</span>}
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section
        title={t("detail.overview.env")}
        aside={env.length > 0 ? <CopyButton text={(cfg.Env ?? []).join("\n")} label={t("common.copy")} /> : undefined}
      >
        {env.length === 0 ? (
          <p style={{ color: "var(--ink-2)" }}>—</p>
        ) : (
          <dl className="kv kv-mono">
            {env.map(([k, v]) => (
              <div key={k} className="kv-row">
                <dt className="select-text">{k}</dt>
                <dd className="select-text">{v}</dd>
              </div>
            ))}
          </dl>
        )}
      </Section>

      <Section title={t("detail.overview.labels")}>
        {labels.length === 0 ? (
          <p style={{ color: "var(--ink-2)" }}>—</p>
        ) : (
          <dl className="kv kv-mono">
            {labels.map(([k, v]) => (
              <div key={k} className="kv-row">
                <dt className="select-text">{k}</dt>
                <dd className="select-text">{v}</dd>
              </div>
            ))}
          </dl>
        )}
      </Section>

      <Section title={t("detail.files.title")}>
        <p className="mb-2" style={{ color: "var(--ink-2)" }}>{t("detail.files.help")}</p>
        <div className="flex flex-wrap items-center gap-2">
          <input className="input mono flex-1" style={{ minWidth: 160 }} value={fromPath} onChange={(e) => setFromPath(e.target.value)} aria-label={t("detail.files.from_path")} />
          <button type="button" className="btn btn-sm" disabled={busy} onClick={() => void copyFrom()}>{t("detail.files.copy_from")}</button>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <input className="input mono flex-1" style={{ minWidth: 160 }} value={toPath} onChange={(e) => setToPath(e.target.value)} aria-label={t("detail.files.to_path")} disabled={!running} />
          <button type="button" className="btn btn-sm" disabled={busy || !running} onClick={() => void copyTo(false)}>{t("detail.files.copy_file_to")}</button>
          <button type="button" className="btn btn-sm" disabled={busy || !running} onClick={() => void copyTo(true)}>{t("detail.files.copy_folder_to")}</button>
        </div>
        {!running && <p className="mt-2 kbd-hint">{t("detail.files.stopped_note")}</p>}
        {result && (
          <p className="mt-2 flex items-center gap-2" style={{ color: result.ok ? "var(--ink-2)" : "var(--bad)" }}>
            <span className="min-w-0 flex-1 break-all">{result.text}</span>
            {result.open && (
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => void openPath(result.open!)}>{t("settings.diagnostic_open")}</button>
            )}
          </p>
        )}
      </Section>
    </div>
  );
}

function InspectPanel({ data }: { data: unknown }) {
  const { t } = useTranslation();
  const text = JSON.stringify(data ?? {}, null, 2);
  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex justify-end border-b p-2" style={{ borderColor: "var(--line)" }}>
        <CopyButton text={text} label={t("detail.inspect.copy")} />
      </div>
      <pre className="mono min-h-0 flex-1 overflow-auto p-3 text-xs leading-5 select-text">{text}</pre>
    </div>
  );
}
