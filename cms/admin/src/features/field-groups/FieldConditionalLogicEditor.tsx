import React from 'react';
import type { ConditionGroups, ConditionOp, ConditionRule, FieldSpec } from '../../fields/types.ts';
import { __, useI18n } from '../../i18n/index.tsx';

const CONTROL =
  'h-8 w-full min-w-0 rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent';

const CONDITION_OPS: Array<{ op: ConditionOp; label: string; needsValue: boolean }> = [
  { op: 'eq', label: 'equals (=)', needsValue: true },
  { op: 'neq', label: 'not equals (!=)', needsValue: true },
  { op: 'contains', label: 'contains', needsValue: true },
  { op: 'in', label: 'in list (a,b)', needsValue: true },
  { op: 'gt', label: 'greater than (>)', needsValue: true },
  { op: 'lt', label: 'less than (<)', needsValue: true },
  { op: 'not_empty', label: 'is not empty', needsValue: false },
  { op: 'empty', label: 'is empty', needsValue: false },
];

interface FieldConditionalLogicEditorProps {
  conditions: ConditionGroups | undefined;
  candidateFields: readonly FieldSpec[];
  disabled?: boolean;
  onChange: (next: ConditionGroups | undefined) => void;
}

function parseRuleInputValue(raw: string, op: ConditionOp, targetSpec?: FieldSpec): unknown {
  if (op === 'in') {
    return raw.split(',').map(v => v.trim()).filter(Boolean);
  }
  if (targetSpec?.type === 'boolean') {
    return raw === 'true' || raw === '1';
  }
  if (targetSpec?.type === 'number' || targetSpec?.type === 'range') {
    const num = Number(raw);
    return Number.isFinite(num) && raw.trim() !== '' ? num : raw;
  }
  return raw;
}

function formatRuleInputValue(value: unknown): string {
  if (Array.isArray(value)) return value.join(', ');
  if (value === undefined || value === null) return '';
  return String(value);
}

