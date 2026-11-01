// Récupération d'espace : images qu'aucun conteneur n'utilise et cache de construction, puis TRIM du
// disque de la machine pour que le fichier VHDX rende les blocs à Windows. Partagé par Réglages → Disque
// et le bandeau « lecteur presque plein ».
import { engine, images, type ReclaimReport } from "./api";

export async function reclaimSpace(): Promise<ReclaimReport> {
  const r = await images.reclaim();
  await engine.exec("fstrim /var/lib/solon 2>/dev/null; fstrim /var/lib/docker 2>/dev/null; true", 180).catch(() => {});
  return r;
}
