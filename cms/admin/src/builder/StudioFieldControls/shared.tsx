import { useI18n } from '../../i18n/index.tsx';
import type { SpacingValue } from '../../fields/types.ts';
import type { BoxSpacing } from '../document.ts';

export const STUDIO_INPUT_CLASS =
  'h-8 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-sky-600 disabled:bg-zinc-100';

export const RESET_BTN_CLASS =
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
  box: SpacingValue | BoxSpacing | undefined,
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
  const { t } = useI18n();
  if (!info?.differs) return null;
  const actionHint = info.viewport === 'desktop'
    ? t('builder.studio_field_controls.sync')
    : t('builder.studio_field_controls.unset');
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

