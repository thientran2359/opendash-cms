import { useI18n } from '../../i18n/index.tsx';
import { useEffect, useId, useState } from 'react';
import { RESET_BTN_CLASS, TW_GAP_STEPS, pxToTwStep, ResponsiveDot, type ResponsiveDotInfo } from './shared.tsx';

export function ToggleSwitchControl({
  id, label, description, checked, disabled = false, onChange, testId,
}: {
  id?: string;
  label: string;
  description?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
  testId?: string;
}) {
  const autoId = useId();
  const controlId = id ?? autoId;
  return (
    <div className="flex items-center justify-between gap-3 py-0.5">
      <label htmlFor={controlId} className="min-w-0 flex-1 cursor-pointer select-none">
        <span className="block text-xs font-medium text-zinc-800">{label}</span>
        {description && <span className="block text-[11px] text-zinc-500">{description}</span>}
      </label>
      <button
        id={controlId}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        data-testid={testId}
        onClick={() => onChange(!checked)}
        className={'relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border border-transparent transition-colors focus-visible:outline-2 focus-visible:outline-sky-600 disabled:cursor-not-allowed disabled:opacity-45 ' +
          (checked ? 'bg-sky-600' : 'bg-zinc-300')}
      >
        <span
          aria-hidden="true"
          className={'inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform ' +
            (checked ? 'translate-x-[18px]' : 'translate-x-[2px]')}
        />
      </button>
    </div>
  );
}

export function RangeNumberControl({
  id, label, readout, badge, value, fallbackValue, min = 0, max = 100, step = 1, unit, placeholder,
  disabled = false, responsiveInfo, onChange, onReset, resetLabel,
}: {
  id?: string;
  label: string;
  readout?: string;
  badge?: string;
  value: number | undefined;
  fallbackValue?: number;
  min?: number;
  max?: number;
  step?: number;
  unit?: string;
  placeholder?: string;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: number | undefined) => void;
  onReset?: () => void;
  resetLabel?: string;
}) {
  const { t } = useI18n();
  const autoId = useId();
  const inputId = id ?? autoId;
  const effective = value ?? fallbackValue;
  const sliderVal = effective ?? min;

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <label htmlFor={inputId} className="text-xs font-semibold text-zinc-800">
            {label}
            {badge && <span className="ml-1 font-normal text-zinc-500">{badge}</span>}
          </label>
          <ResponsiveDot info={responsiveInfo} />
        </div>
        <div className="flex items-center gap-1.5">
          {readout && <span className="font-mono text-[11px] text-zinc-500">{readout}</span>}
          {onReset && (
            <button type="button" disabled={disabled} className={RESET_BTN_CLASS} onClick={onReset}>
              {resetLabel ?? t('builder.studio_field_controls.default')}
            </button>
          )}
        </div>
      </div>
      <div className="flex items-center gap-2">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={Math.min(max, Math.max(min, sliderVal))}
          disabled={disabled}
          aria-label={label}
          onChange={e => {
            const n = Number(e.target.value);
            if (Number.isFinite(n)) onChange(n);
          }}
          className="h-1.5 min-w-0 flex-1 cursor-pointer accent-sky-600"
        />
        <div className="flex h-7 items-center rounded-md border border-zinc-300 bg-white px-1.5">
          <input
            id={inputId}
            type="number"
            min={min}
            max={max}
            step={step}
            value={value ?? ''}
            placeholder={placeholder ?? (fallbackValue !== undefined ? String(fallbackValue) : '')}
            disabled={disabled}
            onChange={e => {
              const raw = e.target.value;
              if (raw === '') {
                onChange(undefined);
                return;
              }
              const n = Number(raw);
              if (Number.isFinite(n)) onChange(n);
            }}
            className="w-11 bg-transparent text-right font-mono text-xs text-zinc-900 tabular-nums focus:outline-none"
          />
          {unit && <span className="ml-1 text-[10px] font-medium text-zinc-500">{unit}</span>}
        </div>
      </div>
    </div>
  );
}

