import React, { useEffect, useRef } from 'react';
import { __, _x } from '../../i18n/index.tsx';

export interface SingleEditTranslationModalProps {
  targetLocale: string | null;
  sourceTitle: string;
  translationTitle: string;
  translationSlug: string;
  creating: boolean;
  error: string | null;
  canCreate: boolean;
  onTitleChange: (next: string) => void;
  onSlugChange: (next: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
}

export const SingleEditTranslationModal: React.FC<SingleEditTranslationModalProps> = ({
  targetLocale,
  sourceTitle,
  translationTitle,
  translationSlug,
  creating,
  error,
  canCreate,
  onTitleChange,
  onSlugChange,
  onCancel,
  onConfirm,
}) => {
  const dialogRef = useRef<HTMLDialogElement | null>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (targetLocale && !dialog.open) dialog.showModal();
    else if (!targetLocale && dialog.open) dialog.close();
  }, [targetLocale]);

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="create-translation-modal-title"
      onClose={onCancel}
      data-testid={targetLocale ? 'create-translation-modal' : undefined}
      className="od-modal-dialog m-auto w-full max-w-md space-y-4 rounded-lg border border-zinc-200 bg-white p-5 text-zinc-900 shadow-xl"
    >
      {targetLocale && (
        <>
          <h1
            id="create-translation-modal-title"
            className="text-base font-semibold text-zinc-900"
          >
            {__("Create translation (")}
            {targetLocale})
          </h1>
          <p className="text-xs text-zinc-600">
            {__(
              "The system copies content, custom fields, matching-language taxonomy terms and referenced media to a new draft translation.",
            )}
          </p>
          <div className="space-y-3">
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Title (optional)")}</span>
              <input
                type="text"
                value={translationTitle}
                onChange={e => onTitleChange(e.target.value)}
                placeholder={
                  sourceTitle || __("Leave empty to copy the original title")
                }
                className="h-9 rounded-md border border-zinc-300 px-2.5 text-sm text-zinc-900"
              />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{__("Slug (optional)")}</span>
              <input
                type="text"
                value={translationSlug}
                onChange={e => onSlugChange(e.target.value)}
                placeholder={__("Leave empty to generate from title")}
                className="h-9 rounded-md border border-zinc-300 px-2.5 text-sm text-zinc-900"
              />
            </label>
            {error && (
              <p role="alert" className="text-xs font-medium text-rose-700">
                {error}
              </p>
            )}
          </div>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              autoFocus
              disabled={creating}
              onClick={onCancel}
              className="inline-flex h-9 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
            >
              {_x("Cancel", "multilingual.cancel")}
            </button>
            <button
              type="button"
              disabled={creating || !canCreate}
              onClick={onConfirm}
              data-testid="confirm-create-translation"
              className="inline-flex h-9 items-center rounded-md bg-od-accent px-3.5 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
            >
              {creating ? __("Creating…") : __("Create translation")}
            </button>
          </div>
        </>
      )}
    </dialog>
  );
};
