import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { applyTheme, loadTheme, type Theme } from "../theme";
import { engine, type Settings } from "../api";
import { setLanguage } from "../i18n";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { diagnostic } from "../api";

export function SettingsView() {
  const [diag, setDiag] = useState<string | null>(null);
  const [diagDir, setDiagDir] = useState<string | null>(null);
  const [theme, setTheme] = useState<Theme>(loadTheme());
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
        <h2 className="font-semibold">{t("settings.theme")}</h2>
        <select className="input mt-2" value={theme} onChange={(e) => { const v = e.target.value as Theme; setTheme(v); applyTheme(v); }} aria-label={t("settings.theme")}>
          <option value="light">{t("settings.theme_light")}</option>
          <option value="dark">{t("settings.theme_dark")}</option>
          <option value="system">{t("settings.theme_system")}</option>
        </select>
      </section>
      <section className="card mt-4 p-4">
        <h2 className="font-semibold">{t("settings.diagnostic")}</h2>
        <p className="mt-1" style={{ color: "var(--ink-2)" }}>{t("settings.diagnostic_help")}</p>
        <div className="mt-2 flex items-center gap-3">
          <button
            type="button"
            className="btn"
            onClick={() => {
              void (async () => {
                setDiag(null);
                const dest = (await saveDialog({ defaultPath: "solon-diagnostic.zip", filters: [{ name: "Zip", extensions: ["zip"] }] })) as string | null;
                if (!dest) return;
                try {
                  const n = await diagnostic.export(dest);
                  setDiag(t("settings.diagnostic_done", { count: n, path: dest }));
                  setDiagDir(dest.replace(/[\\/][^\\/]*$/, ""));
                } catch (e) {
                  setDiag(String(e));
                  setDiagDir(null);
                }
              })();
            }}
          >
            {t("settings.diagnostic_export")}
          </button>
          {diag && <span style={{ color: "var(--ink-2)" }}>{diag}</span>}
          {diagDir && (
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => void openPath(diagDir)}>{t("settings.diagnostic_open")}</button>
          )}
        </div>
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
