// Un conteneur qui se termine aussitôt après son démarrage (hello-world, commande ponctuelle) ne laisse
// rien voir dans la liste : on le détecte pour proposer ses journaux au lieu de laisser croire à un échec.
import { containers } from "./api";

export interface ExitedQuickly {
  id: string;
  name: string;
  code: number;
}

interface InspectState { Name?: string; State?: { Status?: string; ExitCode?: number } }

/** Attend un court instant puis regarde si le conteneur tourne encore ; renvoie les détails s'il s'est arrêté. */
export async function checkExitedQuickly(id: string, waitMs = 1500): Promise<ExitedQuickly | null> {
  await new Promise((r) => setTimeout(r, waitMs));
  try {
    const i = (await containers.inspect(id)) as InspectState;
    if (i.State?.Status === "running" || i.State?.Status === "paused") return null;
    return { id, name: (i.Name ?? id.slice(0, 12)).replace(/^\//, ""), code: i.State?.ExitCode ?? 0 };
  } catch {
    return null;
  }
}
