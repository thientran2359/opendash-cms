import { useI18n } from '../../i18n/index.tsx';
import { useEffect, useId, useState } from 'react';
import { mediaThumbnailUrl } from '../../api/media.ts';
import type { MediaFieldKind } from '../../fields/types.ts';
import { RESET_BTN_CLASS } from './shared.tsx';

export function ColorSwatchControl({
  id, label, value, disabled = false, onChange,
}: {
  id?: string;
  label: string;
  value: string;
  disabled?: boolean;
  onChange: (next: string) => void;
}) {
  const { t } = useI18n();
  const HEX_SWATCHES = [
    { value: 'transparent', title: t('builder.studio_field_controls.transparent') },
    { value: '#0f172a', title: t('builder.studio_field_controls.dark') },
    { value: '#ffffff', title: t('builder.studio_field_controls.white') },
    { value: '#f8fafc', title: t('builder.studio_field_controls.light') },
    { value: '#0284c7', title: t('builder.studio_field_controls.primary') },
    { value: '#e11d48', title: t('builder.studio_field_controls.accent') },
  ];

  const TOKEN_SWATCHES = [
    { value: 'var(--od-token-primary)', label: t('builder.studio_field_controls.token_primary') },
    { value: 'var(--od-token-accent)', label: t('builder.studio_field_controls.token_accent') },
    { value: 'var(--od-token-surface)', label: t('builder.studio_field_controls.token_surface') },
    { value: 'var(--od-token-text)', label: t('builder.studio_field_controls.token_text') },
  ];
  const autoId = useId();
  const inputId = id ?? autoId;
  const hexValue = /^#[0-9a-fA-F]{6}$/.test(value) ? value : '#0284c7';
  const [showTokens, setShowTokens] = useState(value.startsWith('var(--od-token-'));

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <label htmlFor={inputId} className="text-xs font-semibold text-zinc-800">{label}</label>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            disabled={disabled}
            onClick={() => setShowTokens(v => !v)}
            className={RESET_BTN_CLASS}
          >
            {showTokens ? t('builder.studio_field_controls.hide_tokens') : t('builder.studio_field_controls.theme_tokens')}
          </button>
          {value && (
            <button type="button" disabled={disabled} className={RESET_BTN_CLASS} onClick={() => onChange('')}>
              {t('builder.studio_field_controls.clear')}
            </button>
          )}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        {HEX_SWATCHES.map(swatch => {
          const isTrans = swatch.value === 'transparent';
          const selected = value.toLowerCase() === swatch.value.toLowerCase();
          return (
            <button
              key={swatch.value}
              type="button"
              disabled={disabled}
              title={swatch.title}
              aria-label={swatch.title}
              aria-pressed={selected}
              onClick={() => onChange(swatch.value)}
              style={isTrans ? {
                backgroundImage: 'linear-gradient(135deg, transparent 43%, #ef4444 45%, #ef4444 55%, transparent 57%)',
              } : { backgroundColor: swatch.value }}
              className={'h-6 w-6 shrink-0 rounded-full border transition-transform hover:scale-105 disabled:opacity-45 ' +
                (selected ? 'border-2 border-sky-600 ring-2 ring-sky-200' : 'border-zinc-300 bg-white')}
            />
          );
        })}

        <label className="relative flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center overflow-hidden rounded-full border border-zinc-300 bg-white" title={t('builder.studio_field_controls.any_color')}>
          <input
            type="color"
            value={hexValue}
            disabled={disabled}
            aria-label={t('builder.studio_field_controls.color_for', { label })}
            onChange={e => onChange(e.target.value)}
            className="h-8 w-8 cursor-pointer border-0 bg-transparent p-0"
          />
        </label>

        <input
          id={inputId}
          type="text"
          value={value}
          disabled={disabled}
          placeholder={t('builder.studio_field_controls.hex')}
          onChange={e => onChange(e.target.value)}
          className="h-7 min-w-0 flex-1 rounded-md border border-zinc-300 bg-white px-2 font-mono text-[11px] text-zinc-800 focus-visible:outline-2 focus-visible:outline-sky-600"
        />
      </div>

      {showTokens && (
        <div className="flex flex-wrap gap-1 pt-0.5">
          {TOKEN_SWATCHES.map(tok => {
            const active = value === tok.value;
            return (
              <button
                key={tok.value}
                type="button"
                disabled={disabled}
                onClick={() => onChange(tok.value)}
                className={'rounded border px-2 py-0.5 text-[11px] font-medium ' +
                  (active
                    ? 'border-sky-500 bg-sky-50 text-sky-800'
                    : 'border-zinc-200 bg-zinc-50 text-zinc-700 hover:bg-zinc-100')}
              >
                {tok.label}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function MediaCardControl({
  id, label, value, acceptKind, disabled = false, onPick, onClear, testId,
}: {
  id?: string;
  label: string;
  value: string;
  acceptKind: MediaFieldKind;
  disabled?: boolean;
  onPick: () => void;
  onClear: () => void;
  testId?: string;
}) {
  const { t } = useI18n();
  const [imgBroken, setImgBroken] = useState(false);
  useEffect(() => setImgBroken(false), [value]);

  const showImageThumb = Boolean(value) && acceptKind !== 'video' && !imgBroken;
  const kindText = acceptKind === 'video' ? t('builder.studio_field_controls.video_mp4') : t('builder.studio_field_controls.image');

  return (
    <div className="space-y-1.5" data-testid={testId}>
      <span className="block text-xs font-semibold text-zinc-800">{label}</span>
      <div className="flex items-center gap-2.5 rounded-lg border border-zinc-200 bg-zinc-50/70 p-2">
        {value ? (
          showImageThumb ? (
            <img
              src={mediaThumbnailUrl(value)}
              alt=""
              onError={() => setImgBroken(true)}
              className="h-11 w-14 shrink-0 rounded border border-zinc-200 bg-white object-cover"
            />
          ) : (
            <div className="flex h-11 w-14 shrink-0 items-center justify-center rounded border border-zinc-200 bg-zinc-200/80 text-[10px] font-semibold uppercase text-zinc-700">
              {acceptKind === 'video' ? 'MP4' : t('builder.studio_field_controls.media_short')}
            </div>
          )
        ) : (
          <div className="flex h-11 w-14 shrink-0 items-center justify-center rounded border border-dashed border-zinc-300 bg-white text-[10px] text-zinc-400">
            {t('builder.studio_field_controls.empty')}
          </div>
        )}
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-zinc-800">
            {value ? t('builder.studio_field_controls.selected', { kind: kindText }) : t('builder.studio_field_controls.not_selected', { kind: kindText.toLowerCase() })}
          </p>
          {value && (
            <p className="truncate font-mono text-[10px] text-zinc-400" title={value}>
              {value}
            </p>
          )}
          <div className="mt-1 flex flex-wrap gap-1.5">
            <button
              id={id}
              type="button"
              disabled={disabled}
              onClick={onPick}
              className="rounded border border-zinc-300 bg-white px-2 py-0.5 text-[11px] font-medium text-zinc-700 hover:bg-zinc-100 disabled:opacity-45"
            >
              {value ? t('builder.studio_field_controls.change') : t('builder.studio_field_controls.choose', { kind: kindText.toLowerCase() })}
            </button>
            {value && (
              <button
                type="button"
                disabled={disabled}
                onClick={onClear}
                className="rounded border border-zinc-200 bg-white px-2 py-0.5 text-[11px] text-rose-700 hover:bg-rose-50 disabled:opacity-45"
              >
                {t('builder.studio_field_controls.clear')}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

