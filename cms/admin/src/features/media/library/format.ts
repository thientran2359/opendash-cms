import { formatDateTime, formatNumber } from '../../../i18n/format.ts';

export function formatMediaBytes(value: number): string {
  if (value < 1024) return `${formatNumber(value)} B`;
  if (value < 1024 * 1024) return `${formatNumber(value / 1024, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} KiB`;
  return `${formatNumber(value / (1024 * 1024), { minimumFractionDigits: 2, maximumFractionDigits: 2 })} MiB`;
}

export function formatMediaDate(value: string): string {
  // SQLite CURRENT_TIMESTAMP uses UTC with a space rather than an ISO T/Z.
  const iso = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(value)
    ? value.replace(' ', 'T') + 'Z'
    : value;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return value;
  return formatDateTime(date, {
    year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
  });
}
