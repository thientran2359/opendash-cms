import React, { useEffect, useRef, useState } from 'react';
import { createTaxonomyTerm, type TaxonomyDefinition, type TaxonomyTerm } from '../../api/taxonomy.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { __ } from '../../i18n/index.tsx';
import { allowedParents, slugFromName } from './termEditHelpers.ts';

export interface TermInlineCreateModalProps {
  open: boolean;
  definition: TaxonomyDefinition | null;
  existingTerms: TaxonomyTerm[];
  locale?: string | null;
  csrfToken: string | null;
  onCreated: (term: TaxonomyTerm) => void;
  onClose: () => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const TermInlineCreateModal: React.FC<TermInlineCreateModalProps> = ({
  open,
  definition,
  existingTerms,
  locale,
  csrfToken,
  onCreated,
  onClose,
  onApiError,
}) => {
  const [name, setName] = useState('');
  const [slug, setSlug] = useState('');
  const [manualSlug, setManualSlug] = useState(false);
  const [parentId, setParentId] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const nameInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    if (open && !dlg.open) {
      setName('');
      setSlug('');
      setManualSlug(false);
      setParentId('');
      setError('');
      dlg.showModal();
      nameInputRef.current?.focus();
    } else if (!open && dlg.open) {
      dlg.close();
    }
  }, [open]);

  if (!open || !definition) return null;

  const parents = definition.hierarchical ? allowedParents(existingTerms, null) : [];

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    e.stopPropagation();
    if (!csrfToken || busy) return;
    const trimmedName = name.trim();
    const resolvedSlug = (slug.trim() || slugFromName(trimmedName) || 'term').slice(0, 64);
    if (!trimmedName) {
      setError(__("Name cannot be empty."));
      return;
    }
    if (!/^[a-z][a-z0-9-]{0,63}$/.test(resolvedSlug)) {
      setError(__("Slug must start with a-z and contain only lowercase letters, numbers, and hyphens."));
      return;
    }
    setBusy(true);
    setError('');
    try {
      const created = await createTaxonomyTerm(
        {
          kind: definition.key,
          name: trimmedName,
          slug: resolvedSlug,
          parent_id: definition.hierarchical && parentId ? Number(parentId) : null,
          ...(locale ? { locale } : {}),
        },
        csrfToken,
      );
      onCreated(created);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : __("Could not create term."));
      onApiError(cause, __("Could not create term."));
    } finally {
      setBusy(false);
    }
  };

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="inline-term-create-title"
      data-testid={`inline-create-term-dialog-${definition.key}`}
      className="fixed inset-0 m-auto w-full max-w-md rounded-xl border border-zinc-200 bg-white p-5 text-zinc-900 shadow-xl backdrop:bg-zinc-950/40 backdrop:overflow-hidden backdrop:[overscroll-behavior:contain] [overscroll-behavior:contain]"
      onCancel={e => {
        if (busy) {
          e.preventDefault();
          return;
        }
        onClose();
      }}
    >
      <form onSubmit={e => void handleSubmit(e)} className="space-y-4">
        <h2 id="inline-term-create-title" className="text-base font-bold text-zinc-950">
          {__("Add new {{value1}}", { value1: definition.label })}
        </h2>
        {error && <AdminAlert tone="error">{error}</AdminAlert>}

        <label htmlFor="inline-term-name" className="block text-xs font-medium text-zinc-700">
          <span>{__("Name *")}</span>
          <input
            ref={nameInputRef}
            id="inline-term-name"
            type="text"
            required
            maxLength={120}
            disabled={busy}
            className="mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={name}
            onChange={e => {
              const v = e.target.value;
              setName(v);
              if (!manualSlug) setSlug(slugFromName(v));
            }}
          />
        </label>

        <label htmlFor="inline-term-slug" className="block text-xs font-medium text-zinc-700">
          <span>{__("Slug *")}</span>
          <input
            id="inline-term-slug"
            type="text"
            required
            maxLength={64}
            pattern="[a-z](?:[a-z0-9]|-)*"
            disabled={busy}
            className="mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={slug}
            onChange={e => {
              setManualSlug(true);
              setSlug(e.target.value);
            }}
          />
        </label>

        {definition.hierarchical && (
          <label htmlFor="inline-term-parent" className="block text-xs font-medium text-zinc-700">
            <span>{__("Parent term")}</span>
            <select
              id="inline-term-parent"
              disabled={busy}
              className="mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              value={parentId}
              onChange={e => setParentId(e.target.value)}
            >
              <option value="">{__("None (root term)")}</option>
              {parents.map(p => (
                <option key={p.id} value={String(p.id)}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <AdminButton type="button" variant="secondary" disabled={busy} onClick={onClose}>
            {__("Cancel")}
          </AdminButton>
          <AdminButton type="submit" variant="primary" disabled={busy || !csrfToken}>
            {busy ? __("Creating…") : __("Create term")}
          </AdminButton>
        </div>
      </form>
    </dialog>
  );
};
