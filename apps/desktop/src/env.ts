// Variables d'environnement et ports d'un projet Compose, lus et réécrits **ligne à ligne** dans les
// fichiers existants (`.env`, `compose.yaml`) pour conserver commentaires et mise en forme.
// Couvre la forme habituelle des fichiers écrits à la main et de la galerie : deux espaces
// d'indentation par niveau, `environment:` en liste (`- KEY=value`) ou en table (`KEY: value`),
// `ports:` en forme courte. Le reste du YAML n'est jamais touché.

export interface EnvVar {
  key: string;
  value: string;
  /** Numéro de ligne (0-based) dans le fichier d'origine. */
  line: number;
}

export interface ServiceEnv {
  service: string;
  vars: EnvVar[];
  /** Ports publiés en forme courte : ligne, port hôte, reste (`:80` ou `:80/tcp`). */
  ports: { line: number; host: string; rest: string; prefix: string; quote: string }[];
}

/** `.env` : lignes `KEY=value` (guillemets simples ou doubles retirés à la lecture, remis à l'écriture). */
export function parseDotEnv(text: string): EnvVar[] {
  const out: EnvVar[] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const m = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_.-]*)\s*=\s*(.*)$/.exec(raw);
    if (!m || raw.trim().startsWith("#")) return;
    out.push({ key: m[1], value: unquote(m[2].trim()), line: i });
  });
  return out;
}

