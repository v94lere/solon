// Vérification d'un fichier Compose avant enregistrement, en deux couches :
// 1. la syntaxe YAML, dans l'application (paquet `yaml`), à la frappe : ligne et colonne exactes ;
// 2. la structure, par `docker compose config` dans le moteur (clés inconnues, types, réseaux non
//    déclarés, fichiers d'environnement absents…). Compose ne donne pas toujours de numéro de ligne :
//    on le retrouve ici à partir du chemin (`services.web.ports`) ou du texte cité dans le message.
import { isMap, isSeq, isScalar, LineCounter, parseDocument, type Node, type Pair } from "yaml";

export interface ComposeProblem {
  /** Ligne fautive, 1-indexée ; `null` si on ne sait pas la situer. */
  line: number | null;
  message: string;
  /** `syntax` : le YAML ne se lit pas ; `structure` : Compose le refuse. */
  kind: "syntax" | "structure";
}

/** Première erreur de syntaxe YAML, ou `null` si le texte se lit. */
export function yamlSyntaxProblem(text: string): ComposeProblem | null {
  const lineCounter = new LineCounter();
  const doc = parseDocument(text, { lineCounter, prettyErrors: true });
  const err = doc.errors[0];
  if (!err) return null;
  // Le message « joli » répète la position et cite la ligne : on garde la phrase seule.
  const message = err.message.split(/ at line \d+, column \d+/)[0].split("\n")[0].trim();
  return { line: err.linePos?.[0]?.line ?? null, message, kind: "syntax" };
}

/** Ligne (1-indexée) de la clé désignée par `path` (`["services", "web", "ports"]`), ou `null`. */
export function locateComposePath(text: string, path: string[]): number | null {
  const lineCounter = new LineCounter();
  const doc = parseDocument(text, { lineCounter, keepSourceTokens: false });
  let node: unknown = doc.contents;
  let lastOffset: number | null = null;
  for (const segment of path) {
    if (isMap(node)) {
      const pair = (node.items as Pair[]).find((p) => isScalar(p.key) && String(p.key.value) === segment);
      if (!pair) break;
      lastOffset = (pair.key as Node).range?.[0] ?? lastOffset;
      node = pair.value;
    } else if (isSeq(node) && /^\d+$/.test(segment)) {
      const item = node.items[Number(segment)] as Node | undefined;
      if (!item) break;
      lastOffset = item.range?.[0] ?? lastOffset;
      node = item;
    } else {
      break;
    }
  }
  if (lastOffset === null) return null;
  const pos = lineCounter.linePos(lastOffset);
  return pos.line > 0 ? pos.line : null;
}

/** Première ligne (1-indexée) qui contient `needle`, ou `null`. */
function lineContaining(text: string, needle: string): number | null {
  if (!needle) return null;
  const idx = text.split(/\r?\n/).findIndex((l) => l.includes(needle));
  return idx >= 0 ? idx + 1 : null;
}

/**
 * Lit la sortie d'erreur de `docker compose config` et la rattache à une ligne du fichier.
 * Formes rencontrées (Compose v2) :
 *   `validating -: services.web.ports must be a array`
 *   `validating -: services.web additional properties 'foo' not allowed`
 *   `yaml: while parsing a block mapping at line 1, column 3: line 3, column 4: did not find expected key`
 *   `service "web" refers to undefined network back: invalid compose project`
 *   `env file /x/missing.env not found: stat …`
 *   `invalid IP address: 80:80`
 */
export function parseComposeConfigError(stderr: string, text: string, fileName: string): ComposeProblem {
  const firstLine = stderr.split(/\r?\n/).find((l) => l.trim() !== "") ?? stderr;
  let message = firstLine.replace(/^validating -: /, "").replace(/: invalid compose project$/, "").trim();

  // Erreur de lecture YAML côté Compose : dernière position citée.
  const yamlPos = [...message.matchAll(/line (\d+), column \d+/g)].pop();
  if (/^yaml:/.test(message) && yamlPos) {
    const line = Number(yamlPos[1]);
    message = message.replace(/^yaml:\s*/, "").replace(/.*line \d+, column \d+:\s*/, "");
    return { line, message: message || firstLine, kind: "syntax" };
  }

  // Chemin `services.web.ports…` ; une propriété inconnue est citée entre apostrophes.
  const pathMatch = message.match(/\b((?:services|networks|volumes|configs|secrets|x-[\w-]+)(?:\.[^\s]+)*)/);
  if (pathMatch) {
    const path = pathMatch[1].split(".");
    const extra = message.match(/additional propert(?:y|ies) '([^']+)'/);
    if (extra) path.push(extra[1]);
    const line = locateComposePath(text, path) ?? locateComposePath(text, path.slice(0, -1));
    return { line, message, kind: "structure" };
  }

  // `service "web" refers to undefined network back` : la clé du service, ou son bloc networks.
  const svc = message.match(/^service "([^"]+)"/);
  if (svc) {
    const sub = /undefined network/.test(message) ? "networks" : /undefined volume/.test(message) ? "volumes" : null;
    const line = (sub && locateComposePath(text, ["services", svc[1], sub])) ?? locateComposePath(text, ["services", svc[1]]);
    return { line, message, kind: "structure" };
  }

  // Fichier cité : on cherche son nom dans le texte.
  const file = message.match(/(?:env file|file) (\S+) not found/);
  if (file) {
    const base = file[1].split(/[\/]/).pop() ?? file[1];
    return { line: lineContaining(text, base), message, kind: "structure" };
  }

  // Valeur citée après le deux-points final (`invalid IP address: 80:80`).
  const quoted = message.match(/:\s*([^\s:][^\s]*)$/);
  return { line: quoted ? lineContaining(text, quoted[1]) : null, message: message.replace(/^validating [^:]*: /, `${fileName}: `), kind: "structure" };
}