export function TailwindGapControl({
  id, label, value, disabled = false, responsiveInfo, onChange, testIdPrefix,
}: {
  id?: string;
  label?: string;
  value: number;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (px: number) => void;
  testIdPrefix?: string;
}) {
  const { t } = useI18n();
  const displayLabel = label ?? t('builder.studio_field_controls.gap');
  const autoId = useId();
  const inputId = id ?? autoId;
  const matchedStep = pxToTwStep(value, TW_GAP_STEPS);
  const [unitMode, setUnitMode] = useState<'tw' | 'px'>(() => (matchedStep !== null ? 'tw' : 'px'));

  useEffect(() => {
    if (pxToTwStep(value, TW_GAP_STEPS) === null) setUnitMode('px');
  }, [value]);

  const twIndex = matchedStep !== null ? TW_GAP_STEPS.indexOf(matchedStep as (typeof TW_GAP_STEPS)[number]) : 0;

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <label htmlFor={inputId} className="text-xs font-semibold text-zinc-800">{displayLabel}</label>
          <ResponsiveDot info={responsiveInfo} />
        </div>
        <div className="flex items-center gap-1.5">
          <span className="font-mono text-[11px] text-zinc-500">
            {unitMode === 'tw' && matchedStep !== null ? `gap-${matchedStep} (${value}px)` : `${value}px`}
          </span>
          <div className="inline-flex rounded border border-zinc-200 bg-zinc-100/80 p-0.5">
            {(['tw', 'px'] as const).map(mode => (
              <button
                key={mode}
                type="button"
                disabled={disabled}
                aria-pressed={unitMode === mode}
                data-testid={testIdPrefix ? `${testIdPrefix}-unit-${mode}` : undefined}
                onClick={() => {
                  if (mode === 'tw' && matchedStep === null) {
                    const nearest = TW_GAP_STEPS.reduce((best, step) =>
                      Math.abs(step * 4 - value) < Math.abs(best * 4 - value) ? step : best,
                    );
                    onChange(nearest * 4);
                  }
                  setUnitMode(mode);
                }}
                className={'rounded px-1.5 py-0.5 text-[10px] font-semibold transition-colors ' +
                  (unitMode === mode ? 'bg-sky-600 text-white' : 'text-zinc-600 hover:text-zinc-900')}
              >
                {mode === 'tw' ? 'TW' : 'px'}
              </button>
            ))}
          </div>
        </div>
      </div>

      {unitMode === 'tw' ? (
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={0}
            max={TW_GAP_STEPS.length - 1}
            step={1}
            value={twIndex >= 0 ? twIndex : 0}
            disabled={disabled}
            aria-label={`${label} (Tailwind)`}
            onChange={e => {
              const step = TW_GAP_STEPS[Number(e.target.value)];
              if (step !== undefined) onChange(step * 4);
            }}
            className="h-1.5 min-w-0 flex-1 cursor-pointer accent-sky-600"
          />
          <select
            id={inputId}
            disabled={disabled}
            value={matchedStep !== null ? String(matchedStep) : ''}
            onChange={e => {
              const step = Number(e.target.value);
              if (Number.isFinite(step)) onChange(step * 4);
            }}
            className="h-7 rounded-md border border-zinc-300 bg-white px-1.5 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-sky-600"
          >
            {TW_GAP_STEPS.map(step => (
              <option key={step} value={String(step)}>
                {step} ({step * 4}px)
              </option>
            ))}
          </select>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <input
            id={inputId}
            type="number"
            min={0}
            max={64}
            step={1}
            disabled={disabled}
            value={value}
            onChange={e => {
              const n = Number(e.target.value);
              if (Number.isFinite(n)) onChange(Math.min(64, Math.max(0, Math.round(n))));
            }}
            className="h-7 w-24 rounded-md border border-zinc-300 bg-white px-2 font-mono text-xs text-zinc-900 tabular-nums focus-visible:outline-2 focus-visible:outline-sky-600"
          />
          <span className="text-[11px] text-zinc-500">{t('builder.studio_field_controls.free_px')}</span>
        </div>
      )}
    </div>
  );
}

export function SegmentedButtonsControl({
  label, readout, badge, value, options, disabled = false, responsiveInfo, onChange, onReset, resetLabel, testIdPrefix,
}: {
  label: string;
  readout?: string;
  badge?: string;
  value: string;
  options: Array<{ value: string; label: string; title?: string }>;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (val: string) => void;
  onReset?: () => void;
  resetLabel?: string;
  testIdPrefix?: string;
}) {
  const { t } = useI18n();
  const colsClass = options.length <= 2
    ? 'grid-cols-2'
    : options.length === 3
      ? 'grid-cols-3'
      : options.length === 4
        ? 'grid-cols-4'
        : options.length <= 6
          ? 'grid-cols-6'
          : 'grid-cols-4';
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="text-xs font-semibold text-zinc-800">
            {label}
            {badge && <span className="ml-1 font-normal text-zinc-500">{badge}</span>}
          </span>
          <ResponsiveDot info={responsiveInfo} />
        </div>
        <div className="flex items-center gap-1.5">
          {readout && <span className="font-mono text-[11px] text-zinc-500">{readout}</span>}
          {onReset && (
            <button type="button" disabled={disabled} className={RESET_BTN_CLASS} onClick={onReset}>
              {resetLabel ?? t('builder.studio_field_controls.default')}
            </button>
          )}
        </div>
      </div>
      <div className={`grid ${colsClass} gap-1 rounded-md border border-zinc-200 bg-zinc-100/80 p-0.5`} role="group" aria-label={label}>
        {options.map(opt => {
          const active = value === opt.value;
          return (
            <button
              key={opt.value}
              type="button"
              disabled={disabled}
              aria-pressed={active}
              title={opt.title ?? opt.label}
              data-testid={testIdPrefix ? `${testIdPrefix}-${opt.value}` : undefined}
              onClick={() => onChange(opt.value)}
              className={'truncate rounded px-1.5 py-1 text-[11px] font-medium transition-colors disabled:opacity-45 ' +
                (active
                  ? 'bg-sky-600 font-semibold text-white shadow-xs'
                  : 'text-zinc-600 hover:bg-white/80 hover:text-zinc-900')}
            >
              {opt.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

