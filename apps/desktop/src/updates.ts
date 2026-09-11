// Vérification des nouvelles versions : une requête vers l'API GitHub des releases, au lancement
// (si l'utilisateur l'accepte, réglable) ou à la demande. Rien n'est envoyé sur l'utilisateur ni sur
// son installation : la requête ne porte que l'adresse du dépôt. Sans télémétrie, sans mise à jour
// automatique : on informe, et un bouton ouvre le téléchargement.
import { getVersion } from "@tauri-apps/api/app";

export const RELEASES_URL = "https://github.com/v94lere/solon/releases";
const API = "https://api.github.com/repos/v94lere/solon/releases/latest";
const KEY_ENABLED = "solon-update-check";
const KEY_SKIP = "solon-update-skip";
const KEY_LAST = "solon-update-last";

export interface UpdateInfo {
  /** Version installée (`0.1.1`). */
  current: string;
  /** Dernière version publiée (`0.1.2`). */
  latest: string;
  /** La dernière version est plus récente que celle installée. */
  available: boolean;
  /** Page de la version sur GitHub. */
  page: string;
  /** Installateur `.exe` de cette version, s'il est joint à la release. */
  installer: string | null;
  /** Notes de version (Markdown brut), tronquées. */
  notes: string;
  checkedAt: number;
}

export function updateCheckEnabled(): boolean {
  try {
    return localStorage.getItem(KEY_ENABLED) !== "off";
  } catch {
    return true;
  }
}

export function setUpdateCheckEnabled(on: boolean) {
  try {
    localStorage.setItem(KEY_ENABLED, on ? "on" : "off");
  } catch {
    /* stockage indisponible */
  }
}

/** Version que l'utilisateur a choisi d'ignorer avec « Plus tard » (bannière masquée pour elle). */
export function skippedVersion(): string | null {
  try {
    return localStorage.getItem(KEY_SKIP);
  } catch {
    return null;
  }
}

export function skipVersion(v: string) {
  try {
    localStorage.setItem(KEY_SKIP, v);
  } catch {
    /* stockage indisponible */
  }
}

export function lastCheck(): UpdateInfo | null {
  try {
    const raw = localStorage.getItem(KEY_LAST);
    return raw ? (JSON.parse(raw) as UpdateInfo) : null;
  } catch {
    return null;
  }
}

/** Compare deux versions `a.b.c[-pré]` : négatif si `a` < `b`, 0 si égales, positif sinon. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const [core, pre] = v.trim().replace(/^v/i, "").split("-", 2);
    return { nums: core.split(".").map((x) => Number.parseInt(x, 10) || 0), pre: pre ?? null };
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < Math.max(x.nums.length, y.nums.length); i++) {
    const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
    if (d !== 0) return d;
  }
  // Une pré-version (0.1.2-beta.1) précède la version finale (0.1.2).
  if (x.pre && !y.pre) return -1;
  if (!x.pre && y.pre) return 1;
  return (x.pre ?? "").localeCompare(y.pre ?? "");
}

interface Release {
  tag_name: string;
  html_url: string;
  body?: string | null;
  draft?: boolean;
  prerelease?: boolean;
  assets?: { name: string; browser_download_url: string }[];
}

/** Interroge GitHub ; lève une erreur lisible si le réseau ou l'API ne répondent pas. */
export async function checkForUpdate(): Promise<UpdateInfo> {
  const current = await getVersion();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  let release: Release;
  try {
    const res = await fetch(API, { headers: { Accept: "application/vnd.github+json" }, signal: controller.signal, cache: "no-store" });
    if (!res.ok) throw new Error(`GitHub API: HTTP ${res.status}`);
    release = (await res.json()) as Release;
  } finally {
    clearTimeout(timer);
  }
  const latest = release.tag_name.replace(/^v/i, "");
  const installer = release.assets?.find((a) => /-setup\.exe$/i.test(a.name))?.browser_download_url ?? null;
  const info: UpdateInfo = {
    current,
    latest,
    available: compareVersions(latest, current) > 0,
    page: release.html_url,
    installer,
    notes: (release.body ?? "").slice(0, 2000),
    checkedAt: Date.now(),
  };
  try {
    localStorage.setItem(KEY_LAST, JSON.stringify(info));
  } catch {
    /* stockage indisponible */
  }
  return info;
}
