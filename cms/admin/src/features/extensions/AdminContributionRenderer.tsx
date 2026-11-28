import { __, _x, useI18n } from '../../i18n/index.tsx';
import type { EntryTableRow } from '../content/types.ts';

/** Declarative server-projected configuration only: no HTML/JS/CSS injection. */
export interface ProviderColumn {
  key: string;
  owner_kind: 'theme' | 'plugin';
  owner_id: string;
  label: string;
  field_key: string;
  format: 'text' | 'badge' | 'link';
  order: number;
  action_id?: string;
}

export interface EffectiveListConfig {
  schema_version: 1;
  columns: ProviderColumn[];
}

/** A path is safe only if it cannot resolve outside this origin. */
export function safeAdminProviderLink(value: unknown): string | null {
  if (typeof value !== 'string' || value.length > 4096 ||
    !value.startsWith('/') || value.startsWith('//') ||
    /[\\\u0000-\u001f\u007f]/.test(value)) return null;
  try {
    const url = new URL(value, window.location.origin);
    if (url.origin !== window.location.origin || !['http:', 'https:'].includes(url.protocol)) return null;
    return url.pathname + url.search + url.hash;
  } catch {
    return null;
  }
}

function cellLabel(value: unknown): string {
  if (typeof value === 'string') return value.slice(0, 4096);
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  if (typeof value === 'boolean') return value ? __("Yes") : __("No");
  return '—';
}

export function AdminColumnCell({
  column, row, busy, onRun,
}: {
  column: ProviderColumn;
  row: EntryTableRow;
  busy: boolean;
  onRun: (column: ProviderColumn, row: EntryTableRow) => void;
}) {
  useI18n();
  const raw = row.fields[column.field_key];
  const label = cellLabel(raw);
  const safeLink = column.format === 'link' ? safeAdminProviderLink(raw) : null;
  return (
    <span className="inline-flex max-w-xs flex-wrap items-center gap-2" data-provider-column={column.key}>
      {column.format === 'badge' ?
        <span className="rounded-full border border-slate-300 px-2 py-0.5 text-xs text-slate-700">{label}</span>
        : safeLink ?
          <a className="break-all text-sky-700 underline" href={safeLink}
            target="_blank" rel="noopener noreferrer">{label}</a>
          : <span className="break-words text-slate-700">{column.format === 'link' && raw ? __("Invalid path") : label}</span>}
      {column.owner_kind === 'plugin' && column.action_id && <button type="button"
        className="rounded border border-slate-300 px-2 py-1 text-xs text-slate-700 hover:bg-slate-100"
        disabled={busy} onClick={() => onRun(column, row)}
        aria-label={__("Run {{value1}} for item #{{value2}}", { value1: column.label, value2: row.id })}>{__("Run")}</button>}
    </span>
  );
}
