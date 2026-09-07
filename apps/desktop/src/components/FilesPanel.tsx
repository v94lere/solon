import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { files, formatBytes, type FileEntry, type FilesTarget } from "../api";
import { ConfirmDialog } from "./ConfirmDialog";
import { IconTrash } from "./Icons";

const IconFolder = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
  </svg>
);
const IconFile = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z" />
    <path d="M14 3v5h5" />
  </svg>
);
const IconDownload = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
    <path d="M12 4v11m0 0 4-4m-4 4-4-4M4 19h16" />
  </svg>
);

/** Explorateur de fichiers d'un conteneur (en marche) ou d'un volume : parcourir, télécharger, envoyer
 *  (bouton ou glisser-déposer depuis Windows), créer un dossier, supprimer. */
export function FilesPanel({ target }: { target: FilesTarget }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [segments, setSegments] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string; open?: string } | null>(null);
  const [deleting, setDeleting] = useState<FileEntry | null>(null);
  const [newFolder, setNewFolder] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const path = segments.join("/");
  const key = useMemo(() => ["files", target.kind, target.kind === "container" ? target.id : target.name, path], [target, path]);
  const query = useQuery({ queryKey: key, queryFn: () => files.list(target, path), retry: false });

  const refresh = () => queryClient.invalidateQueries({ queryKey: key.slice(0, 3) });

  async function run(label: string, action: () => Promise<void>) {
    setBusy(label);
    setMessage(null);
    try {
      await action();
      await refresh();
    } catch (e) {
      setMessage({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  }

  async function sendPaths(paths: string[]) {
    for (const p of paths) {
      await run(p, async () => {
        await files.upload(target, path, p);
        setMessage({ ok: true, text: t("files.uploaded", { name: p.split(/[\\/]/).pop() ?? p }) });
      });
    }
  }

  // Glisser-déposer depuis l'Explorateur : la fenêtre reçoit les chemins des fichiers déposés.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") setDragging(true);
        else if (event.payload.type === "leave") setDragging(false);
        else if (event.payload.type === "drop") {
          setDragging(false);
          void sendPaths(event.payload.paths);
        }
      })
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, path]);

  async function pickAndSend(directory: boolean) {
    const chosen = (await openDialog({ directory, multiple: !directory, title: t("files.pick_source") })) as string | string[] | null;
    if (!chosen) return;
    await sendPaths(Array.isArray(chosen) ? chosen : [chosen]);
  }

  async function download(entry: FileEntry | null) {
    const dest = (await openDialog({ directory: true, multiple: false, title: t("files.pick_dest") })) as string | null;
    if (!dest) return;
    const rel = entry ? [...segments, entry.name].join("/") : path;
    await run("download", async () => {
      const out = await files.download(target, rel, dest);
      setMessage({ ok: true, text: t("files.downloaded", { name: entry?.name ?? t("files.root"), dest: out }), open: out });
    });
  }

  const notRunning = query.error && String(query.error).includes("not running");

  return (
    <div className={`card flex h-full flex-col overflow-hidden ${dragging ? "is-dropping" : ""}`}>
      <div className="flex flex-wrap items-center gap-1 border-b px-3 py-2" style={{ borderColor: "var(--line)" }}>
        <nav aria-label={t("files.path")} className="flex min-w-0 flex-wrap items-center gap-1 mono text-[12.5px]">
          <button type="button" className="crumb" onClick={() => setSegments([])}>/</button>
          {segments.map((s, i) => (
            <span key={i} className="flex items-center gap-1">
              <button type="button" className="crumb" onClick={() => setSegments(segments.slice(0, i + 1))}>{s}</button>
              {i < segments.length - 1 && <span style={{ color: "var(--ink-3)" }}>/</span>}
            </span>
          ))}
        </nav>
        <span className="flex-1" />
        {segments.length > 0 && (
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => setSegments(segments.slice(0, -1))}>{t("files.up")}</button>
        )}
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => void refresh()}>{t("files.refresh")}</button>
        <button type="button" className="btn btn-ghost btn-sm" disabled={!!busy || !!notRunning} onClick={() => setNewFolder("")}>{t("files.new_folder")}</button>
        <button type="button" className="btn btn-ghost btn-sm" disabled={!!busy || !!notRunning} onClick={() => void pickAndSend(false)}>{t("files.send_files")}</button>
        <button type="button" className="btn btn-ghost btn-sm" disabled={!!busy || !!notRunning} onClick={() => void pickAndSend(true)}>{t("files.send_folder")}</button>
        <button type="button" className="btn btn-sm" disabled={!!busy || !!notRunning} onClick={() => void download(null)} title={t("files.download_here")}>
          {t("files.download_here")}
        </button>
      </div>
      {newFolder !== null && (
        <form
          className="flex items-center gap-2 border-b px-3 py-2"
          style={{ borderColor: "var(--line)" }}
          onSubmit={(e) => {
            e.preventDefault();
            const n = newFolder.trim();
            if (!n) return;
            setNewFolder(null);
            void run("mkdir", () => files.mkdir(target, [...segments, n].join("/")));
          }}
        >
          <input className="input mono flex-1" autoFocus placeholder={t("files.new_folder_name")} value={newFolder} onChange={(e) => setNewFolder(e.target.value)} aria-label={t("files.new_folder_name")} />
          <button type="submit" className="btn btn-primary btn-sm" disabled={!newFolder.trim()}>{t("files.create")}</button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => setNewFolder(null)}>{t("common.cancel")}</button>
        </form>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        {query.isLoading ? (
          <p className="p-4" style={{ color: "var(--ink-2)" }}>{t("common.loading")}</p>
        ) : query.error ? (
          <p className="p-6 text-center" style={{ color: notRunning ? "var(--ink-2)" : "var(--bad)" }}>
            {notRunning ? t("files.not_running") : String(query.error)}
          </p>
        ) : (query.data ?? []).length === 0 ? (
          <p className="p-6 text-center" style={{ color: "var(--ink-2)" }}>{t("files.empty")}</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t("files.columns.name")}</th>
                <th className="text-right">{t("files.columns.size")}</th>
                <th>{t("files.columns.modified")}</th>
                <th>{t("files.columns.mode")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {(query.data ?? []).map((e) => (
                <tr
                  key={e.name}
                  tabIndex={0}
                  onDoubleClick={() => { if (e.kind === "dir") setSegments([...segments, e.name]); }}
                  onKeyDown={(ev) => { if (ev.key === "Enter" && e.kind === "dir") setSegments([...segments, e.name]); }}
                >
                  <td>
                    <span className="flex items-center gap-2">
                      <span style={{ color: e.kind === "dir" ? "var(--accent-ink)" : "var(--ink-3)" }}>{e.kind === "dir" ? <IconFolder /> : <IconFile />}</span>
                      {e.kind === "dir" ? (
                        <button type="button" className="font-medium hover:underline" style={{ color: "var(--ink)" }} onClick={() => setSegments([...segments, e.name])}>{e.name}</button>
                      ) : (
                        <span className={e.kind === "link" ? "italic" : ""}>{e.name}</span>
                      )}
                    </span>
                  </td>
                  <td className="mono text-right whitespace-nowrap">{e.kind === "file" ? formatBytes(e.size) : "—"}</td>
                  <td className="whitespace-nowrap">{e.mtime ? new Date(e.mtime * 1000).toLocaleString() : "—"}</td>
                  <td className="mono">{e.mode}</td>
                  <td>
                    <div className="flex justify-end gap-0.5">
                      <button type="button" className="icon-btn" title={t("files.download")} aria-label={t("files.download")} disabled={!!busy} onClick={() => void download(e)}>
                        <IconDownload />
                      </button>
                      <button type="button" className="icon-btn icon-btn-danger" title={t("files.delete")} aria-label={t("files.delete")} disabled={!!busy} onClick={() => setDeleting(e)}>
                        <IconTrash />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      <div className="flex items-center gap-3 border-t px-3 py-1.5" style={{ borderColor: "var(--line)" }}>
        <span className="kbd-hint">{busy ? t("files.working") : t("files.drop_hint")}</span>
        <span className="flex-1" />
        {message && (
          <span className="flex items-center gap-2 text-[12.5px]" style={{ color: message.ok ? "var(--ink-2)" : "var(--bad)" }}>
            <span className="min-w-0 truncate" title={message.text}>{message.text}</span>
            {message.open && <button type="button" className="btn btn-ghost btn-sm" onClick={() => void openPath(message.open!)}>{t("settings.diagnostic_open")}</button>}
          </span>
        )}
      </div>
      <ConfirmDialog
        open={deleting !== null}
        title={t("files.delete_confirm.title", { name: deleting?.name ?? "" })}
        confirmLabel={t("files.delete_confirm.confirm")}
        cancelLabel={t("common.cancel")}
        danger
        onCancel={() => setDeleting(null)}
        onConfirm={() => {
          const e = deleting;
          setDeleting(null);
          if (e) void run("delete", () => files.remove(target, [...segments, e.name].join("/")));
        }}
      >
        <p>{t("files.delete_confirm.body")}</p>
      </ConfirmDialog>
    </div>
  );
}
