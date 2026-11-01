import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

/** Affiche un objet JSON (inspection) dans une boîte modale, avec copie. */
export function JsonDialog({ title, load, onClose }: { title: string; load: () => Promise<unknown>; onClose: () => void }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDialogElement>(null);
  const [text, setText] = useState<string>(t("common.loading"));
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    ref.current?.showModal();
    load()
      .then((v) => setText(JSON.stringify(v, null, 2)))
      .catch((e: unknown) => setText(String(e)));
  }, [load]);
  return (
    <dialog ref={ref} onCancel={(e) => { e.preventDefault(); onClose(); }} className="card m-auto w-[760px] max-w-[90vw] p-0 backdrop:bg-black/40" style={{ color: "var(--ink)" }} aria-label={title}>
      <div className="flex items-center gap-2 border-b px-4 py-2" style={{ borderColor: "var(--line)" }}>
        <h2 className="font-semibold">{title}</h2>
        <div className="flex-1" />
        <button type="button" className="btn btn-sm" onClick={() => { void navigator.clipboard.writeText(text).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1500); }); }}>
          {copied ? t("common.copied") : t("common.copy")}
        </button>
        <button type="button" className="btn btn-sm" onClick={onClose} autoFocus>{t("common.close")}</button>
      </div>
      <pre className="mono max-h-[70vh] overflow-auto p-4 text-xs leading-5">{text}</pre>
    </dialog>
  );
}
