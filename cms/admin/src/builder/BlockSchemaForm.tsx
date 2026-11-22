import { useEffect, useId, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { BlockFieldSpec, BlockSchemaV1, BlockMediaKind, BlockShowIfRule } from '../types.ts';
import type { BuilderViewport } from './document.ts';
import { VIEWPORT_LABELS } from './document.ts';
import { MediaPicker } from '../features/media/MediaPicker.tsx';

interface BlockSchemaFormProps {
  schema: BlockSchemaV1;
  values: Record<string, unknown>;
  onChange: (values: Record<string, unknown>) => void;
  viewport: BuilderViewport;
  csrfToken: string | null;
  onError: (error: unknown, fallback: string) => void;
  disabled?: boolean;
}

const INPUT_CLASS = 'w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900 disabled:bg-zinc-100';
const BUTTON_CLASS = 'rounded-md border border-zinc-300 bg-white px-2.5 py-1 text-xs text-zinc-700 disabled:opacity-50';
const VIEWPORTS: BuilderViewport[] = ['desktop', 'tablet', 'mobile'];
const COLOR_TOKENS = [
  'transparent', 'currentColor', 'var(--od-token-primary)', 'var(--od-token-accent)',
  'var(--od-token-surface)', 'var(--od-token-muted)', 'var(--od-token-text)', 'var(--od-token-border)',
];

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function primitiveDefault(field: BlockFieldSpec): unknown {
  return 'default' in field ? field.default : undefined;
}

/** Group fields are stored flat, never under the group key. */
function topFields(fields: BlockFieldSpec[]): BlockFieldSpec[] {
  return fields.flatMap(field => field.type === 'group' ? topFields(field.fields) : [field]);
}

function visible(rule: BlockShowIfRule | null | undefined, root: Record<string, unknown>,
  defaults: Map<string, unknown>): boolean {
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
      item[field.key] = field.type === 'number' || field.type === 'range' || field.type === 'select'
        ? field.responsive ? { desktop: initial } : initial
        : initial;
    }
  }
  return item;
}

/** A field can become required when show_if changes. Materialize its declared
 * default at that moment so the values sent to Rust match what the UI shows.
 * Existing hidden/unknown keys are never removed or overwritten. */
