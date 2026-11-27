import React, { useEffect, useId, useState } from 'react';
import {
  ResponsiveDot,
  SegmentedButtonsControl,
  STUDIO_INPUT_CLASS,
  ToggleSwitchControl,
  type ResponsiveDotInfo,
} from '../../builder/StudioFieldControls.tsx';
import { useI18n } from '../../i18n/index.tsx';
import { CORE_ICON_MAP } from '../icons.ts';
import { fetchOptionsSource } from '../optionsSource.ts';
import type { DynamicSelectOption, FieldSpec, SelectOption } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

export const SelectControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  contextTypeKey?: string | null;
  locale?: string | null;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: string | string[]) => void;
  onApiError: (error: unknown, fallback: string) => void;
}> = ({
  field,
  value,
  contextTypeKey,
  locale,
  disabled = false,
  responsiveInfo,
  onChange,
  onApiError,
}) => {
  if (field.options_source) {
    return (
      <DynamicSelectControl
        field={field}
        value={value}
        contextTypeKey={contextTypeKey}
        locale={locale}
        disabled={disabled}
        onChange={onChange}
        onApiError={onApiError}
      />
    );
  }
  return (
    <StaticSelectControl
      field={field}
      value={value}
      disabled={disabled}
      responsiveInfo={responsiveInfo}
      onChange={onChange}
    />
  );
};

const StaticSelectControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: string | string[]) => void;
}> = ({ field, value, disabled = false, responsiveInfo, onChange }) => {
  const { __ } = useI18n();
  const id = useId();
  const options = field.options ?? [];

  if (field.multiple) {
    const selected = Array.isArray(value)
      ? value.filter((v): v is string => typeof v === 'string')
      : [];
    const selectedSet = new Set(selected);
    const toggle = (val: string) => {
      if (selectedSet.has(val)) {
        onChange(selected.filter(v => v !== val));
      } else {
        onChange([...selected, val]);
      }
    };
    return (
      <div className="space-y-1.5" data-testid={`field-select-multi-${field.key}`}>
        <FieldLabelHeader label={field.label} required={field.required} />
        <div className="flex flex-wrap gap-1.5">
          {options.map(opt => {
            const active = selectedSet.has(opt.value);
            return (
              <button
                key={opt.value}
                type="button"
                disabled={disabled}
                aria-pressed={active}
                onClick={() => toggle(opt.value)}
                className={
                  'rounded-md border px-2.5 py-1 text-xs font-medium transition-colors disabled:opacity-45 ' +
                  (active
                    ? 'border-sky-600 bg-sky-50 text-sky-800'
                    : 'border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50')
                }
              >
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>
    );
  }

  const strVal = typeof value === 'string' ? value : '';
  if (field.display === 'buttons' && options.length > 0 && options.length <= 6 && !field.allow_null) {
    return (
      <SegmentedButtonsControl
        label={field.label}
        value={strVal}
        options={options}
        disabled={disabled}
        responsiveInfo={responsiveInfo}
        onChange={val => onChange(val)}
      />
    );
  }

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
        <ResponsiveDot info={responsiveInfo} />
      </div>
      <select
        id={id}
        disabled={disabled}
        value={strVal}
        onChange={e => onChange(e.target.value)}
        className={STUDIO_INPUT_CLASS}
      >
        {(field.allow_null || !strVal) && (
          <option value="">{__("— Select —")}</option>
        )}
        {options.map(opt => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
};

export const DynamicSelectControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  contextTypeKey?: string | null;
  locale?: string | null;
  disabled?: boolean;
  onChange: (next: string | string[]) => void;
  onApiError: (error: unknown, fallback: string) => void;
}> = ({ field, value, contextTypeKey, locale, disabled = false, onChange, onApiError }) => {
  const { __ } = useI18n();
  const id = useId();
  const source = field.options_source;
  const [items, setItems] = useState<DynamicSelectOption[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!source) return;
    const controller = new AbortController();
    setLoading(true);
    fetchOptionsSource(source, {
      typeKey: contextTypeKey,
      locale,
      signal: controller.signal,
    })
      .then(list => {
        if (!controller.signal.aborted) setItems(list);
      })
      .catch(err => {
        if (!controller.signal.aborted) {
          onApiError(err, __("Could not load dynamic options source."));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [source, contextTypeKey, locale, onApiError]);

  if (field.multiple) {
    const selected = Array.isArray(value)
      ? value.filter((v): v is string => typeof v === 'string')
      : [];
    const selectedSet = new Set(selected);
    const toggle = (val: string) => {
      if (selectedSet.has(val)) {
        onChange(selected.filter(v => v !== val));
      } else {
        onChange([...selected, val]);
      }
    };
    return (
      <div className="space-y-1.5">
        <FieldLabelHeader label={field.label} required={field.required} />
        {loading ? (
          <p className="text-xs text-zinc-500">{__("Loading…")}</p>
        ) : items.length === 0 ? (
          <p className="text-xs italic text-zinc-400">{__("No available items in this source.")}</p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {items.map(opt => {
              const active = selectedSet.has(opt.value);
              return (
                <button
                  key={opt.value}
                  type="button"
                  disabled={disabled}
                  aria-pressed={active}
                  onClick={() => toggle(opt.value)}
                  className={
                    'rounded-md border px-2.5 py-1 text-xs font-medium transition-colors disabled:opacity-45 ' +
                    (active
                      ? 'border-sky-600 bg-sky-50 text-sky-800'
                      : 'border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50')
                  }
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  const strVal = typeof value === 'string' ? value : '';
  const hasSavedMissing = Boolean(strVal) && !items.some(item => item.value === strVal);

  return (
    <div className="space-y-1" data-testid={`field-dynamic-select-${field.key}`}>
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
      <select
        id={id}
        disabled={disabled || loading}
        value={strVal}
        onChange={e => onChange(e.target.value)}
        className={STUDIO_INPUT_CLASS}
      >
        <option value="">
          {loading ? __("Loading…") : __("— Select —")}
        </option>
        {hasSavedMissing && (
          <option value={strVal}>
            {__("Saved value (unavailable): {{value}}", { value: strVal })}
          </option>
        )}
        {items.map(item => (
          <option key={item.value} value={item.value}>
            {item.meta ? `${item.label} (${item.meta})` : item.label}
          </option>
        ))}
      </select>
    </div>
  );
};

export const CheckboxGroupControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string[]) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const options = field.options ?? [];
  const selected = Array.isArray(value)
    ? value.filter((v): v is string => typeof v === 'string')
    : [];
  const maxItems = field.max_items ?? options.length;
  const isHorizontal = field.layout === 'horizontal';

  const toggle = (optVal: string, checked: boolean) => {
    if (checked) {
      if (selected.length >= maxItems || selected.includes(optVal)) return;
      onChange([...selected, optVal]);
    } else {
      onChange(selected.filter(v => v !== optVal));
    }
  };

  return (
    <div className="space-y-1.5" data-testid={`field-checkbox-${field.key}`}>
      <FieldLabelHeader label={field.label} required={field.required} />
      <div className={isHorizontal ? 'flex flex-wrap gap-3' : 'space-y-1.5'}>
        {options.map(opt => {
          const checked = selected.includes(opt.value);
          return (
            <label
              key={opt.value}
              className="inline-flex cursor-pointer items-center gap-2 text-xs text-zinc-800"
            >
              <input
                type="checkbox"
                checked={checked}
                disabled={disabled || (!checked && selected.length >= maxItems)}
                onChange={e => toggle(opt.value, e.target.checked)}
                className="h-3.5 w-3.5 rounded border-zinc-300 text-sky-600 focus:ring-sky-600"
              />
              <span>{opt.label}</span>
            </label>
          );
        })}
      </div>
    </div>
  );
};

export const RadioButtonsControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: string) => void;
}> = ({ field, value, disabled = false, responsiveInfo, onChange }) => {
  const options: SelectOption[] = field.options ?? [];
  const strVal = typeof value === 'string' ? value : '';
  const hasIcons = options.some(o => Boolean(o.icon));

  if (field.layout === 'radio') {
    return (
      <div className="space-y-1.5" data-testid={`field-radio-${field.key}`}>
        <FieldLabelHeader label={field.label} required={field.required} />
        <div className="space-y-1.5" role="radiogroup" aria-label={field.label}>
          {options.map(opt => (
            <label
              key={opt.value}
              className="flex cursor-pointer items-center gap-2 text-xs text-zinc-800"
            >
              <input
                type="radio"
                name={`radio-${field.key}`}
                value={opt.value}
                checked={strVal === opt.value}
                disabled={disabled}
                onChange={() => onChange(opt.value)}
                className="h-3.5 w-3.5 border-zinc-300 text-sky-600 focus:ring-sky-600"
              />
              <span>{opt.label}</span>
            </label>
          ))}
        </div>
      </div>
    );
  }

  if (hasIcons) {
    return (
      <div className="space-y-1.5" data-testid={`field-radio-buttons-${field.key}`}>
        <div className="flex items-center justify-between gap-2">
          <FieldLabelHeader label={field.label} required={field.required} />
          <ResponsiveDot info={responsiveInfo} />
        </div>
        <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label={field.label}>
          {options.map(opt => {
            const active = strVal === opt.value;
            const iconPaths = opt.icon ? CORE_ICON_MAP.get(opt.icon) : undefined;
            return (
              <button
                key={opt.value}
                type="button"
                role="radio"
                aria-checked={active}
                disabled={disabled}
                onClick={() => onChange(opt.value)}
                className={
                  'inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs font-medium transition-colors disabled:opacity-45 ' +
                  (active
                    ? 'border-sky-600 bg-sky-600 text-white'
                    : 'border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50')
                }
              >
                {iconPaths && (
                  <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className="h-3.5 w-3.5 shrink-0"
                    aria-hidden="true"
                    dangerouslySetInnerHTML={{ __html: iconPaths }}
                  />
                )}
                <span>{opt.label}</span>
              </button>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <SegmentedButtonsControl
      label={field.label}
      value={strVal}
      options={options}
      disabled={disabled}
      responsiveInfo={responsiveInfo}
      onChange={val => onChange(val)}
    />
  );
};

export const BooleanControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: boolean) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const checked = Boolean(value);
  const stateLabel = checked ? field.on_label : field.off_label;

  return (
    <ToggleSwitchControl
      label={field.label}
      description={stateLabel}
      checked={checked}
      disabled={disabled}
      testId={`field-boolean-${field.key}`}
      onChange={onChange}
    />
  );
};
