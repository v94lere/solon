// Couleur d'accent : orange Solon (défaut) ou couleur d'accent de Windows, lue par le service Tauri.
import { invoke } from "@tauri-apps/api/core";

export type Accent = "solon" | "windows";
const KEY = "solon.accent";

export function loadAccent(): Accent {
  try {
    return localStorage.getItem(KEY) === "windows" ? "windows" : "solon";
  } catch {
    return "solon";
  }
}

/** Applique le choix : la classe `accent-custom` et la variable `--accent-base` pilotent les
 *  déclinaisons (`--accent`, `--accent-ink`, `--accent-soft`) définies dans la feuille de style. */
export async function applyAccent(accent: Accent): Promise<void> {
  const root = document.documentElement;
  try {
    localStorage.setItem(KEY, accent);
  } catch {
    /* préférence non mémorisée */
  }
  if (accent === "windows") {
    try {
      const color = await invoke<string | null>("system_accent_color");
      if (color) {
        root.style.setProperty("--accent-base", color);
        root.classList.add("accent-custom");
        return;
      }
    } catch {
      /* couleur indisponible : orange Solon */
    }
  }
  root.classList.remove("accent-custom");
  root.style.removeProperty("--accent-base");
}

void applyAccent(loadAccent());
