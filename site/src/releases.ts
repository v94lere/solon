// Dernières versions publiées, lues une fois au moment de la construction du site (jamais chez le visiteur).
export interface Asset { name: string; browser_download_url: string; size: number }
export interface Release { name: string; tag_name: string; published_at: string; html_url: string; body: string; assets: Asset[] }

export const REPO_API = "https://api.github.com/repos/v94lere/solon";
const HEADERS = { Accept: "application/vnd.github+json", "User-Agent": "solon-site-build" };

let cache: Promise<Release[]> | null = null;

export function fetchReleases(): Promise<Release[]> {
  cache ??= (async () => {
    try {
      const r = await fetch(`${REPO_API}/releases?per_page=5`, { headers: HEADERS });
      return r.ok ? ((await r.json()) as Release[]) : [];
    } catch {
      return [];
    }
  })();
  return cache;
}

/** L'installateur de la dernière version, avec son empreinte si le fichier SHA256SUMS.txt l'accompagne. */
export async function latestInstaller(): Promise<{ version: string; url: string; size: number; sha256: string | null } | null> {
  const releases = await fetchReleases();
  const latest = releases[0];
  const asset = latest?.assets.find((a) => /setup\.exe$/i.test(a.name));
  if (!latest || !asset) return null;
  let sha256: string | null = null;
  const sums = latest.assets.find((a) => /^SHA256SUMS/i.test(a.name));
  if (sums) {
    try {
      const text = await (await fetch(sums.browser_download_url, { headers: { "User-Agent": HEADERS["User-Agent"] } })).text();
      const line = text.split("\n").find((l) => l.includes(asset.name));
      sha256 = line?.trim().split(/\s+/)[0] ?? null;
    } catch {
      sha256 = null;
    }
  }
  return { version: latest.tag_name.replace(/^v/, ""), url: asset.browser_download_url, size: asset.size, sha256 };
}
