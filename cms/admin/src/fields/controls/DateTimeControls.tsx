import React, { useId } from 'react';
import { STUDIO_INPUT_CLASS } from '../../builder/StudioFieldControls.tsx';
import { useI18n } from '../../i18n/index.tsx';
import { formatDateTimeForSiteTz, parseSiteTzToUtcIso } from '../helpers.ts';
import type { FieldSpec } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

export const DateControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const id = useId();
  const strVal = typeof value === 'string' ? value : '';

  return (
    <div className="space-y-1" data-testid={`field-date-${field.key}`}>
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
      <input
        id={id}
        type="date"
        value={strVal}
        min={field.min_date}
        max={field.max_date}
        disabled={disabled}
        onChange={e => onChange(e.target.value)}
        className={STUDIO_INPUT_CLASS}
      />
    </div>
  );
};

export const DateTimeControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  siteTimezone?: string;
  disabled?: boolean;
  onChange: (next: string) => void;
}> = ({ field, value, siteTimezone = 'UTC', disabled = false, onChange }) => {
  const { __ } = useI18n();
  const id = useId();
  const utcStr = typeof value === 'string' ? value : '';
  const localStr = formatDateTimeForSiteTz(utcStr, siteTimezone);

  return (
    <div className="space-y-1" data-testid={`field-datetime-${field.key}`}>
      <FieldLabelHeader
        htmlFor={id}
        label={field.label}
        required={field.required}
        badge={siteTimezone || 'UTC'}
      />
      <div className="flex items-center gap-1.5">
        <input
          id={id}
          type="datetime-local"
          step={1}
          value={localStr}
          disabled={disabled}
          onChange={e => {
            const raw = e.target.value;
            if (!raw) {
              onChange('');
              return;
            }
            const iso = parseSiteTzToUtcIso(raw, siteTimezone);
            if (iso) onChange(iso);
          }}
          className={`${STUDIO_INPUT_CLASS} flex-1`}
        />
        {utcStr && !field.required && (
          <button
            type="button"
            disabled={disabled}
            onClick={() => onChange('')}
            className="rounded border border-zinc-200 px-2 py-1 text-[11px] text-zinc-600 hover:bg-zinc-100"
          >
            {__("Clear")}
          </button>
        )}
      </div>
    </div>
  );
};

export const TimeControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const id = useId();
  const strVal = typeof value === 'string' ? value : '';

  return (
    <div className="space-y-1" data-testid={`field-time-${field.key}`}>
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
      <input
        id={id}
        type="time"
        step={1}
        value={strVal}
        disabled={disabled}
        onChange={e => onChange(e.target.value)}
        className={STUDIO_INPUT_CLASS}
      />
    </div>
  );
};