function unquote(v: string): string {
  if (v.length >= 2 && ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'")))) return v.slice(1, -1);
  return v.replace(/\s+#.*$/, "");
}

/** Écrit une valeur dans le `.env` : remplace la ligne si la clé existe, sinon l'ajoute à la fin. */
export function setDotEnv(text: string, key: string, value: string): string {
  const lines = text.split(/\r?\n/);
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const needsQuotes = /[\s#'"$]/.test(value) || value === "";
  const rendered = `${key}=${needsQuotes ? JSON.stringify(value) : value}`;
  for (let i = 0; i < lines.length; i++) {
    const m = /^(\s*(?:export\s+)?)([A-Za-z_][A-Za-z0-9_.-]*)\s*=/.exec(lines[i]);
    if (m && m[2] === key && !lines[i].trim().startsWith("#")) {
      lines[i] = `${m[1]}${rendered}`;
      return lines.join(nl);
    }
  }
  if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  lines.push(rendered, "");
  return lines.join(nl);
}

export function removeDotEnv(text: string, key: string): string {
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  return text
    .split(/\r?\n/)
    .filter((l) => {
      const m = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_.-]*)\s*=/.exec(l);
      return !(m && m[1] === key && !l.trim().startsWith("#"));
    })
    .join(nl);
}

function indentOf(line: string): number {
  return line.length - line.trimStart().length;
}

/** Services du fichier Compose avec leurs variables `environment:` et leurs `ports:` (forme courte). */
export function parseComposeServices(text: string): ServiceEnv[] {
  const lines = text.split(/\r?\n/);
  const out: ServiceEnv[] = [];
  let inServices = false;
  let servicesIndent = -1;
  let current: ServiceEnv | null = null;
  let serviceIndent = -1;
  let block: "environment" | "ports" | null = null;
  let blockIndent = -1;
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const trimmed = raw.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    const ind = indentOf(raw);
    if (!inServices) {
      if (/^services:\s*$/.test(trimmed) && ind === 0) {
        inServices = true;
        servicesIndent = 0;
      }
      continue;
    }
    if (ind <= servicesIndent) {
      // Autre clé de premier niveau : fin de `services:`.
      inServices = false;
      current = null;
      block = null;
      continue;
    }
    if (serviceIndent < 0 || ind === serviceIndent) {
      const m = /^([A-Za-z0-9_.-]+):\s*$/.exec(trimmed);
      if (m && (serviceIndent < 0 || ind === serviceIndent)) {
        serviceIndent = ind;
        current = { service: m[1], vars: [], ports: [] };
        out.push(current);
        block = null;
        continue;
      }
    }
    if (!current) continue;
    if (ind === serviceIndent + 2 || (block === null && ind > serviceIndent)) {
      if (/^environment:\s*$/.test(trimmed)) {
        block = "environment";
        blockIndent = ind;
        continue;
      }
      if (/^ports:\s*$/.test(trimmed)) {
        block = "ports";
        blockIndent = ind;
        continue;
      }
      if (ind <= blockIndent || block === null) {
        block = null;
      }
    }
    if (block && ind > blockIndent) {
      if (block === "environment") {
        const list = /^-\s*["']?([A-Za-z_][A-Za-z0-9_.-]*)=(.*?)["']?\s*$/.exec(trimmed);
        const map = /^([A-Za-z_][A-Za-z0-9_.-]*):\s*(.*)$/.exec(trimmed);
        if (list) current.vars.push({ key: list[1], value: unquote(list[2]), line: i });
        else if (map) current.vars.push({ key: map[1], value: unquote(map[2].trim()), line: i });
      } else {
        const m = /^(-\s*)(["']?)((?:\d{1,3}\.){3}\d{1,3}:)?(\d{1,5})(:\d{1,5}(?:-\d{1,5})?(?:\/(?:tcp|udp))?)\2\s*$/.exec(trimmed);
        if (m) current.ports.push({ line: i, prefix: `${raw.slice(0, ind)}${m[1]}${m[2]}${m[3] ?? ""}`, quote: m[2], host: m[4], rest: m[5] });
      }
    } else if (block) {
      block = null;
    }
  }
  return out;
}

/** Remplace la valeur d'une variable d'environnement à sa ligne, en conservant la forme (liste ou table). */
export function setComposeVar(text: string, line: number, key: string, value: string): string {
  const lines = text.split(/\r?\n/);
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const raw = lines[line] ?? "";
  const ind = raw.slice(0, indentOf(raw));
  const trimmed = raw.trim();
  const quoted = /[\s#:'"{}[\],&*?|<>=!%@`]/.test(value) || value === "" ? JSON.stringify(value) : value;
  if (trimmed.startsWith("-")) lines[line] = `${ind}- ${key}=${value.includes(" ") || value === "" ? JSON.stringify(value) : value}`;
  else lines[line] = `${ind}${key}: ${quoted}`;
  return lines.join(nl);
}

/** Change le port hôte d'une ligne `ports:` (forme courte). */
export function setComposeHostPort(text: string, line: number, host: string): string {
  const lines = text.split(/\r?\n/);
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const raw = lines[line] ?? "";
  lines[line] = raw.replace(/^(\s*-\s*["']?(?:(?:\d{1,3}\.){3}\d{1,3}:)?)(\d{1,5})(:)/, `$1${host}$3`);
  return lines.join(nl);
}

/** Ajoute une variable au bloc `environment:` d'un service (le bloc est créé après `image:` s'il manque). */
export function addComposeVar(text: string, service: string, key: string, value: string): string {
  const lines = text.split(/\r?\n/);
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const services = parseComposeServices(text);
  const svc = services.find((s) => s.service === service);
  const rendered = value.includes(" ") || value === "" ? JSON.stringify(value) : value;
  if (svc && svc.vars.length > 0) {
    const last = svc.vars[svc.vars.length - 1];
    const ref = lines[last.line];
    const ind = ref.slice(0, indentOf(ref));
    const asList = ref.trim().startsWith("-");
    lines.splice(last.line + 1, 0, asList ? `${ind}- ${key}=${rendered}` : `${ind}${key}: ${rendered}`);
    return lines.join(nl);
  }
  // Pas encore de bloc : après la ligne `<service>:`, à l'indentation de ses clés.
  let i = lines.findIndex((l) => new RegExp(`^\\s+${service.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}:\\s*$`).test(l));
  if (i < 0) return text;
  const svcIndent = indentOf(lines[i]);
  let keyIndent = svcIndent + 2;
  for (let j = i + 1; j < lines.length; j++) {
    if (lines[j].trim() === "" || lines[j].trim().startsWith("#")) continue;
    if (indentOf(lines[j]) <= svcIndent) break;
    keyIndent = indentOf(lines[j]);
    i = j;
    if (/^image:/.test(lines[j].trim())) break;
  }
  const ind = " ".repeat(keyIndent);
  lines.splice(i + 1, 0, `${ind}environment:`, `${ind}  ${key}: ${rendered}`);
  return lines.join(nl);
}

/** Retire la ligne d'une variable d'environnement (et le bloc `environment:` s'il devient vide). */
export function removeComposeVar(text: string, line: number): string {
  const lines = text.split(/\r?\n/);
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const removedIndent = indentOf(lines[line] ?? "");
  lines.splice(line, 1);
  const prev = lines[line - 1];
  const next = lines[line];
  if (prev && /^\s*environment:\s*$/.test(prev) && (!next || indentOf(next) < removedIndent)) lines.splice(line - 1, 1);
  return lines.join(nl);
}

/** Valeurs qui ressemblent à un secret : affichées masquées par défaut. */
export function looksSecret(key: string): boolean {
  return /pass|secret|token|key|pwd|credential/i.test(key);
}
