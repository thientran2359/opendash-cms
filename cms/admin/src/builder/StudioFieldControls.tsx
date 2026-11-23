import { useEffect, useId, useState } from 'react';
import { queryContentEntryTable } from '../api/content.ts';
import { mediaThumbnailUrl } from '../api/media.ts';
import type { EntryTableRow } from '../features/content/types.ts';
import type { BlockLinkValue, BlockMediaKind, BlockSpacingValue } from '../types.ts';
import type { BoxSpacing } from './document.ts';
import { StudioRichTextLightbox, stripRichHtmlToPreview } from './StudioRichTextLightbox.tsx';

export const STUDIO_INPUT_CLASS =
  'h-8 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-sky-600 disabled:bg-zinc-100';

const RESET_BTN_CLASS =
  'rounded px-1.5 py-0.5 text-[11px] font-medium text-zinc-500 hover:bg-zinc-100 hover:text-zinc-800 disabled:opacity-40';

export const TW_PADDING_STEPS = [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64] as const;
export const TW_MARGIN_STEPS = [
  -32, -24, -16, -12, -8, -6, -4, -3, -2, -1,
  0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32,
] as const;
export const TW_GAP_STEPS = [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16] as const;

export function pxToTwStep(px: number | null | undefined, steps: readonly number[]): number | null {
  if (px === null || px === undefined || !Number.isInteger(px) || px % 4 !== 0) return null;
  const step = px / 4;
  return steps.includes(step) ? step : null;
}

export function formatGapSummary(gapPx: number | undefined): string {
  const px = gapPx ?? 0;
  const step = pxToTwStep(px, TW_GAP_STEPS);
  return step !== null ? `gap-${step} (${px}px)` : `${px}px`;
}

export function formatSpacingSummary(
  box: BlockSpacingValue | BoxSpacing | undefined,
  prop: 'p' | 'm',
  allowNegative = false,
): string {
  const t = box?.top ?? 0;
  const r = box?.right ?? 0;
  const b = box?.bottom ?? 0;
  const l = box?.left ?? 0;
  const steps = allowNegative ? TW_MARGIN_STEPS : TW_PADDING_STEPS;
  const st = pxToTwStep(t, steps);
  const sr = pxToTwStep(r, steps);
  const sb = pxToTwStep(b, steps);
  const sl = pxToTwStep(l, steps);
  const fmt = (axis: string, step: number) =>
    step < 0 ? `-${prop}${axis}-${Math.abs(step)}` : `${prop}${axis}-${step}`;
  if (st !== null && sr !== null && sb !== null && sl !== null) {
    if (st === sr && sr === sb && sb === sl) return fmt('', st);
    const parts: string[] = [];
    if (sr === sl) parts.push(fmt('x', sr));
    else parts.push(fmt('r', sr), fmt('l', sl));
    if (st === sb) parts.push(fmt('y', st));
    else parts.push(fmt('t', st), fmt('b', sb));
    return parts.join(' ');
  }
  if (t === r && r === b && b === l) return `${t}px`;
  if (t === b && l === r) return `${t}px ${r}px`;
  return `${t}px ${r}px ${b}px ${l}px`;
}

export interface ResponsiveDotInfo {
  differs: boolean;
  summary: string;
  viewport: 'desktop' | 'tablet' | 'mobile';
  onReset: () => void;
  testId?: string;
}

