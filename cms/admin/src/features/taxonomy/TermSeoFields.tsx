import React, { useState } from 'react';
import { mediaThumbnailUrl } from '../../api/media.ts';
import type { TermSeoInput } from '../../api/taxonomy.ts';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { __ } from '../../i18n/index.tsx';
import { MediaPicker } from '../media/MediaPicker.tsx';

export interface TermSeoFieldsProps {
  value: TermSeoInput;
  disabled: boolean;
  csrfToken: string | null;
  editorRestricted?: boolean;
  onChange: (next: TermSeoInput) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

const INPUT_CLASS =
  'mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

export const TermSeoFields: React.FC<TermSeoFieldsProps> = ({
  value,
  disabled,
  csrfToken,
  editorRestricted = false,
  onChange,
  onApiError,
}) => {
  const [pickingOg, setPickingOg] = useState(false);

  return (
    <fieldset
      disabled={disabled || editorRestricted}
      className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/60 p-4"
      data-testid="term-seo-fields"
    >
      <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-800">
        {__("SEO metadata")}
      </legend>
      {editorRestricted && (
        <p className="text-xs text-zinc-500">
          {__("Only admins can modify archive SEO metadata for terms.")}
        </p>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        <label htmlFor="term-seo-title" className="block text-xs font-medium text-zinc-700">
          <span>{__("SEO title (up to 120 characters)")}</span>
          <input
            id="term-seo-title"
            type="text"
            className={INPUT_CLASS}
            maxLength={120}
            value={value.title ?? ''}
            onChange={e => onChange({ ...value, title: e.target.value || null })}
          />
        </label>

        <label htmlFor="term-seo-canonical" className="block text-xs font-medium text-zinc-700">
          <span>{__("Custom canonical URL")}</span>
          <input
            id="term-seo-canonical"
            type="url"
            className={INPUT_CLASS}
            autoCorrect="off"
            autoCapitalize="none"
            spellCheck={false}
            value={value.canonical_url ?? ''}
            onChange={e => onChange({ ...value, canonical_url: e.target.value || null })}
          />
        </label>
      </div>

      <label htmlFor="term-seo-desc" className="block text-xs font-medium text-zinc-700">
        <span>{__("SEO description (up to 320 characters)")}</span>
        <textarea
          id="term-seo-desc"
          rows={2}
          maxLength={320}
          className="mt-1 w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
          value={value.description ?? ''}
          onChange={e => onChange({ ...value, description: e.target.value || null })}
        />
      </label>

      <div className="space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <label htmlFor="term-seo-og" className="text-xs font-medium text-zinc-700">
            {__("OG image media ID")}
          </label>
          <div className="flex gap-2">
            <AdminButton
              type="button"
              variant="secondary"
              size="sm"
              disabled={disabled || editorRestricted}
              onClick={() => setPickingOg(true)}
            >
              {value.og_image_id ? __("Change OG image") : __("Choose OG image")}
            </AdminButton>
            {value.og_image_id && (
              <AdminButton
                type="button"
                variant="secondary"
                size="sm"
                disabled={disabled || editorRestricted}
                onClick={() => onChange({ ...value, og_image_id: null })}
              >
                {__("Clear")}
              </AdminButton>
            )}
          </div>
        </div>
        <div className="flex items-center gap-3">
          {value.og_image_id && (
            <img
              src={mediaThumbnailUrl(value.og_image_id)}
              alt=""
              className="h-12 w-12 shrink-0 rounded border border-zinc-200 bg-white object-cover"
            />
          )}
          <input
            id="term-seo-og"
            type="text"
            className="h-9 min-w-0 flex-1 rounded-md border border-zinc-300 bg-white px-2.5 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
            value={value.og_image_id ?? ''}
            placeholder={__("SHA-256 media ID")}
            onChange={e => onChange({ ...value, og_image_id: e.target.value.trim() || null })}
          />
        </div>
      </div>

      <label className="flex items-center gap-2 text-xs text-zinc-800">
        <input
          type="checkbox"
          className="h-4 w-4 rounded border-zinc-300 accent-sky-600"
          checked={value.noindex}
          onChange={e => onChange({ ...value, noindex: e.target.checked })}
        />
        <span>{__("Prevent search engines from indexing (noindex)")}</span>
      </label>

      {pickingOg && (
        <MediaPicker
          mode="image"
          acceptKind="image"
          selectedIds={value.og_image_id ? [value.og_image_id] : []}
          maxItems={1}
          csrfToken={csrfToken}
          onApply={ids => {
            onChange({ ...value, og_image_id: ids[0] ?? null });
            setPickingOg(false);
          }}
          onClose={() => setPickingOg(false)}
          onError={onApiError}
        />
      )}
    </fieldset>
  );
};
