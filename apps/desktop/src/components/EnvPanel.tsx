// Onglet « Environment » d'un projet : variables du `.env` et des blocs `environment:` du fichier
// Compose, ports hôte publiés, modifiables sans ouvrir le YAML. Les fichiers sont réécrits ligne à
// ligne (commentaires et mise en forme conservés) ; « Save and Up » relance le projet.
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { compose, host, type PortProbe } from "../api";
import {
  addComposeVar,
  looksSecret,
  parseComposeServices,
  parseDotEnv,
  removeComposeVar,
  removeDotEnv,
  setComposeHostPort,
  setComposeVar,
  setDotEnv,
} from "../env";
import { IconPencil, IconTrash } from "./Icons";

interface Props {
  dir: string;
  /** Fichier Compose du projet (`compose.yaml`) ; `null` si aucun. */
  composeFile: string | null;
  /** Adresse principale du projet, affichée en lecture seule. */
  address: string | null;
  /** Ports hôte déjà publiés par les conteneurs de ce projet : pris par lui, donc pas un conflit. */
  ownPorts: number[];
  busy: boolean;
  onUp: () => Promise<void>;
  /** Le YAML a changé sur disque : la vue Compose doit se recharger. */
  onFilesChanged: () => void;
}

export function EnvPanel({ dir, composeFile, address, ownPorts, busy, onUp, onFilesChanged }: Props) {
  const { t } = useTranslation();
  const [dotenv, setDotenv] = useState<string>("");
  const [yaml, setYaml] = useState<string>("");
  const [savedDotenv, setSavedDotenv] = useState("");
  const [savedYaml, setSavedYaml] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [shown, setShown] = useState<Set<string>>(new Set());
  const [adding, setAdding] = useState<{ target: string; key: string; value: string } | null>(null);
  const [conflicts, setConflicts] = useState<PortProbe[]>([]);
  const [saving, setSaving] = useState(false);
  const dirty = dotenv !== savedDotenv || yaml !== savedYaml;

  async function load() {
    setError(null);
    try {
      const [e, y] = await Promise.all([compose.envRead(dir), composeFile ? compose.read(dir) : Promise.resolve("")]);
      setDotenv(e);
      setSavedDotenv(e);
      setYaml(y);
      setSavedYaml(y);
    } catch (err) {
      setError(String(err));
    }
  }
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dir, composeFile]);

  const envVars = useMemo(() => parseDotEnv(dotenv), [dotenv]);
  const services = useMemo(() => parseComposeServices(yaml), [yaml]);

  // Ports hôte déjà pris sur ce PC (autre programme ou autre pile), vérifiés après chaque changement.
  useEffect(() => {
    const ports = [...new Set(services.flatMap((s) => s.ports.map((p) => Number(p.host))).filter((p) => p > 0))];
    if (ports.length === 0) {
      setConflicts([]);
      return;
    }
    let alive = true;
    const timer = window.setTimeout(() => {
      host.portsProbe(ports).then((r) => { if (alive) setConflicts(r.filter((x) => x.in_use && !ownPorts.includes(x.port))); }).catch(() => {});
    }, 400);
    return () => { alive = false; window.clearTimeout(timer); };
  }, [services, ownPorts]);

  async function save(): Promise<boolean> {
    setSaving(true);
    setError(null);
    try {
      if (dotenv !== savedDotenv) {
        await compose.envWrite(dir, dotenv);
        setSavedDotenv(dotenv);
      }
      if (yaml !== savedYaml && composeFile) {
        await compose.write(dir, yaml);
        setSavedYaml(yaml);
        onFilesChanged();
      }
      return true;
    } catch (err) {
      setError(String(err));
      return false;
    } finally {
      setSaving(false);
    }
  }

  function toggleShown(id: string) {
    setShown((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id); else n.add(id);
      return n;
    });
  }

  const row = (id: string, key: string, value: string, onChange: (v: string) => void, onRemove: () => void) => {
    const secret = looksSecret(key) && !shown.has(id);
    return (
      <tr key={id}>
        <td className="mono env-key" title={key}>{key}</td>
        <td>
          <span className="flex items-center gap-1">
            <input className="input input-sm mono env-value" type={secret ? "password" : "text"} value={value} onChange={(e) => onChange(e.target.value)} aria-label={key} spellCheck={false} />
            {looksSecret(key) && (
              <button type="button" className="icon-btn" title={shown.has(id) ? t("project.env.hide") : t("project.env.show")} aria-label={shown.has(id) ? t("project.env.hide") : t("project.env.show")} onClick={() => toggleShown(id)}>
                <IconPencil />
              </button>
            )}
            <button type="button" className="icon-btn icon-btn-danger" title={t("project.env.remove")} aria-label={t("project.env.remove")} onClick={onRemove}><IconTrash /></button>
          </span>
        </td>
      </tr>
    );
  };

  const addForm = (target: string) =>
    adding?.target === target ? (
      <tr>
        <td><input className="input input-sm mono env-key-input" placeholder="KEY" value={adding.key} autoFocus onChange={(e) => setAdding({ ...adding, key: e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, "_") })} aria-label={t("project.env.key")} /></td>
        <td>
          <span className="flex items-center gap-1">
            <input className="input input-sm mono env-value" placeholder={t("project.env.value")} value={adding.value} onChange={(e) => setAdding({ ...adding, value: e.target.value })} aria-label={t("project.env.value")} onKeyDown={(e) => { if (e.key === "Enter") commitAdd(); }} />
            <button type="button" className="btn btn-sm" disabled={!adding.key} onClick={commitAdd}>{t("project.env.add")}</button>
            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setAdding(null)}>{t("common.cancel")}</button>
          </span>
        </td>
      </tr>
    ) : null;

  function commitAdd() {
    if (!adding || !adding.key) return;
    if (adding.target === ".env") setDotenv((cur) => setDotEnv(cur, adding.key, adding.value));
    else setYaml((cur) => addComposeVar(cur, adding.target, adding.key, adding.value));
    setAdding(null);
  }

  return (
    <div className="card mx-4 mb-4 flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b px-3 py-1.5" style={{ borderColor: "var(--line)" }}>
        <span className="text-xs" style={{ color: "var(--ink-2)" }}>{t("project.env.hint")}</span>
        <span className="kbd-hint">{dirty ? t("project.unsaved") : t("project.saved")}</span>
        <span className="flex-1" />
        <button type="button" className="btn btn-ghost btn-sm" disabled={saving} onClick={() => void load()}>{t("project.reload")}</button>
        <button type="button" className="btn btn-sm" disabled={saving || !dirty} onClick={() => void save()}>{t("project.save")}</button>
        <button type="button" className="btn btn-primary btn-sm" disabled={saving || busy} onClick={() => void (async () => { if (await save()) await onUp(); })()}>{t("project.save_up")}</button>
      </div>
      {error && <p className="px-3 pt-2" role="alert" style={{ color: "var(--bad)" }}>{error}</p>}
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {address && (
          <section className="env-section">
            <h3 className="env-title">{t("project.env.address")}</h3>
            <p className="kbd-hint">{t("project.env.address_hint")}</p>
            <p className="mono" style={{ color: "var(--accent-ink)" }}>{address}</p>
          </section>
        )}

        <section className="env-section">
          <h3 className="env-title">.env <span className="kbd-hint">{t("project.env.dotenv_hint")}</span></h3>
          <table className="table env-table">
            <tbody>
              {envVars.map((v) => row(`.env:${v.key}`, v.key, v.value, (val) => setDotenv((cur) => setDotEnv(cur, v.key, val)), () => setDotenv((cur) => removeDotEnv(cur, v.key))))}
              {addForm(".env")}
              {envVars.length === 0 && adding?.target !== ".env" && <tr><td colSpan={2} className="kbd-hint">{t("project.env.empty")}</td></tr>}
            </tbody>
          </table>
          {adding?.target !== ".env" && <button type="button" className="btn btn-ghost btn-sm" onClick={() => setAdding({ target: ".env", key: "", value: "" })}>+ {t("project.env.add_var")}</button>}
        </section>

        {services.map((s) => (
          <section key={s.service} className="env-section">
            <h3 className="env-title">{t("project.env.service", { name: s.service })}</h3>
            {s.ports.length > 0 && (
              <div className="env-ports">
                <span className="kbd-hint">{t("project.env.ports")}</span>
                {s.ports.map((p) => {
                  const conflict = conflicts.find((c) => String(c.port) === p.host);
                  return (
                    <span key={p.line} className="env-port">
                      <input className={`input input-sm mono env-port-input${conflict ? " is-conflict" : ""}`} value={p.host} inputMode="numeric" aria-label={t("project.env.host_port")} onChange={(e) => setYaml((cur) => setComposeHostPort(cur, p.line, e.target.value.replace(/\D/g, "").slice(0, 5)))} />
                      <span className="mono kbd-hint">{p.rest}</span>
                      {conflict?.suggestion && (
                        <button type="button" className="btn btn-sm" onClick={() => setYaml((cur) => setComposeHostPort(cur, p.line, String(conflict.suggestion)))}>{t("stacks.use_port", { from: conflict.port, to: conflict.suggestion })}</button>
                      )}
                    </span>
                  );
                })}
              </div>
            )}
            <table className="table env-table">
              <tbody>
                {s.vars.map((v) => row(`${s.service}:${v.key}`, v.key, v.value, (val) => setYaml((cur) => setComposeVar(cur, v.line, v.key, val)), () => setYaml((cur) => removeComposeVar(cur, v.line))))}
                {addForm(s.service)}
                {s.vars.length === 0 && adding?.target !== s.service && <tr><td colSpan={2} className="kbd-hint">{t("project.env.no_vars")}</td></tr>}
              </tbody>
            </table>
            {adding?.target !== s.service && <button type="button" className="btn btn-ghost btn-sm" onClick={() => setAdding({ target: s.service, key: "", value: "" })}>+ {t("project.env.add_var")}</button>}
          </section>
        ))}
        {composeFile && services.length === 0 && <p className="kbd-hint">{t("project.env.no_services")}</p>}
      </div>
    </div>
  );
}
