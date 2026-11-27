import React, { useEffect, useRef } from 'react';
import { __, _x } from '../../i18n/index.tsx';

export interface SingleEditBuilderPromptDialogProps {
  open: boolean;
  entryId: number | null;
  saving: boolean;
  onClose: () => void;
  onDiscardAndOpen: () => void;
  onSaveAndOpen: () => void;
}

export const SingleEditBuilderPromptDialog: React.FC<SingleEditBuilderPromptDialogProps> = ({
  open,
  entryId,
  saving,
  onClose,
  onDiscardAndOpen,
  onSaveAndOpen,
}) => {
  const dialogRef = useRef<HTMLDialogElement | null>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="builder-unsaved-title"
      onClose={onClose}
      className="od-modal-dialog m-auto max-w-md rounded-xl border border-zinc-200 bg-white p-5 text-zinc-900 shadow-xl"
    >
      <h1 id="builder-unsaved-title" className="text-base font-semibold text-zinc-950">
        {__("Unsaved editor changes")}
      </h1>
      <p className="mt-2 text-sm text-zinc-600">
        {__(
          "Do you want to save your current edits before opening Appearance customization (Builder)?",
        )}
      </p>
      <div className="mt-4 flex flex-wrap justify-end gap-2">
        {entryId !== null && (
          <button
            type="button"
            disabled={saving}
            onClick={onDiscardAndOpen}
            data-testid="builder-prompt-discard"
            className="inline-flex h-9 items-center rounded-md border border-rose-200 bg-rose-50 px-3 text-xs font-medium text-rose-700 hover:bg-rose-100 disabled:opacity-50"
          >
            {__("Discard changes & open")}
          </button>
        )}
        <button
          type="button"
          autoFocus
          disabled={saving}
          onClick={onClose}
          data-testid="builder-prompt-cancel"
          className="inline-flex h-9 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
        >
          {_x("Cancel", "multilingual.cancel")}
        </button>
        <button
          type="button"
          disabled={saving}
          onClick={onSaveAndOpen}
          data-testid="builder-prompt-save"
          className="inline-flex h-9 items-center rounded-md bg-od-accent px-3.5 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
        >
          {saving ? __("Saving…") : __("Save & open Builder")}
        </button>
      </div>
    </dialog>
  );
};
