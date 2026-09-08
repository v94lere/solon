// Projets Compose connus : dossiers ouverts récemment (stockage local) et identification des conteneurs
// par leurs étiquettes Compose.
import type { ContainerSummary } from "./api";

const RECENT_KEY = "monodon.compose.recent";
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

/** `/mnt/host/c/Users/x/proj` (chemin vu par le moteur) → `C:\Users\x\proj`. */
export function hostPathFromGuest(guest: string | undefined): string | null {
  if (!guest) return null;
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
