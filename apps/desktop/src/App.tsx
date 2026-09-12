import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from "react";
import logo from "./assets/logo.png";
import { useTranslation } from "react-i18next";
import { EngineProvider, useEngine } from "./engine";
import "./accent";
import { EngineFooter } from "./components/EngineFooter";
import { Notices } from "./components/Notices";
import { MachineTerminalPanel } from "./components/MachineTerminalPanel";
import { CommandPalette } from "./components/CommandPalette";
import { ProjectView } from "./views/ProjectView";
import { ProjectsView } from "./views/ProjectsView";
import { SetupScreen } from "./views/SetupScreen";
import { ContainersView } from "./views/ContainersView";
import { ContainerDetail, type Tab as DetailTab } from "./views/ContainerDetail";
import { ImagesView } from "./views/ImagesView";
import { VolumesView } from "./views/VolumesView";
import { NetworksView } from "./views/NetworksView";
import { ActivityView } from "./views/ActivityView";
import { SettingsView } from "./views/SettingsView";

export type Section = "projects" | "containers" | "volumes" | "images" | "networks" | "activity" | "terminal" | "settings";

/** Ordre d'affichage du menu, aussi celui des raccourcis Ctrl+1 … Ctrl+8. */
export const SECTION_KEYS: Section[] = ["projects", "containers", "volumes", "images", "networks", "activity", "terminal", "settings"];
const GROUPS: { id: "docker" | "general"; items: Section[] }[] = [
  { id: "docker", items: ["projects", "containers", "volumes", "images", "networks"] },
  { id: "general", items: ["activity", "terminal", "settings"] },
];
const SIDEBAR_KEY = "solon.sidebar";

const icons: Record<Section, JSX.Element> = {
  projects: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <rect x="3" y="3" width="8" height="8" rx="2" />
      <rect x="13" y="3" width="8" height="8" rx="2" />
      <rect x="3" y="13" width="8" height="8" rx="2" />
      <path d="M17 14v6M14 17h6" />
    </svg>
  ),
  containers: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <path d="M12 3 3.5 7.5v9L12 21l8.5-4.5v-9Z" />
      <path d="M3.5 7.5 12 12l8.5-4.5M12 12v9" />
    </svg>
  ),
  images: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <path d="M12 3 3 8l9 5 9-5-9-5Z" />
      <path d="m3 12 9 5 9-5M3 16l9 5 9-5" />
    </svg>
  ),
  volumes: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <ellipse cx="12" cy="6" rx="8" ry="3" />
      <path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
    </svg>
  ),
  networks: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c2.8 3 2.8 15 0 18M12 3c-2.8 3-2.8 15 0 18" />
    </svg>
  ),
  activity: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <path d="M3 12h4l3-8 4 16 3-8h4" />
    </svg>
  ),
  terminal: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="m7 9 3 3-3 3M12 15h5" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
    </svg>
  ),
};

const IconCollapse = ({ collapsed }: { collapsed: boolean }) => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M9 4v16" />
    {collapsed ? <path d="m13 10 2 2-2 2" /> : <path d="m16 10-2 2 2 2" />}
  </svg>
);

function loadCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_KEY) === "collapsed";
  } catch {
    return false;
  }
}

function Nav({ section, collapsed, onSelect, onToggle }: { section: Section; collapsed: boolean; onSelect: (s: Section) => void; onToggle: () => void }) {
  const { t } = useTranslation();
  const toggleLabel = collapsed ? t("nav.expand") : t("nav.collapse");
  return (
    <nav aria-label="Sections" className={`sidebar flex shrink-0 flex-col gap-0.5 p-2 ${collapsed ? "is-collapsed" : ""}`}>
      <div className="sidebar-brand">
        <img src={logo} alt="" />
        <span>{t("app.name")}</span>
        {!collapsed && (
          <button type="button" className="icon-btn sidebar-toggle" title={`${toggleLabel} (Ctrl+B)`} aria-label={toggleLabel} onClick={onToggle}>
            <IconCollapse collapsed={collapsed} />
          </button>
        )}
      </div>
      {GROUPS.map((g) => (
        <div key={g.id} className="nav-group" role="group" aria-label={t(`nav.${g.id}`)}>
          <div className="nav-group-title">{t(`nav.${g.id}`)}</div>
          {g.items.map((id) => {
            const active = id === section;
            const label = t(`nav.${id}`);
            const shortcut = `Ctrl+${SECTION_KEYS.indexOf(id) + 1}`;
            return (
              <button key={id} type="button" aria-current={active ? "page" : undefined} onClick={() => onSelect(id)} className="nav-item" title={collapsed ? `${label} (${shortcut})` : shortcut}>
                {icons[id]}
                <span>{label}</span>
              </button>
            );
          })}
        </div>
      ))}
      {collapsed && (
        <button type="button" className="nav-item" title={`${toggleLabel} (Ctrl+B)`} aria-label={toggleLabel} onClick={onToggle}>
          <IconCollapse collapsed={collapsed} />
        </button>
      )}
      <div className="sidebar-foot">
        <EngineFooter compact={collapsed} />
      </div>
    </nav>
  );
}

