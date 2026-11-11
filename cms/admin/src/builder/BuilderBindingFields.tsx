import { useEffect, useRef, useState } from 'react';
import { getContentEntry, listContentEntries, listContentTypes } from '../api/content.ts';
import { ApiClientError } from '../api/client.ts';
import type { ContentEntryRecord, ContentFieldType, OwnedContentType } from '../features/content/types.ts';
import type { BindingSource, BindingTarget, BuilderElement, ElementBinding, ElementType } from './document.ts';

/** ADR-0009. Do not infer safe coercions from the displayed entry value. */
const COMPATIBLE: Record<BindingTarget, readonly ContentFieldType[]> = {
  text: ['short_text', 'long_text', 'rich_text', 'number', 'boolean', 'date', 'select'],
  media_id: ['image'],
  alt: ['short_text', 'select'],
  label: ['short_text', 'select'],
  url: ['short_text', 'select'],
};
// Heading text has a stricter matrix than core/text.
const HEADING_TEXT: readonly ContentFieldType[] = ['short_text', 'select'];
const TARGETS: Record<ElementType, readonly { key: BindingTarget; label: string }[]> = {
  'core/heading': [{ key: 'text', label: 'Tiêu đề (text)' }],
  'core/text': [{ key: 'text', label: 'Nội dung (text)' }],
  'core/image': [
    { key: 'media_id', label: 'Mã ảnh (media_id)' },
    { key: 'alt', label: 'Mô tả ảnh (alt)' },
  ],
  'core/button': [
    { key: 'label', label: 'Nhãn nút (label)' },
    { key: 'url', label: 'Đường dẫn (url)' },
  ],
};
const PAGE_SIZE = 50;

interface Props {
  element: BuilderElement;
  onBindingChange: (id: string, binding: ElementBinding | null) => void;
  onError: (error: unknown, fallback: string) => void;
}

function readBinding(element: BuilderElement): ElementBinding | null {
  const value = element.props.binding;
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const binding = value as Partial<ElementBinding>;
  return typeof binding.type_key === 'string' && typeof binding.entry_slug === 'string'
    && binding.source && typeof binding.source.kind === 'string'
    && typeof binding.target === 'string' ? binding as ElementBinding : null;
}

function sourceValue(source: BindingSource): string {
  return source.kind === 'title' ? 'title' : 'field:' + source.key;
}

function allowed(element: BuilderElement, target: BindingTarget, fieldType: ContentFieldType): boolean {
  if (element.type === 'core/heading' && target === 'text') return HEADING_TEXT.includes(fieldType);
  return COMPATIBLE[target].includes(fieldType);
}

