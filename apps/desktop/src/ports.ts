// Ports publiés dans un fichier Compose (forme courte `"hôte:conteneur"`, avec adresse ou protocole
// facultatifs) : lecture, remplacement d'un port hôte, et repérage d'un conflit dans la sortie
// de `docker compose up`.

/** Ports hôte publiés dans `text`, dédoublonnés, dans l'ordre d'apparition. */
export function publishedHostPorts(text: string): number[] {
  const out: number[] = [];
  for (const m of text.matchAll(/^\s*-\s*["']?(?:(?:\d{1,3}\.){3}\d{1,3}:)?(\d{1,5})(?:-\d{1,5})?:\d{1,5}(?:-\d{1,5})?(?:\/(?:tcp|udp))?["']?\s*(?:#.*)?$/gm)) {
    const p = Number(m[1]);
    if (p > 0 && p < 65536 && !out.includes(p)) out.push(p);
  }
  return out;
}

/** Remplace le port hôte `from` par `to` dans toutes les lignes `- "from:xxx"` du texte. */
export function replaceHostPort(text: string, from: number, to: number): string {
  return text.replace(/^(\s*-\s*["']?(?:(?:\d{1,3}\.){3}\d{1,3}:)?)(\d{1,5})(:\d{1,5}(?:-\d{1,5})?(?:\/(?:tcp|udp))?["']?\s*(?:#.*)?)$/gm, (line, pre: string, port: string, post: string) => (Number(port) === from ? `${pre}${to}${post}` : line));
}

/** Port hôte en conflit d'après un message de Docker (`Bind for 0.0.0.0:8080 failed: port is
 *  already allocated`, `bind: address already in use`, `permission denied` sur un port réservé). */
export function portConflictIn(output: string): number | null {
  const m = output.match(/(?:Bind for|bind:|listen tcp4?|address)\s+(?:\[?[\d.:a-fA-F]*\]?:)?(\d{2,5})\b[^\n]*?(?:already allocated|address already in use|permission denied|access permissions)/i) ?? output.match(/port is already allocated[^\n]*?(\d{2,5})/i);
  if (!m) return null;
  const p = Number(m[1]);
  return p > 0 && p < 65536 ? p : null;
}
