import React, { useState } from 'react';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { useI18n } from '../../i18n/index.tsx';
import { initialValuesFromFields } from '../helpers.ts';
import type { FieldSpec } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

export type NestedFieldsRenderer = (props: {
  fields: readonly FieldSpec[];
  scopeValues: Record<string, unknown>;
  onScopeChange: (next: Record<string, unknown>) => void;
  disabled?: boolean;
  fieldPathPrefix: string;
}) => React.ReactNode;

export const GroupControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  fieldPathPrefix: string;
  renderNestedFields: NestedFieldsRenderer;
  onChange: (next: Record<string, unknown>) => void;
}> = ({
  field,
  value,
  disabled = false,
  fieldPathPrefix,
  renderNestedFields,
  onChange,
}) => {
  const obj =
    value && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};

  return (
    <fieldset
      className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3"
      data-testid={`field-group-${field.key}`}
    >
      <legend className="px-1 text-xs font-semibold text-zinc-900">
        {field.label}
        {field.required && <span className="ml-0.5 text-rose-600">*</span>}
      </legend>
      {field.instructions && (
        <p className="text-[11px] text-zinc-500">{field.instructions}</p>
      )}
      {renderNestedFields({
        fields: field.fields ?? [],
        scopeValues: obj,
        onScopeChange: onChange,
        disabled,
        fieldPathPrefix: `${fieldPathPrefix}.${field.key}`,
      })}
    </fieldset>
  );
};

function resolveRowSummary(
  row: Record<string, unknown>,
  field: FieldSpec,
  index: number,
): string {
  const key = field.item_label_field;
  if (key) {
    const raw = row[key];
    if (typeof raw === 'string' && raw.trim()) return raw.trim();
    if (typeof raw === 'number' && Number.isFinite(raw)) return String(raw);
  }
  return `#${index + 1}`;
}