export function BuilderBindingFields({ element, onBindingChange, onError }: Props) {
  const existing = readBinding(element);
  const signature = JSON.stringify(existing);
  const targets = TARGETS[element.type as ElementType];
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [entries, setEntries] = useState<ContentEntryRecord[]>([]);
  const [typeKey, setTypeKey] = useState(existing?.type_key ?? '');
  const [entrySlug, setEntrySlug] = useState(existing?.entry_slug ?? '');
  const [target, setTarget] = useState<BindingTarget>(existing?.target ?? targets[0].key);
  const [sourceKey, setSourceKey] = useState(existing ? sourceValue(existing.source) : '');
  const [typesBusy, setTypesBusy] = useState(false);
  const [entriesBusy, setEntriesBusy] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [lookupError, setLookupError] = useState('');
  const [lookupRevision, setLookupRevision] = useState(0);
  const [selectionChecked, setSelectionChecked] = useState(false);
  const [savedStatus, setSavedStatus] = useState('');
  const entrySequence = useRef(0);

  useEffect(() => {
    setTypeKey(existing?.type_key ?? '');
    setEntrySlug(existing?.entry_slug ?? '');
    setTarget(existing?.target ?? targets[0].key);
    setSourceKey(existing ? sourceValue(existing.source) : '');
    setLookupError('');
    setSelectionChecked(false);
    setSavedStatus('');
  }, [element.id, signature]);

  // Read-only lookup. A failed request must never erase a previously saved
  // reference or a user's local selection.
  useEffect(() => {
    let alive = true;
    setTypesBusy(true);
    listContentTypes().then(result => {
      if (alive) setTypes(result);
    }).catch(cause => {
      if (!alive) return;
      setLookupError('Không thể tải loại nội dung; liên kết đang lưu vẫn được giữ.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, 'Phiên đăng nhập hết hạn khi tra cứu nội dung.');
    }).finally(() => { if (alive) setTypesBusy(false); });
    return () => { alive = false; };
  }, [element.id, lookupRevision]);

  useEffect(() => {
    const sequence = ++entrySequence.current;
    setEntries([]);
    setHasMore(false);
    setSelectionChecked(false);
    if (!typeKey) return;
    setEntriesBusy(true);
    listContentEntries(typeKey, PAGE_SIZE, 0).then(result => {
      if (sequence !== entrySequence.current) return;
      setEntries(result);
      setHasMore(result.length === PAGE_SIZE);
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError('Không thể tải entry; lựa chọn và binding hiện tại vẫn được giữ.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, 'Phiên đăng nhập hết hạn khi tra cứu entry.');
    }).finally(() => {
      if (sequence === entrySequence.current) setEntriesBusy(false);
    });
    return () => { entrySequence.current += 1; };
  }, [typeKey, lookupRevision]);

  const selectedType = types.find(type => type.descriptor.key === typeKey);
  const typeActive = selectedType?.status === 'active';
  const selectedEntry = entries.find(entry => entry.slug === entrySlug);
  const allowedFields = selectedType?.descriptor.fields ?? [];
  const currentField = sourceKey.startsWith('field:')
    ? allowedFields.find(field => field.key === sourceKey.slice(6)) : undefined;
  const canUseTitle = target !== 'media_id' && target !== 'url';
  const sourceAllowed = sourceKey === 'title' ? canUseTitle
    : !!currentField && allowed(element, target, currentField.type);
  // A bound alt alone does not supply an image; the server requires a valid
  // static media_id when media_id is not the bound target.
  const imageAltNeedsMedia = element.type === 'core/image' && target === 'alt'
    && (typeof element.props.media_id !== 'string' || element.props.media_id.trim() === '');
  const valid = !!typeActive && !!selectedEntry && !!entrySlug && sourceAllowed
    && (selectionChecked || existing?.entry_slug === entrySlug && existing.type_key === typeKey)
    && !imageAltNeedsMedia && !entriesBusy && !typesBusy && !lookupError;

  const chooseEntry = (slug: string) => {
    setEntrySlug(slug);
    setSelectionChecked(false);
    setLookupError('');
    const entry = entries.find(candidate => candidate.slug === slug);
    if (!entry) return;
    const sequence = ++entrySequence.current;
    // Check the full record using the existing Admin endpoint; do not assume
    // stale list rows are authoritative for the selected source.
    getContentEntry(typeKey, entry.id).then(actual => {
      if (sequence !== entrySequence.current) return;
      if (actual.slug === slug && actual.content_type_key === typeKey) setSelectionChecked(true);
      else setLookupError('Entry đã đổi slug; hãy tải lại danh sách trước khi áp dụng.');
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError('Entry không thể tra cứu; lựa chọn của bạn vẫn được giữ.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, 'Phiên đăng nhập hết hạn khi đọc entry.');
    });
  };

  const loadMore = () => {
    if (entriesBusy || !hasMore) return;
    const sequence = ++entrySequence.current;
    setEntriesBusy(true);
    listContentEntries(typeKey, PAGE_SIZE, entries.length).then(next => {
      if (sequence !== entrySequence.current) return;
      setEntries(current => [...current, ...next]);
      setHasMore(next.length === PAGE_SIZE);
    }).catch(cause => {
      if (sequence !== entrySequence.current) return;
      setLookupError('Không thể tải thêm entry; binding hiện tại không bị thay đổi.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, 'Phiên đăng nhập hết hạn khi tải thêm entry.');
    }).finally(() => {
      if (sequence === entrySequence.current) setEntriesBusy(false);
    });
  };

  const apply = () => {
    if (!valid) return;
    const source: BindingSource = sourceKey === 'title'
      ? { kind: 'title' } : { kind: 'field', key: sourceKey.slice(6) };
    onBindingChange(element.id, { type_key: typeKey, entry_slug: entrySlug, source, target });
    setSavedStatus('Đã áp dụng vào bản nháp. Bấm Preview hoặc Lưu homepage để kiểm tra.');
  };

  return (
    <fieldset className="space-y-3 rounded-lg border border-indigo-200 p-3" aria-label="Field Binding">
      <legend className="px-1 text-sm font-semibold text-indigo-900">Field Binding</legend>
      <p className="text-xs text-slate-600">Liên kết dùng entry theo slug, không sao chép giá trị field. Preview có thể xem draft; trang công khai chỉ hiển thị published.</p>
      <label className="block text-xs font-medium">
        Thuộc tính đích
        <select aria-label="Binding target" className="mt-1 w-full" value={target}
          onChange={event => {
            setTarget(event.target.value as BindingTarget);
            setLookupError('');
          }}>
          {targets.map(option => <option value={option.key} key={option.key}>{option.label}</option>)}
        </select>
      </label>
      <label className="block text-xs font-medium">
        Loại nội dung
        <select aria-label="Binding content type" className="mt-1 w-full" value={typeKey}
          onChange={event => {
            setTypeKey(event.target.value);
            setEntrySlug('');
            setSourceKey('');
            setLookupError('');
            setSavedStatus('');
          }}>
          <option value="">Chọn Post / Page / CPT</option>
          {typeKey && !types.some(item => item.descriptor.key === typeKey)
            && <option value={typeKey}>{typeKey} (không còn tồn tại)</option>}
          {types.map(item => <option key={item.descriptor.key} value={item.descriptor.key}
            disabled={item.status !== 'active'}>{item.descriptor.label} ({item.descriptor.key}){item.status !== 'active' ? ' — ngừng hoạt động' : ''}</option>)}
        </select>
      </label>
      {typeKey && !typeActive && !typesBusy &&
        <p role="status" className="text-xs text-amber-800">CPT không tồn tại hoặc provider không hoạt động. Binding đã lưu vẫn được giữ.</p>}
      <label className="block text-xs font-medium">
        Entry
        <select aria-label="Binding entry" className="mt-1 w-full" value={entrySlug}
          disabled={!typeActive} onChange={event => chooseEntry(event.target.value)}>
          <option value="">Chọn entry</option>
          {entrySlug && !entries.some(entry => entry.slug === entrySlug)
            && <option value={entrySlug}>{entrySlug} (không có trong danh sách đã tải)</option>}
          {entries.map(entry => <option key={entry.id} value={entry.slug}>{entry.title} ({entry.slug})</option>)}
        </select>
      </label>
      {hasMore && <button type="button" className="btn-secondary !py-1.5 text-xs" disabled={entriesBusy}
        onClick={loadMore}>Tải thêm entry</button>}
      {entrySlug && !selectedEntry && !entriesBusy &&
        <p role="status" className="text-xs text-amber-800">Không tìm thấy entry trong danh sách đã tải. Có thể đã đổi slug hoặc bị xóa; không tự xóa binding.</p>}
      <label className="block text-xs font-medium">
        Nguồn
        <select aria-label="Binding source field" className="mt-1 w-full" value={sourceKey}
          onChange={event => { setSourceKey(event.target.value); setLookupError(''); }}>
          <option value="">Chọn trường tương thích</option>
          {sourceKey && sourceKey !== 'title' && !allowedFields.some(field => 'field:' + field.key === sourceKey)
            && <option value={sourceKey} disabled>{sourceKey} (field đã mất)</option>}
          <option value="title" disabled={!canUseTitle}>Tiêu đề entry (title){!canUseTitle ? ' — không tương thích' : ''}</option>
          {allowedFields.map(field => {
            const compatible = allowed(element, target, field.type);
            return <option key={field.key} value={'field:' + field.key} disabled={!compatible}>
              {field.label} ({field.key}, {field.type}){!compatible ? ' — không tương thích' : ''}
            </option>;
          })}
        </select>
      </label>
      {sourceKey && !sourceAllowed &&
        <p role="status" className="text-xs text-amber-800">Field không tương thích với thuộc tính đích. Không thể áp dụng cấu hình này.</p>}
      {imageAltNeedsMedia &&
        <p role="status" className="text-xs text-amber-800">Binding cho alt yêu cầu chọn Media ID tĩnh hợp lệ trước.</p>}
      {lookupError && <div className="space-y-2">
        <p role="alert" className="text-xs text-rose-700">{lookupError}</p>
        <button type="button" className="btn-secondary !py-1.5 text-xs" onClick={() => {
          setLookupError('');
          setLookupRevision(value => value + 1);
        }}>Thử tải lại nguồn</button>
      </div>}
      {savedStatus && <p role="status" className="text-xs text-emerald-800">{savedStatus}</p>}
      <div className="flex flex-wrap gap-2">
        <button type="button" className="btn-secondary !py-1.5 text-xs" disabled={!valid}
          onClick={apply}>Áp dụng binding</button>
        {existing && <button type="button" className="btn-secondary !py-1.5 text-xs"
          onClick={() => {
            onBindingChange(element.id, null);
            setSavedStatus('Đã gỡ binding khỏi bản nháp; giá trị tĩnh giữ nguyên.');
          }}>Gỡ binding</button>}
      </div>
    </fieldset>
  );
}
