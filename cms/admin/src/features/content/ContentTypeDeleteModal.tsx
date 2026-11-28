import React, { useEffect, useRef } from 'react';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { __, useI18n } from '../../i18n/index.tsx';

interface ContentTypeDeleteModalProps {
  open: boolean;
  typeKey: string;
  label: string;
  busy: boolean;
  error: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Native modal `<dialog>` for deleting a user-owned CPT safely (`autoFocus` on Cancel). */
export const ContentTypeDeleteModal: React.FC<ContentTypeDeleteModalProps> = ({
  open,
  typeKey,
  label,
  busy,
  error,
  onConfirm,
  onCancel,
}) => {
  useI18n();
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const cancelBtnRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    if (open && !dlg.open) {
      dlg.showModal();
      cancelBtnRef.current?.focus();
    } else if (!open && dlg.open) {
      dlg.close();
    }
  }, [open]);

  if (!open) return null;

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="delete-cpt-dialog-title"
      data-testid="delete-cpt-dialog"
      onCancel={e => {
        if (busy) {
          e.preventDefault();
          return;
        }
        onCancel();
      }}
      className="fixed inset-0 m-auto max-w-md rounded-xl border border-zinc-200 bg-white p-5 shadow-xl overscroll-contain backdrop:bg-zinc-950/50 backdrop:overscroll-contain"
    >
      <div className="space-y-4">
        <div>
          <h3 id="delete-cpt-dialog-title" className="text-base font-semibold text-zinc-950">
            {__("Delete content type {{value1}}?", { value1: label })}
          </h3>
          <p className="mt-1.5 text-sm text-zinc-600">
            {__("You are deleting CPT")} <code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-xs text-zinc-800">{typeKey}</code>.{' '}
            {__("Content types that still have active or trashed entries cannot be deleted until all entries are purged.")}
          </p>
        </div>

        {error && <AdminAlert tone="error">{error}</AdminAlert>}

        <div className="flex flex-wrap justify-end gap-2 pt-1">
          <AdminButton
            ref={cancelBtnRef}
            type="button"
            variant="secondary"
            size="md"
            autoFocus
            disabled={busy}
            onClick={onCancel}
          >
            {__("Cancel")}
          </AdminButton>
          <AdminButton
            type="button"
            variant="danger"
            size="md"
            disabled={busy}
            data-testid="confirm-delete-cpt-btn"
            onClick={onConfirm}
          >
            {busy ? __("Deleting…") : __("Delete permanently")}
          </AdminButton>
        </div>
      </div>
    </dialog>
  );
};
