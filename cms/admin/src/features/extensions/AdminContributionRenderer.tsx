import { __, _x, useI18n } from '../../i18n/index.tsx';
import type { EntryTableRow } from '../content/types.ts';
import type { FieldSpec } from '../../fields/types.ts';
import { UnifiedFieldForm } from '../../fields/UnifiedFieldForm.tsx';

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

export interface EffectiveTermPanel {
  key: string;
  label: string;
  order: number;
  fields: FieldSpec[];
  read_only_keys?: string[];
}

export type TermExtensionValues = Record<string, Record<string, unknown>>;

export interface EffectiveTermConfig {
  schema_version: 1;
  schema_token: string;
  panels: EffectiveTermPanel[];
  extensions: TermExtensionValues;
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



export function AdminTermPanels({
  config, values, disabled, onChange, csrfToken = null, locale = null, siteTimezone = 'UTC', onApiError,
}: {
  config: EffectiveTermConfig;
  values: TermExtensionValues;
  disabled: boolean;
  onChange: (panelKey: string, key: string, value: unknown) => void;
  csrfToken?: string | null;
  locale?: string | null;
  siteTimezone?: string;
  onApiError?: (err: unknown, fallback: string) => void;
}) {
  useI18n();
  if (!config.panels.length) return null;
  return <div className="space-y-4" aria-label={__("Provider extension fields")} data-testid="term-extension-panels">
    {config.panels.map(panel => <fieldset key={panel.key} className="space-y-3 rounded-xl border border-slate-200 p-4"
      data-provider-panel={panel.key}>
      <legend className="px-1 text-sm font-semibold text-slate-900">{panel.label}</legend>
      <UnifiedFieldForm
        context="term"
        fields={panel.fields}
        values={values[panel.key] ?? {}}
        onChange={next => {
          for (const field of panel.fields) {
            if (field.key in next && next[field.key] !== values[panel.key]?.[field.key]) {
              onChange(panel.key, field.key, next[field.key]);
            }
          }
        }}
        csrfToken={csrfToken}
        locale={locale}
        siteTimezone={siteTimezone}
        disabled={disabled}
        readOnlyKeys={new Set(panel.read_only_keys ?? [])}
        onApiError={onApiError ?? (() => {})}
      />
    </fieldset>)}
  </div>;
}
