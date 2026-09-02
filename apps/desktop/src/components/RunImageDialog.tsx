import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { images } from "../api";

/** Lancer un conteneur depuis une image : nom, ports publiés, variables, commande. */
export function RunImageDialog({ image, onClose, onStarted }: { image: string; onClose: () => void; onStarted: () => void }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState("");
  const [ports, setPorts] = useState("");
  const [env, setEnv] = useState("");
  const [cmd, setCmd] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);

  async function run() {
    setBusy(true);
    setError(null);
    try {
      const portList = ports
        .split(/[\s,]+/)
        .filter(Boolean)
        .map((p) => {
          const m = /^(\d+):(\d+)(?:\/(tcp|udp))?$/.exec(p);
          if (!m) throw new Error(t("images.run.bad_port", { value: p }));
          return { host: Number(m[1]), container: Number(m[2]), proto: m[3] ?? "tcp" };
        });
      await images.run({
        image,
        name: name.trim() || null,
        cmd: cmd.trim() ? cmd.trim().split(/\s+/) : null,
        env: env.split(/\n/).map((l) => l.trim()).filter(Boolean),
        ports: portList,
      });
      onStarted();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <dialog ref={ref} onCancel={(e) => { e.preventDefault(); onClose(); }} className="card m-auto w-[520px] p-0 backdrop:bg-black/40" style={{ color: "var(--ink)" }} aria-labelledby="run-title">
      <form className="p-5" onSubmit={(e) => { e.preventDefault(); void run(); }}>
        <h2 id="run-title" className="text-base font-semibold">{t("images.run.title", { image })}</h2>
        <div className="mt-3 grid gap-3">
          <label className="grid gap-1">{t("images.run.name")}<input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="my-app" autoFocus /></label>
          <label className="grid gap-1">{t("images.run.ports")}<input className="input mono" value={ports} onChange={(e) => setPorts(e.target.value)} placeholder="8080:80, 5432:5432" /></label>
          <label className="grid gap-1">{t("images.run.env")}<textarea className="input mono h-20 py-1" value={env} onChange={(e) => setEnv(e.target.value)} placeholder={"KEY=value\nOTHER=value"} /></label>
          <label className="grid gap-1">{t("images.run.cmd")}<input className="input mono" value={cmd} onChange={(e) => setCmd(e.target.value)} placeholder={t("images.run.cmd_hint")} /></label>
        </div>
        {error && <p className="mt-3" role="alert" style={{ color: "var(--bad)" }}>{error}</p>}
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button type="submit" className="btn btn-primary" disabled={busy}>{t("images.run.start")}</button>
        </div>
      </form>
    </dialog>
  );
}
