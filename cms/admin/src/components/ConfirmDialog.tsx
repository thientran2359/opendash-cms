import React, { useEffect, useId, useRef } from 'react';
import { AdminButton } from './AdminPrimitives.tsx';
import { useI18n } from '../i18n/index.tsx';

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  cancelLabel?: string;
  tone?: 'danger' | 'primary';
  busy?: boolean;
  testId?: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export const ConfirmDialog: React.FC<ConfirmDialogProps> = ({
  open,
  title,
  description,
  confirmLabel,
  cancelLabel,
  tone = 'danger',
  busy = false,
  testId,
  onCancel,
  onConfirm,
}) => {
  const { __ } = useI18n();
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const titleId = useId();
  const descId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      cancelRef.current?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby={titleId}
      aria-describedby={descId}
      data-testid={open ? testId : undefined}
      onCancel={event => {
        if (busy) {
          event.preventDefault();
          return;
        }
        event.preventDefault();
        onCancel();
      }}
      onMouseDown={event => {
        if (event.target === dialogRef.current && !busy) {
          onCancel();
        }
      }}
      className="fixed inset-0 m-auto w-[min(28rem,calc(100vw-2rem))] rounded-xl border border-zinc-200 bg-white p-5 text-zinc-900 shadow-xl overscroll-contain backdrop:bg-zinc-950/50 backdrop:overscroll-contain"
    >
      {open && (
        <div className="space-y-4">
          <h2 id={titleId} className="text-base font-semibold text-zinc-950">
            {title}
          </h2>
          <p id={descId} className="text-sm text-zinc-600">
            {description}
          </p>
          <div className="flex flex-wrap justify-end gap-2">
            <AdminButton
              ref={cancelRef}
              type="button"
              variant="secondary"
              size="md"
              disabled={busy}
              onClick={onCancel}
            >
              {cancelLabel ?? __("Cancel")}
            </AdminButton>
            <AdminButton
              type="button"
              variant={tone === 'danger' ? 'danger' : 'primary'}
              size="md"
              disabled={busy}
              onClick={onConfirm}
              data-testid={testId ? `${testId}-confirm` : undefined}
            >
              {confirmLabel}
            </AdminButton>
          </div>
        </div>
      )}
    </dialog>
  );
};
