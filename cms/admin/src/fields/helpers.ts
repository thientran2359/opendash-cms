import type {
  FieldSpec,
  FieldType,
  FieldViewport,
  ResponsiveValue,
  SpacingProperty,
  SpacingValue,
} from './types.ts';

export function isUiOnlyFieldType(fieldType: FieldType): boolean {
  return fieldType === 'tab' || fieldType === 'accordion' || fieldType === 'message';
}

export function canonicalEmptyValue(spec: FieldSpec): unknown {
  switch (spec.type) {
    case 'text':
    case 'textarea':
    case 'email':
    case 'url':
    case 'richtext':
    case 'color':
    case 'icon':
    case 'date':
    case 'datetime':
    case 'time':
    case 'radio_buttons':
      return '';
    case 'select':
      return spec.multiple ? [] : '';
    case 'checkbox':
    case 'gallery':
    case 'repeater':
      return [];
    case 'entry_ref':
    case 'term_ref':
      return spec.multiple ? [] : null;
    case 'boolean':
      return false;
    case 'group':
      return {};
    case 'spacing':
      return {};
    case 'link':
      return { url: '', label: '', new_tab: false, nofollow: false };
    case 'number':
    case 'range':
    case 'media':
    case 'secret':
    case 'tab':
    case 'accordion':
    case 'message':
      return null;
  }
}

export function defaultValueForSpec(spec: FieldSpec): unknown {
  if (spec.default !== undefined && spec.default !== null) {
    return typeof spec.default === 'object'
      ? structuredClone(spec.default)
      : spec.default;
  }
  switch (spec.type) {
    case 'number':
      return spec.min ?? 0;
    case 'range':
      return spec.min ?? 0;
    case 'select':
      if (spec.multiple) return [];
      if (spec.allow_null || spec.options_source) return '';
      return spec.options?.[0]?.value ?? '';
    case 'radio_buttons':
      return spec.options?.[0]?.value ?? '';
    case 'group': {
      const nested: Record<string, unknown> = {};
      for (const child of spec.fields ?? []) {
        if (!isUiOnlyFieldType(child.type)) {
          nested[child.key] = defaultValueForSpec(child);
        }
      }
      return nested;
    }
    default:
      return canonicalEmptyValue(spec);
  }
}

export function initialValuesFromFields(
  fields: readonly FieldSpec[],
  saved?: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const field of fields) {
    if (isUiOnlyFieldType(field.type)) continue;
    const savedVal = saved && Object.prototype.hasOwnProperty.call(saved, field.key)
      ? saved[field.key]
      : undefined;

    if (field.type === 'group') {
      const savedObj =
        savedVal && typeof savedVal === 'object' && !Array.isArray(savedVal)
          ? (savedVal as Record<string, unknown>)
          : undefined;
      out[field.key] = initialValuesFromFields(field.fields ?? [], savedObj);
      continue;
    }

    if (savedVal !== undefined) {
      out[field.key] = typeof savedVal === 'object' && savedVal !== null
        ? structuredClone(savedVal)
        : savedVal;
    } else {
      out[field.key] = defaultValueForSpec(field);
    }
  }
  return out;
}

/** Returns concrete data fields at the given scope, stripping UI-only `tab`/`accordion`/`message` items. */
export function flattenDataFields(fields: readonly FieldSpec[]): FieldSpec[] {
  const out: FieldSpec[] = [];
  for (const field of fields) {
    if (isUiOnlyFieldType(field.type)) continue;
    out.push(field);
  }
  return out;
}

/** Returns all leaf data fields recursively (descending into `group` and `repeater`, skipping UI-only). */
export function collectLeafDataFields(fields: readonly FieldSpec[]): FieldSpec[] {
  const out: FieldSpec[] = [];
  for (const field of fields) {
    if (isUiOnlyFieldType(field.type)) continue;
    if (field.type === 'group' || field.type === 'repeater') {
      out.push(...collectLeafDataFields(field.fields ?? []));
    } else {
      out.push(field);
    }
  }
  return out;
}

export function normalizeSpacingValue(
  raw: unknown,
  property: SpacingProperty = 'padding',
  min?: number,
  max?: number,
): SpacingValue {
  const loBound = property === 'margin' ? -256 : 0;
  const lo = Math.max(loBound, min ?? (property === 'margin' ? -128 : 0));
  const hi = Math.min(256, max ?? 256);
  const clampSide = (v: unknown): number | undefined => {
    if (typeof v !== 'number' || !Number.isFinite(v)) return undefined;
    const norm = v === 0 ? 0 : v;
    return Math.min(hi, Math.max(lo, norm));
  };
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return {};
  }
  const obj = raw as Record<string, unknown>;
  const out: SpacingValue = {};
  const t = clampSide(obj.top);
  const r = clampSide(obj.right);
  const b = clampSide(obj.bottom);
  const l = clampSide(obj.left);
  if (t !== undefined) out.top = t;
  if (r !== undefined) out.right = r;
  if (b !== undefined) out.bottom = b;
  if (l !== undefined) out.left = l;
  return out;
}