export function ResponsiveDot({ info }: { info?: ResponsiveDotInfo }) {
  if (!info?.differs) return null;
  const actionHint = info.viewport === 'desktop'
    ? 'Click để đồng bộ 3 màn hình'
    : 'Click để bỏ override màn này';
  const tipText = `${info.summary} — ${actionHint}`;
  return (
    <span className="group relative inline-flex items-center">
      <button
        type="button"
        data-testid={info.testId}
        aria-label={tipText}
        onClick={info.onReset}
        className="h-2 w-2 shrink-0 rounded-full bg-amber-400 ring-2 ring-amber-400/30 transition-transform hover:scale-125 focus-visible:outline-2 focus-visible:outline-amber-500"
      />
      <span
        role="tooltip"
        className="pointer-events-none invisible absolute left-0 top-full z-30 mt-1.5 w-max max-w-[230px] rounded-md border border-zinc-200 bg-zinc-900 px-2 py-1 text-[11px] font-normal leading-snug text-white opacity-0 shadow-lg transition-opacity group-focus-within:visible group-focus-within:opacity-100 group-hover:visible group-hover:opacity-100"
      >
        {tipText}
      </span>
    </span>
  );
}

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
  disabled = false, responsiveInfo, onChange, onReset, resetLabel = 'Mặc định',
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
              {resetLabel}
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
  id, label = 'Gap', value, disabled = false, responsiveInfo, onChange, testIdPrefix,
}: {
  id?: string;
  label?: string;
  value: number;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (px: number) => void;
  testIdPrefix?: string;
}) {
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
          <label htmlFor={inputId} className="text-xs font-semibold text-zinc-800">{label}</label>
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
          <span className="text-[11px] text-zinc-500">px (tự do ngoài thang Tailwind)</span>
        </div>
      )}
    </div>
  );
}

