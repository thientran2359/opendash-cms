import { getEffectiveBcp47 } from './index.tsx';

export function formatDateTime(
  value: Date | string | number,
  options?: Intl.DateTimeFormatOptions,
): string {
  return new Intl.DateTimeFormat(getEffectiveBcp47(), options).format(
    value instanceof Date ? value : new Date(value),
  );
}

export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return new Intl.NumberFormat(getEffectiveBcp47(), options).format(value);
}
