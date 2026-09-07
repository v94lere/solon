import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { volumes, type Volume } from "../api";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { JsonDialog } from "../components/JsonDialog";
import { FilesPanel } from "../components/FilesPanel";
import { EmptyState, IconDisk, PageHeader, SkeletonRows } from "../components/ui";

export function VolumesView() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [newName, setNewName] = useState("");
  const [removing, setRemoving] = useState<Volume | null>(null);
  const [inspecting, setInspecting] = useState<string | null>(null);
  const [browsing, setBrowsing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const query = useQuery({ queryKey: ["volumes"], queryFn: volumes.list });

  const rows = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const list = query.data?.Volumes ?? [];
    return (f ? list.filter((v) => v.Name.toLowerCase().includes(f)) : list).slice().sort((a, b) => a.Name.localeCompare(b.Name));
  }, [query.data, filter]);

  async function act(action: () => Promise<unknown>) {
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["volumes"] });
    } catch (e) {
      setError(String(e));
    }
  }

  if (browsing) {
    return (
      <div className="flex h-full flex-col">
        <div className="flex items-center gap-3 px-4 pt-3 pb-2">
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => setBrowsing(null)}>
            ← {t("volumes.title")}
          </button>
          <h1 className="text-base font-semibold">{browsing}</h1>
          <span className="kbd-hint">{t("volumes.files_hint")}</span>
        </div>
        <div className="min-h-0 flex-1 px-4 pb-4">
          <FilesPanel target={{ kind: "volume", name: browsing }} />
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={t("volumes.title")}
        count={t("volumes.count", { count: rows.length })}
        actions={
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const n = newName.trim();
              if (n) void act(() => volumes.create(n)).then(() => setNewName(""));
            }}
          >
            <input className="input input-sm w-52" placeholder={t("volumes.new_name")} value={newName} onChange={(e) => setNewName(e.target.value)} aria-label={t("volumes.new_name")} />
            <button type="submit" className="btn btn-primary btn-sm" disabled={!newName.trim()}>{t("volumes.create")}</button>
          </form>
        }
        search={<input type="search" className="input w-56" placeholder={t("volumes.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("volumes.search")} />}
      />
      {error && <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>{error}</div>}
      <div className="card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {query.isLoading ? (
          <SkeletonRows rows={4} cols={4} />
        ) : rows.length === 0 ? (
          <EmptyState icon={<IconDisk />} title={filter ? t("containers.no_match") : t("volumes.empty_title")} hint={filter ? t("containers.no_match_hint") : t("volumes.empty")} />
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("volumes.columns.name")}</th>
                <th>{t("volumes.columns.driver")}</th>
                <th>{t("volumes.columns.created")}</th>
                <th>{t("volumes.columns.mountpoint")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((v) => (
                <tr key={v.Name} tabIndex={0}>
                  <td>
                    <button type="button" className="font-medium hover:underline" style={{ color: "var(--ink)" }} onClick={() => setBrowsing(v.Name)}>{v.Name}</button>
                  </td>
                  <td>{v.Driver}</td>
                  <td>{v.CreatedAt ? new Date(v.CreatedAt).toLocaleString() : "—"}</td>
                  <td className="mono max-w-[320px] truncate" title={v.Mountpoint}>{v.Mountpoint}</td>
                  <td>
                    <div className="flex justify-end gap-1">
                      <button type="button" className="btn btn-ghost btn-sm" onClick={() => setBrowsing(v.Name)}>{t("volumes.actions.files")}</button>
                      <button type="button" className="btn btn-ghost btn-sm" onClick={() => setInspecting(v.Name)}>{t("volumes.actions.inspect")}</button>
                      <button type="button" className="btn btn-ghost btn-sm" style={{ color: "var(--bad)" }} onClick={() => setRemoving(v)}>{t("volumes.actions.remove")}</button>
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
        title={t("volumes.remove_confirm.title", { name: removing?.Name ?? "" })}
        confirmLabel={t("volumes.remove_confirm.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          const v = removing;
          setRemoving(null);
          if (v) void act(() => volumes.remove(v.Name, false));
        }}
      >
        <p>{t("volumes.remove_confirm.body")}</p>
      </ConfirmDialog>
      {inspecting && <JsonDialog title={t("volumes.actions.inspect")} load={() => volumes.inspect(inspecting)} onClose={() => setInspecting(null)} />}
    </div>
  );
}