function initializeNewlyVisibleFields(
  fields: BlockFieldSpec[], previous: Record<string, unknown>, updated: Record<string, unknown>,
  defaults: Map<string, unknown>, oldParentVisible = true, newParentVisible = true,
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
    updated[field.key] = (field.type === 'number' || field.type === 'range' || field.type === 'select')
      && field.responsive ? { desktop: fallback } : fallback;
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

/** Keep any old scalar as desktop when introducing breakpoint overrides. */
function toResponsiveObject(value: unknown): Record<string, unknown> {
  if (isObject(value)) return { ...value };
  return value === undefined || value === null ? {} : { desktop: value };
}

function directBreakpoint(value: unknown, breakpoint: BuilderViewport): unknown {
  if (isObject(value)) return value[breakpoint];
  return breakpoint === 'desktop' ? value : undefined;
}

function inheritedBreakpoint(value: unknown, breakpoint: BuilderViewport, fallback: unknown): unknown {
  if (!isObject(value)) return value ?? fallback;
  if (breakpoint === 'mobile') return value.mobile ?? value.tablet ?? value.desktop ?? fallback;
  if (breakpoint === 'tablet') return value.tablet ?? value.desktop ?? fallback;
  return value.desktop ?? fallback;
}

interface RichTextProps {
  value: string;
  onChange: (next: string) => void;
  disabled: boolean;
  id: string;
  maxBytes?: number | null;
}

/** HTML source editor with safe allowlisted tag shortcuts; Rust sanitizes on Save/render.
 * Keeping HTML as text avoids inserting untrusted markup into Admin's own DOM. */
export function RichTextFieldControl({ value, onChange, disabled, id, maxBytes }: RichTextProps) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const inputBytes = new TextEncoder().encode(value).length;
  const wrap = (before: string, after: string) => {
    const el = ref.current;
    if (!el) return;
    const start = el.selectionStart;
    const end = el.selectionEnd;
    const selected = value.slice(start, end);
    const updated = value.slice(0, start) + before + selected + after + value.slice(end);
    onChange(updated);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(start + before.length, end + before.length);
    });
  };
  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap gap-1" role="toolbar" aria-label="Định dạng RichText">
        {([
          ['Đậm', '<strong>', '</strong>'],
          ['Nghiêng', '<em>', '</em>'],
          ['Đoạn', '<p>', '</p>'],
          ['Danh sách', '<ul><li>', '</li></ul>'],
          ['Liên kết', '<a href=\"#\">', '</a>'],
        ] as const).map(([label, before, after]) => (
          <button key={label} type="button" disabled={disabled} className={BUTTON_CLASS}
            onClick={() => wrap(before, after)}>{label}</button>
        ))}
      </div>
      <textarea ref={ref} id={id} rows={5} value={value} disabled={disabled}
        className={INPUT_CLASS} onChange={event => onChange(event.target.value)}
        aria-describedby={id + '-help'} />
      <p id={id + '-help'} className="text-xs text-zinc-500">
        Hỗ trợ p, br, strong, em, u, ul, ol, li, blockquote và liên kết an toàn.
        {maxBytes ? ` Tối đa ${maxBytes} byte sau khi làm sạch.` : ''}
      </p>
      {maxBytes != null && <p role="status" className={'text-xs ' +
        (inputBytes > maxBytes ? 'text-amber-700' : 'text-zinc-500')}>
        Văn bản hiện tại: {inputBytes} byte trước khi làm sạch.
        {inputBytes > maxBytes ? ' Nội dung có thể bị từ chối khi Lưu nếu bản đã làm sạch vẫn vượt giới hạn.' : ''}
      </p>}
    </div>
  );
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
  const responsive = (field.type === 'number' || field.type === 'range' || field.type === 'select')
    && Boolean(field.responsive);
  const resolved = responsive ? directBreakpoint(current, viewport) : current;
  const value = resolved === undefined ? (responsive && viewport !== 'desktop' ? undefined : fallback) : resolved;
  const parentValue = responsive ? inheritedBreakpoint(current,
    viewport === 'mobile' ? 'tablet' : 'desktop', fallback) : undefined;
  const setValue = (next: unknown) => {
    if (!responsive) {
      onSet(next);
      return;
    }
    const overrides = toResponsiveObject(current);
    if (next === undefined) delete overrides[viewport];
    else overrides[viewport] = next;
    onSet(Object.keys(overrides).length ? overrides : undefined);
  };
  const unset = responsive && directBreakpoint(current, viewport) !== undefined;
  const stringValue = value == null ? '' : String(value);
  const effectiveValue = responsive && value === undefined
    ? inheritedBreakpoint(current, viewport, fallback) : value;
  const reset = responsive && viewport !== 'desktop'
    ? <button type="button" className={BUTTON_CLASS} disabled={disabled || !unset}
        onClick={() => setValue(undefined)}>Kế thừa</button>
    : null;
  const unit = 'unit' in field && field.unit ? field.unit : '';
  let control;
  switch (field.type) {
    case 'text':
    case 'url':
      control = (
        <input id={id} type="text" className={INPUT_CLASS} value={stringValue}
          maxLength={field.type === 'text' ? field.max_chars ?? 256 : 2048}
          placeholder={field.placeholder ?? ''} disabled={disabled}
          onChange={event => setValue(event.target.value)} />
      );
      break;
    case 'textarea':
      control = <textarea id={id} className={INPUT_CLASS} rows={4} value={stringValue}
        maxLength={field.max_chars ?? 4000} disabled={disabled}
        onChange={event => setValue(event.target.value)} />;
      break;
    case 'richtext':
      control = <RichTextFieldControl id={id} value={stringValue}
        disabled={disabled} maxBytes={field.max_bytes}
        onChange={setValue} />;
      break;
    case 'boolean':
      control = <input id={id} type="checkbox" checked={value === true}
        disabled={disabled} onChange={event => setValue(event.target.checked)} />;
      break;
    case 'number':
    case 'range': {
      const min = field.min ?? (field.type === 'range' ? 0 : undefined);
      const max = field.max ?? (field.type === 'range' ? 100 : undefined);
      const isRange = field.type === 'range';
      const numberValue = value == null ? '' : String(value);
      const displayValue = effectiveValue == null ? '' : String(effectiveValue);
      control = (
        <div className="flex items-center gap-2">
          <input id={id} type={isRange ? 'range' : 'number'} className={isRange ? 'min-w-0 flex-1' : INPUT_CLASS}
            min={min ?? undefined} max={max ?? undefined} step={field.step ?? (isRange ? 1 : 'any')}
            value={isRange ? (displayValue || String(min ?? 0)) : numberValue}
            placeholder={responsive && viewport !== 'desktop' ? `Kế thừa: ${displayValue}` : undefined}
            disabled={disabled}
            onChange={event => {
              if (!event.target.value) { setValue(null); return; }
              const next = Number(event.target.value);
              if (Number.isFinite(next)) setValue(next);
            }} />
          {unit && <span className="text-xs text-zinc-500">{unit}</span>}
          {isRange && <output className="text-xs tabular-nums">{displayValue}</output>}
          {reset}
        </div>
      );
      break;
    }
    case 'select':
      control = (
        <div className="flex items-center gap-2">
          <select id={id} className={INPUT_CLASS} value={value == null ? '' : String(value)}
            disabled={disabled} onChange={event => setValue(event.target.value || (responsive ? undefined : ''))}>
            {(responsive && viewport !== 'desktop' || value == null) &&
              <option value="">{responsive && viewport !== 'desktop'
                ? `Kế thừa: ${String(parentValue ?? '')}` : 'Chưa chọn'}</option>}
            {field.options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
          {reset}
        </div>
      );
      break;
    case 'color': {
      const hex = /^#[0-9a-fA-F]{6}$/.test(stringValue);
      control = (
        <div className="flex items-center gap-2">
          <input id={id} type="text" className={INPUT_CLASS} list={id + '-tokens'}
            value={stringValue} disabled={disabled}
            placeholder="#ffffff, transparent hoặc token" onChange={event => setValue(event.target.value)} />
          <datalist id={id + '-tokens'}>{COLOR_TOKENS.map(token => <option key={token} value={token} />)}</datalist>
          {hex && <input type="color" value={stringValue} disabled={disabled}
            aria-label={`Chọn màu cho ${field.label}`} onChange={event => setValue(event.target.value)} />}
        </div>
      );
      break;
    }
    case 'media':
      control = (
        <div className="flex flex-wrap items-center gap-2">
          <input id={id} type="text" readOnly className={INPUT_CLASS}
            value={stringValue} placeholder="Chưa chọn media" aria-label={field.label} />
          <button type="button" className={BUTTON_CLASS} disabled={disabled}
            onClick={() => onPickMedia(field.media_kind ?? 'any', stringValue)}>
            Chọn media
          </button>
          {stringValue && <button type="button" className={BUTTON_CLASS}
            disabled={disabled} onClick={() => setValue('')}>Bỏ chọn</button>}
        </div>
      );
      break;
  }
  return (
    <div className="space-y-1.5">
      <label htmlFor={id} className="block text-xs font-semibold text-zinc-800">
        {field.label}{'required' in field && field.required ? ' *' : ''}
        {responsive && <span className="ml-1 font-normal text-zinc-500">({VIEWPORT_LABELS[viewport]})</span>}
      </label>
      {control}
    </div>
  );
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
  const [breakpoint, setBreakpoint] = useState<BuilderViewport>(viewport);
  const [picker, setPicker] = useState<PickerState | null>(null);
  const latestValues = useRef(values);
  const latestOnChange = useRef(onChange);
  latestValues.current = values;
  latestOnChange.current = onChange;
  useEffect(() => setBreakpoint(viewport), [viewport]);
  const defaults = new Map(topFields(schema.fields).map(field => [field.key, primitiveDefault(field)]));
  const hasResponsive = topFields(schema.fields).some(field =>
    (field.type === 'number' || field.type === 'range' || field.type === 'select') && field.responsive);

  const patchRoot = (key: string, next: unknown) => {
    const previous = latestValues.current;
    const updated = { ...previous };
    if (next === undefined) delete updated[key];
    else updated[key] = next;
    initializeNewlyVisibleFields(schema.fields, previous, updated, defaults);
    onChange(updated);
  };

  const renderFields = (
    fields: BlockFieldSpec[], current: Record<string, unknown>,
    patch: (key: string, value: unknown) => void, prefix: string,
    path: Array<string | number>,
  ): ReactNode => fields.map(field => {
    const key = prefix + field.key;
    if (!visible(field.show_if, values, defaults)) return null;
    if (field.type === 'group') {
      return (
        <fieldset key={key} className="space-y-3 rounded-md border border-zinc-200 p-3">
          <legend className="px-1 text-sm font-semibold text-zinc-800">{field.label}</legend>
          {renderFields(field.fields, current, patch, key + '.', path)}
        </fieldset>
      );
    }
    if (field.type === 'repeater') {
      const items = Array.isArray(current[field.key]) ? current[field.key] as unknown[] : [];
      const min = field.min_items ?? 0;
      const max = field.max_items ?? 32;
      const mutate = (next: unknown[]) => patch(field.key, next);
      return (
        <fieldset key={key} className="space-y-3 rounded-md border border-zinc-200 p-3">
          <legend className="px-1 text-sm font-semibold text-zinc-800">{field.label}</legend>
          <p className="text-xs text-zinc-500">{items.length} / {max} mục{items.length < min ? ` · Cần tối thiểu ${min} mục` : ''}</p>
          {items.map((item, index) => {
            const row = isObject(item) ? item : {};
            const labelValue = field.item_label_field ? row[field.item_label_field] : undefined;
            return (
              <div key={index} className="space-y-3 rounded-md border border-zinc-200 bg-zinc-50 p-2.5">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <strong className="text-xs text-zinc-700">
                    {typeof labelValue === 'string' && labelValue ? labelValue : `Mục ${index + 1}`}
                  </strong>
                  <div className="flex flex-wrap gap-1">
                    <button type="button" className={BUTTON_CLASS} disabled={disabled || items.length >= max}
                      onClick={() => mutate([...items.slice(0, index + 1), JSON.parse(JSON.stringify(row)), ...items.slice(index + 1)])}>
                      Nhân bản
                    </button>
                    <button type="button" className={BUTTON_CLASS} disabled={disabled || index === 0}
                      onClick={() => {
                        const next = [...items];
                        [next[index - 1], next[index]] = [next[index], next[index - 1]];
                        mutate(next);
                      }}>Lên</button>
                    <button type="button" className={BUTTON_CLASS} disabled={disabled || index === items.length - 1}
                      onClick={() => {
                        const next = [...items];
                        [next[index + 1], next[index]] = [next[index], next[index + 1]];
                        mutate(next);
                      }}>Xuống</button>
                    <button type="button" className={BUTTON_CLASS} disabled={disabled || items.length <= min}
                      onClick={() => mutate(items.filter((_, idx) => idx !== index))}>Xóa</button>
                  </div>
                </div>
                {renderFields(field.fields, row, (subKey, subValue) => {
                  const updated = { ...row };
                  if (subValue === undefined) delete updated[subKey];
                  else updated[subKey] = subValue;
                  mutate(items.map((old, idx) => idx === index ? updated : old));
                }, key + '.' + index + '.', [...path, field.key, index])}
              </div>
            );
          })}
          <button type="button" className={BUTTON_CLASS} disabled={disabled || items.length >= max}
            onClick={() => mutate([...items, initialItem(field.fields)])}>+ Thêm mục</button>
        </fieldset>
      );
    }
    return (
      <FieldControl key={key} field={field} current={current[field.key]} viewport={breakpoint}
        disabled={disabled} onSet={next => patch(field.key, next)}
        onPickMedia={(kind, currentId) => setPicker({ kind, current: currentId, path: [...path, field.key] })} />
    );
  });

  return (
    <div className="space-y-4" aria-label={`Thuộc tính block ${schema.title}`}>
      {hasResponsive && (
        <fieldset className="space-y-1">
          <legend className="text-xs font-semibold text-zinc-700">Breakpoint cho trường responsive</legend>
          <div className="flex flex-wrap gap-1">
            {VIEWPORTS.map(bp => (
              <button key={bp} type="button"
                className={BUTTON_CLASS + (breakpoint === bp ? ' border-od-accent bg-zinc-100 font-semibold' : '')}
                aria-pressed={breakpoint === bp} disabled={disabled}
                onClick={() => setBreakpoint(bp)}>{VIEWPORT_LABELS[bp]}</button>
            ))}
          </div>
          <p className="text-xs text-zinc-500">Tablet và Mobile kế thừa giá trị từ màn hình rộng hơn cho đến khi đặt riêng.</p>
        </fieldset>
      )}
      {renderFields(schema.fields, values, patchRoot, 'block.', [])}
      {picker && (
        <MediaPicker mode="image" acceptKind={picker.kind} csrfToken={csrfToken}
          selectedIds={picker.current ? [picker.current] : []}
          onApply={ids => {
            if (ids[0]) {
              const next = replaceValueAtPath(latestValues.current, picker.path, ids[0]);
              latestOnChange.current(next as Record<string, unknown>);
            }
            setPicker(null);
          }}
          onClose={() => setPicker(null)} onError={onError} />
      )}
    </div>
  );
}
