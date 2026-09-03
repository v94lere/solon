import { useEffect, useState, type JSX } from "react";
import logo from "./assets/logo.svg";
import { useTranslation } from "react-i18next";
import { EngineProvider, useEngine } from "./engine";
import { EngineFooter } from "./components/EngineFooter";
import { SetupScreen } from "./views/SetupScreen";
import { ContainersView } from "./views/ContainersView";
import { ContainerDetail } from "./views/ContainerDetail";
import { ImagesView } from "./views/ImagesView";
import { VolumesView } from "./views/VolumesView";
import { NetworksView } from "./views/NetworksView";
import { SettingsView } from "./views/SettingsView";

export type Section = "containers" | "images" | "volumes" | "networks" | "settings";

const icons: Record<Section, JSX.Element> = {
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
  settings: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" aria-hidden="true">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
    </svg>
  ),
};

function Nav({ section, onSelect }: { section: Section; onSelect: (s: Section) => void }) {
  const { t } = useTranslation();
  const items: { id: Section; label: string }[] = [
    { id: "containers", label: t("nav.containers") },
    { id: "images", label: t("nav.images") },
    { id: "volumes", label: t("nav.volumes") },
    { id: "networks", label: t("nav.networks") },
    { id: "settings", label: t("nav.settings") },
  ];
  return (
    <nav aria-label="Sections" className="sidebar flex w-52 shrink-0 flex-col gap-0.5 p-2">
      <div className="sidebar-brand">
        <img src={logo} alt="" />
        <span>{t("app.name")}</span>
      </div>
      {items.map((it) => {
        const active = it.id === section;
        return (
          <button key={it.id} type="button" aria-current={active ? "page" : undefined} onClick={() => onSelect(it.id)} className="nav-item">
            {icons[it.id]}
            <span>{it.label}</span>
          </button>
        );
      })}
      <div className="sidebar-foot">
        <EngineFooter />
      </div>
    </nav>
  );
}

function Shell() {
  const { ready } = useEngine();
  const [section, setSection] = useState<Section>("containers");
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    if (!ready) setSelected(null);
  }, [ready]);

  let content;
  if (section === "settings") content = <SettingsView />;
  else if (!ready) content = <SetupScreen />;
  else if (section === "containers") content = selected ? <ContainerDetail id={selected} onBack={() => setSelected(null)} /> : <ContainersView onOpen={setSelected} />;
  else if (section === "images") content = <ImagesView />;
  else if (section === "volumes") content = <VolumesView />;
  else content = <NetworksView />;

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Nav
          section={section}
          onSelect={(s) => {
            setSection(s);
            setSelected(null);
          }}
        />
        <main className="min-w-0 flex-1 overflow-hidden" style={{ background: "var(--bg)" }}>
          {content}
        </main>
      </div>
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
