import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ContainerSummary } from "../api";

export type PortRow = { host: number; guest: number; proto: string };

export function publishedPorts(c: ContainerSummary): PortRow[] {
  const seen = new Map<string, PortRow>();
  for (const p of c.Ports ?? []) {
    if (p.PublicPort) seen.set(`${p.PublicPort}/${p.Type}`, { host: p.PublicPort, guest: p.PrivatePort, proto: p.Type ?? "tcp" });
  }
  return [...seen.values()].sort((a, b) => a.host - b.host);
}

/** Liste des ports publiés ; un port TCP d'un conteneur en marche est un lien vers http://localhost:<port>. */
export function PortLinks({ c, running }: { c: ContainerSummary; running: boolean }) {
  const { t } = useTranslation();
  const ports = publishedPorts(c);
  if (ports.length === 0) return null;
  return (
    <span className="flex flex-wrap gap-x-2">
      {ports.map((p) => {
        const text = `${p.host}→${p.guest}/${p.proto}`;
        const url = `http://localhost:${p.host}/`;
        return running && p.proto === "tcp" ? (
          <button key={text} type="button" className="port-link mono" title={t("containers.open_port", { url })} onClick={() => void openUrl(url)}>
            {text}
          </button>
        ) : (
          <span key={text}>{text}</span>
        );
      })}
    </span>
  );
}
