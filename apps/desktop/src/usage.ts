// Qui utilise quoi : images, volumes et réseaux rapprochés de la liste de tous les conteneurs (actifs ou non).
import type { ContainerSummary, ImageSummary } from "./api";

export interface Usage {
  /** Conteneurs qui s'appuient sur l'objet, en cours d'exécution ou non. */
  total: number;
  running: number;
  names: string[];
}

const EMPTY: Usage = { total: 0, running: 0, names: [] };

function collect(list: ContainerSummary[], matches: (c: ContainerSummary) => boolean): Usage {
  const hits = list.filter(matches);
  if (hits.length === 0) return EMPTY;
  return {
    total: hits.length,
    running: hits.filter((c) => c.State === "running" || c.State === "paused").length,
    names: hits.map((c) => (c.Names?.[0] ?? c.Id.slice(0, 12)).replace(/^\//, "")).sort(),
  };
}

/** Conteneurs créés à partir de l'image (par identifiant, ou par étiquette quand l'image a été re-taguée). */
export function imageUsage(list: ContainerSummary[], img: ImageSummary): Usage {
  const id = img.Id;
  const short = id.replace(/^sha256:/, "");
  const tags = new Set(img.RepoTags ?? []);
  return collect(list, (c) => {
    const cid = c.ImageID ?? "";
    if (cid === id || cid.replace(/^sha256:/, "") === short) return true;
    return tags.has(c.Image) || tags.has(`${c.Image}:latest`);
  });
}

/** Conteneurs qui montent le volume nommé. */
export function volumeUsage(list: ContainerSummary[], name: string): Usage {
  return collect(list, (c) => (c.Mounts ?? []).some((m) => m.Type === "volume" && m.Name === name));
}

/** Conteneurs attachés au réseau. */
export function networkUsage(list: ContainerSummary[], name: string): Usage {
  return collect(list, (c) => Boolean(c.NetworkSettings?.Networks && name in c.NetworkSettings.Networks));
}
