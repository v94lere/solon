import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { containers } from "../api";
import { LogsPanel } from "../components/LogsPanel";
import { TerminalPanel } from "../components/TerminalPanel";

type Tab = "logs" | "terminal" | "inspect";

interface Inspect {
  Name?: string;
  State?: { Status?: string; Running?: boolean };
  Config?: { Image?: string };
}

export function ContainerDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("logs");
  const inspect = useQuery({ queryKey: ["container", id], queryFn: () => containers.inspect(id) as Promise<Inspect> });
  const name = inspect.data?.Name?.replace(/^\//, "") ?? id.slice(0, 12);
  const running = inspect.data?.State?.Running ?? false;
  const tabs: Tab[] = ["logs", "terminal", "inspect"];

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-4 pt-3 pb-2">
        <button type="button" className="btn btn-ghost btn-sm" onClick={onBack}>
          ← {t("detail.back")}
        </button>
        <h1 className="text-base font-semibold">{name}</h1>
        {inspect.data?.Config?.Image && <span className="mono kbd-hint">{inspect.data.Config.Image}</span>}
        <span className={`pill ${running ? "pill-ok" : "pill-muted"}`}>{t(`containers.state.${inspect.data?.State?.Status ?? "created"}`, { defaultValue: inspect.data?.State?.Status })}</span>
      </div>
      <div role="tablist" className="flex gap-1 border-b px-4" style={{ borderColor: "var(--line)" }}>
        {tabs.map((tb) => (
          <button
            key={tb}
            role="tab"
            type="button"
            aria-selected={tab === tb}
            onClick={() => setTab(tb)}
            className="px-3 py-2"
            style={{ borderBottom: tab === tb ? "2px solid var(--accent)" : "2px solid transparent", color: tab === tb ? "var(--accent-ink)" : "var(--ink-2)", fontWeight: tab === tb ? 600 : 400 }}
          >
            {t(`detail.tabs.${tb}`)}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 p-4" role="tabpanel">
        {tab === "logs" && <LogsPanel id={id} />}
        {tab === "terminal" && <TerminalPanel id={id} running={running} />}
        {tab === "inspect" && <InspectPanel data={inspect.data} />}
      </div>
    </div>
  );
}

function InspectPanel({ data }: { data: unknown }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const text = JSON.stringify(data ?? {}, null, 2);
  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex justify-end border-b p-2" style={{ borderColor: "var(--line)" }}>
        <button
          type="button"
          className="btn btn-sm"
          onClick={() => {
            void navigator.clipboard.writeText(text).then(() => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            });
          }}
        >
          {copied ? t("common.copied") : t("detail.inspect.copy")}
        </button>
      </div>
      <pre className="mono min-h-0 flex-1 overflow-auto p-3 text-xs leading-5">{text}</pre>
    </div>
  );
}
