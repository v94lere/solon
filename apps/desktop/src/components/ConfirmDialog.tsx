import { useEffect, useRef, type ReactNode } from "react";

interface Props {
  open: boolean;
  title: string;
  children?: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Boîte de confirmation accessible : focus piégé, Échap annule, aucune action destructive sans clic explicite. */
export function ConfirmDialog({ open, title, children, confirmLabel, cancelLabel, danger, onConfirm, onCancel }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        onCancel();
      }}
      className="card m-auto w-[420px] p-0 backdrop:bg-black/40"
      style={{ color: "var(--ink)" }}
      aria-labelledby="confirm-title"
    >
      <form
        method="dialog"
        onSubmit={(e) => {
          e.preventDefault();
          onConfirm();
        }}
        className="p-5"
      >
        <h2 id="confirm-title" className="text-base font-semibold">
          {title}
        </h2>
        <div className="mt-2" style={{ color: "var(--ink-2)" }}>
          {children}
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" className="btn" onClick={onCancel} autoFocus>
            {cancelLabel}
          </button>
          <button type="submit" className={`btn ${danger ? "btn-danger" : "btn-primary"}`}>
            {confirmLabel}
          </button>
        </div>
      </form>
    </dialog>
  );
}
