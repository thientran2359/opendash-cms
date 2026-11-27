import React, { useId } from 'react';
import {
  RangeNumberControl,
  ResponsiveDot,
  STUDIO_INPUT_CLASS,
  type ResponsiveDotInfo,
} from '../../builder/StudioFieldControls.tsx';
import { useI18n } from '../../i18n/index.tsx';
import type { FieldSpec } from '../types.ts';

function AffixWrap({
  prepend,
  append,
  children,
}: {
  prepend?: string;
  append?: string;
  children: React.ReactNode;
}) {
  if (!prepend && !append) return <>{children}</>;
  return (
    <div className="flex items-stretch overflow-hidden rounded-md border border-zinc-300 bg-white focus-within:border-sky-600 focus-within:ring-1 focus-within:ring-sky-600">
      {prepend && (
        <span className="flex items-center border-r border-zinc-200 bg-zinc-50 px-2 text-xs text-zinc-600">
          {prepend}
        </span>
      )}
      <div className="min-w-0 flex-1 [&_input]:border-0 [&_input]:rounded-none [&_input]:focus-visible:outline-none">
        {children}
      </div>
      {append && (
        <span className="flex items-center border-l border-zinc-200 bg-zinc-50 px-2 text-xs text-zinc-600">
          {append}
        </span>
      )}
    </div>
  );
}

export function FieldLabelHeader({
  htmlFor,
  label,
  required,
  badge,
}: {
  htmlFor?: string;
  label: string;
  required?: boolean;
  badge?: string;
}) {
  const { __ } = useI18n();
  return (
    <div className="flex items-center justify-between gap-2">
      {htmlFor ? (
        <label htmlFor={htmlFor} className="text-xs font-semibold text-zinc-800">
          {label}
          {required && <span className="ml-0.5 text-rose-600" aria-label={__("Required")}>*</span>}
        </label>
      ) : (
        <span className="text-xs font-semibold text-zinc-800">
          {label}
          {required && <span className="ml-0.5 text-rose-600" aria-label={__("Required")}>*</span>}
        </span>
      )}
      {badge && <span className="font-mono text-[11px] text-zinc-500">{badge}</span>}
    </div>
  );
}

export function TextControl({
  field,
  value,
  disabled,
  onChange,
}: {
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string) => void;
}) {
  const id = useId();
  const str = typeof value === 'string' ? value : '';
  const inputType = field.type === 'email' ? 'email' : field.type === 'url' ? 'url' : 'text';
  const maxLen = field.max_chars ?? (field.type === 'email' ? 254 : field.type === 'url' ? 2048 : 512);

  return (
    <div className="space-y-1">
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
      <AffixWrap prepend={field.prepend} append={field.append}>
        <input
          id={id}
          type={inputType}
          value={str}
          maxLength={maxLen}
          placeholder={field.placeholder}
          disabled={disabled}
          onChange={e => onChange(e.target.value)}
          className={STUDIO_INPUT_CLASS}
        />
      </AffixWrap>
    </div>
  );
}

export function TextareaControl({
  field,
  value,
  disabled,
  onChange,
}: {
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (next: string) => void;
}) {
  const id = useId();
  const str = typeof value === 'string' ? value : '';
  const rows = field.rows ?? 4;
  const maxLen = field.max_chars ?? 4000;

  return (
    <div className="space-y-1">
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
      <textarea
        id={id}
        rows={rows}
        value={str}
        maxLength={maxLen}
        placeholder={field.placeholder}
        disabled={disabled}
        onChange={e => onChange(e.target.value)}
        className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-xs text-zinc-900 placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-sky-600 disabled:bg-zinc-100 disabled:text-zinc-500"
      />
    </div>
  );
}

export function NumberControl({
  field,
  value,
  disabled,
  responsiveInfo,
  onChange,
}: {
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: number | null) => void;
}) {
  const id = useId();
  const numVal = typeof value === 'number' && Number.isFinite(value) ? value : '';
  const badge = field.unit ? field.unit : undefined;

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} badge={badge} />
        <ResponsiveDot info={responsiveInfo} />
      </div>
      <AffixWrap prepend={field.prepend} append={field.append}>
        <input
          id={id}
          type="number"
          min={field.min}
          max={field.max}
          step={field.step ?? 'any'}
          placeholder={field.placeholder}
          disabled={disabled}
          value={numVal}
          onChange={e => {
            const raw = e.target.value.trim();
            if (!raw) {
              onChange(null);
              return;
            }
            const parsed = Number(raw);
            if (Number.isFinite(parsed)) onChange(parsed);
          }}
          className={STUDIO_INPUT_CLASS}
        />
      </AffixWrap>
    </div>
  );
}

export function RangeControl({
  field,
  value,
  disabled,
  responsiveInfo,
  onChange,
}: {
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: number | undefined) => void;
}) {
  const numVal = typeof value === 'number' && Number.isFinite(value) ? value : undefined;
  return (
    <RangeNumberControl
      label={field.label}
      value={numVal}
      fallbackValue={typeof field.default === 'number' ? field.default : field.min ?? 0}
      min={field.min ?? 0}
      max={field.max ?? 100}
      step={field.step ?? 1}
      unit={field.unit}
      placeholder={field.placeholder}
      disabled={disabled}
      responsiveInfo={responsiveInfo}
      onChange={onChange}
    />
  );
}
