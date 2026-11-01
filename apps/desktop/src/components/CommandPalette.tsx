import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { containers, images, networks, volumes, engine, type ContainerSummary } from "../api";
import { loadRecentProjects, projectBaseName, projectDirOf } from "../projects";
import { SECTION_KEYS, type Section } from "../App";

export interface PaletteActions {
  go: (section: Section) => void;
  openContainer: (id: string) => void;
  openProject: (dir: string) => void;
  openTerminal: () => void;
}

interface Item {
  key: string;
  group: string;
  label: string;
  hint?: string;
  run: () => void;
}

/** Recherche globale (Ctrl+K) : sections, actions du moteur, conteneurs, images, volumes, réseaux, projets. */
export function CommandPalette({ open, onClose, actions }: { open: boolean; onClose: () => void; actions: PaletteActions }) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const [data, setData] = useState<{ containers: ContainerSummary[]; images: string[]; volumes: string[]; networks: string[] }>({ containers: [], images: [], volumes: [], networks: [] });
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setCursor(0);
    setTimeout(() => inputRef.current?.focus(), 0);
    let cancelled = false;
    void Promise.allSettled([containers.list(true), images.list(), volumes.list(), networks.list()]).then(([c, i, v, n]) => {
      if (cancelled) return;
      setData({
        containers: c.status === "fulfilled" ? c.value : [],
        images: i.status === "fulfilled" ? i.value.flatMap((im) => im.RepoTags ?? []).filter((tag) => tag !== "<none>:<none>") : [],
        volumes: v.status === "fulfilled" ? (v.value.Volumes ?? []).map((vol) => vol.Name) : [],
        networks: n.status === "fulfilled" ? n.value.map((nw) => nw.Name ?? "").filter(Boolean) : [],
      });
    });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const items = useMemo<Item[]>(() => {
    const done = (fn: () => void) => () => {
      fn();
      onClose();
    };
    const list: Item[] = [
      ...SECTION_KEYS.map((id, i) => ({ key: `go:${id}`, group: t("palette.groups.actions"), label: t(`nav.${id}`), hint: id === "terminal" ? "Ctrl+`" : `Ctrl+${i + 1}`, run: done(() => actions.go(id)) })),
      { key: "engine:start", group: t("palette.groups.actions"), label: t("engine.start"), run: done(() => void engine.start()) },
      { key: "engine:restart", group: t("palette.groups.actions"), label: t("engine.restart"), run: done(() => void engine.restart()) },
      { key: "engine:stop", group: t("palette.groups.actions"), label: t("engine.stop"), run: done(() => void engine.stop()) },
    ];
    for (const c of data.containers) {
      const name = (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
      list.push({ key: `c:${c.Id}`, group: t("palette.groups.containers"), label: name, hint: `${c.Image} · ${c.State}`, run: done(() => actions.openContainer(c.Id)) });
    }
    // Projets : récents + détectés d'après les étiquettes Compose des conteneurs.
    const projectDirs = new Map<string, string>();
    for (const dir of loadRecentProjects()) projectDirs.set(dir.toLowerCase(), dir);
    for (const c of data.containers) {
      const dir = projectDirOf(c);
      if (dir && !projectDirs.has(dir.toLowerCase())) projectDirs.set(dir.toLowerCase(), dir);
    }
    for (const dir of projectDirs.values()) {
      list.push({ key: `p:${dir}`, group: t("palette.groups.projects"), label: projectBaseName(dir), hint: dir, run: done(() => actions.openProject(dir)) });
    }
    for (const im of data.images) list.push({ key: `i:${im}`, group: t("palette.groups.images"), label: im, run: done(() => actions.go("images")) });
    for (const v of data.volumes) list.push({ key: `v:${v}`, group: t("palette.groups.volumes"), label: v, run: done(() => actions.go("volumes")) });
    for (const n of data.networks) list.push({ key: `n:${n}`, group: t("palette.groups.networks"), label: n, run: done(() => actions.go("networks")) });
    return list;
  }, [data, actions, onClose, t]);

  const filtered = useMemo(() => {
    const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0) return items.slice(0, 40);
    return items.filter((it) => {
      const hay = `${it.label} ${it.hint ?? ""} ${it.group}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    }).slice(0, 40);
  }, [items, query]);

  useEffect(() => setCursor(0), [query]);

  if (!open) return null;
  let lastGroup = "";
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal modal-palette" role="dialog" aria-modal="true" aria-label={t("palette.title")}>
        <input
          ref={inputRef}
          className="palette-input"
          placeholder={t("palette.placeholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") { e.preventDefault(); setCursor((c) => Math.min(c + 1, filtered.length - 1)); }
            else if (e.key === "ArrowUp") { e.preventDefault(); setCursor((c) => Math.max(c - 1, 0)); }
            else if (e.key === "Enter") { filtered[cursor]?.run(); }
            else if (e.key === "Escape") onClose();
          }}
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={filtered[cursor] ? `palette-${filtered[cursor].key}` : undefined}
        />
        <ul id="palette-list" role="listbox" className="palette-list">
          {filtered.length === 0 && <li className="palette-empty">{t("palette.no_results")}</li>}
          {filtered.map((it, i) => {
            const header = it.group !== lastGroup ? it.group : null;
            lastGroup = it.group;
            return (
              <li key={it.key} id={`palette-${it.key}`} role="option" aria-selected={i === cursor}>
                {header && <div className="palette-group">{header}</div>}
                <button type="button" className={`palette-item${i === cursor ? " is-active" : ""}`} onMouseEnter={() => setCursor(i)} onClick={it.run}>
                  <span className="truncate">{it.label}</span>
                  {it.hint && <span className="palette-hint truncate">{it.hint}</span>}
                </button>
              </li>
            );
          })}
        </ul>
        <div className="palette-foot">{t("palette.foot")}</div>
      </div>
    </div>
  );
}