export const FieldConditionalLogicEditor: React.FC<FieldConditionalLogicEditorProps> = ({
  conditions,
  candidateFields,
  disabled = false,
  onChange,
}) => {
  useI18n();
  const groups = conditions ?? [];
  const selectable = candidateFields.filter(
    f => f.type !== 'tab' && f.type !== 'accordion' && f.type !== 'message',
  );

  const toggleEnabled = (enabled: boolean) => {
    if (!enabled) {
      onChange(undefined);
      return;
    }
    const defaultField = selectable[0]?.key ?? '';
    onChange([[{ field: defaultField, op: 'not_empty' }]]);
  };

  const updateGroup = (orIdx: number, nextBranch: ConditionRule[]) => {
    const next = groups
      .map((branch, idx) => (idx === orIdx ? nextBranch : branch))
      .filter(branch => branch.length > 0);
    onChange(next.length > 0 ? next : undefined);
  };

  const addOrGroup = () => {
    const defaultField = selectable[0]?.key ?? '';
    onChange([...groups, [{ field: defaultField, op: 'not_empty' }]]);
  };

  if (selectable.length === 0) {
    return (
      <p className="text-xs text-zinc-500">
        {__("Add at least one sibling data field to configure conditional logic.")}
      </p>
    );
  }

  return (
    <div className="space-y-2.5">
      <label className="inline-flex items-center gap-2 text-xs font-medium text-zinc-800">
        <input
          type="checkbox"
          checked={groups.length > 0}
          disabled={disabled}
          onChange={e => toggleEnabled(e.target.checked)}
        />
        <span>{__("Enable conditional logic (DNF: OR of AND groups)")}</span>
      </label>

      {groups.map((branch, orIdx) => (
        <div key={orIdx} className="space-y-2 rounded-md border border-zinc-200 bg-zinc-50 p-2.5">
          <div className="flex items-center justify-between gap-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
            <span>{orIdx === 0 ? __("Show if") : __("OR if")}</span>
            <button
              type="button"
              disabled={disabled}
              onClick={() => updateGroup(orIdx, [])}
              className="text-rose-600 hover:underline disabled:opacity-40"
            >
              {__("Remove group")}
            </button>
          </div>
          {branch.map((rule, andIdx) => {
            const targetSpec = selectable.find(f => f.key === rule.field);
            const opMeta = CONDITION_OPS.find(o => o.op === rule.op) ?? CONDITION_OPS[0];
            return (
              <div key={andIdx} className="grid grid-cols-1 gap-2 sm:grid-cols-[1fr_140px_1fr_auto]">
                <select
                  aria-label={__("Condition target field")}
                  className={CONTROL}
                  value={rule.field}
                  disabled={disabled}
                  onChange={e =>
                    updateGroup(
                      orIdx,
                      branch.map((r, idx) => (idx === andIdx ? { ...r, field: e.target.value } : r)),
                    )
                  }
                >
                  {selectable.map(cand => (
                    <option key={cand.key} value={cand.key}>
                      {cand.label} ({cand.key})
                    </option>
                  ))}
                  {rule.field && !selectable.some(c => c.key === rule.field) && (
                    <option value={rule.field}>{rule.field}</option>
                  )}
                </select>
                <select
                  aria-label={__("Condition operator")}
                  className={CONTROL}
                  value={rule.op}
                  disabled={disabled}
                  onChange={e => {
                    const nextOp = e.target.value as ConditionOp;
                    const needsVal = CONDITION_OPS.find(o => o.op === nextOp)?.needsValue ?? false;
                    updateGroup(
                      orIdx,
                      branch.map((r, idx) =>
                        idx === andIdx ? { field: r.field, op: nextOp, ...(needsVal ? { value: r.value ?? '' } : {}) } : r,
                      ),
                    );
                  }}
                >
                  {CONDITION_OPS.map(item => (
                    <option key={item.op} value={item.op}>{item.label}</option>
                  ))}
                </select>
                {opMeta.needsValue ? (
                  targetSpec?.type === 'boolean' ? (
                    <select
                      aria-label={__("Condition value")}
                      className={CONTROL}
                      value={rule.value === true ? 'true' : 'false'}
                      disabled={disabled}
                      onChange={e =>
                        updateGroup(
                          orIdx,
                          branch.map((r, idx) =>
                            idx === andIdx ? { ...r, value: e.target.value === 'true' } : r,
                          ),
                        )
                      }
                    >
                      <option value="true">{__("Yes / True")}</option>
                      <option value="false">{__("No / False")}</option>
                    </select>
                  ) : (
                    <input
                      type="text"
                      aria-label={__("Condition value")}
                      className={CONTROL}
                      value={formatRuleInputValue(rule.value)}
                      disabled={disabled}
                      placeholder={rule.op === 'in' ? __("a, b, c") : __("Value")}
                      onChange={e =>
                        updateGroup(
                          orIdx,
                          branch.map((r, idx) =>
                            idx === andIdx
                              ? { ...r, value: parseRuleInputValue(e.target.value, rule.op, targetSpec) }
                              : r,
                          ),
                        )
                      }
                    />
                  )
                ) : (
                  <span className="flex items-center text-[11px] text-zinc-400">—</span>
                )}
                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    disabled={disabled || branch.length >= 16}
                    onClick={() =>
                      updateGroup(orIdx, [...branch, { field: selectable[0].key, op: 'not_empty' }])
                    }
                    className="h-8 rounded border border-zinc-300 bg-white px-2 text-[11px] font-medium text-zinc-700 hover:bg-zinc-100"
                  >
                    + AND
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => updateGroup(orIdx, branch.filter((_, idx) => idx !== andIdx))}
                    className="h-8 rounded border border-rose-200 bg-white px-2 text-[11px] font-medium text-rose-600 hover:bg-rose-50"
                  >
                    ×
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      ))}

      {groups.length > 0 && groups.length < 16 && (
        <button
          type="button"
          disabled={disabled}
          onClick={addOrGroup}
          className="rounded border border-zinc-300 bg-white px-2.5 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-50"
        >
          {__("+ Add OR condition group")}
        </button>
      )}
    </div>
  );
};
