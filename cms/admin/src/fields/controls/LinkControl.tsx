import React, { useId, useState } from 'react';
import { queryContentEntryTable } from '../../api/content.ts';
import type { EntryTableRow } from '../../features/content/types.ts';
import { STUDIO_INPUT_CLASS, ToggleSwitchControl } from '../../builder/StudioFieldControls.tsx';
import { useI18n } from '../../i18n/index.tsx';
import type { FieldSpec, LinkValue } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

function normalizeLink(raw: unknown): LinkValue {
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
    const obj = raw as Record<string, unknown>;
    return {
      url: typeof obj.url === 'string' ? obj.url : '',
      label: typeof obj.label === 'string' ? obj.label : '',
      new_tab: Boolean(obj.new_tab),
      nofollow: Boolean(obj.nofollow),
    };
  }
  return { url: '', label: '', new_tab: false, nofollow: false };
}

export const LinkControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  locale?: string | null;
  disabled?: boolean;
  onChange: (next: LinkValue) => void;
}> = ({ field, value, locale, disabled = false, onChange }) => {
  const { t } = useI18n();
  const labelId = useId();
  const urlId = useId();
  const [showInternal, setShowInternal] = useState(false);
  const [entries, setEntries] = useState<EntryTableRow[]>([]);
  const [loadingEntries, setLoadingEntries] = useState(false);

  const link = normalizeLink(value);

  const toggleInternal = () => {
    const nextOpen = !showInternal;
    setShowInternal(nextOpen);
    if (nextOpen && entries.length === 0 && !loadingEntries) {
      setLoadingEntries(true);
      void Promise.all([
        queryContentEntryTable('page', { status: 'published', limit: 25, locale: locale ?? undefined }).catch(() => null),
        queryContentEntryTable('post', { status: 'published', limit: 25, locale: locale ?? undefined }).catch(() => null),
      ])
        .then(([pagesRes, postsRes]) => {
          setEntries([...(pagesRes?.items ?? []), ...(postsRes?.items ?? [])]);
        })
        .finally(() => setLoadingEntries(false));
    }
  };

  return (
    <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-2.5" data-testid={`field-link-${field.key}`}>
      <div className="flex items-center justify-between gap-2">
        <FieldLabelHeader htmlFor={urlId} label={field.label} required={field.required} />
        <button
          type="button"
          disabled={disabled}
          onClick={toggleInternal}
          className="text-[11px] font-medium text-sky-700 hover:underline disabled:opacity-45"
        >
          {showInternal
            ? t('builder.studio_field_controls.close_list')
            : t('builder.studio_field_controls.choose_entry')}
        </button>
      </div>

      <div className="space-y-1">
        <label htmlFor={labelId} className="block text-[11px] font-medium text-zinc-600">
          {t('fields.link.label_text')}
        </label>
        <input
          id={labelId}
          type="text"
          maxLength={160}
          value={link.label ?? ''}
          placeholder={t('fields.link.label_placeholder')}
          disabled={disabled}
          onChange={e => onChange({ ...link, label: e.target.value })}
          className={STUDIO_INPUT_CLASS}
        />
      </div>

      <div className="space-y-1">
        <label htmlFor={urlId} className="block text-[11px] font-medium text-zinc-600">
          {t('builder.studio_field_controls.url')}
        </label>
        <input
          id={urlId}
          type="text"
          maxLength={2048}
          value={link.url}
          placeholder={field.placeholder ?? t('builder.studio_field_controls.url_placeholder')}
          disabled={disabled}
          onChange={e => onChange({ ...link, url: e.target.value })}
          className={STUDIO_INPUT_CLASS}
        />
      </div>

      {showInternal && (
        <select
          aria-label={t('builder.studio_field_controls.choose_published')}
          disabled={disabled || loadingEntries}
          value=""
          onChange={e => {
            if (e.target.value) {
              const chosen = entries.find(row => (row.public_url ?? (row.type_key === 'page' ? `/${row.slug}/` : `/${row.type_key}/${row.slug}/`)) === e.target.value);
              onChange({
                ...link,
                url: e.target.value,
                label: link.label || chosen?.title || '',
              });
              setShowInternal(false);
            }
          }}
          className={STUDIO_INPUT_CLASS}
        >
          <option value="">
            {loadingEntries
              ? t('builder.studio_field_controls.loading')
              : entries.length
                ? t('builder.studio_field_controls.choose_internal')
                : t('builder.studio_field_controls.no_published')}
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

      <div className="space-y-1 pt-0.5">
        <ToggleSwitchControl
          label={t('builder.studio_field_controls.new_tab')}
          checked={Boolean(link.new_tab)}
          disabled={disabled}
          onChange={new_tab => onChange({ ...link, new_tab })}
        />
        <ToggleSwitchControl
          label={t('builder.studio_field_controls.nofollow')}
          checked={Boolean(link.nofollow)}
          disabled={disabled}
          onChange={nofollow => onChange({ ...link, nofollow })}
        />
      </div>
    </div>
  );
};