export const RepeaterControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  contextMaxItems: number;
  disabled?: boolean;
  fieldPathPrefix: string;
  renderNestedFields: NestedFieldsRenderer;
  onChange: (next: Record<string, unknown>[]) => void;
}> = ({
  field,
  value,
  contextMaxItems,
  disabled = false,
  fieldPathPrefix,
  renderNestedFields,
  onChange,
}) => {
  const { __, _x, _n } = useI18n();
  const rows: Record<string, unknown>[] = Array.isArray(value)
    ? value.map(item =>
        item && typeof item === 'object' && !Array.isArray(item)
          ? (item as Record<string, unknown>)
          : {},
      )
    : [];
  const subFields = field.fields ?? [];
  const maxItems = Math.max(1, Math.min(contextMaxItems, field.max_items ?? contextMaxItems));
  const minItems = Math.max(0, field.min_items ?? 0);
  const layout = field.layout ?? 'accordion';
  const [collapsed, setCollapsed] = useState<Record<number, boolean>>({});

  const addRow = () => {
    if (rows.length >= maxItems) return;
    const fresh = initialValuesFromFields(subFields, undefined);
    onChange([...rows, fresh]);
  };

  const updateRow = (idx: number, nextRow: Record<string, unknown>) => {
    onChange(rows.map((r, i) => (i === idx ? nextRow : r)));
  };

  const removeRow = (idx: number) => {
    if (rows.length <= minItems) return;
    onChange(rows.filter((_, i) => i !== idx));
  };

  const duplicateRow = (idx: number) => {
    if (rows.length >= maxItems) return;
    const copy = structuredClone(rows[idx]);
    onChange([...rows.slice(0, idx + 1), copy, ...rows.slice(idx + 1)]);
  };

  const moveRow = (idx: number, delta: -1 | 1) => {
    const nextIdx = idx + delta;
    if (nextIdx < 0 || nextIdx >= rows.length) return;
    const next = [...rows];
    [next[idx], next[nextIdx]] = [next[nextIdx], next[idx]];
    onChange(next);
  };

  return (
    <div className="space-y-2" data-testid={`field-repeater-${field.key}`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FieldLabelHeader
          label={field.label}
          required={field.required}
          badge={_n("{{count}} / {{max}} row", "{{count}} / {{max}} rows", rows.length, { max: maxItems })}
        />
        <AdminButton
          size="sm"
          variant="secondary"
          disabled={disabled || rows.length >= maxItems}
          onClick={addRow}
        >
          + {field.button_label || __("Add item")}
        </AdminButton>
      </div>

      {rows.length === 0 ? (
        <div className="rounded-lg border border-dashed border-zinc-300 bg-zinc-50/70 px-3 py-3 text-center text-xs text-zinc-500">
          {__("No items added yet.")}
        </div>
      ) : layout === 'table' ? (
        <div className="overflow-x-auto rounded-lg border border-zinc-200 bg-white">
          <div className="divide-y divide-zinc-200">
            {rows.map((row, idx) => (
              <div key={idx} className="flex flex-col gap-2 p-2.5 md:flex-row md:items-start">
                <div className="flex shrink-0 items-center gap-1 pt-1 text-[11px] font-semibold text-zinc-500 md:w-10">
                  #{idx + 1}
                </div>
                <div className="min-w-0 flex-1">
                  {renderNestedFields({
                    fields: subFields,
                    scopeValues: row,
                    onScopeChange: nextRow => updateRow(idx, nextRow),
                    disabled,
                    fieldPathPrefix: `${fieldPathPrefix}.${field.key}[${idx}]`,
                  })}
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <button
                    type="button"
                    disabled={disabled || idx === 0}
                    aria-label={_x("Move up", "fields.common")}
                    onClick={() => moveRow(idx, -1)}
                    className="rounded px-1.5 py-0.5 text-xs text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    disabled={disabled || idx === rows.length - 1}
                    aria-label={_x("Move down", "fields.common")}
                    onClick={() => moveRow(idx, 1)}
                    className="rounded px-1.5 py-0.5 text-xs text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                  >
                    ↓
                  </button>
                  <button
                    type="button"
                    disabled={disabled || rows.length <= minItems}
                    aria-label={__("Remove")}
                    onClick={() => removeRow(idx)}
                    className="rounded px-1.5 py-0.5 text-xs text-rose-600 hover:bg-rose-50 disabled:opacity-35"
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="space-y-2">
          {rows.map((row, idx) => {
            const isCollapsed = layout === 'accordion' && Boolean(collapsed[idx]);
            const summary = resolveRowSummary(row, field, idx);
            return (
              <div
                key={idx}
                className="rounded-lg border border-zinc-200 bg-white"
                data-testid={`field-repeater-row-${field.key}-${idx}`}
              >
                <div className="flex items-center justify-between gap-2 border-b border-zinc-100 bg-zinc-50/70 px-2.5 py-1.5">
                  {layout === 'accordion' ? (
                    <button
                      type="button"
                      aria-expanded={!isCollapsed}
                      onClick={() => setCollapsed(prev => ({ ...prev, [idx]: !isCollapsed }))}
                      className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-xs font-semibold text-zinc-800"
                    >
                      <span className="font-mono text-zinc-500">{isCollapsed ? '▸' : '▾'}</span>
                      <span className="truncate">{summary}</span>
                    </button>
                  ) : (
                    <span className="min-w-0 flex-1 truncate text-xs font-semibold text-zinc-800">
                      {summary}
                    </span>
                  )}
                  <div className="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      disabled={disabled || idx === 0}
                      aria-label={_x("Move up", "fields.common")}
                      onClick={() => moveRow(idx, -1)}
                      className="rounded px-1 py-0.5 text-[11px] text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      disabled={disabled || idx === rows.length - 1}
                      aria-label={_x("Move down", "fields.common")}
                      onClick={() => moveRow(idx, 1)}
                      className="rounded px-1 py-0.5 text-[11px] text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                    >
                      ↓
                    </button>
                    <button
                      type="button"
                      disabled={disabled || rows.length >= maxItems}
                      aria-label={__("Duplicate row")}
                      onClick={() => duplicateRow(idx)}
                      className="rounded px-1 py-0.5 text-[11px] text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                    >
                      ⧉
                    </button>
                    <button
                      type="button"
                      disabled={disabled || rows.length <= minItems}
                      aria-label={__("Remove")}
                      onClick={() => removeRow(idx)}
                      className="rounded px-1 py-0.5 text-[11px] text-rose-600 hover:bg-rose-50 disabled:opacity-35"
                    >
                      ✕
                    </button>
                  </div>
                </div>
                {!isCollapsed && (
                  <div className="p-3">
                    {renderNestedFields({
                      fields: subFields,
                      scopeValues: row,
                      onScopeChange: nextRow => updateRow(idx, nextRow),
                      disabled,
                      fieldPathPrefix: `${fieldPathPrefix}.${field.key}[${idx}]`,
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
