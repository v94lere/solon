import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { networks, type Network } from "../api";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { JsonDialog } from "../components/JsonDialog";
import { EmptyState, IconGlobe, PageHeader, SkeletonRows } from "../components/ui";

const BUILTIN = new Set(["bridge", "host", "none"]);

export function NetworksView() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [newName, setNewName] = useState("");
  const [removing, setRemoving] = useState<Network | null>(null);
  const [inspecting, setInspecting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const query = useQuery({ queryKey: ["networks"], queryFn: networks.list });

  const rows = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const list = query.data ?? [];
    return (f ? list.filter((n) => n.Name.toLowerCase().includes(f) || (n.Driver ?? "").includes(f)) : list).slice().sort((a, b) => a.Name.localeCompare(b.Name));
  }, [query.data, filter]);

  async function act(action: () => Promise<unknown>) {
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["networks"] });
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("networks.title")}
        count={t("networks.count", { count: rows.length })}
        actions={
          <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); const n = newName.trim(); if (n) void act(() => networks.create(n)).then(() => setNewName("")); }}>
            <input className="input input-sm w-52" placeholder={t("networks.new_name")} value={newName} onChange={(e) => setNewName(e.target.value)} aria-label={t("networks.new_name")} />
            <button type="submit" className="btn btn-primary btn-sm" disabled={!newName.trim()}>{t("networks.create")}</button>
          </form>
        }
        search={<input type="search" className="input w-56" placeholder={t("networks.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("networks.search")} />}
      />
      {error && <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>}
      <div className="card list-card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {query.isLoading ? (
          <SkeletonRows rows={4} cols={4} />
        ) : rows.length === 0 ? (
          <EmptyState icon={<IconGlobe />} title={filter ? t("containers.no_match") : t("networks.empty_title")} hint={filter ? t("containers.no_match_hint") : t("networks.empty")} />
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("networks.columns.name")}</th>
                <th className="col-driver">{t("networks.columns.driver")}</th>
                <th className="col-subnet">{t("networks.columns.subnet")}</th>
                <th className="col-id">{t("networks.columns.id")}</th>
                <th className="col-actions-wide" />
              </tr>
            </thead>
            <tbody>
              {rows.map((n) => (
                <tr key={n.Id} tabIndex={0}>
                  <td className="font-medium">{n.Name}</td>
                  <td className="col-driver">{n.Driver}</td>
                  <td className="col-subnet mono">{(n.IPAM?.Config ?? []).map((c) => c.Subnet).filter(Boolean).join(", ")}</td>
                  <td className="col-id mono">{n.Id.slice(0, 12)}</td>
                  <td>
                    <div className="flex justify-end gap-1">
                      <button type="button" className="btn btn-ghost btn-sm" onClick={() => setInspecting(n.Id)}>{t("networks.actions.inspect")}</button>
                      {!BUILTIN.has(n.Name) && (
                        <button type="button" className="btn btn-ghost btn-sm" style={{ color: "var(--bad)" }} onClick={() => setRemoving(n)}>{t("networks.actions.remove")}</button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      <ConfirmDialog
        open={removing !== null}
        title={t("networks.remove_confirm.title", { name: removing?.Name ?? "" })}
        confirmLabel={t("networks.remove_confirm.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          const n = removing;
          setRemoving(null);
          if (n) void act(() => networks.remove(n.Id));
        }}
      >
        <p>{t("networks.remove_confirm.body")}</p>
      </ConfirmDialog>
      {inspecting && <JsonDialog title={t("networks.actions.inspect")} load={() => networks.inspect(inspecting)} onClose={() => setInspecting(null)} />}
    </div>
  );
}
