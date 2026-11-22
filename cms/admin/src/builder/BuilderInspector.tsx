import { useState } from 'react';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import type { BlockCatalogItemDto, BlockPropValue } from '../types.ts';
import type { BuilderDocument, BuilderElement, BuilderViewport, ElementBinding, ListProps, ResponsiveAlign, ResponsiveField, ResponsiveContainer } from './document.ts';
import {
  VALID_TEMPLATE_PART_KEY,
  canDeleteElement,
  effectiveResponsive,
  isElement,
  isLockedElement,
  locate,
  nodeTitle,
  RESPONSIVE_FIELDS,
  VIEWPORT_LABELS,
} from './document.ts';
import { BuilderBindingFields } from './BuilderBindingFields.tsx';
import { BuilderListFields } from './BuilderListFields.tsx';
import { BuilderContactFormFields } from './BuilderContactFormFields.tsx';
import { BlockSchemaForm } from './BlockSchemaForm.tsx';
import type { PatternRecord } from '../api/patterns.ts';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  registry: Record<string, Record<string, string>>;
  blockSchemas?: BlockCatalogItemDto[];
  csrfToken: string | null;
  viewport: BuilderViewport;
  allowRouteContext?: boolean;
  requiresContentSlot?: boolean;
  availablePartKeys?: string[];
  patterns: PatternRecord[];
  patternsLoaded: boolean;
  onRefreshPatterns: () => void;
  onDeleteNode?: (id: string) => void;
  onChange: (id: string, key: string, value: BlockPropValue | undefined) => void;
  onBindingChange: (id: string, binding: ElementBinding | null) => void;
  onListChange: (id: string, config: ListProps) => void;
  onResponsiveChange: (id: string, viewport: BuilderViewport, field: ResponsiveField, value: number | ResponsiveAlign | null) => void;
  onError: (error: unknown, fallback: string) => void;
}

const INSPECTOR_INPUT_CLASS =
  'h-8 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-od-accent';

