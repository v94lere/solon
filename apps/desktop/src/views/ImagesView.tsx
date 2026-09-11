import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { formatBytes, images, type ImageSummary } from "../api";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { JsonDialog } from "../components/JsonDialog";
import { RunImageDialog } from "../components/RunImageDialog";
import { EmptyState, IconLayers, PageHeader, SkeletonRows } from "../components/ui";

export function ImagesView() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [removing, setRemoving] = useState<ImageSummary | null>(null);
  const [running, setRunning] = useState<ImageSummary | null>(null);
  const [inspecting, setInspecting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const query = useQuery({ queryKey: ["images"], queryFn: images.list });

  const rows = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const list = (query.data ?? []).flatMap((img) => {
      const tags = img.RepoTags && img.RepoTags.length > 0 ? img.RepoTags : ["<none>:<none>"];
      return tags.map((tag) => ({ img, tag }));
    });
    return (f ? list.filter((r) => r.tag.toLowerCase().includes(f) || r.img.Id.includes(f)) : list).sort((a, b) => b.img.Created - a.img.Created);
  }, [query.data, filter]);

  async function act(action: () => Promise<unknown>) {
    setError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ["images"] });
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <PageHeader title={t("images.title")} count={t("images.count", { count: rows.length })} search={<input type="search" className="input w-64" placeholder={t("images.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("images.search")} />} />
      {error && (
        <div className="mx-4 mb-2 rounded px-3 py-2" role="alert" style={{ background: "var(--bad-soft)", color: "var(--bad)" }}>
          {error}
        </div>
      )}
      <div className="card list-card mx-4 mb-4 min-h-0 flex-1 overflow-auto">
        {query.isLoading ? (
          <SkeletonRows rows={5} cols={4} />
        ) : rows.length === 0 ? (
          <EmptyState icon={<IconLayers />} title={filter ? t("containers.no_match") : t("images.empty_title")} hint={filter ? t("containers.no_match_hint") : t("images.empty")}>
            {!filter && <code className="start-code">docker pull nginx</code>}
          </EmptyState>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("images.columns.tag")}</th>
                <th className="col-id">{t("images.columns.id")}</th>
                <th className="col-size text-right">{t("images.columns.size")}</th>
                <th className="col-date">{t("images.columns.created")}</th>
                <th className="col-actions-wide" />
              </tr>
            </thead>
            <tbody>
              {rows.map(({ img, tag }) => (
                <tr key={`${img.Id}-${tag}`} tabIndex={0}>
                  <td className="font-medium">{tag}</td>
                  <td className="col-id mono">{img.Id.replace(/^sha256:/, "").slice(0, 12)}</td>
                  <td className="col-size mono text-right">{formatBytes(img.Size)}</td>
                  <td className="col-date">{new Date(img.Created * 1000).toLocaleString()}</td>
                  <td>
                    <div className="flex justify-end gap-1">
                      <button type="button" className="btn btn-ghost btn-sm" onClick={() => setRunning(img)}>{t("images.actions.run")}</button>
                      <button type="button" className="btn btn-ghost btn-sm" onClick={() => setInspecting(img.Id)}>{t("images.actions.inspect")}</button>
                      <button type="button" className="btn btn-ghost btn-sm" style={{ color: "var(--bad)" }} onClick={() => setRemoving(img)}>{t("images.actions.remove")}</button>
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
        title={t("images.remove_confirm.title", { name: removing?.RepoTags?.[0] ?? removing?.Id.slice(7, 19) ?? "" })}
        confirmLabel={t("images.remove_confirm.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        onCancel={() => setRemoving(null)}
        onConfirm={() => {
          const img = removing;
          setRemoving(null);
          if (img) void act(() => images.remove(img.RepoTags?.[0] ?? img.Id, true));
        }}
      >
        <p>{t("images.remove_confirm.body")}</p>
      </ConfirmDialog>

      {running && (
        <RunImageDialog
          image={running.RepoTags?.[0] ?? running.Id}
          onClose={() => setRunning(null)}
          onStarted={() => {
            setRunning(null);
            void queryClient.invalidateQueries({ queryKey: ["containers"] });
          }}
        />
      )}
      {inspecting && <JsonDialog title={t("images.actions.inspect")} load={() => images.inspect(inspecting)} onClose={() => setInspecting(null)} />}
    </div>
  );
}
