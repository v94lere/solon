import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { containers, formatBytes, type ContainerSummary, type StatSample } from "../api";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ComposePanel } from "../components/ComposePanel";

const COMPOSE_LABEL = "com.docker.compose.project";

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

function portsText(c: ContainerSummary): string {
  const seen = new Set<string>();
  for (const p of c.Ports ?? []) {
    if (p.PublicPort) seen.add(`${p.PublicPort}→${p.PrivatePort}/${p.Type}`);
  }
  return [...seen].sort().join(", ");
}

export function ContainersView({ onOpen }: { onOpen: (id: string) => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showStopped, setShowStopped] = useState(true);
  const [filter, setFilter] = useState("");
  const [stats, setStats] = useState<Record<string, StatSample>>({});
  const [removing, setRemoving] = useState<ContainerSummary | null>(null);
  const [removeVolumes, setRemoveVolumes] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const query = useQuery({ queryKey: ["containers", showStopped], queryFn: () => containers.list(showStopped) });

  // Statistiques en flux : un seul canal pour tous les conteneurs en marche.
  useEffect(() => {
    let streamId: number | null = null;
    let cancelled = false;
    containers
      .statsOpen((s) => setStats((prev) => ({ ...prev, [s.id]: s })))
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

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold">{t("containers.title")}</h1>
        <span className="kbd-hint">{t("containers.count", { count: rows.length })}</span>
        <div className="flex-1" />
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={showStopped} onChange={(e) => setShowStopped(e.target.checked)} />
          {t("containers.show_stopped")}
        </label>
        <input type="search" className="input w-72" placeholder={t("containers.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("containers.search")} />
      </div>
      <ComposePanel />
      {error && (
        <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>
          {error}
        </div>
      )}
      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {query.isLoading ? (
          <p className="p-4" style={{ color: "var(--ink-2)" }}>
            {t("common.loading")}
          </p>
        ) : rows.length === 0 ? (
          <p className="p-6 text-center" style={{ color: "var(--ink-2)" }}>
            {t("containers.empty")}
          </p>
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
                  busy={busy}
                  onOpen={onOpen}
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
  busy,
  onOpen,
  onAct,
  onRemove,
}: {
  project: string;
  list: ContainerSummary[];
  stats: Record<string, StatSample>;
  busy: string | null;
  onOpen: (id: string) => void;
  onAct: (id: string, action: () => Promise<void>) => Promise<void>;
  onRemove: (c: ContainerSummary) => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      {project && (
        <tr>
          <td colSpan={7} className="!py-1 text-[11.5px] font-semibold uppercase tracking-wide" style={{ color: "var(--accent-ink)", background: "var(--surface-2)" }}>
            {t("containers.compose")} · {project}
          </td>
        </tr>
      )}
      {list.map((c) => {
        const name = (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
        const s = stats[c.Id];
        const running = c.State === "running";
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
              <button type="button" className="font-medium hover:underline" onClick={() => onOpen(c.Id)} style={{ color: "var(--ink)" }}>
                {name}
              </button>
            </td>
            <td className="mono max-w-[260px] truncate" title={c.Image}>
              {c.Image}
            </td>
            <td>
              <span className={`pill ${stateClass(c.State)}`} title={c.Status}>
                {t(`containers.state.${c.State}`, { defaultValue: c.State })}
              </span>
            </td>
            <td className="mono">{portsText(c)}</td>
            <td className="mono text-right">{running && s ? `${s.cpu_percent.toFixed(1)} %` : "—"}</td>
            <td className="mono text-right">{running && s ? formatBytes(s.mem_usage) : "—"}</td>
            <td>
              <div className="flex justify-end gap-1">
                {running ? (
                  <>
                    <button type="button" className="btn btn-ghost btn-sm" disabled={busy === c.Id} onClick={() => void onAct(c.Id, () => containers.stop(c.Id))}>
                      {t("containers.actions.stop")}
                    </button>
                    <button type="button" className="btn btn-ghost btn-sm" disabled={busy === c.Id} onClick={() => void onAct(c.Id, () => containers.restart(c.Id))}>
                      {t("containers.actions.restart")}
                    </button>
                  </>
                ) : (
                  <button type="button" className="btn btn-ghost btn-sm" disabled={busy === c.Id} onClick={() => void onAct(c.Id, () => containers.start(c.Id))}>
                    {t("containers.actions.start")}
                  </button>
                )}
                <button type="button" className="btn btn-ghost btn-sm" onClick={() => onOpen(c.Id)}>
                  {t("containers.actions.logs")}
                </button>
                <button type="button" className="btn btn-ghost btn-sm" style={{ color: "var(--bad)" }} disabled={busy === c.Id} onClick={() => onRemove(c)}>
                  {t("containers.actions.remove")}
                </button>
              </div>
            </td>
          </tr>
        );
      })}
    </>
  );
}
