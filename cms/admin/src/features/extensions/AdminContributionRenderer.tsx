import { tStatic, useI18n } from '../../i18n/index.tsx';
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

export interface TermPanelField {
  key: string;
  label: string;
  type: 'text' | 'textarea' | 'boolean';
  required: boolean;
  min_role: 'admin' | 'editor';
}

export interface EffectiveTermPanel {
  key: string;
  label: string;
  order: number;
  fields: TermPanelField[];
}

export type TermExtensionValues = Record<string, Record<string, string | boolean | null>>;

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
  if (typeof value === 'boolean') return value ? tStatic("feature.extensions.admincontributionrenderer.yes") : tStatic("feature.extensions.admincontributionrenderer.no");
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
          : <span className="break-words text-slate-700">{column.format === 'link' && raw ? tStatic("feature.extensions.admincontributionrenderer.invalid_path") : label}</span>}
      {column.owner_kind === 'plugin' && column.action_id && <button type="button"
        className="rounded border border-slate-300 px-2 py-1 text-xs text-slate-700 hover:bg-slate-100"
        disabled={busy} onClick={() => onRun(column, row)}
        aria-label={tStatic("feature.extensions.admincontributionrenderer.run_value_for_item_value", { value1: column.label, value2: row.id })}>{tStatic("feature.extensions.admincontributionrenderer.run")}</button>}
    </span>
  );
}

const fieldClass = 'mt-1 block w-full min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm disabled:opacity-50';

export function AdminTermPanels({
  config, values, disabled, onChange,
}: {
  config: EffectiveTermConfig;
  values: TermExtensionValues;
  disabled: boolean;
  onChange: (panelKey: string, key: string, value: string | boolean | null) => void;
}) {
  useI18n();
  if (!config.panels.length) return null;
  return <div className="space-y-4" aria-label={tStatic("feature.extensions.admincontributionrenderer.provider_extension_fields")} data-testid="term-extension-panels">
    {config.panels.map(panel => <fieldset key={panel.key} className="space-y-3 rounded-xl border border-slate-200 p-4"
      data-provider-panel={panel.key}>
      <legend className="px-1 text-sm font-semibold text-slate-900">{panel.label}</legend>
      {panel.fields.map((field, index) => {
        const id = `term-panel-${panel.key.replace(/[^a-zA-Z0-9]/g, '-')}-${index}`;
        const stored = values[panel.key]?.[field.key];
        return <label key={field.key} htmlFor={id} className="block text-sm font-medium text-slate-800">
          {field.label}{field.required ? ' *' : ''}
          {field.type === 'boolean' ? <span className="mt-1 flex gap-2">
            <input id={id} type="checkbox" checked={stored === true} disabled={disabled}
              onChange={event => onChange(panel.key, field.key, event.target.checked)} />
            <span>{stored === true ? tStatic("feature.content.fieldvalueinput.on") : tStatic("shell.locale_disabled")}</span>
          </span> : field.type === 'textarea' ?
            <textarea id={id} className={fieldClass} rows={3} maxLength={4096} disabled={disabled}
              required={field.required} value={typeof stored === 'string' ? stored : ''}
              onChange={event => onChange(panel.key, field.key, event.target.value)} /> :
            <input id={id} className={fieldClass} maxLength={4096} disabled={disabled}
              required={field.required} value={typeof stored === 'string' ? stored : ''}
              onChange={event => onChange(panel.key, field.key, event.target.value)} />}
        </label>;
      })}
    </fieldset>)}
  </div>;
}
