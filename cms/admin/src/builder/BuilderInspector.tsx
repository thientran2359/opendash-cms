import { useState } from 'react';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import type { BuilderDocument, BuilderElement, BuilderViewport, ElementBinding, ListProps, ResponsiveAlign, ResponsiveField, ResponsiveContainer } from './document.ts';
import { effectiveResponsive, isElement, isLockedElement, locate, nodeTitle, RESPONSIVE_FIELDS, VIEWPORT_LABELS } from './document.ts';
import { BuilderBindingFields } from './BuilderBindingFields.tsx';
import { BuilderListFields } from './BuilderListFields.tsx';
import { BuilderContactFormFields } from './BuilderContactFormFields.tsx';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  registry: Record<string, Record<string, string>>;
  csrfToken: string | null;
  viewport: BuilderViewport;
  allowRouteContext?: boolean;
  onChange: (id: string, key: string, value: string | number) => void;
  onBindingChange: (id: string, binding: ElementBinding | null) => void;
  onListChange: (id: string, config: ListProps) => void;
  onResponsiveChange: (id: string, viewport: BuilderViewport, field: ResponsiveField, value: number | ResponsiveAlign | null) => void;
  onError: (error: unknown, fallback: string) => void;
}

export function BuilderInspector({ document, selectedId, registry, csrfToken, viewport, allowRouteContext, onChange, onBindingChange, onListChange, onResponsiveChange, onError }: Props) {
  const [showPicker, setShowPicker] = useState(false);
  const item = locate(document, selectedId);
  if (!item) return (
    <section className="rounded-xl border border-slate-200 bg-white p-4" aria-label="Inspector">
      <h2 className="!mt-0 text-base font-semibold">Thuộc tính</h2>
      <p className="text-sm text-slate-500">Chọn một node trong Tree hoặc Canvas để chỉnh sửa.</p>
    </section>
  );
  const element = isElement(item.node) ? item.node : null;
  const locked = element ? isLockedElement(element, registry) : false;
  const fields: Array<{ key: string; title: string; kind?: 'number' | 'textarea' }> = [];
  if (element) {
    switch (element.type) {
      case 'core/heading':
        fields.push({ key: 'level', title: 'Cấp tiêu đề (1–6)', kind: 'number' },
          { key: 'text', title: 'Tiêu đề' });
        break;
      case 'core/text':
        fields.push({ key: 'text', title: 'Nội dung', kind: 'textarea' });
        break;
      case 'core/image':
        fields.push({ key: 'media_id', title: 'Media ID' }, { key: 'alt', title: 'Mô tả ảnh (alt)' });
        break;
      case 'core/button':
        fields.push({ key: 'label', title: 'Nhãn nút' }, { key: 'url', title: 'Đường dẫn' });
        break;
      case 'legacy/block': {
        const blockId = String(element.props.block_id ?? '');
        fields.push(...Object.entries(registry[blockId] || {}).map(([key, title]) => ({ key, title })));
        break;
      }
    }
  }
  const staticValue = (node: BuilderElement, key: string): string => {
    const values = node.type === 'legacy/block' ? node.props.values : node.props;
    const current = values && typeof values === 'object' && !Array.isArray(values)
      ? (values as Record<string, unknown>)[key] : undefined;
    return typeof current === 'string' || typeof current === 'number' ? String(current) : '';
  };
  const binding = element?.props.binding;
  const responsiveNode = element ? null : item.node as ResponsiveContainer;
  const responsiveFields = item.kind === 'element' ? [] : RESPONSIVE_FIELDS[item.kind];
  const direct = responsiveNode?.responsive?.[viewport] ?? {};
  const inherited = responsiveNode ? effectiveResponsive(responsiveNode,
    viewport === 'mobile' ? 'tablet' : 'desktop') : {};
  const fieldNames: Record<ResponsiveField, string> = {
    padding_px: 'Padding (px)', gap_px: 'Gap (px)',
    columns_per_row: 'Số cột mỗi hàng', text_align: 'Căn chữ',
  };
  const defaults: Record<ResponsiveField, number | string> = {
    padding_px: 0, gap_px: 0, columns_per_row: 1, text_align: 'start',
  };
  return (
    <section className="space-y-4 rounded-xl border border-slate-200 bg-white p-4" aria-label="Inspector">
      <div>
        <h2 className="!m-0 text-base font-semibold text-slate-900">Thuộc tính</h2>
        <p className="mt-1 text-sm text-slate-600">{nodeTitle(item)}</p>
        <code className="block break-all text-[11px] text-slate-400">{item.node.id}</code>
      </div>
      {responsiveNode && (
        <fieldset className="space-y-3 rounded-lg border border-slate-200 p-3" aria-label="Responsive overrides">
          <legend className="px-1 text-sm font-semibold text-slate-800">
            Responsive · {VIEWPORT_LABELS[viewport]}
          </legend>
          <p className="text-xs text-slate-500">
            Để trống sẽ kế thừa từng thuộc tính từ viewport rộng hơn. Giá trị 0 là hợp lệ.
          </p>
          {responsiveFields.map(field => {
            const directValue = direct[field];
            const inheritedValue = inherited[field] ?? defaults[field];
            const isText = field === 'text_align';
            return <div key={field} className="space-y-1">
              <label htmlFor={'responsive-' + field} className="block text-xs font-semibold text-slate-700">
                {fieldNames[field]}
              </label>
              <div className="flex flex-wrap items-center gap-2">
                {isText ? <select
                  id={'responsive-' + field} aria-label={fieldNames[field]}
                  className="min-w-0 flex-1 text-sm"
                  value={directValue ?? ''}
                  onChange={event => onResponsiveChange(item.node.id, viewport, field,
                    event.target.value === '' ? null : event.target.value as ResponsiveAlign)}>
                  <option value="">Kế thừa ({inheritedValue})</option>
                  <option value="start">Start</option><option value="center">Center</option><option value="end">End</option>
                </select> : <input
                  id={'responsive-' + field} aria-label={fieldNames[field]}
                  className="min-w-0 flex-1 text-sm" type="number" step={1}
                  min={field === 'columns_per_row' ? 1 : 0}
                  max={field === 'columns_per_row' ? 6 : 64}
                  placeholder={'Kế thừa: ' + inheritedValue}
                  value={directValue ?? ''}
                  onChange={event => {
                    const raw = event.target.value;
                    if (raw === '') return onResponsiveChange(item.node.id, viewport, field, null);
                    const number = Number(raw);
                    if (Number.isInteger(number)
                        && number >= (field === 'columns_per_row' ? 1 : 0)
                        && number <= (field === 'columns_per_row' ? 6 : 64))
                      onResponsiveChange(item.node.id, viewport, field, number);
                  }} />}
                {directValue !== undefined && <button type="button"
                  className="btn-secondary !px-2 !py-1.5 text-xs"
                  aria-label={'Bỏ override ' + field}
                  onClick={() => onResponsiveChange(item.node.id, viewport, field, null)}>Bỏ override</button>}
              </div>
            </div>;
          })}
        </fieldset>
      )}
      {locked && (
        <p role="status" className="rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
          Node không resolve được hoặc provider đã mất. Nội dung và vị trí được bảo toàn;
          chỉ có thể xóa sau khi xác nhận.
        </p>
      )}
      {element?.type === 'legacy/block' && !locked && (
        <p className="text-xs text-slate-500">Block theme: {String(element.props.block_id)}</p>
      )}
      {!element && <p className="text-sm text-slate-500">
        Các thay đổi responsive chỉ ghi vào document khi bấm Lưu. Thêm hoặc đổi thứ tự node từ Tree.
      </p>}
      {element && !locked && element.type !== 'core/list' && element.type !== 'core/contact_form' && (
        <form className="space-y-3" onSubmit={event => event.preventDefault()} aria-label="Thuộc tính phần tử">
          {fields.map(field => (
            <label key={field.key} className="block text-sm font-medium text-slate-700">
              <span className="mb-1 block">{field.title}</span>
              {field.kind === 'textarea' ? (
                <textarea className="w-full" rows={6} value={staticValue(element, field.key)}
                  maxLength={16384} onChange={event => onChange(element.id, field.key, event.target.value)} />
              ) : (
                <input className="w-full" type={field.kind === 'number' ? 'number' : 'text'}
                  min={field.kind === 'number' ? 1 : undefined}
                  max={field.kind === 'number' ? 6 : undefined}
                  maxLength={field.key === 'url' ? 2048 : field.key === 'label' ? 200 : 512}
                  value={staticValue(element, field.key)}
                  onChange={event => onChange(element.id, field.key, field.kind === 'number'
                    ? Number(event.target.value) : event.target.value)} />
              )}
            </label>
          ))}
          {element.type === 'core/image' && (
            <button type="button" className="btn-secondary w-full"
              onClick={() => setShowPicker(true)}>Chọn ảnh trong thư viện</button>
          )}
          {fields.length === 0 && <p className="text-xs text-slate-500">Block không có field có thể chỉnh.</p>}
        </form>
      )}
      {element && !locked && ['core/heading', 'core/text', 'core/image', 'core/button'].includes(element.type) && (
        <BuilderBindingFields key={element.id} element={element} allowRouteContext={allowRouteContext} onBindingChange={onBindingChange} onError={onError} />
      )}
      {element?.type === 'core/list' && !locked && (
        <BuilderListFields key={element.id} element={element} onChange={onListChange} onError={onError} />
      )}
      {element?.type === 'core/contact_form' && !locked && (
        <BuilderContactFormFields key={element.id} element={element} onChange={onChange} onError={onError} />
      )}
      {binding != null && (
        <div className="rounded-lg border border-indigo-200 bg-indigo-50 p-3 text-xs text-indigo-900">
          <strong>Field Binding hiện có</strong>
          <p className="mt-1">Thông tin liên kết được giữ nguyên khi chỉnh thuộc tính tĩnh.</p>
          <pre className="mt-2 overflow-auto whitespace-pre-wrap break-all">{JSON.stringify(binding, null, 2)}</pre>
        </div>
      )}
      {showPicker && element?.type === 'core/image' && (
        <MediaPicker mode="image" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'media_id') ? [staticValue(element, 'media_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'media_id', ids[0]);
            setShowPicker(false);
          }}
          onClose={() => setShowPicker(false)}
          onError={onError} />
      )}
    </section>
  );
}