export function BuilderInspector({
  document,
  selectedId,
  registry,
  blockSchemas = [],
  csrfToken,
  viewport,
  allowRouteContext,
  requiresContentSlot = false,
  availablePartKeys,
  patterns,
  patternsLoaded,
  onRefreshPatterns,
  onDeleteNode,
  onChange,
  onBindingChange,
  onListChange,
  onResponsiveChange,
  onError,
}: Props) {
  const [showPicker, setShowPicker] = useState<'image' | 'video' | 'poster' | null>(null);
  const item = locate(document, selectedId);
  if (!item) return (
    <section className="rounded-xl border border-slate-200 bg-white p-4" aria-label="Inspector">
      <h2 className="!mt-0 text-base font-semibold">Thuộc tính</h2>
      <p className="text-sm text-slate-500">Chọn một node trong Tree hoặc Canvas để chỉnh sửa.</p>
    </section>
  );
  const element = isElement(item.node) ? item.node : null;
  const block = element?.type === 'plugin/block'
    ? blockSchemas.find(item => item.block_id === element.props?.block_id) : undefined;
  const locked = element ? isLockedElement(element, registry, blockSchemas) : false;
  const deletable = canDeleteElement(document, item.node.id, { requiresContentSlot });
  const suggestedPartKeys = [...new Set([...(availablePartKeys ?? []), 'header', 'footer'])];
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
  const binding = element?.props?.binding;
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
        <p className="mt-1 text-sm text-slate-600">{nodeTitle(item, blockSchemas)}</p>
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
                  className={INSPECTOR_INPUT_CLASS + ' min-w-0 flex-1'}
                  value={directValue ?? ''}
                  onChange={event => onResponsiveChange(item.node.id, viewport, field,
                    event.target.value === '' ? null : event.target.value as ResponsiveAlign)}>
                  <option value="">Kế thừa ({inheritedValue})</option>
                  <option value="start">Start</option><option value="center">Center</option><option value="end">End</option>
                </select> : <input
                  id={'responsive-' + field} aria-label={fieldNames[field]}
                  className={INSPECTOR_INPUT_CLASS + ' min-w-0 flex-1'} type="number" step={1}
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
                  className="btn-secondary rounded-md border border-zinc-300 bg-white px-2 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-50"
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
      {element?.type === 'plugin/block' && !locked && block && (
        <div className="space-y-2" aria-label="Thuộc tính block khai báo">
          <p className="break-all text-xs text-slate-600">{block.block_id}</p>
          <BlockSchemaForm key={element.id + ':' + block.block_id} schema={block.schema}
            values={element.props.values as Record<string, unknown>}
            onChange={values => onChange(element.id, 'values', values as BlockPropValue)}
            viewport={viewport} csrfToken={csrfToken} onError={onError} />
        </div>
      )}
      {!element && <p className="text-sm text-slate-500">
        Các thay đổi responsive chỉ ghi vào document khi bấm Lưu. Thêm hoặc đổi thứ tự node từ Tree.
      </p>}
      {element && !locked && ![
        'core/list',
        'core/contact_form',
        'core/video',
        'core/pattern_ref',
        'core/template_part',
        'core/content_slot',
        'plugin/block',
      ].includes(element.type) && (
        <form className="space-y-3" onSubmit={event => event.preventDefault()} aria-label="Thuộc tính phần tử">
          {fields.map(field => (
            <label key={field.key} className="block text-sm font-medium text-slate-700">
              <span className="mb-1 block">{field.title}</span>
              {field.kind === 'textarea' ? (
                <textarea
                  className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                  rows={6}
                  value={staticValue(element, field.key)}
                  maxLength={16384}
                  onChange={event => onChange(element.id, field.key, event.target.value)}
                />
              ) : (
                <input
                  className={INSPECTOR_INPUT_CLASS}
                  type={field.kind === 'number' ? 'number' : 'text'}
                  min={field.kind === 'number' ? 1 : undefined}
                  max={field.kind === 'number' ? 6 : undefined}
                  maxLength={field.key === 'url' ? 2048 : field.key === 'label' ? 200 : 512}
                  value={staticValue(element, field.key)}
                  onChange={event => onChange(element.id, field.key, field.kind === 'number'
                    ? Number(event.target.value) : event.target.value)}
                />
              )}
            </label>
          ))}
          {element.type === 'core/image' && (
            <button type="button" className="btn-secondary w-full rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-sm font-medium text-zinc-800 hover:bg-zinc-50"
              onClick={() => setShowPicker('image')}>Chọn ảnh trong thư viện</button>
          )}
          {fields.length === 0 && <p className="text-xs text-slate-500">Block không có field có thể chỉnh.</p>}
        </form>
      )}
      {element?.type === 'core/template_part' && !locked && (
        <div className="space-y-2" aria-label="Template Part">
          <label htmlFor="template-part-key" className="block text-sm font-medium text-slate-700">
            Part Key (ví dụ: header, footer)
          </label>
          <input
            id="template-part-key"
            list="od-template-part-keys"
            className={INSPECTOR_INPUT_CLASS + ' font-mono'}
            maxLength={64}
            value={staticValue(element, 'part_key')}
            onChange={event => onChange(element.id, 'part_key', event.target.value)}
          />
          <datalist id="od-template-part-keys">
            {suggestedPartKeys.map(key => <option key={key} value={key} />)}
          </datalist>
          {!VALID_TEMPLATE_PART_KEY.test(staticValue(element, 'part_key')) && (
            <p role="alert" className="rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-900">
              <code>part_key</code> chỉ dùng chữ thường <code>a-z</code>, số <code>0-9</code> và dấu gạch ngang <code>-</code> (bắt đầu bằng chữ cái, tối đa 64 ký tự).
            </p>
          )}
          <p className="text-xs text-slate-500">
            Chèn part tái sử dụng từ DB override hoặc thư mục <code>parts/</code> của theme v3. Có thể sửa, thay khóa hoặc xóa bình thường.
          </p>
        </div>
      )}
      {element?.type === 'core/content_slot' && !locked && (
        <div className="space-y-2 rounded-lg border border-sky-200 bg-sky-50/70 p-3 text-xs text-sky-950" aria-label="Content Slot">
          <p className="font-semibold">Khung nội dung chính (Content Slot)</p>
          <p>Đây là vị trí chèn nội dung bài viết / trang khi render template. Node này không có thuộc tính cấu hình thêm.</p>
          {requiresContentSlot && !deletable && (
            <p role="status" className="font-medium text-amber-900" data-testid="content-slot-locked-notice">
              Bắt buộc giữ ít nhất 1 <code>core/content_slot</code> cho loại template này — không thể xóa slot cuối cùng.
            </p>
          )}
        </div>
      )}
      {element?.type === 'core/pattern_ref' && !locked && (
        <div className="space-y-2" aria-label="Reusable Pattern">
          <label htmlFor="pattern-ref-id" className="block text-sm font-medium text-slate-700">Reusable Pattern</label>
          <select id="pattern-ref-id" className={INSPECTOR_INPUT_CLASS} value={Number(element.props.pattern_id) || 0}
            onChange={event => onChange(element.id, 'pattern_id', Number(event.target.value))}>
            <option value={0}>Chọn Pattern…</option>
            {patterns.map(pattern => <option key={pattern.id} value={pattern.id}>{pattern.title} (#{pattern.id})</option>)}
            {Number(element.props.pattern_id) > 0 && !patterns.some(pattern => pattern.id === element.props.pattern_id) &&
              <option value={Number(element.props.pattern_id)}>Pattern #{String(element.props.pattern_id)} · không còn tồn tại</option>}
          </select>
          <button type="button" className="btn-secondary rounded-md border border-zinc-300 bg-white px-2.5 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-50" onClick={onRefreshPatterns}>Tải lại danh sách Pattern</button>
          {Number(element.props.pattern_id) === 0 && <p role="alert" className="text-sm text-amber-800">
            Cần chọn Pattern hợp lệ trước khi lưu Builder.
          </p>}
          {patternsLoaded && Number(element.props.pattern_id) > 0
            && !patterns.some(pattern => pattern.id === element.props.pattern_id) && (
              <p role="alert" className="rounded border border-amber-300 bg-amber-50 p-2 text-sm text-amber-900">
                Pattern #{String(element.props.pattern_id)} đã bị xóa hoặc không còn tồn tại.
                Block được giữ trong Builder để bạn chọn Pattern thay thế; nội dung public sẽ bỏ qua block bị thiếu.
              </p>
            )}
          <p className="text-xs text-slate-500">Pattern đồng bộ: mọi trang tham chiếu sẽ nhận bố cục mới khi bản gốc được lưu.</p>
        </div>
      )}
      {element && !locked && ['core/heading', 'core/text', 'core/image', 'core/button'].includes(element.type) && (
        <BuilderBindingFields key={element.id} element={element} allowRouteContext={allowRouteContext} onBindingChange={onBindingChange} onError={onError} />
      )}
      {element?.type === 'core/list' && !locked && (
        <BuilderListFields key={element.id} element={element} onChange={onListChange} onError={onError} />
      )}
      {element?.type === 'core/video' && !locked && (
        <div className="space-y-3" aria-label="Thuộc tính video">
          <div className="space-y-1">
            <span className="block text-sm font-medium text-slate-700">Video MP4</span>
            <p className="break-all text-xs text-slate-600">{staticValue(element, 'media_id') || 'Chưa chọn video'}</p>
            <button type="button" className="btn-secondary w-full rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-sm font-medium text-zinc-800 hover:bg-zinc-50" onClick={() => setShowPicker('video')}>
              Chọn video MP4 trong thư viện
            </button>
          </div>
          <div className="space-y-1">
            <span className="block text-sm font-medium text-slate-700">Ảnh poster (tùy chọn)</span>
            <p className="break-all text-xs text-slate-600">{staticValue(element, 'poster_image_id') || 'Không dùng poster'}</p>
            <div className="flex gap-2">
              <button type="button" className="btn-secondary flex-1 rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm font-medium text-zinc-800 hover:bg-zinc-50" onClick={() => setShowPicker('poster')}>
                Chọn ảnh poster
              </button>
              {staticValue(element, 'poster_image_id') && (
                <button type="button" className="btn-secondary rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm font-medium text-zinc-800 hover:bg-zinc-50" onClick={() => onChange(element.id, 'poster_image_id', undefined)}>
                  Xóa
                </button>
              )}
            </div>
          </div>
          <label className="flex items-center gap-2 text-sm text-slate-700">
            <input type="checkbox" checked={element.props.controls !== false}
              onChange={event => onChange(element.id, 'controls', event.target.checked)} />
            <span>Hiển thị thanh điều khiển (controls)</span>
          </label>
          <label className="flex items-center gap-2 text-sm text-slate-700">
            <input type="checkbox" checked={element.props.muted === true}
              onChange={event => onChange(element.id, 'muted', event.target.checked)} />
            <span>Tắt tiếng mặc định (muted)</span>
          </label>
        </div>
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
      {onDeleteNode && (
        <div className="border-t border-zinc-100 pt-3">
          <button
            type="button"
            disabled={!deletable}
            onClick={() => onDeleteNode(item.node.id)}
            data-testid="inspector-delete-node-btn"
            className="w-full rounded-md border border-rose-200 bg-rose-50 px-3 py-1.5 text-xs font-medium text-rose-700 hover:bg-rose-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {deletable ? 'Xóa node này' : 'Không thể xóa Content Slot bắt buộc'}
          </button>
        </div>
      )}
      {showPicker === 'image' && element?.type === 'core/image' && (
        <MediaPicker mode="image" acceptKind="image" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'media_id') ? [staticValue(element, 'media_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'media_id', ids[0]);
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
      {showPicker === 'video' && element?.type === 'core/video' && (
        <MediaPicker mode="image" acceptKind="video" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'media_id') ? [staticValue(element, 'media_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'media_id', ids[0]);
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
      {showPicker === 'poster' && element?.type === 'core/video' && (
        <MediaPicker mode="image" acceptKind="image" csrfToken={csrfToken}
          selectedIds={staticValue(element, 'poster_image_id') ? [staticValue(element, 'poster_image_id')] : []}
          onApply={ids => {
            if (ids[0]) onChange(element.id, 'poster_image_id', ids[0]);
            setShowPicker(null);
          }}
          onClose={() => setShowPicker(null)}
          onError={onError} />
      )}
    </section>
  );
}
