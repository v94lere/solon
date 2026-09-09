import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { stacks, type Probe } from "../api";
import { TEMPLATES, fillProject, suggestStacks, tx, type StackFile, type StackSuggestion, type StackTemplate } from "../templates";
import { Avatar } from "./ui";
import { rememberProject } from "../projects";

type Pick = { kind: "template"; t: StackTemplate } | { kind: "suggestion"; s: StackSuggestion };

function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "") || "projet";
}

/** Galerie de piles prêtes et suggestions pour un dossier. `probeDir` : dossier ouvert sans fichier
 *  Compose, dont on propose un environnement ; sinon la galerie seule (le dossier est à choisir). */
export function StackDialog({ open, probe, onClose, onCreated }: { open: boolean; probe: Probe | null; onClose: () => void; onCreated: (dir: string, autoUp: boolean) => void }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [filter, setFilter] = useState("");
  const [pick, setPick] = useState<Pick | null>(null);
  const [dir, setDir] = useState("");
  const [name, setName] = useState("");
  const [content, setContent] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const suggestions = useMemo(() => (probe ? suggestStacks(probe) : []), [probe]);

  useEffect(() => {
    if (!open) return;
    setPick(null);
    setFilter("");
    setError(null);
    setDir(probe?.dir ?? "");
    setName(probe?.name ?? "");
  }, [open, probe]);

  // Choix d'un modèle : pré-remplit le nom et le contenu éditable du compose.yaml.
  function choose(p: Pick) {
    setPick(p);
    setError(null);
    const files = p.kind === "template" ? p.t.files : p.s.files;
    const project = probe ? probe.name : p.kind === "template" ? p.t.defaultName ?? p.t.id : "app";
    if (!probe) setName(project);
    setContent(fillProject(files[0].content, slug(project)));
  }

  const files: StackFile[] = useMemo(() => {
    if (!pick) return [];
    const src = pick.kind === "template" ? pick.t.files : pick.s.files;
    return src.map((f, i) => ({ path: f.path, content: i === 0 ? content : fillProject(f.content, slug(name || probe?.name || "app")) }));
  }, [pick, content, name, probe]);

  const visible = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return f ? TEMPLATES.filter((x) => x.name.toLowerCase().includes(f) || tx(x.tagline, lang).toLowerCase().includes(f) || x.tags.some((g) => g.includes(f))) : TEMPLATES;
  }, [filter, lang]);

  async function pickDir() {
    const chosen = (await openDialog({ directory: true, multiple: false, title: t("stacks.pick_dir") })) as string | null;
    if (chosen) setDir(chosen);
  }

  async function create(autoUp: boolean) {
    if (!pick) return;
    setBusy(true);
    setError(null);
    try {
      // Dossier existant sondé : on écrit dedans ; galerie : sous-dossier au nom du projet.
      const target = probe ? probe.dir : `${dir.replace(/[\\/]+$/, "")}\\${slug(name)}`;
      if (!probe && !dir) throw new Error(t("stacks.need_dir"));
      const created = await stacks.scaffold(target, files);
      rememberProject(created);
      onCreated(created, autoUp);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal modal-stacks" role="dialog" aria-modal="true" aria-label={t("stacks.title")}>
        <div className="flex items-center gap-3 border-b px-4 py-3" style={{ borderColor: "var(--line)" }}>
          <span className="font-semibold">{probe ? t("stacks.title_for", { name: probe.name }) : t("stacks.title")}</span>
          <span className="flex-1" />
          {!pick && <input type="search" className="input w-60" placeholder={t("stacks.search")} value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={t("stacks.search")} />}
          <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>{t("common.close")}</button>
        </div>

        {!pick ? (
          <div className="min-h-0 flex-1 overflow-auto p-4">
            {probe && (
              <section className="mb-5">
                <h3 className="stacks-section">{suggestions.some((s) => s.id !== "blank") ? t("stacks.suggested") : t("stacks.nothing_detected")}</h3>
                {suggestions.length > 0 && (
                  <div className="stacks-grid">
                    {suggestions.map((s) => (
                      <button key={s.id} type="button" className="stack-card is-suggested" onClick={() => choose({ kind: "suggestion", s })}>
                        <Avatar label={tx(s.title, lang)} seed={s.id} size={30} />
                        <span className="stack-name">{tx(s.title, lang)}</span>
                        <span className="stack-tagline">{tx(s.summary, lang)}</span>
                      </button>
                    ))}
                  </div>
                )}
              </section>
            )}
            <h3 className="stacks-section">{t("stacks.gallery")}</h3>
            <div className="stacks-grid">
              {visible.map((tpl) => (
                <button key={tpl.id} type="button" className="stack-card" onClick={() => choose({ kind: "template", t: tpl })}>
                  <Avatar label={tpl.name} seed={tpl.id} size={30} />
                  <span className="stack-name">{tpl.name}</span>
                  <span className="stack-tagline">{tx(tpl.tagline, lang)}</span>
                  <span className="stack-tags">{tpl.tags.join(" · ")}</span>
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
            <div className="flex items-center gap-3">
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => setPick(null)}>← {t("stacks.back")}</button>
              <span className="font-semibold">{pick.kind === "template" ? pick.t.name : tx(pick.s.title, lang)}</span>
            </div>
            {!probe && (
              <div className="grid gap-3" style={{ gridTemplateColumns: "1fr auto" }}>
                <label className="flex flex-col gap-1 text-[13px]">
                  {t("stacks.parent_dir")}
                  <span className="flex gap-2">
                    <input className="input mono flex-1" value={dir} onChange={(e) => setDir(e.target.value)} placeholder={"C:\\Users\\…\\Projects"} />
                    <button type="button" className="btn btn-sm" onClick={() => void pickDir()}>{t("stacks.choose")}</button>
                  </span>
                </label>
                <label className="flex flex-col gap-1 text-[13px]">
                  {t("stacks.project_name")}
                  <input className="input w-56" value={name} onChange={(e) => { setName(e.target.value); if (pick.kind === "template") setContent(fillProject(pick.t.files[0].content, slug(e.target.value))); }} />
                </label>
              </div>
            )}
            <p className="kbd-hint">{probe ? t("stacks.will_write_here", { dir: probe.dir }) : t("stacks.will_write", { dir: `${dir || "…"}\\${slug(name)}` })}{files.length > 1 ? ` · ${t("stacks.extra_files", { count: files.length - 1 })}` : ""}</p>
            <textarea className="input mono min-h-0 flex-1 resize-none p-3 text-xs leading-5" spellCheck={false} value={content} onChange={(e) => setContent(e.target.value)} aria-label="compose.yaml" style={{ height: "100%", userSelect: "text" }} />
            {error && <p role="alert" style={{ color: "var(--bad)" }}>{error}</p>}
            <div className="flex items-center gap-2">
              <span className="flex-1" />
              <button type="button" className="btn" disabled={busy} onClick={() => void create(false)}>{t("stacks.create")}</button>
              <button type="button" className="btn btn-primary" disabled={busy} onClick={() => void create(true)}>{busy ? t("compose.running") : t("stacks.create_up")}</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
