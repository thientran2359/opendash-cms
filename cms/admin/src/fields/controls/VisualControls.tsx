import React, { useId, useMemo, useState } from 'react';
import { ColorSwatchControl } from '../../builder/StudioFieldControls.tsx';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { useI18n } from '../../i18n/index.tsx';
import { CORE_ICONS, CORE_ICON_MAP } from '../icons.ts';
import type { FieldSpec } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

function splitHexAlpha(raw: string): { rgbHex: string; alpha: number } | null {
  if (/^#[0-9a-fA-F]{6}$/.test(raw)) {
    return { rgbHex: raw.toLowerCase(), alpha: 255 };
  }
  if (/^#[0-9a-fA-F]{8}$/.test(raw)) {
    return {
      rgbHex: raw.slice(0, 7).toLowerCase(),
      alpha: Number.parseInt(raw.slice(7, 9), 16),
    };
  }
  if (/^#[0-9a-fA-F]{3}$/.test(raw)) {
    const r = raw[1];
    const g = raw[2];
    const b = raw[3];
    return { rgbHex: `#${r}${r}${g}${g}${b}${b}`.toLowerCase(), alpha: 255 };
  }
  return null;
}

export const ColorControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const { t } = useI18n();
  const id = useId();
  const strVal = typeof value === 'string' ? value : '';
  const parsedAlpha = field.allow_alpha ? splitHexAlpha(strVal) : null;

  return (
    <div className="space-y-1.5" data-testid={`field-color-${field.key}`}>
      <ColorSwatchControl
        id={id}
        label={field.label}
        value={strVal}
        disabled={disabled}
        onChange={onChange}
      />
      {field.allow_alpha && parsedAlpha && (
        <div className="flex items-center gap-2 rounded border border-zinc-200 bg-zinc-50 px-2 py-1 text-[11px] text-zinc-600">
          <span>{t('fields.color.opacity')}</span>
          <input
            type="range"
            min={0}
            max={255}
            step={1}
            disabled={disabled}
            value={parsedAlpha.alpha}
            aria-label={t('fields.color.opacity_for', { label: field.label })}
            onChange={e => {
              const a = Number(e.target.value);
              const hexA = a.toString(16).padStart(2, '0');
              onChange(a === 255 ? parsedAlpha.rgbHex : `${parsedAlpha.rgbHex}${hexA}`);
            }}
            className="h-1.5 flex-1 cursor-pointer accent-sky-600"
          />
          <span className="w-9 text-right font-mono tabular-nums">
            {Math.round((parsedAlpha.alpha / 255) * 100)}%
          </span>
        </div>
      )}
    </div>
  );
};

export const IconPickerControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const current = typeof value === 'string' ? value : '';
  const currentPaths = current ? CORE_ICON_MAP.get(current) : undefined;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return CORE_ICONS;
    return CORE_ICONS.filter(icon => icon.name.includes(q));
  }, [query]);

  return (
    <div className="space-y-1.5" data-testid={`field-icon-${field.key}`}>
      <FieldLabelHeader label={field.label} required={field.required} />
      <div className="flex items-center gap-2 rounded-md border border-zinc-300 bg-white px-2.5 py-1.5">
        <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded border border-zinc-200 bg-zinc-50 text-zinc-700">
          {currentPaths ? (
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              className="h-4 w-4"
              aria-hidden="true"
              dangerouslySetInnerHTML={{ __html: currentPaths }}
            />
          ) : (
            <span className="text-[10px] text-zinc-400">—</span>
          )}
        </div>
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-zinc-800">
          {current || t('fields.icon.none_selected')}
        </span>
        <AdminButton
          size="sm"
          variant="secondary"
          disabled={disabled}
          onClick={() => setOpen(v => !v)}
        >
          {open ? t('fields.common.close') : t('fields.icon.choose')}
        </AdminButton>
        {current && (
          <AdminButton
            size="sm"
            variant="ghost"
            disabled={disabled}
            onClick={() => onChange('')}
          >
            {t('fields.common.clear')}
          </AdminButton>
        )}
      </div>

      {open && (
        <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/80 p-2.5">
          <input
            type="search"
            value={query}
            disabled={disabled}
            placeholder={t('fields.icon.search_placeholder')}
            onChange={e => setQuery(e.target.value)}
            className="h-7 w-full rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-sky-600"
          />
          {filtered.length === 0 ? (
            <p className="text-xs text-zinc-500">{t('fields.icon.no_matches')}</p>
          ) : (
            <div className="grid max-h-48 grid-cols-6 gap-1 overflow-y-auto sm:grid-cols-8">
              {filtered.map(icon => {
                const selected = icon.name === current;
                return (
                  <button
                    key={icon.name}
                    type="button"
                    disabled={disabled}
                    title={icon.name}
                    aria-label={icon.name}
                    aria-pressed={selected}
                    onClick={() => {
                      onChange(icon.name);
                      setOpen(false);
                    }}
                    className={
                      'flex h-8 w-full items-center justify-center rounded border transition-colors ' +
                      (selected
                        ? 'border-sky-600 bg-sky-600 text-white'
                        : 'border-zinc-200 bg-white text-zinc-700 hover:border-zinc-300 hover:bg-zinc-100')
                    }
                  >
                    <svg
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth={2}
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      className="h-4 w-4"
                      aria-hidden="true"
                      dangerouslySetInnerHTML={{ __html: icon.paths }}
                    />
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
