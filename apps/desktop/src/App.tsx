import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { EngineProvider, useEngine } from "./engine";
import { EngineBar } from "./components/EngineBar";
import { SetupScreen } from "./views/SetupScreen";
import { ContainersView } from "./views/ContainersView";
import { ContainerDetail } from "./views/ContainerDetail";
import { ImagesView } from "./views/ImagesView";
import { VolumesView } from "./views/VolumesView";
import { NetworksView } from "./views/NetworksView";
import { SettingsView } from "./views/SettingsView";

export type Section = "containers" | "images" | "volumes" | "networks" | "settings";

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
    <nav aria-label="Sections" className="flex w-48 shrink-0 flex-col gap-0.5 border-r p-2" style={{ borderColor: "var(--line)", background: "var(--surface-2)" }}>
      <div className="px-2 pt-1 pb-3 text-[15px] font-semibold tracking-tight">{t("app.name")}</div>
      {items.map((it) => {
        const active = it.id === section;
        return (
          <button
            key={it.id}
            type="button"
            aria-current={active ? "page" : undefined}
            onClick={() => onSelect(it.id)}
            className="rounded px-2 py-1.5 text-left"
            style={{ background: active ? "var(--accent-soft)" : "transparent", color: active ? "var(--accent-ink)" : "var(--ink)", fontWeight: active ? 600 : 400 }}
          >
            {it.label}
          </button>
        );
      })}
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
      <EngineBar />
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
