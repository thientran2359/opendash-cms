import React, { useState } from 'react';
import { mediaThumbnailUrl, type MediaKind } from '../../api/media.ts';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { MediaPicker } from '../../features/media/MediaPicker.tsx';
import { useI18n } from '../../i18n/index.tsx';
import type { FieldSpec } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

export const GalleryControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  csrfToken: string | null;
  disabled?: boolean;
  onChange: (nextIds: string[]) => void;
  onApiError: (error: unknown, fallback: string) => void;
}> = ({ field, value, csrfToken, disabled = false, onChange, onApiError }) => {
  const { __, _x, _n } = useI18n();
  const [pickerOpen, setPickerOpen] = useState(false);
  const ids = Array.isArray(value)
    ? value.filter((v): v is string => typeof v === 'string' && v.length > 0)
    : [];
  const maxItems = Math.max(1, Math.min(50, field.max_items ?? 50));
  const acceptKind: MediaKind | 'any' =
    field.media_kind === 'video' ? 'video' : field.media_kind === 'any' ? 'any' : 'image';

  const move = (index: number, delta: -1 | 1) => {
    const nextIdx = index + delta;
    if (nextIdx < 0 || nextIdx >= ids.length) return;
    const copy = [...ids];
    [copy[index], copy[nextIdx]] = [copy[nextIdx], copy[index]];
    onChange(copy);
  };

  const remove = (index: number) => {
    onChange(ids.filter((_, i) => i !== index));
  };

  return (
    <div className="space-y-2" data-testid={`field-gallery-${field.key}`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FieldLabelHeader
          label={field.label}
          required={field.required}
          badge={_n("{{count}} / {{max}} item", "{{count}} / {{max}} items", ids.length, { max: maxItems })}
        />
        <div className="flex items-center gap-1.5">
          <AdminButton
            size="sm"
            variant="secondary"
            disabled={disabled || ids.length >= maxItems}
            onClick={() => setPickerOpen(true)}
          >
            {ids.length ? __("Edit gallery") : _x("Select media", "fields.gallery")}
          </AdminButton>
          {ids.length > 0 && (
            <AdminButton
              size="sm"
              variant="ghost"
              disabled={disabled}
              onClick={() => onChange([])}
            >
              {__("Clear")}
            </AdminButton>
          )}
        </div>
      </div>

      {ids.length === 0 ? (
        <div className="rounded-lg border border-dashed border-zinc-300 bg-zinc-50/70 px-3 py-4 text-center text-xs text-zinc-500">
          {__("No media items selected yet.")}
        </div>
      ) : (
        <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4">
          {ids.map((id, idx) => (
            <li
              key={`${id}-${idx}`}
              className="group relative overflow-hidden rounded-lg border border-zinc-200 bg-white p-1.5"
            >
              {acceptKind === 'video' ? (
                <div className="flex h-20 items-center justify-center rounded bg-zinc-100 font-mono text-[11px] font-semibold text-zinc-600">
                  MP4
                </div>
              ) : (
                <img
                  src={mediaThumbnailUrl(id)}
                  alt={id}
                  className="h-20 w-full rounded bg-zinc-100 object-cover"
                />
              )}
              <div className="mt-1 flex items-center justify-between gap-1">
                <span className="truncate font-mono text-[10px] text-zinc-400" title={id}>
                  #{idx + 1}
                </span>
                <div className="flex items-center gap-0.5">
                  <button
                    type="button"
                    disabled={disabled || idx === 0}
                    aria-label={_x("Move up", "fields.common")}
                    onClick={() => move(idx, -1)}
                    className="rounded px-1 py-0.5 text-[11px] text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    disabled={disabled || idx === ids.length - 1}
                    aria-label={_x("Move down", "fields.common")}
                    onClick={() => move(idx, 1)}
                    className="rounded px-1 py-0.5 text-[11px] text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                  >
                    ↓
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    aria-label={__("Remove")}
                    onClick={() => remove(idx)}
                    className="rounded px-1 py-0.5 text-[11px] text-rose-600 hover:bg-rose-50 disabled:opacity-35"
                  >
                    ✕
                  </button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      {pickerOpen && (
        <MediaPicker
          mode="gallery"
          acceptKind={acceptKind}
          selectedIds={ids}
          maxItems={maxItems}
          csrfToken={csrfToken}
          onApply={next => {
            onChange(next.slice(0, maxItems));
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
          onError={onApiError}
        />
      )}
    </div>
  );
};
