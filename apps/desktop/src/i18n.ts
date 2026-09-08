import { invoke } from "@tauri-apps/api/core";
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import fr from "./locales/fr.json";

// Anglais par défaut (décision du 2 septembre 2026), français disponible.
const stored = (() => {
  try {
    return localStorage.getItem("monodon.language");
  } catch {
    return null;
  }
})();

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, fr: { translation: fr } },
  lng: stored ?? "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnNull: false,
});

function tellTray(lng: string) {
  // Le menu de la barre des tâches vit côté Rust : il suit la même langue.
  invoke("set_language", { lang: lng }).catch(() => undefined);
}
tellTray(stored ?? "en");

export function setLanguage(lng: "en" | "fr") {
  void i18n.changeLanguage(lng);
  tellTray(lng);
  try {
    localStorage.setItem("monodon.language", lng);
  } catch {
    /* stockage indisponible : la langue ne sera pas mémorisée */
  }
  document.documentElement.lang = lng;
}

export default i18n;
