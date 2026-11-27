import React, { useState } from 'react';
import type { ContentListColumn, ContentListColumnKind } from '../../api/content.ts';
import type { TaxonomyDefinition } from '../../api/taxonomy.ts';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { __, useI18n } from '../../i18n/index.tsx';

const MAX_COLUMNS = 8;
const SELECT_CLASS =
  'h-8 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const INPUT_CLASS =
  'h-8 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

interface ContentTypeColumnsEditorProps {
  columns: ContentListColumn[];
  availableTaxonomies: TaxonomyDefinition[];
  hierarchical: boolean;
  disabled: boolean;
  onChange: (next: ContentListColumn[]) => void;
}

export const ContentTypeColumnsEditor: React.FC<ContentTypeColumnsEditorProps> = ({
  columns,
  availableTaxonomies,
  hierarchical,
  disabled,
  onChange,
}) => {
  useI18n();
  const [liveStatus, setLiveStatus] = useState('');

  const addColumn = () => {
    if (columns.length >= MAX_COLUMNS) return;
    onChange([...columns, { kind: 'field', key: '', label: null }]);
    setLiveStatus(__("Added list column slot."));
  };

  const updateColumn = (index: number, patch: Partial<ContentListColumn>) => {
    onChange(columns.map((col, idx) => (idx === index ? { ...col, ...patch } : col)));
  };

  const removeColumn = (index: number) => {
    onChange(columns.filter((_, idx) => idx !== index));
    setLiveStatus(__("Removed column {{value1}}.", { value1: index + 1 }));
  };

  const moveColumn = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= columns.length) return;
    const next = [...columns];
    const [moved] = next.splice(index, 1);
    next.splice(target, 0, moved);
    onChange(next);
    setLiveStatus(__("Moved column to position {{value1}}.", { value1: target + 1 }));
  };

  return (
    <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <legend className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
            {__("Admin table columns (list_columns)")} ({columns.length}/{MAX_COLUMNS})
          </legend>
          <p className="mt-0.5 text-xs text-zinc-500">
            {__("Configure extra columns shown in the Entry List table (Private field columns are filtered by role).")}
          </p>
        </div>
        <AdminButton
          type="button"
          variant="secondary"
          size="sm"
          disabled={disabled || columns.length >= MAX_COLUMNS}
          onClick={addColumn}
        >
          {__("+ Add column")}
        </AdminButton>
      </div>

      <span className="sr-only" aria-live="polite">{liveStatus}</span>

      {columns.length === 0 ? (
        <p className="rounded border border-dashed border-zinc-200 bg-white px-3 py-2 text-xs text-zinc-500">
          {__("No extra list columns configured yet.")}
        </p>
      ) : (
        <ul className="space-y-2">
          {columns.map((col, idx) => (
            <li
              key={`${idx}-${col.kind}`}
              className="grid grid-cols-1 gap-2 rounded-md border border-zinc-200 bg-white p-2.5 sm:grid-cols-[140px_1fr_1fr_auto]"
            >
              <select
                aria-label={__("Column {{value1}} type", { value1: idx + 1 })}
                className={SELECT_CLASS}
                value={col.kind}
                disabled={disabled}
                onChange={e => {
                  const kind = e.target.value as ContentListColumnKind;
                  const defaultKey =
                    kind === 'taxonomy'
                      ? (availableTaxonomies[0]?.key ?? 'category')
                      : kind === 'menu_order'
                        ? 'menu_order'
                        : kind === 'parent'
                          ? 'parent_id'
                          : '';
                  updateColumn(idx, { kind, key: defaultKey });
                }}
              >
                <option value="field">{__("Custom field")}</option>
                <option value="taxonomy">{__("Taxonomy")}</option>
                <option value="menu_order">{__("Menu order")}</option>
                {hierarchical && <option value="parent">{__("Parent entry")}</option>}
              </select>

              {col.kind === 'taxonomy' ? (
                <select
                  aria-label={__("Column {{value1}} taxonomy key", { value1: idx + 1 })}
                  className={SELECT_CLASS}
                  value={col.key}
                  disabled={disabled}
                  onChange={e => updateColumn(idx, { key: e.target.value })}
                >
                  {availableTaxonomies.map(tax => (
                    <option key={tax.key} value={tax.key}>
                      {tax.label} ({tax.key})
                    </option>
                  ))}
                  {col.key && !availableTaxonomies.some(t => t.key === col.key) && (
                    <option value={col.key}>{col.key}</option>
                  )}
                </select>
              ) : col.kind === 'field' ? (
                <input
                  type="text"
                  aria-label={__("Column {{value1}} field key", { value1: idx + 1 })}
                  className={`${INPUT_CLASS} font-mono`}
                  placeholder={__("field_key")}
                  value={col.key}
                  disabled={disabled}
                  maxLength={64}
                  onChange={e => updateColumn(idx, { key: e.target.value })}
                />
              ) : (
                <input
                  type="text"
                  readOnly
                  aria-label={__("System column key")}
                  className={`${INPUT_CLASS} bg-zinc-100 font-mono text-zinc-500`}
                  value={col.kind === 'parent' ? 'parent_id' : 'menu_order'}
                />
              )}

              <input
                type="text"
                aria-label={__("Column {{value1}} display label", { value1: idx + 1 })}
                className={INPUT_CLASS}
                placeholder={__("Optional heading override")}
                value={col.label ?? ''}
                disabled={disabled}
                maxLength={100}
                onChange={e => updateColumn(idx, { label: e.target.value || null })}
              />

              <div className="flex items-center justify-end gap-1">
                <button
                  type="button"
                  disabled={disabled || idx === 0}
                  aria-label={__("Move column {{value1}} up", { value1: idx + 1 })}
                  onClick={() => moveColumn(idx, -1)}
                  className="inline-flex h-8 w-8 items-center justify-center rounded border border-zinc-300 bg-white text-xs text-zinc-700 hover:bg-zinc-50 disabled:opacity-40"
                >
                  ↑
                </button>
                <button
                  type="button"
                  disabled={disabled || idx === columns.length - 1}
                  aria-label={__("Move column {{value1}} down", { value1: idx + 1 })}
                  onClick={() => moveColumn(idx, 1)}
                  className="inline-flex h-8 w-8 items-center justify-center rounded border border-zinc-300 bg-white text-xs text-zinc-700 hover:bg-zinc-50 disabled:opacity-40"
                >
                  ↓
                </button>
                <button
                  type="button"
                  disabled={disabled}
                  aria-label={__("Remove column {{value1}}", { value1: idx + 1 })}
                  onClick={() => removeColumn(idx)}
                  className="inline-flex h-8 items-center rounded border border-rose-200 bg-white px-2 text-xs font-medium text-rose-600 hover:bg-rose-50 disabled:opacity-40"
                >
                  ×
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </fieldset>
  );
};
