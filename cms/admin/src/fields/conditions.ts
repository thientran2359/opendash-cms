import { canonicalEmptyValue } from './helpers.ts';
import type { ConditionGroups, ConditionRule, FieldSpec } from './types.ts';

function parseIsoDateParts(raw: string): [number, number, number] | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(raw);
  if (!m) return null;
  const year = Number(m[1]);
  const month = Number(m[2]);
  const day = Number(m[3]);
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const daysInMonth =
    month === 1 || month === 3 || month === 5 || month === 7 || month === 8 || month === 10 || month === 12
      ? 31
      : month === 4 || month === 6 || month === 9 || month === 11
        ? 30
        : month === 2
          ? (leap ? 29 : 28)
          : 0;
  if (year < 1 || day < 1 || day > daysInMonth) return null;
  return [year, month, day];
}

export function parseUtcDateTimeMillis(raw: unknown): number | null {
  if (typeof raw !== 'string') return null;
  const m = /^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?Z$/.exec(raw);
  if (!m) return null;
  const dateParts = parseIsoDateParts(m[1]);
  if (!dateParts) return null;
  const hour = Number(m[2]);
  const min = Number(m[3]);
  const sec = m[4] ? Number(m[4]) : 0;
  if (hour > 23 || min > 59 || sec > 59) return null;
  const ms = m[5] ? Number(m[5].slice(0, 3).padEnd(3, '0')) : 0;
  const utc = Date.UTC(dateParts[0], dateParts[1] - 1, dateParts[2], hour, min, sec, ms);
  return Number.isFinite(utc) ? utc : null;
}

export function parseTimeSeconds(raw: unknown): number | null {
  if (typeof raw !== 'string') return null;
  const m = /^(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(raw);
  if (!m) return null;
  const hour = Number(m[1]);
  const min = Number(m[2]);
  const sec = m[3] ? Number(m[3]) : 0;
  if (hour > 23 || min > 59 || sec > 59) return null;
  return hour * 3600 + min * 60 + sec;
}

export function isConditionValueEmpty(value: unknown): boolean {
  if (value === null || value === undefined) return true;
  if (typeof value === 'string') return value.trim().length === 0;
  if (Array.isArray(value)) return value.length === 0;
  if (typeof value === 'object') return Object.keys(value).length === 0;
  return false;
}

function scalarEqual(a: unknown, b: unknown): boolean {
  if (a === null || a === undefined || b === null || b === undefined) {
    return (a ?? null) === (b ?? null);
  }
  if (typeof a === 'number' && typeof b === 'number') {
    if (!Number.isFinite(a) || !Number.isFinite(b)) return false;
    return a === b;
  }
  if (typeof a === 'boolean' && typeof b === 'boolean') return a === b;
  if (typeof a === 'string' && typeof b === 'string') return a === b;
  return false;
}

function resolveEffectiveTargetValue(
  key: string,
  scopeSpecs: readonly FieldSpec[],
  scopeValues: Record<string, unknown>,
  rootSpecs: readonly FieldSpec[],
  rootValues: Record<string, unknown>,
): { spec?: FieldSpec; value: unknown } {
  const scopeSpec = scopeSpecs.find(s => s.key === key);
  if (scopeSpec) {
    const raw = scopeValues[key];
    const value =
      raw !== undefined && raw !== null
        ? raw
        : scopeSpec.default !== undefined && scopeSpec.default !== null
          ? scopeSpec.default
          : canonicalEmptyValue(scopeSpec);
    return { spec: scopeSpec, value };
  }
  const rootSpec = rootSpecs.find(s => s.key === key);
  if (rootSpec) {
    const raw = rootValues[key];
    const value =
      raw !== undefined && raw !== null
        ? raw
        : rootSpec.default !== undefined && rootSpec.default !== null
          ? rootSpec.default
          : canonicalEmptyValue(rootSpec);
    return { spec: rootSpec, value };
  }
  return { spec: undefined, value: null };
}

function evaluateSingleCondition(
  rule: ConditionRule,
  scopeSpecs: readonly FieldSpec[],
  scopeValues: Record<string, unknown>,
  rootSpecs: readonly FieldSpec[],
  rootValues: Record<string, unknown>,
): boolean {
  const { spec, value: curVal } = resolveEffectiveTargetValue(
    rule.field,
    scopeSpecs,
    scopeValues,
    rootSpecs,
    rootValues,
  );

  switch (rule.op) {
    case 'empty':
      return isConditionValueEmpty(curVal);
    case 'not_empty':
      return !isConditionValueEmpty(curVal);
    case 'eq':
      return scalarEqual(curVal, rule.value);
    case 'neq':
      return !scalarEqual(curVal, rule.value);
    case 'in': {
      if (!Array.isArray(rule.value)) return false;
      if (Array.isArray(curVal)) {
        return curVal.some(item => rule.value && Array.isArray(rule.value) && rule.value.some(cand => scalarEqual(item, cand)));
      }
      return rule.value.some(cand => scalarEqual(curVal, cand));
    }
    case 'contains': {
      if (rule.value === undefined) return false;
      if (typeof curVal === 'string' && typeof rule.value === 'string') {
        return curVal.includes(rule.value);
      }
      if (Array.isArray(curVal)) {
        return curVal.some(item => scalarEqual(item, rule.value));
      }
      return false;
    }
    case 'gt':
    case 'lt': {
      const expected = rule.value;
      if (expected === undefined || expected === null) return false;
      const isGt = rule.op === 'gt';
      let cmp: number | null = null;
      if (spec?.type === 'datetime') {
        const a = parseUtcDateTimeMillis(curVal);
        const b = parseUtcDateTimeMillis(expected);
        if (a !== null && b !== null) cmp = a - b;
      } else if (spec?.type === 'time') {
        const a = parseTimeSeconds(curVal);
        const b = parseTimeSeconds(expected);
        if (a !== null && b !== null) cmp = a - b;
      } else if (spec?.type === 'date') {
        if (
          typeof curVal === 'string'
          && typeof expected === 'string'
          && parseIsoDateParts(curVal)
          && parseIsoDateParts(expected)
        ) {
          cmp = curVal < expected ? -1 : curVal > expected ? 1 : 0;
        }
      } else if (
        typeof curVal === 'number'
        && typeof expected === 'number'
        && Number.isFinite(curVal)
        && Number.isFinite(expected)
      ) {
        cmp = curVal - expected;
      }
      if (cmp === null) return false;
      return isGt ? cmp > 0 : cmp < 0;
    }
  }
}

export function evaluateConditions(
  conditions: ConditionGroups | null | undefined,
  scopeSpecs: readonly FieldSpec[],
  scopeValues: Record<string, unknown>,
  rootSpecs: readonly FieldSpec[],
  rootValues: Record<string, unknown>,
): boolean {
  if (!conditions || conditions.length === 0) return true;
  return conditions.some(andGroup =>
    andGroup.every(rule =>
      evaluateSingleCondition(rule, scopeSpecs, scopeValues, rootSpecs, rootValues),
    ),
  );
}
