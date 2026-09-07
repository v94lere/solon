import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { containers } from "../api";
import { LogsPanel } from "../components/LogsPanel";
import { TerminalPanel } from "../components/TerminalPanel";
import { DebugPanel } from "../components/DebugPanel";
import { FilesPanel } from "../components/FilesPanel";

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

export function ContainerDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("overview");
  const inspect = useQuery({ queryKey: ["container", id], queryFn: () => containers.inspect(id) as Promise<Inspect>, refetchInterval: 5000 });
  const name = inspect.data?.Name?.replace(/^\//, "") ?? id.slice(0, 12);
  const running = inspect.data?.State?.Running ?? false;
  const tabs: Tab[] = ["overview", "logs", "files", "terminal", "debug", "inspect"];

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-3 pb-2">
        <button type="button" className="btn btn-ghost btn-sm" onClick={onBack}>
          ← {t("detail.back")}
        </button>
        <h1 className="text-base font-semibold">{name}</h1>
        {inspect.data?.Config?.Image && <span className="mono kbd-hint">{inspect.data.Config.Image}</span>}
        <span className={`pill ${running ? "pill-ok" : "pill-muted"}`}>{t(`containers.state.${inspect.data?.State?.Status ?? "created"}`, { defaultValue: inspect.data?.State?.Status })}</span>
      </div>
      <div role="tablist" className="flex gap-1 border-b px-4" style={{ borderColor: "var(--line)" }}>
        {tabs.map((tb) => (
          <button
            key={tb}
            role="tab"
            type="button"
            aria-selected={tab === tb}
            onClick={() => setTab(tb)}
            className="px-3 py-2"
            style={{ borderBottom: tab === tb ? "2px solid var(--accent)" : "2px solid transparent", color: tab === tb ? "var(--accent-ink)" : "var(--ink-2)", fontWeight: tab === tb ? 600 : 400 }}
          >
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
