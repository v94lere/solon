// Thème de l'interface : clair par défaut (maquette validée le 3 septembre 2026), sombre ou système au choix.
export type Theme = "light" | "dark" | "system";

const KEY = "solon.theme";

export function loadTheme(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* stockage indisponible */
  }
  return "light";
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    /* la préférence ne sera pas mémorisée */
  }
}

applyTheme(loadTheme());
