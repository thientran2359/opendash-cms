import { useState } from 'react';
import { MediaPicker } from '../features/media/MediaPicker.tsx';
import type { BuilderDocument, BuilderElement } from './document.ts';
import { isElement, isLockedElement, locate, nodeTitle } from './document.ts';

interface Props {
  document: BuilderDocument;
  selectedId: string | null;
  registry: Record<string, Record<string, string>>;
  csrfToken: string | null;
  onChange: (id: string, key: string, value: string | number) => void;
  onError: (error: unknown, fallback: string) => void;
}

export function BuilderInspector({ document, selectedId, registry, csrfToken, onChange, onError }: Props) {
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
  return (
    <section className="space-y-4 rounded-xl border border-slate-200 bg-white p-4" aria-label="Inspector">
      <div>
        <h2 className="!m-0 text-base font-semibold text-slate-900">Thuộc tính</h2>
        <p className="mt-1 text-sm text-slate-600">{nodeTitle(item)}</p>
        <code className="block break-all text-[11px] text-slate-400">{item.node.id}</code>
      </div>
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
        Node bố cục hiện dùng thiết lập mặc định. Chọn các nút trong Tree để thêm, xóa hoặc đổi thứ tự.
      </p>}
      {element && !locked && (
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