function Shell() {
  const { ready } = useEngine();
  const [section, setSection] = useState<Section>("projects");
  const [selected, setSelected] = useState<string | null>(null);
  const [selectedTab, setSelectedTab] = useState<DetailTab>("overview");
  const [project, setProject] = useState<string | null>(null);
  const [projectAutoUp, setProjectAutoUp] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<boolean>(loadCollapsed);
  // Le terminal n'est monté qu'à la première visite, puis reste vivant (masqué) entre deux sections.
  const [terminalMounted, setTerminalMounted] = useState(false);
  const previousSection = useRef<Section>("projects");

  useEffect(() => {
    if (!ready) {
      setSelected(null);
      setTerminalMounted(false);
    }
  }, [ready]);
  useEffect(() => {
    if (ready && section === "terminal") setTerminalMounted(true);
  }, [ready, section]);

  const go = useCallback((s: Section) => {
    setSection((cur) => {
      if (cur !== s) previousSection.current = cur;
      return s;
    });
    setSelected(null);
    setProject(null);
  }, []);
  // Depuis l'accueil Projets, la fiche d'un conteneur s'ouvre sur place (Retour revient au projet) ;
  // partout ailleurs, dans la section Conteneurs.
  const openContainer = useCallback((id: string, tab?: DetailTab) => {
    setSection((cur) => {
      if (cur !== "projects" && cur !== "containers") previousSection.current = cur;
      return cur === "projects" ? cur : "containers";
    });
    setSelectedTab(tab ?? "overview");
    setSelected(id);
  }, []);
  const openProject = useCallback((dir: string, autoUp = false) => {
    go("projects");
    setProjectAutoUp(autoUp);
    setProject(dir);
  }, [go]);
  const toggleTerminal = useCallback(() => {
    if (!ready) return;
    setPaletteOpen(false);
    setSection((cur) => {
      if (cur === "terminal") return previousSection.current === "terminal" ? "projects" : previousSection.current;
      previousSection.current = cur;
      return "terminal";
    });
  }, [ready]);
  const toggleSidebar = useCallback(() => {
    setCollapsed((c) => {
      try {
        localStorage.setItem(SIDEBAR_KEY, c ? "expanded" : "collapsed");
      } catch {
        /* préférence non mémorisée */
      }
      return !c;
    });
  }, []);
  const closePalette = useCallback(() => setPaletteOpen(false), []);
  const paletteActions = useMemo(() => ({ go, openContainer, openProject, openTerminal: toggleTerminal }), [go, openContainer, openProject, toggleTerminal]);

  // Raccourcis globaux : Ctrl+K recherche, Ctrl+` terminal, Ctrl+B barre latérale, Ctrl+1…8 sections.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      } else if (e.key === "`" || e.code === "Backquote") {
        e.preventDefault();
        toggleTerminal();
      } else if (k === "b") {
        e.preventDefault();
        toggleSidebar();
      } else if (/^[1-8]$/.test(e.key)) {
        e.preventDefault();
        go(SECTION_KEYS[Number(e.key) - 1]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go, toggleTerminal, toggleSidebar]);

  let content: JSX.Element | null;
  if (section === "settings") content = <SettingsView />;
  else if (!ready) content = <SetupScreen />;
  else if (section === "terminal") content = null; // rendu à part, pour rester monté
  else if (section === "projects") {
    if (selected) content = <ContainerDetail key={selected} id={selected} initialTab={selectedTab} onBack={() => setSelected(null)} onOpenProject={(d) => { setSelected(null); openProject(d); }} />;
    else if (project) content = <ProjectView dir={project} autoUp={projectAutoUp} onBack={() => setProject(null)} onOpenContainer={openContainer} />;
    else content = <ProjectsView onOpenProject={openProject} onOpenContainer={openContainer} />;
  } else if (section === "containers") {
    if (selected) content = <ContainerDetail key={selected} id={selected} initialTab={selectedTab} onBack={() => setSelected(null)} onOpenProject={(d) => { setSelected(null); openProject(d); }} />;
    else if (project) content = <ProjectView dir={project} autoUp={projectAutoUp} onBack={() => setProject(null)} onOpenContainer={openContainer} />;
    else content = <ContainersView onOpen={(id, tab) => { setSelectedTab(tab ?? "overview"); setSelected(id); }} onOpenProject={openProject} />;
  } else if (section === "images") content = <ImagesView onOpenContainer={openContainer} />;
  else if (section === "volumes") content = <VolumesView />;
  else if (section === "networks") content = <NetworksView />;
  else content = <ActivityView onOpenContainer={openContainer} />;

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Nav section={section} collapsed={collapsed} onSelect={go} onToggle={toggleSidebar} />
        <main className="flex min-w-0 flex-1 flex-col overflow-hidden" style={{ background: "var(--bg)" }}>
          <Notices onOpenSettings={() => go("settings")} />
          {content && (
            <div key={section} className="page min-h-0 flex-1">
              {content}
            </div>
          )}
          {terminalMounted && ready && (
            <div className="min-h-0 flex-1" hidden={section !== "terminal"}>
              <MachineTerminalPanel />
            </div>
          )}
        </main>
      </div>
      <CommandPalette open={paletteOpen} onClose={closePalette} actions={paletteActions} />
    </div>
  );
}

export function App() {
  return (
    <EngineProvider>
      <Shell />
    </EngineProvider>
  );
}
