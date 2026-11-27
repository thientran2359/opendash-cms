import { useI18n } from '../../i18n/index.tsx';
import { useEffect, useState } from 'react';
import type { SpacingValue } from '../../fields/types.ts';
import type { BoxSpacing } from '../document.ts';
import { TW_PADDING_STEPS, TW_MARGIN_STEPS, pxToTwStep, ResponsiveDot, type ResponsiveDotInfo } from './shared.tsx';

export function SpacingBoxControl({
  label, propPrefix = 'p', value, min = 0, max = 256, unit = 'px',
  allowTailwind = true, disabled = false, responsiveInfo, onChange, testIdPrefix,
}: {
  label: string;
  propPrefix?: 'p' | 'm';
  badge?: string;
  value: SpacingValue | BoxSpacing | undefined;
  fallbackValue?: SpacingValue | BoxSpacing;
  min?: number;
  max?: number;
  unit?: string;
  allowTailwind?: boolean;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: SpacingValue) => void;
  onReset?: () => void;
  resetLabel?: string;
  testIdPrefix?: string;
}) {
  const { __ } = useI18n();
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
              { id: 'equal', label: __("Equal") },
              { id: 'xy', label: 'X · Y' },
              { id: 'sides', label: __("4 sides") },
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
            {__("4 sides ({{unit}}):", { unit: unitMode === 'tw' && allowTw ? `${propPrefix}-*` : unit })}
          </span>
          <div className="min-w-0 flex-1">
            {renderTwOrPxField(
              __("{{label}} all four sides", { label }),
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
              {__("Horizontal ({{unit}})", { unit: unitMode === 'tw' && allowTw ? `${propPrefix}x-*` : unit })}
            </span>
            {renderTwOrPxField(
              __("{{label}} horizontal", { label }),
              curL,
              v => emitBox(curT, v, curB, v),
              testIdPrefix ? `${testIdPrefix}-x` : undefined,
            )}
          </label>
          <label className="space-y-0.5">
            <span className="block text-[10px] text-zinc-500">
              {__("Vertical ({{unit}})", { unit: unitMode === 'tw' && allowTw ? `${propPrefix}y-*` : unit })}
            </span>
            {renderTwOrPxField(
              __("{{label}} vertical", { label }),
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
            { key: 'top', label: __("Top"), val: curT, apply: (v: number) => emitBox(v, curR, curB, curL) },
            { key: 'right', label: __("Right"), val: curR, apply: (v: number) => emitBox(curT, v, curB, curL) },
            { key: 'bottom', label: __("Bottom"), val: curB, apply: (v: number) => emitBox(curT, curR, v, curL) },
            { key: 'left', label: __("Left"), val: curL, apply: (v: number) => emitBox(curT, curR, curB, v) },
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

