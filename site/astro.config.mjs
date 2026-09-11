// Site statique de Solon. Sur GitHub Pages sans domaine, le site vit sous /solon/ ; avec un domaine
// personnalisé, définir SITE_URL (ex. https://solon.dev) et SITE_BASE=/ au moment du build.
import { defineConfig } from "astro/config";

const site = process.env.SITE_URL || "https://v94lere.github.io";   // `||` : une variable vide compte comme absente
const base = process.env.SITE_BASE || "/solon";

export default defineConfig({
  site,
  base,
  trailingSlash: "always",
  build: { format: "directory" },
  i18n: {
    defaultLocale: "en",
    locales: ["en", "fr"],
    routing: { prefixDefaultLocale: false },
  },
});
