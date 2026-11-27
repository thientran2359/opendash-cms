import { __, _x, useI18n } from '../../i18n/index.tsx';
import { useId, useState } from 'react';
import { queryContentEntryTable } from '../../api/content.ts';
import type { EntryTableRow } from '../../features/content/types.ts';
import type { LinkValue } from '../../fields/types.ts';
import { StudioRichTextLightbox, stripRichHtmlToPreview } from '../StudioRichTextLightbox.tsx';
import { ToggleSwitchControl } from './basic.tsx';
import { STUDIO_INPUT_CLASS, RESET_BTN_CLASS } from './shared.tsx';

export function LinkControl({
  id, label, value, placeholder, showFlags = true,
  disabled = false, onChange,
}: {
  id?: string;
  label?: string;
  value: LinkValue;
  placeholder?: string;
  showFlags?: boolean;
  disabled?: boolean;
  onChange: (next: LinkValue) => void;
}) {
  const { __: _rich__ } = useI18n();
  const autoId = useId();
  const inputId = id ?? autoId;
  const [showInternal, setShowInternal] = useState(false);
  const [entries, setEntries] = useState<EntryTableRow[]>([]);
  const [loadingEntries, setLoadingEntries] = useState(false);

  const toggleInternal = () => {
    const nextOpen = !showInternal;
    setShowInternal(nextOpen);
    if (nextOpen && entries.length === 0 && !loadingEntries) {
      setLoadingEntries(true);
      void Promise.all([
        queryContentEntryTable('page', { status: 'published', limit: 25 }).catch(() => null),
        queryContentEntryTable('post', { status: 'published', limit: 25 }).catch(() => null),
      ]).then(([pagesRes, postsRes]) => {
        const merged = [...(pagesRes?.items ?? []), ...(postsRes?.items ?? [])];
        setEntries(merged);
      }).finally(() => setLoadingEntries(false));
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <label htmlFor={inputId} className="text-xs font-semibold text-zinc-800">
          {label ?? _x("URL", "builder.studio_field_controls")}
        </label>
        <button
          type="button"
          disabled={disabled}
          onClick={toggleInternal}
          className={RESET_BTN_CLASS}
        >
          {showInternal ? __("Close list") : __("Choose page/post…")}
        </button>
      </div>

      <input
        id={inputId}
        type="text"
        maxLength={2048}
        value={value.url}
        placeholder={placeholder ?? __("https://... or /path")}
        disabled={disabled}
        onChange={e => onChange({ ...value, url: e.target.value })}
        className={STUDIO_INPUT_CLASS}
      />

      {showInternal && (
        <select
          aria-label={__("Choose a published page or post")}
          disabled={disabled || loadingEntries}
          value=""
          onChange={e => {
            if (e.target.value) {
              onChange({ ...value, url: e.target.value });
              setShowInternal(false);
            }
          }}
          className={STUDIO_INPUT_CLASS}
        >
          <option value="">
            {loadingEntries ? __("Loading list…") : entries.length ? __("Choose internal link…") : __("No published content")}
          </option>
          {entries.map(row => {
            const href = row.public_url ?? (row.type_key === 'page' ? `/${row.slug}/` : `/${row.type_key}/${row.slug}/`);
            return (
              <option key={`${row.type_key}-${row.id}`} value={href}>
                [{row.type_key}] {row.title} ({href})
              </option>
            );
          })}
        </select>
      )}

      {showFlags && (
        <div className="space-y-1.5 pt-0.5">
          <ToggleSwitchControl
            label={__("Open in new tab")}
            checked={Boolean(value.new_tab)}
            disabled={disabled}
            onChange={new_tab => onChange({ ...value, new_tab })}
          />
          <ToggleSwitchControl
            label={__("rel=\"nofollow\" attribute")}
            checked={Boolean(value.nofollow)}
            disabled={disabled}
            onChange={nofollow => onChange({ ...value, nofollow })}
          />
        </div>
      )}
    </div>
  );
}

export function LightboxRichTextTrigger({
  id, label, value, placeholder, maxBytes,
  disabled = false, onChange,
}: {
  id?: string;
  label: string;
  value: string;
  placeholder?: string;
  maxBytes?: number | null;
  disabled?: boolean;
  onChange: (nextHtml: string) => void;
}) {
  const { __: _rich__ } = useI18n();
  const [open, setOpen] = useState(false);
  const preview = stripRichHtmlToPreview(value);

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-zinc-800">{label}</span>
      </div>
      <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/70 p-2.5">
        <p className={'!m-0 line-clamp-2 text-xs ' + (preview ? 'text-zinc-700' : 'italic text-zinc-400')}>
          {preview || placeholder || __("No text content yet…")}
        </p>
        <button
          id={id}
          type="button"
          disabled={disabled}
          data-testid="studio-open-richtext-btn"
          onClick={() => setOpen(true)}
          className="flex w-full items-center justify-center gap-1.5 rounded-md border border-sky-200 bg-white px-2.5 py-1.5 text-xs font-semibold text-sky-700 hover:bg-sky-50 disabled:opacity-45"
        >
          <span aria-hidden="true">✎</span>
          <span>{__("Open Text Editor")}</span>
        </button>
      </div>
      <StudioRichTextLightbox
        open={open}
        title={label}
        value={value}
        maxBytes={maxBytes}
        disabled={disabled}
        onChange={onChange}
        onClose={() => setOpen(false)}
      />
    </div>
  );
}
