// Environnements par branche Git : un projet Compose dont le dossier est un dépôt peut isoler ses
// conteneurs et ses volumes par branche. Le nom de projet Compose devient `<projet>-<branche>`
// (`blog-feature-login`) ; Compose nomme alors volumes, réseaux et conteneurs à part, et les adresses
// suivent (`web.blog-feature-login.solon.local`). Réglage mémorisé par dossier, côté application.

const KEY = "solon.branch-env";

function load(): Record<string, boolean> {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}") as unknown;
    return v && typeof v === "object" ? (v as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

function norm(dir: string): string {
  return dir.replace(/[\\/]+$/, "").toLowerCase();
}

export function branchEnvEnabled(dir: string): boolean {
  return load()[norm(dir)] === true;
}

export function setBranchEnvEnabled(dir: string, on: boolean) {
  const all = load();
  if (on) all[norm(dir)] = true;
  else delete all[norm(dir)];
  try {
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* stockage indisponible */
  }
}

/** `feature/Login-Page` → `feature-login-page` (ce que Compose accepte dans un nom de projet). */
export function branchSlug(branch: string): string {
  return branch
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40);
}

/** Nom de projet Compose pour une branche : `blog-main`, `blog-feature-login`. */
export function branchProjectName(base: string, branch: string): string {
  const s = branchSlug(branch);
  return s ? `${base}-${s}` : base;
}

/** Branche encodée dans un nom de projet issu de `branchProjectName`, sinon `null`. */
export function branchOfProjectName(base: string, name: string): string | null {
  return name.startsWith(`${base}-`) && name.length > base.length + 1 ? name.slice(base.length + 1) : null;
}
