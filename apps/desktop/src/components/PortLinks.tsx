import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ContainerSummary } from "../api";
import { useEngine } from "../engine";
import { LABEL_PROJECT, LABEL_SERVICE } from "../projects";

export type PortRow = { host: number; guest: number; proto: string };

export function publishedPorts(c: ContainerSummary): PortRow[] {
  const seen = new Map<string, PortRow>();
  for (const p of c.Ports ?? []) {
    if (p.PublicPort) seen.set(`${p.PublicPort}/${p.Type}`, { host: p.PublicPort, guest: p.PrivatePort, proto: p.Type ?? "tcp" });
  }
  return [...seen.values()].sort((a, b) => a.host - b.host);
}

/** Liste des ports publiés ; un port TCP d'un conteneur en marche est un lien vers http://localhost:<port>. */
function sanitize(label: string): string {
  return label.replace(/^\//, "").toLowerCase().replace(/[^a-z0-9._-]/g, "").replace(/[._]/g, "-").replace(/^-+|-+$/g, "");
}

/** Domaine local d'un conteneur : `service.projet.solon.local` pour Compose, sinon `nom.solon.local`. */
export function localDomain(c: ContainerSummary): string | null {
  const project = c.Labels?.[LABEL_PROJECT];
  const service = c.Labels?.[LABEL_SERVICE];
  if (project && service) {
    const p = sanitize(project);
    const s = sanitize(service);
    if (p && s) return `${s}.${p}.solon.local`;
  }
  const name = sanitize(c.Names?.[0] ?? "");
  return name ? `${name}.solon.local` : null;
}

export function PortLinks({ c, running }: { c: ContainerSummary; running: boolean }) {
  const { t } = useTranslation();
  const { snapshot } = useEngine();
  const ports = publishedPorts(c);
  // Domaine local : pour tout conteneur en marche, port publié ou non (HTTPS si l'autorité locale est active).
  const domain = running && snapshot?.local_domains ? localDomain(c) : null;
  const scheme = snapshot?.local_domains_tls ? "https" : "http";
  if (ports.length === 0 && !domain) return null;
  return (
    <span className="flex flex-wrap gap-x-2">
      {domain && (
        <button type="button" className="port-link mono" title={t("containers.open_port", { url: `${scheme}://${domain}/` })} onClick={() => void openUrl(`${scheme}://${domain}/`)}>
          {domain}
        </button>
      )}
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