export function isResponsiveMap(value: unknown): value is ResponsiveValue<unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value as Record<string, unknown>);
  return keys.length > 0 && keys.every(k => k === 'desktop' || k === 'tablet' || k === 'mobile');
}

export function resolveResponsiveValue<T>(
  raw: unknown,
  viewport: FieldViewport,
  fallback?: T,
): { current: T | undefined; inheritedFrom?: FieldViewport } {
  if (!isResponsiveMap(raw)) {
    return { current: (raw as T | undefined) ?? fallback };
  }
  const d = raw.desktop as T | undefined;
  const t = raw.tablet as T | undefined;
  const m = raw.mobile as T | undefined;

  if (viewport === 'desktop') {
    return { current: d ?? fallback };
  }
  if (viewport === 'tablet') {
    if (t !== undefined && t !== null) return { current: t };
    if (d !== undefined && d !== null) return { current: d, inheritedFrom: 'desktop' };
    return { current: fallback };
  }
  if (m !== undefined && m !== null) return { current: m };
  if (t !== undefined && t !== null) return { current: t, inheritedFrom: 'tablet' };
  if (d !== undefined && d !== null) return { current: d, inheritedFrom: 'desktop' };
  return { current: fallback };
}

export function writeResponsiveValue<T>(
  existing: unknown,
  viewport: FieldViewport,
  nextViewportValue: T | undefined,
): ResponsiveValue<T> {
  const base: ResponsiveValue<T> = isResponsiveMap(existing)
    ? { ...(existing as ResponsiveValue<T>) }
    : existing !== undefined && existing !== null
      ? { desktop: existing as T }
      : {};
  if (nextViewportValue === undefined) {
    delete base[viewport];
  } else {
    base[viewport] = nextViewportValue;
  }
  return base;
}

function getTzParts(date: Date, timeZone: string): {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
} {
  let formatter: Intl.DateTimeFormat;
  try {
    formatter = new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hourCycle: 'h23',
    });
  } catch {
    formatter = new Intl.DateTimeFormat('en-CA', {
      timeZone: 'UTC',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hourCycle: 'h23',
    });
  }
  const map: Record<string, number> = {};
  for (const part of formatter.formatToParts(date)) {
    if (part.type !== 'literal') {
      map[part.type] = Number(part.value);
    }
  }
  return {
    year: map.year,
    month: map.month,
    day: map.day,
    hour: map.hour === 24 ? 0 : map.hour,
    minute: map.minute,
    second: map.second,
  };
}

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

export function formatDateTimeForSiteTz(
  utcIso: string,
  siteTimezone = 'UTC',
): string {
  if (!utcIso || typeof utcIso !== 'string') return '';
  const ts = Date.parse(utcIso);
  if (!Number.isFinite(ts)) return '';
  const p = getTzParts(new Date(ts), siteTimezone || 'UTC');
  const base = `${String(p.year).padStart(4, '0')}-${pad2(p.month)}-${pad2(p.day)}T${pad2(p.hour)}:${pad2(p.minute)}`;
  return p.second !== 0 ? `${base}:${pad2(p.second)}` : base;
}

export function parseSiteTzToUtcIso(
  localInput: string,
  siteTimezone = 'UTC',
): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(localInput.trim());
  if (!m) return '';
  const year = Number(m[1]);
  const month = Number(m[2]);
  const day = Number(m[3]);
  const hour = Number(m[4]);
  const minute = Number(m[5]);
  const second = m[6] ? Number(m[6]) : 0;
  const targetAsUtc = Date.UTC(year, month - 1, day, hour, minute, second, 0);
  if (!Number.isFinite(targetAsUtc)) return '';
  // Validate Gregorian date didn't overflow (e.g. Feb 30 -> Mar 2)
  const check = new Date(targetAsUtc);
  if (
    check.getUTCFullYear() !== year
    || check.getUTCMonth() + 1 !== month
    || check.getUTCDate() !== day
    || check.getUTCHours() !== hour
    || check.getUTCMinutes() !== minute
    || check.getUTCSeconds() !== second
  ) {
    return '';
  }

  const tz = siteTimezone || 'UTC';
  // Two-step fixed-point offset solve handles DST transitions accurately.
  let guess = targetAsUtc;
  for (let i = 0; i < 3; i += 1) {
    const p = getTzParts(new Date(guess), tz);
    const displayedUtc = Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute, p.second, 0);
    const diff = targetAsUtc - displayedUtc;
    if (diff === 0) break;
    guess += diff;
  }
  return new Date(guess).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

export function readWidthColsClass(width?: FieldSpec['width']): string {
  switch (width) {
    case 25:
      return 'md:col-span-3';
    case 33:
      return 'md:col-span-4';
    case 50:
      return 'md:col-span-6';
    case 66:
      return 'md:col-span-8';
    case 75:
      return 'md:col-span-9';
    case 100:
    default:
      return 'md:col-span-12';
  }
}
