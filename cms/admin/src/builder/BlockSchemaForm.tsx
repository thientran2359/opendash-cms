import { useId, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type {
  BlockFieldSpec,
  BlockLinkValue,
  BlockMediaKind,
  BlockSchemaV1,
  BlockShowIfRule,
  BlockSpacingValue,
} from '../types.ts';
import type { BuilderViewport } from './document.ts';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import {
  ColorSwatchControl,
  LightboxRichTextTrigger,
  LinkControl,
  MediaCardControl,
  RangeNumberControl,
  ResponsiveDot,
  SegmentedButtonsControl,
  SpacingBoxControl,
  STUDIO_INPUT_CLASS,
  ToggleSwitchControl,
  formatSpacingSummary,
  type ResponsiveDotInfo,
} from './StudioFieldControls.tsx';

interface BlockSchemaFormProps {
  schema: BlockSchemaV1;
  values: Record<string, unknown>;
  onChange: (values: Record<string, unknown>) => void;
  viewport: BuilderViewport;
  csrfToken: string | null;
  onError: (error: unknown, fallback: string) => void;
  disabled?: boolean;
}

const BUTTON_CLASS =
  'rounded border border-zinc-300 bg-white px-2 py-0.5 text-[11px] font-medium text-zinc-700 hover:bg-zinc-50 disabled:opacity-45';

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function primitiveDefault(field: BlockFieldSpec): unknown {
  return 'default' in field ? field.default : undefined;
}

function isResponsiveField(field: BlockFieldSpec): boolean {
  return (
    field.type === 'number' ||
    field.type === 'range' ||
    field.type === 'select' ||
    field.type === 'radio_buttons' ||
    field.type === 'spacing'
  ) && Boolean(field.responsive);
}

/** Group fields are stored flat, never under the group key. */
function topFields(fields: BlockFieldSpec[]): BlockFieldSpec[] {
  return fields.flatMap(field => field.type === 'group' ? topFields(field.fields) : [field]);
}

function visible(
  rule: BlockShowIfRule | null | undefined,
  root: Record<string, unknown>,
  defaults: Map<string, unknown>,
): boolean {
  if (!rule) return true;
  const candidate = root[rule.field] == null ? defaults.get(rule.field) : root[rule.field];
  return (candidate ?? null) === rule.equals;
}

function initialItem(fields: BlockFieldSpec[]): Record<string, unknown> {
  const item: Record<string, unknown> = {};
  for (const field of fields) {
    if (field.type === 'group' || field.type === 'repeater') continue;
    const initial = primitiveDefault(field);
    if (initial !== undefined && initial !== null) {
      item[field.key] = isResponsiveField(field) ? { desktop: initial } : initial;
    }
  }
  return item;
}

/** A field can become required when show_if changes. Materialize its declared
 * default at that moment so the values sent to Rust match what the UI shows. */
function initializeNewlyVisibleFields(
  fields: BlockFieldSpec[],
  previous: Record<string, unknown>,
  updated: Record<string, unknown>,
  defaults: Map<string, unknown>,
  oldParentVisible = true,
  newParentVisible = true,
): void {
  for (const field of fields) {
    const wasVisible = oldParentVisible && visible(field.show_if, previous, defaults);
    const nowVisible = newParentVisible && visible(field.show_if, updated, defaults);
    if (field.type === 'group') {
      initializeNewlyVisibleFields(field.fields, previous, updated, defaults, wasVisible, nowVisible);
      continue;
    }
    if (wasVisible || !nowVisible || updated[field.key] != null) continue;
    if (field.type === 'repeater') {
      updated[field.key] = Array.from({ length: field.min_items ?? 0 }, () => initialItem(field.fields));
      continue;
    }
    const fallback = primitiveDefault(field);
    if (fallback === undefined || fallback === null) continue;
    updated[field.key] = isResponsiveField(field) ? { desktop: fallback } : fallback;
  }
}

function replaceValueAtPath(root: unknown, path: Array<string | number>, value: unknown): unknown {
  if (!path.length) return value;
  const [head, ...tail] = path;
  if (typeof head === 'number') {
    const copy = Array.isArray(root) ? [...root] : [];
    copy[head] = replaceValueAtPath(copy[head], tail, value);
    return copy;
  }
  const copy = isObject(root) ? { ...root } : {};
  copy[head] = replaceValueAtPath(copy[head], tail, value);
  return copy;
}

function isSpacingSideObj(value: unknown): value is BlockSpacingValue {
  if (!isObject(value)) return false;
  return 'top' in value || 'right' in value || 'bottom' in value || 'left' in value;
}

/** Keep any old scalar as desktop when introducing breakpoint overrides. */
function toResponsiveObject(value: unknown, isSpacingType = false): Record<string, unknown> {
  if (isObject(value)) {
    if (isSpacingType && isSpacingSideObj(value)) return { desktop: value };
    return { ...value };
  }
  return value === undefined || value === null ? {} : { desktop: value };
}

function directBreakpoint(value: unknown, breakpoint: BuilderViewport, isSpacingType = false): unknown {
  if (isObject(value)) {
    if (isSpacingType && isSpacingSideObj(value)) {
      return breakpoint === 'desktop' ? value : undefined;
    }
    return value[breakpoint];
  }
  return breakpoint === 'desktop' ? value : undefined;
}

function spacingObjectsEqual(a: unknown, b: unknown): boolean {
  const ao = isObject(a) ? a as BlockSpacingValue : {};
  const bo = isObject(b) ? b as BlockSpacingValue : {};
  return (ao.top ?? 0) === (bo.top ?? 0)
    && (ao.right ?? 0) === (bo.right ?? 0)
    && (ao.bottom ?? 0) === (bo.bottom ?? 0)
    && (ao.left ?? 0) === (bo.left ?? 0);
}

function normalizeLinkValue(value: unknown): BlockLinkValue {
  if (typeof value === 'string') {
    return { url: value, new_tab: false, nofollow: false };
  }
  if (isObject(value)) {
    return {
      url: typeof value.url === 'string' ? value.url : '',
      new_tab: Boolean(value.new_tab),
      nofollow: Boolean(value.nofollow),
    };
  }
  return { url: '', new_tab: false, nofollow: false };
}

interface FieldControlProps {
  field: Exclude<BlockFieldSpec, { type: 'group' | 'repeater' }>;
  current: unknown;
  viewport: BuilderViewport;
  disabled: boolean;
  onSet: (value: unknown) => void;
  onPickMedia: (kind: BlockMediaKind, current: string) => void;
}

function FieldControl({ field, current, viewport, disabled, onSet, onPickMedia }: FieldControlProps) {
  const id = useId();
  const fallback = primitiveDefault(field);
  const responsive = isResponsiveField(field);
  const isSpacing = field.type === 'spacing';

  const effDesktop = responsive ? (directBreakpoint(current, 'desktop', isSpacing) ?? fallback) : current;
  const effTablet = responsive ? (directBreakpoint(current, 'tablet', isSpacing) ?? effDesktop) : effDesktop;
  const effMobile = responsive ? (directBreakpoint(current, 'mobile', isSpacing) ?? effTablet) : effTablet;

  const value = responsive
    ? (viewport === 'desktop' ? effDesktop : viewport === 'tablet' ? effTablet : effMobile)
    : current;

  const setValue = (next: unknown) => {
    if (!responsive) {
      onSet(next);
      return;
    }
    const overrides = toResponsiveObject(current, isSpacing);
    if (next === undefined) delete overrides[viewport];
    else overrides[viewport] = next;
    onSet(Object.keys(overrides).length ? overrides : undefined);
  };

  const resetResponsive = () => {
    if (!responsive) return;
    if (viewport === 'desktop') {
      onSet(effDesktop !== undefined ? { desktop: effDesktop } : undefined);
    } else {
      const overrides = toResponsiveObject(current, isSpacing);
      delete overrides[viewport];
      onSet(Object.keys(overrides).length ? overrides : undefined);
    }
  };

  const differs = responsive && (isSpacing
    ? (!spacingObjectsEqual(effDesktop, effTablet) || !spacingObjectsEqual(effTablet, effMobile))
    : ((effDesktop ?? null) !== (effTablet ?? null) || (effTablet ?? null) !== (effMobile ?? null)));

  const unit = 'unit' in field && field.unit ? field.unit : '';
  const formatSummaryVal = (v: unknown): string => {
    if (isSpacing) {
      const box = isObject(v) ? v as BlockSpacingValue : undefined;
      return unit && unit !== 'px'
        ? `${box?.top ?? 0}${unit} ${box?.right ?? 0}${unit}`
        : formatSpacingSummary(box, 'p', false);
    }
    if (field.type === 'select' || field.type === 'radio_buttons') {
      const str = v == null ? '' : String(v);
      return field.options.find(o => o.value === str)?.label ?? (str || '—');
    }
    return v == null || v === '' ? '—' : `${String(v)}${unit}`;
  };

  const responsiveInfo: ResponsiveDotInfo | undefined = responsive
    ? {
        differs,
        viewport,
        summary: `Desktop: ${formatSummaryVal(effDesktop)} · Tablet: ${formatSummaryVal(effTablet)} · Mobile: ${formatSummaryVal(effMobile)}`,
        onReset: resetResponsive,
        testId: `field-amber-${field.key}`,
      }
    : undefined;

  const reqStar = 'required' in field && field.required ? ' *' : '';
  const fieldLabel = `${field.label}${reqStar}`;
  const stringValue = value == null ? '' : String(value);

  switch (field.type) {
    case 'boolean':
      return (
        <ToggleSwitchControl
          id={id}
          label={field.label}
          checked={value === true}
          disabled={disabled}
          onChange={setValue}
        />
      );

    case 'richtext':
      return (
        <LightboxRichTextTrigger
          id={id}
          label={fieldLabel}
          value={stringValue}
          maxBytes={field.max_bytes}
          disabled={disabled}
          onChange={setValue}
        />
      );

    case 'color':
      return (
        <ColorSwatchControl
          id={id}
          label={fieldLabel}
          value={stringValue}
          disabled={disabled}
          onChange={setValue}
        />
      );

    case 'media':
      return (
        <MediaCardControl
          id={id}
          label={fieldLabel}
          value={stringValue}
          acceptKind={field.media_kind ?? 'any'}
          disabled={disabled}
          onPick={() => onPickMedia(field.media_kind ?? 'any', stringValue)}
          onClear={() => setValue('')}
        />
      );

    case 'link':
      return (
        <LinkControl
          id={id}
          label={fieldLabel}
          value={normalizeLinkValue(value)}
          placeholder={field.placeholder ?? undefined}
          disabled={disabled}
          onChange={next => setValue(next)}
        />
      );

    case 'spacing': {
      const currentSpacing = isObject(value) ? value as BlockSpacingValue : undefined;
      return (
        <SpacingBoxControl
          label={fieldLabel}
          propPrefix="p"
          value={currentSpacing}
          min={field.min ?? 0}
          max={field.max ?? 256}
          unit={unit || 'px'}
          allowTailwind={!unit || unit === 'px'}
          disabled={disabled}
          responsiveInfo={responsiveInfo}
          testIdPrefix={`field-spacing-${field.key}`}
          onChange={next => {
            const hasSide = Object.values(next).some(v => typeof v === 'number');
            setValue(hasSide ? next : undefined);
          }}
        />
      );
    }

    case 'range':
    case 'number': {
      const min = field.min ?? (field.type === 'range' ? 0 : undefined);
      const max = field.max ?? (field.type === 'range' ? 100 : undefined);
      const useSlider = field.type === 'range'
        || (min != null && max != null && max - min <= 400);
      const numericVal = typeof value === 'number' ? value : undefined;

      if (useSlider) {
        return (
          <RangeNumberControl
            id={id}
            label={fieldLabel}
            value={numericVal}
            fallbackValue={typeof fallback === 'number' ? fallback : undefined}
            min={min ?? 0}
            max={max ?? 100}
            step={field.step ?? 1}
            unit={unit || undefined}
            disabled={disabled}
            responsiveInfo={responsiveInfo}
            onChange={next => setValue(next === undefined ? (responsive ? undefined : null) : next)}
          />
        );
      }

      return (
        <div className="space-y-1.5">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-1.5">
              <label htmlFor={id} className="text-xs font-semibold text-zinc-800">{fieldLabel}</label>
              <ResponsiveDot info={responsiveInfo} />
            </div>
          </div>
          <div className="flex items-center gap-2">
            <input
              id={id}
              type="number"
              className={STUDIO_INPUT_CLASS}
              min={min ?? undefined}
              max={max ?? undefined}
              step={field.step ?? 'any'}
              value={numericVal ?? ''}
              disabled={disabled}
              onChange={event => {
                if (!event.target.value) { setValue(responsive ? undefined : null); return; }
                const next = Number(event.target.value);
                if (Number.isFinite(next)) setValue(next);
              }}
            />
            {unit && <span className="text-xs text-zinc-500">{unit}</span>}
          </div>
        </div>
      );
    }

    case 'radio_buttons':
    case 'select': {
      const asButtons = field.type === 'radio_buttons'
        || field.display === 'buttons'
        || (field.display !== 'dropdown' && field.options.length <= 4 && field.options.every(o => o.label.length <= 14));
      if (asButtons) {
        return (
          <SegmentedButtonsControl
            label={fieldLabel}
            value={value == null ? '' : String(value)}
            options={field.options}
            disabled={disabled}
            responsiveInfo={responsiveInfo}
            onChange={next => setValue(next)}
          />
        );
      }
      return (
        <div className="space-y-1.5">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-1.5">
              <label htmlFor={id} className="text-xs font-semibold text-zinc-800">{fieldLabel}</label>
              <ResponsiveDot info={responsiveInfo} />
            </div>
          </div>
          <select
            id={id}
            className={STUDIO_INPUT_CLASS}
            value={value == null ? '' : String(value)}
            disabled={disabled}
            onChange={event => setValue(event.target.value || (responsive ? undefined : ''))}
          >
            {value == null && <option value="">Chưa chọn</option>}
            {field.options.map(option => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </div>
      );
    }

    case 'textarea':
      return (
        <div className="space-y-1.5">
          <label htmlFor={id} className="block text-xs font-semibold text-zinc-800">{fieldLabel}</label>
          <textarea
            id={id}
            rows={4}
            value={stringValue}
            maxLength={field.max_chars ?? 4000}
            disabled={disabled}
            onChange={event => setValue(event.target.value)}
            className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-sky-600 disabled:bg-zinc-100"
          />
        </div>
      );

    case 'text':
    case 'url':
      return (
        <div className="space-y-1.5">
          <label htmlFor={id} className="block text-xs font-semibold text-zinc-800">{fieldLabel}</label>
          <input
            id={id}
            type="text"
            className={STUDIO_INPUT_CLASS}
            value={stringValue}
            maxLength={field.type === 'text' ? field.max_chars ?? 256 : 2048}
            placeholder={field.placeholder ?? ''}
            disabled={disabled}
            onChange={event => setValue(event.target.value)}
          />
        </div>
      );
  }
}

interface PickerState {
  kind: BlockMediaKind;
  current: string;
  path: Array<string | number>;
}

/** Controlled editor: patches only the selected key so old schema keys and hidden
 * show_if values survive theme/plugin upgrades, 401 drafts and 412 conflicts. */
export function BlockSchemaForm({
  schema, values, onChange, viewport, csrfToken, onError, disabled = false,
}: BlockSchemaFormProps) {
  const [picker, setPicker] = useState<PickerState | null>(null);
  const [expandedRepeaters, setExpandedRepeaters] = useState<Set<string>>(() => new Set());
  const latestValues = useRef(values);
  const latestOnChange = useRef(onChange);
  latestValues.current = values;
  latestOnChange.current = onChange;

  const defaults = new Map(topFields(schema.fields).map(field => [field.key, primitiveDefault(field)]));

  const patchRoot = (key: string, next: unknown) => {
    const previous = latestValues.current;
    const updated = { ...previous };
    if (next === undefined) delete updated[key];
    else updated[key] = next;
    initializeNewlyVisibleFields(schema.fields, previous, updated, defaults);
    onChange(updated);
  };

  const toggleRepeaterItem = (itemKey: string) => {
    setExpandedRepeaters(prev => {
      const next = new Set(prev);
      if (next.has(itemKey)) next.delete(itemKey);
      else next.add(itemKey);
      return next;
    });
  };

  const renderFields = (
    fields: BlockFieldSpec[],
    current: Record<string, unknown>,
    patch: (key: string, value: unknown) => void,
    prefix: string,
    path: Array<string | number>,
  ): ReactNode => fields.map(field => {
    const key = prefix + field.key;
    if (!visible(field.show_if, values, defaults)) return null;
    if (field.type === 'group') {
      return (
        <div key={key} role="group" aria-label={field.label} className="space-y-3 border-t border-zinc-200 pt-3">
          <h3 className="!m-0 text-xs font-semibold uppercase tracking-wide text-zinc-700">{field.label}</h3>
          {renderFields(field.fields, current, patch, key + '.', path)}
        </div>
      );
    }
    if (field.type === 'repeater') {
      const items = Array.isArray(current[field.key]) ? current[field.key] as unknown[] : [];
      const min = field.min_items ?? 0;
      const max = field.max_items ?? 32;
      const mutate = (next: unknown[]) => patch(field.key, next);
      return (
        <div key={key} role="group" aria-label={field.label} className="space-y-2.5 border-t border-zinc-200 pt-3">
          <div className="flex items-center justify-between gap-2">
            <h3 className="!m-0 text-xs font-semibold uppercase tracking-wide text-zinc-700">{field.label}</h3>
            <span className="text-[11px] text-zinc-500">
              {items.length}/{max}{items.length < min ? ` · tối thiểu ${min}` : ''}
            </span>
          </div>
          <div className="divide-y divide-zinc-200 border-y border-zinc-200">
            {items.map((item, index) => {
              const row = isObject(item) ? item : {};
              const labelValue = field.item_label_field ? row[field.item_label_field] : undefined;
              const itemKey = `${key}.${index}`;
              const expanded = expandedRepeaters.has(itemKey);
              const titleText = typeof labelValue === 'string' && labelValue ? labelValue : `Mục ${index + 1}`;
              return (
                <div key={index} className="py-1.5">
                  <div className="flex items-center justify-between gap-1">
                    <button
                      type="button"
                      aria-expanded={expanded}
                      onClick={() => toggleRepeaterItem(itemKey)}
                      className="flex min-w-0 flex-1 items-center gap-1.5 rounded px-1 py-1 text-left text-xs font-medium text-zinc-800 hover:bg-zinc-50"
                    >
                      <span className="text-zinc-400" aria-hidden="true">{expanded ? '▾' : '▸'}</span>
                      <span className="truncate">{titleText}</span>
                    </button>
                    <div className="flex shrink-0 items-center gap-0.5">
                      <button
                        type="button"
                        title="Di chuyển lên"
                        aria-label={`Lên ${titleText}`}
                        className={BUTTON_CLASS}
                        disabled={disabled || index === 0}
                        onClick={() => {
                          const next = [...items];
                          [next[index - 1], next[index]] = [next[index], next[index - 1]];
                          mutate(next);
                        }}
                      >↑</button>
                      <button
                        type="button"
                        title="Di chuyển xuống"
                        aria-label={`Xuống ${titleText}`}
                        className={BUTTON_CLASS}
                        disabled={disabled || index === items.length - 1}
                        onClick={() => {
                          const next = [...items];
                          [next[index + 1], next[index]] = [next[index], next[index + 1]];
                          mutate(next);
                        }}
                      >↓</button>
                      <button
                        type="button"
                        title="Nhân bản mục"
                        aria-label={`Nhân bản ${titleText}`}
                        className={BUTTON_CLASS}
                        disabled={disabled || items.length >= max}
                        onClick={() => {
                          const nextIdx = index + 1;
                          mutate([...items.slice(0, nextIdx), JSON.parse(JSON.stringify(row)), ...items.slice(nextIdx)]);
                          setExpandedRepeaters(prev => new Set(prev).add(`${key}.${nextIdx}`));
                        }}
                      >⧉</button>
                      <button
                        type="button"
                        title="Xóa mục"
                        aria-label={`Xóa ${titleText}`}
                        className={BUTTON_CLASS + ' text-rose-700 hover:bg-rose-50'}
                        disabled={disabled || items.length <= min}
                        onClick={() => mutate(items.filter((_, idx) => idx !== index))}
                      >×</button>
                    </div>
                  </div>
                  {expanded && (
                    <div className="mt-2 space-y-3 pl-3">
                      {renderFields(field.fields, row, (subKey, subValue) => {
                        const updated = { ...row };
                        if (subValue === undefined) delete updated[subKey];
                        else updated[subKey] = subValue;
                        mutate(items.map((old, idx) => idx === index ? updated : old));
                      }, `${itemKey}.`, [...path, field.key, index])}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          <button
            type="button"
            className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-45"
            disabled={disabled || items.length >= max}
            onClick={() => {
              const newIdx = items.length;
              mutate([...items, initialItem(field.fields)]);
              setExpandedRepeaters(prev => new Set(prev).add(`${key}.${newIdx}`));
            }}
          >
            + Thêm mục
          </button>
        </div>
      );
    }
    return (
      <FieldControl
        key={key}
        field={field}
        current={current[field.key]}
        viewport={viewport}
        disabled={disabled}
        onSet={next => patch(field.key, next)}
        onPickMedia={(kind, currentId) => setPicker({ kind, current: currentId, path: [...path, field.key] })}
      />
    );
  });

  return (
    <div className="space-y-3.5" aria-label={`Thuộc tính block ${schema.title}`}>
      {renderFields(schema.fields, values, patchRoot, 'block.', [])}
      {picker && (
        <MediaPicker
          mode="image"
          acceptKind={picker.kind}
          csrfToken={csrfToken}
          selectedIds={picker.current ? [picker.current] : []}
          onApply={ids => {
            if (ids[0]) {
              const next = replaceValueAtPath(latestValues.current, picker.path, ids[0]);
              latestOnChange.current(next as Record<string, unknown>);
            }
            setPicker(null);
          }}
          onClose={() => setPicker(null)}
          onError={onError}
        />
      )}
    </div>
  );
}
