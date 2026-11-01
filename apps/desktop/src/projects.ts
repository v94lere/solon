// Projets Compose connus : dossiers ouverts récemment (stockage local) et identification des conteneurs
// par leurs étiquettes Compose.
import type { ContainerSummary } from "./api";

const RECENT_KEY = "solon.compose.recent";
export const LABEL_PROJECT = "com.docker.compose.project";
export const LABEL_SERVICE = "com.docker.compose.service";
export const LABEL_WORKDIR = "com.docker.compose.project.working_dir";

export function loadRecentProjects(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as unknown;
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function rememberProject(dir: string) {
  const next = [dir, ...loadRecentProjects().filter((x) => x.toLowerCase() !== dir.toLowerCase())].slice(0, 12);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* stockage indisponible */
  }
}

export function forgetProject(dir: string) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(loadRecentProjects().filter((x) => x.toLowerCase() !== dir.toLowerCase())));
  } catch {
    /* ignoré */
  }
}

/** `/mnt/host/c/Users/x/proj` (chemin vu par le moteur) → `C:\Users\x\proj`. Un projet lancé avec le CLI
 *  Windows porte déjà un chemin Windows dans son étiquette : il est gardé tel quel. */
export function hostPathFromGuest(guest: string | undefined): string | null {
  if (!guest) return null;
  if (/^[a-z]:[\\/]/i.test(guest)) return guest.replace(/\//g, "\\");
  const m = /^\/mnt\/host\/([a-z])(\/.*)?$/i.exec(guest);
  if (!m) return null;
  return `${m[1].toUpperCase()}:${(m[2] ?? "/").replace(/\//g, "\\")}`;
}

/** Dossier Windows d'un conteneur Compose, d'après son étiquette `working_dir`. */
export function projectDirOf(c: ContainerSummary): string | null {
  return hostPathFromGuest(c.Labels?.[LABEL_WORKDIR]);
}

export function projectNameOf(c: ContainerSummary): string | null {
  return c.Labels?.[LABEL_PROJECT] ?? null;
}

export function serviceNameOf(c: ContainerSummary): string {
  return c.Labels?.[LABEL_SERVICE] ?? (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "");
}

/** Nom court d'un dossier projet (dernier segment). */
export function projectBaseName(dir: string): string {
  return dir.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? dir;
}

export function samePath(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  return a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();
}

/** Les conteneurs mis en pause par Solon (réveil à la demande) répondent à la première requête : pour
 *  les cartes, les adresses et les compteurs, ils comptent comme en marche. */
export function withSleeping(list: ContainerSummary[], sleeping: string[]): ContainerSummary[] {
  if (sleeping.length === 0) return list;
  return list.map((c) => (c.State === "paused" && sleeping.includes(c.Id) ? { ...c, State: "running", Status: c.Status.replace(/\s*\(Paused\)/, "") } : c));
}

// ---- Adresse principale et dernière activité d'un projet ----

/** Ports HTTP habituels, par ordre de préférence (même liste que le mandataire du service). */
export const HTTP_PORTS = [80, 8080, 3000, 8000, 8069, 5000, 4200, 5173, 8888, 9000, 443, 8025, 2368, 3001, 5678, 8081, 8088];
/** Ports de bases de données et de files : jamais l'adresse « principale » d'un projet. */
export const DB_PORTS = new Set([5432, 3306, 33060, 6379, 27017, 1025, 5672, 11211, 9200, 2181, 9092, 22]);

function sanitizeLabel(label: string): string {
  return label.replace(/^\//, "").toLowerCase().replace(/[^a-z0-9._-]/g, "").replace(/[._]/g, "-").replace(/^-+|-+$/g, "");
}

/** Domaine local d'un conteneur : `service.projet.solon.local` pour Compose, sinon `nom.solon.local`. */
export function domainOf(c: ContainerSummary): string | null {
  const project = c.Labels?.[LABEL_PROJECT];
  const service = c.Labels?.[LABEL_SERVICE];
  if (project && service) {
    const p = sanitizeLabel(project);
    const s = sanitizeLabel(service);
    if (p && s) return `${s}.${p}.solon.local`;
  }
  const name = sanitizeLabel(c.Names?.[0] ?? "");
  return name ? `${name}.solon.local` : null;
}

/** Score « c'est une application web » d'un conteneur, d'après ses ports (exposés ou publiés). */
function webScore(c: ContainerSummary): number {
  let best = -1;
  for (const p of c.Ports ?? []) {
    if (p.Type && p.Type !== "tcp") continue;
    if (DB_PORTS.has(p.PrivatePort)) continue;
    const rank = HTTP_PORTS.indexOf(p.PrivatePort);
    const score = (rank >= 0 ? 100 - rank : 10) + (p.PublicPort ? 50 : 0);
    best = Math.max(best, score);
  }
  return best;
}

/** Le conteneur d'un projet qui porte l'adresse à mettre en avant : le service web le plus probable,
 *  en marche de préférence. `null` si aucun conteneur n'a de port qui ressemble à du web. */
export function primaryContainer(list: ContainerSummary[]): ContainerSummary | null {
  let best: ContainerSummary | null = null;
  let bestScore = -1;
  // Seuls les conteneurs en marche portent une adresse qui répond : un service web arrêté n'en a pas.
  for (const c of list.filter((c) => c.State === "running")) {
    const s = webScore(c);
    if (s > bestScore && s >= 0) {
      best = c;
      bestScore = s;
    }
  }
  return best;
}

/** Adresse principale d'un projet (`https://web.proj.solon.local`), ou `null`. */
export function primaryAddress(list: ContainerSummary[], tls: boolean): { url: string; host: string; container: ContainerSummary } | null {
  const c = primaryContainer(list);
  if (!c) return null;
  const host = domainOf(c);
  if (!host) return null;
  return { url: `${tls ? "https" : "http"}://${host}/`, host, container: c };
}

/** Texte « dernière activité » d'un groupe : le statut Docker du conteneur en marche le plus récent
 *  (« Up 12 minutes »), sinon la date de création la plus récente. */
export function lastActivity(list: ContainerSummary[]): { running: boolean; text: string; created: number } {
  const running = list.filter((c) => c.State === "running");
  const created = Math.max(0, ...list.map((c) => c.Created ?? 0));
  if (running.length > 0) {
    const newest = running.reduce((a, b) => ((a.Created ?? 0) >= (b.Created ?? 0) ? a : b));
    return { running: true, text: newest.Status ?? "", created };
  }
  return { running: false, text: "", created };
}