export function SegmentedButtonsControl({
  label, readout, badge, value, options, disabled = false, responsiveInfo, onChange, onReset, resetLabel = 'Mặc định', testIdPrefix,
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
              {resetLabel}
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

const HEX_SWATCHES = [
  { value: 'transparent', title: 'Trong suốt (transparent)' },
  { value: '#0f172a', title: 'Đen xám (#0f172a)' },
  { value: '#ffffff', title: 'Trắng (#ffffff)' },
  { value: '#f8fafc', title: 'Xám sáng (#f8fafc)' },
  { value: '#0284c7', title: 'Xanh chủ đạo (#0284c7)' },
  { value: '#e11d48', title: 'Đỏ nhấn (#e11d48)' },
];

const TOKEN_SWATCHES = [
  { value: 'var(--od-token-primary)', label: 'Primary' },
  { value: 'var(--od-token-accent)', label: 'Accent' },
  { value: 'var(--od-token-surface)', label: 'Surface' },
  { value: 'var(--od-token-text)', label: 'Text' },
];

export function ColorSwatchControl({
  id, label, value, disabled = false, onChange,
}: {
  id?: string;
  label: string;
  value: string;
  disabled?: boolean;
  onChange: (next: string) => void;
}) {
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
            {showTokens ? 'Ẩn token' : 'Theme tokens'}
          </button>
          {value && (
            <button type="button" disabled={disabled} className={RESET_BTN_CLASS} onClick={() => onChange('')}>
              Xóa
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

        <label className="relative flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center overflow-hidden rounded-full border border-zinc-300 bg-white" title="Chọn màu bất kỳ">
          <input
            type="color"
            value={hexValue}
            disabled={disabled}
            aria-label={`Chọn màu cho ${label}`}
            onChange={e => onChange(e.target.value)}
            className="h-8 w-8 cursor-pointer border-0 bg-transparent p-0"
          />
        </label>

        <input
          id={inputId}
          type="text"
          value={value}
          disabled={disabled}
          placeholder="#hex hoặc var(--od-token-...)"
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
  acceptKind: BlockMediaKind;
  disabled?: boolean;
  onPick: () => void;
  onClear: () => void;
  testId?: string;
}) {
  const [imgBroken, setImgBroken] = useState(false);
  useEffect(() => setImgBroken(false), [value]);

  const showImageThumb = Boolean(value) && acceptKind !== 'video' && !imgBroken;
  const kindText = acceptKind === 'video' ? 'Video MP4' : 'Hình ảnh';

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
              {acceptKind === 'video' ? 'MP4' : 'Media'}
            </div>
          )
        ) : (
          <div className="flex h-11 w-14 shrink-0 items-center justify-center rounded border border-dashed border-zinc-300 bg-white text-[10px] text-zinc-400">
            Trống
          </div>
        )}
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-zinc-800">
            {value ? `${kindText} đã chọn` : `Chưa chọn ${kindText.toLowerCase()}`}
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
              {value ? 'Thay đổi' : `Chọn ${kindText.toLowerCase()}`}
            </button>
            {value && (
              <button
                type="button"
                disabled={disabled}
                onClick={onClear}
                className="rounded border border-zinc-200 bg-white px-2 py-0.5 text-[11px] text-rose-700 hover:bg-rose-50 disabled:opacity-45"
              >
                Xóa
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export function SpacingBoxControl({
  label, propPrefix = 'p', value, min = 0, max = 256, unit = 'px',
  allowTailwind = true, disabled = false, responsiveInfo, onChange, testIdPrefix,
}: {
  label: string;
  propPrefix?: 'p' | 'm';
  badge?: string;
  value: BlockSpacingValue | BoxSpacing | undefined;
  fallbackValue?: BlockSpacingValue | BoxSpacing;
  min?: number;
  max?: number;
  unit?: string;
  allowTailwind?: boolean;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: BlockSpacingValue) => void;
  onReset?: () => void;
  resetLabel?: string;
  testIdPrefix?: string;
}) {
  const baseSteps = min < 0 ? TW_MARGIN_STEPS : TW_PADDING_STEPS;
  const twSteps = baseSteps.filter(step => step * 4 >= min && step * 4 <= max);
  const allowTw = allowTailwind && unit === 'px' && twSteps.length > 0;

  const curT = value?.top ?? 0;
  const curR = value?.right ?? 0;
  const curB = value?.bottom ?? 0;
  const curL = value?.left ?? 0;

  const definedSides = [value?.top, value?.right, value?.bottom, value?.left].filter(
    (n): n is number => typeof n === 'number',
  );
  const allAligned = definedSides.every(n => pxToTwStep(n, twSteps) !== null);

  type SpacingMode = 'equal' | 'xy' | 'sides';
  const inferInitialMode = (): SpacingMode => {
    if (definedSides.length === 0) return 'xy';
    if (curT === curR && curR === curB && curB === curL) return 'equal';
    if (curT === curB && curL === curR) return 'xy';
    return 'sides';
  };

  const [mode, setMode] = useState<SpacingMode>(inferInitialMode);
  const [unitMode, setUnitMode] = useState<'tw' | 'px'>(() => (allowTw && allAligned ? 'tw' : 'px'));

  useEffect(() => {
    if (!allowTw || !allAligned) setUnitMode('px');
  }, [allowTw, allAligned]);

  useEffect(() => {
    if (mode === 'equal' && !(curT === curR && curR === curB && curB === curL)) {
      setMode(curT === curB && curL === curR ? 'xy' : 'sides');
    } else if (mode === 'xy' && !(curT === curB && curL === curR)) {
      setMode('sides');
    }
  }, [mode, curT, curR, curB, curL]);

  const snapPx = (px: number): number => {
    const clamped = Math.min(max, Math.max(min, Math.round(px)));
    if (!twSteps.length) return clamped;
    const nearest = twSteps.reduce((best, step) =>
      Math.abs(step * 4 - clamped) < Math.abs(best * 4 - clamped) ? step : best,
    );
    return nearest * 4;
  };

  const emitBox = (nextT: number, nextR: number, nextB: number, nextL: number) => {
    onChange({
      top: Math.min(max, Math.max(min, Math.round(nextT))),
      right: Math.min(max, Math.max(min, Math.round(nextR))),
      bottom: Math.min(max, Math.max(min, Math.round(nextB))),
      left: Math.min(max, Math.max(min, Math.round(nextL))),
    });
  };

  const renderTwOrPxField = (
    ariaLabel: string,
    currentPx: number,
    onUpdatePx: (nextPx: number) => void,
    sideTestId?: string,
  ) => {
    if (unitMode === 'tw' && allowTw) {
      const matched = pxToTwStep(currentPx, twSteps);
      return (
        <select
          aria-label={ariaLabel}
          disabled={disabled}
          data-testid={sideTestId}
          value={matched !== null ? String(matched) : ''}
          onChange={e => {
            const step = Number(e.target.value);
            if (Number.isFinite(step)) onUpdatePx(step * 4);
          }}
          className="h-7 w-full rounded border border-zinc-300 bg-white px-1 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-sky-600"
        >
          {matched === null && <option value="">{currentPx}px</option>}
          {twSteps.map(step => (
            <option key={step} value={String(step)}>
              {step} ({step * 4}px)
            </option>
          ))}
        </select>
      );
    }
    return (
      <input
        type="number"
        min={min}
        max={max}
        step={unit === 'px' ? 1 : 'any'}
        disabled={disabled}
        aria-label={ariaLabel}
        data-testid={sideTestId}
        value={currentPx}
        onChange={e => {
          const parsed = Number(e.target.value);
          if (Number.isFinite(parsed)) {
            onUpdatePx(Math.min(max, Math.max(min, unit === 'px' ? Math.round(parsed) : parsed)));
          }
        }}
        className="h-7 w-full rounded border border-zinc-300 bg-white px-1 text-center font-mono text-xs text-zinc-900 tabular-nums focus-visible:outline-2 focus-visible:outline-sky-600"
      />
    );
  };

  return (
    <div className="space-y-1.5" data-testid={testIdPrefix}>
      <div className="flex flex-wrap items-center justify-between gap-1.5">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="text-xs font-semibold text-zinc-800">{label}</span>
          <ResponsiveDot info={responsiveInfo} />
        </div>
        <div className="flex flex-wrap items-center gap-1">
          <div className="inline-flex rounded border border-zinc-200 bg-zinc-100/80 p-0.5">
            {([
              { id: 'equal', label: 'Đều' },
              { id: 'xy', label: 'X · Y' },
              { id: 'sides', label: '4 cạnh' },
            ] as const).map(opt => (
              <button
                key={opt.id}
                type="button"
                disabled={disabled}
                aria-pressed={mode === opt.id}
                data-testid={testIdPrefix ? `${testIdPrefix}-mode-${opt.id}` : undefined}
                onClick={() => setMode(opt.id)}
                className={'rounded px-1.5 py-0.5 text-[10px] font-medium transition-colors ' +
                  (mode === opt.id ? 'bg-sky-600 font-semibold text-white' : 'text-zinc-600 hover:text-zinc-900')}
              >
                {opt.label}
              </button>
            ))}
          </div>
          {allowTw && (
            <div className="inline-flex rounded border border-zinc-200 bg-zinc-100/80 p-0.5">
              {(['tw', 'px'] as const).map(u => (
                <button
                  key={u}
                  type="button"
                  disabled={disabled}
                  aria-pressed={unitMode === u}
                  data-testid={testIdPrefix ? `${testIdPrefix}-unit-${u}` : undefined}
                  onClick={() => {
                    if (u === 'tw' && !allAligned && definedSides.length > 0) {
                      emitBox(snapPx(curT), snapPx(curR), snapPx(curB), snapPx(curL));
                    }
                    setUnitMode(u);
                  }}
                  className={'rounded px-1.5 py-0.5 text-[10px] font-semibold transition-colors ' +
                    (unitMode === u ? 'bg-sky-600 text-white' : 'text-zinc-600 hover:text-zinc-900')}
                >
                  {u === 'tw' ? 'TW' : 'px'}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      {mode === 'equal' && (
        <label className="flex items-center gap-2">
          <span className="shrink-0 text-[11px] text-zinc-500">
            4 cạnh ({unitMode === 'tw' && allowTw ? `${propPrefix}-*` : unit}):
          </span>
          <div className="min-w-0 flex-1">
            {renderTwOrPxField(
              `${label} đều 4 cạnh`,
              curT,
              v => emitBox(v, v, v, v),
              testIdPrefix ? `${testIdPrefix}-all` : undefined,
            )}
          </div>
        </label>
      )}

      {mode === 'xy' && (
        <div className="grid grid-cols-2 gap-2">
          <label className="space-y-0.5">
            <span className="block text-[10px] text-zinc-500">
              Ngang ({unitMode === 'tw' && allowTw ? `${propPrefix}x-*` : unit})
            </span>
            {renderTwOrPxField(
              `${label} ngang`,
              curL,
              v => emitBox(curT, v, curB, v),
              testIdPrefix ? `${testIdPrefix}-x` : undefined,
            )}
          </label>
          <label className="space-y-0.5">
            <span className="block text-[10px] text-zinc-500">
              Dọc ({unitMode === 'tw' && allowTw ? `${propPrefix}y-*` : unit})
            </span>
            {renderTwOrPxField(
              `${label} dọc`,
              curT,
              v => emitBox(v, curR, v, curL),
              testIdPrefix ? `${testIdPrefix}-y` : undefined,
            )}
          </label>
        </div>
      )}

      {mode === 'sides' && (
        <div className="grid grid-cols-4 gap-1.5">
          {([
            { key: 'top', label: 'Trên', val: curT, apply: (v: number) => emitBox(v, curR, curB, curL) },
            { key: 'right', label: 'Phải', val: curR, apply: (v: number) => emitBox(curT, v, curB, curL) },
            { key: 'bottom', label: 'Dưới', val: curB, apply: (v: number) => emitBox(curT, curR, v, curL) },
            { key: 'left', label: 'Trái', val: curL, apply: (v: number) => emitBox(curT, curR, curB, v) },
          ] as const).map(side => (
            <label key={side.key} className="space-y-0.5 text-center">
              <span className="block text-[10px] text-zinc-500">{side.label}</span>
              {renderTwOrPxField(
                `${label} ${side.label}`,
                side.val,
                side.apply,
                testIdPrefix ? `${testIdPrefix}-${side.key}` : undefined,
              )}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}

export function LinkControl({
  id, label, value, placeholder = 'https://... hoặc /duong-dan', showFlags = true,
  disabled = false, onChange,
}: {
  id?: string;
  label?: string;
  value: BlockLinkValue;
  placeholder?: string;
  showFlags?: boolean;
  disabled?: boolean;
  onChange: (next: BlockLinkValue) => void;
}) {
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
          {label ?? 'Đường dẫn'}
        </label>
        <button
          type="button"
          disabled={disabled}
          onClick={toggleInternal}
          className={RESET_BTN_CLASS}
        >
          {showInternal ? 'Đóng danh sách' : 'Chọn trang/bài…'}
        </button>
      </div>

      <input
        id={inputId}
        type="text"
        maxLength={2048}
        value={value.url}
        placeholder={placeholder}
        disabled={disabled}
        onChange={e => onChange({ ...value, url: e.target.value })}
        className={STUDIO_INPUT_CLASS}
      />

      {showInternal && (
        <select
          aria-label="Chọn trang hoặc bài viết đã xuất bản"
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
            {loadingEntries ? 'Đang tải danh sách…' : entries.length ? 'Chọn đường dẫn nội bộ…' : 'Chưa có nội dung xuất bản'}
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
            label="Mở trong tab mới"
            checked={Boolean(value.new_tab)}
            disabled={disabled}
            onChange={new_tab => onChange({ ...value, new_tab })}
          />
          <ToggleSwitchControl
            label="Thuộc tính rel=&quot;nofollow&quot;"
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
  id, label, value, placeholder = 'Chưa có nội dung văn bản…', maxBytes,
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
  const [open, setOpen] = useState(false);
  const preview = stripRichHtmlToPreview(value);

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-zinc-800">{label}</span>
      </div>
      <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/70 p-2.5">
        <p className={'!m-0 line-clamp-2 text-xs ' + (preview ? 'text-zinc-700' : 'italic text-zinc-400')}>
          {preview || placeholder}
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
          <span>Mở Text Editor</span>
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
