import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import fr from "./locales/fr.json";

// Anglais par défaut (décision du 2 septembre 2026), français disponible.
const stored = (() => {
  try {
    return localStorage.getItem("solon.language");
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

export function setLanguage(lng: "en" | "fr") {
  void i18n.changeLanguage(lng);
  try {
    localStorage.setItem("solon.language", lng);
  } catch {
    /* stockage indisponible : la langue ne sera pas mémorisée */
  }
  document.documentElement.lang = lng;
}

export default i18n;
