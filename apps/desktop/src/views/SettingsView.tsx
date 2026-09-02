import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { engine, type Settings } from "../api";
import { setLanguage } from "../i18n";

export function SettingsView() {
  const { t, i18n } = useTranslation();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    engine
      .settingsGet()
      .then(setSettings)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  async function save() {
    if (!settings) return;
    setError(null);
    try {
      await engine.settingsSet(settings);
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="max-w-xl p-6">
      <h1 className="text-lg font-semibold">{t("settings.title")}</h1>

      <section className="card mt-4 p-4">
        <h2 className="font-semibold">{t("settings.language")}</h2>
        <select className="input mt-2" value={i18n.language.startsWith("fr") ? "fr" : "en"} onChange={(e) => setLanguage(e.target.value as "en" | "fr")} aria-label={t("settings.language")}>
          <option value="en">English</option>
          <option value="fr">Français</option>
        </select>
      </section>

      <section className="card mt-4 p-4">
        <h2 className="font-semibold">{t("settings.engine")}</h2>
        {settings ? (
          <div className="mt-2 grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1">
              {t("settings.memory")}
              <input type="number" className="input" min={1024} step={256} value={settings.memory_mb} onChange={(e) => setSettings({ ...settings, memory_mb: Number(e.target.value) })} />
            </label>
            <label className="flex flex-col gap-1">
              {t("settings.processors")}
              <input type="number" className="input" min={1} max={64} value={settings.processors} onChange={(e) => setSettings({ ...settings, processors: Number(e.target.value) })} />
            </label>
            <label className="flex flex-col gap-1">
              {t("settings.disk")}
              <input type="number" className="input" min={16} value={settings.data_disk_gib} onChange={(e) => setSettings({ ...settings, data_disk_gib: Number(e.target.value) })} />
            </label>
            <label className="flex items-center gap-2 self-end">
              <input type="checkbox" checked={settings.autostart} onChange={(e) => setSettings({ ...settings, autostart: e.target.checked })} />
              {t("settings.autostart")}
            </label>
            <div className="col-span-2 flex items-center gap-3">
              <button type="button" className="btn btn-primary" onClick={() => void save()}>
                {t("settings.save")}
              </button>
              {saved && <span style={{ color: "var(--ok)" }}>{t("settings.saved")}</span>}
            </div>
          </div>
        ) : (
          <p className="mt-2" style={{ color: "var(--ink-2)" }}>
            {error ?? t("common.loading")}
          </p>
        )}
        {error && settings && (
          <p className="mt-2" style={{ color: "var(--bad)" }}>
            {error}
          </p>
        )}
      </section>
    </div>
  );
}
