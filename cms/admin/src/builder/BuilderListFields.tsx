import { useEffect, useState } from 'react';
import { ApiClientError } from '../api/client.ts';
import { listContentTypes } from '../api/content.ts';
import { listAllTaxonomyTerms, type TaxonomyTerm } from '../api/taxonomy.ts';
import type { OwnedContentType } from '../features/content/types.ts';
import type { BuilderElement, ListProps, ListSort } from './document.ts';

interface Props {
  element: BuilderElement;
  onChange: (id: string, config: ListProps) => void;
  onError: (cause: unknown, fallback: string) => void;
}

function readConfig(element: BuilderElement): ListProps {
  const p = element.props;
  return {
    type_key: String(p.type_key ?? 'post'),
    ...(typeof p.category_id === 'number' ? { category_id: p.category_id } : {}),
    ...(typeof p.tag_id === 'number' ? { tag_id: p.tag_id } : {}),
    include_descendants: p.include_descendants === true,
    sort: p.sort === 'id_asc' ? 'id_asc' : 'id_desc',
    page_size: typeof p.page_size === 'number' ? p.page_size : 6,
  };
}

/** G3: selects existing CPT and taxonomy through the authenticated Admin API.
 * Missing schema/terms and failed lookups must never clear stored configuration.
 */
export function BuilderListFields({ element, onChange, onError }: Props) {
  const config = readConfig(element);
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [categories, setCategories] = useState<TaxonomyTerm[]>([]);
  const [tags, setTags] = useState<TaxonomyTerm[]>([]);
  const [lookupError, setLookupError] = useState('');
  const [reload, setReload] = useState(0);
  const [busy, setBusy] = useState(true);

  useEffect(() => {
    let alive = true;
    const controller = new AbortController();
    setBusy(true);
    Promise.all([
      listContentTypes(),
      listAllTaxonomyTerms('category', controller.signal),
      listAllTaxonomyTerms('tag', controller.signal),
    ]).then(([available, categoryTerms, tagTerms]) => {
      if (!alive) return;
      setTypes(available);
      setCategories(categoryTerms);
      setTags(tagTerms);
      setLookupError('');
    }).catch(cause => {
      if (!alive) return;
      setLookupError('Không thể tải CPT/category/tag. Cấu hình List đã lưu vẫn được giữ nguyên.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, 'Phiên đăng nhập đã hết hạn khi tải List Block.');
    }).finally(() => { if (alive) setBusy(false); });
    return () => { alive = false; controller.abort(); };
  }, [element.id, reload]);

  const update = (changes: Partial<ListProps>) => {
    const next = { ...config, ...changes };
    if (!next.category_id) {
      delete next.category_id;
      next.include_descendants = false;
    }
    if (!next.tag_id) delete next.tag_id;
    onChange(element.id, next);
  };
  const missingType = !busy && !types.some(item =>
    item.descriptor.key === config.type_key && item.status === 'active');
  const missingCategory = !busy && config.category_id !== undefined
    && !categories.some(term => term.id === config.category_id);
  const missingTag = !busy && config.tag_id !== undefined
    && !tags.some(term => term.id === config.tag_id);

  return (
    <fieldset className="space-y-3 rounded-lg border border-sky-200 p-3" aria-label="List Block">
      <legend className="px-1 text-sm font-semibold text-sky-900">Danh sách nội dung</legend>
      <p className="text-xs text-slate-600">
        Chỉ truy vấn nội dung published; Preview/Lưu xem trang đầu. Trang công khai có phân trang riêng cho từng node.
      </p>
      <label className="block text-xs font-semibold">
        Loại nội dung
        <select className="mt-1 w-full" aria-label="List content type" value={config.type_key}
          onChange={event => update({ type_key: event.target.value })}>
          {config.type_key && !types.some(type => type.descriptor.key === config.type_key)
            && <option value={config.type_key}>{config.type_key} (không còn tồn tại)</option>}
          {types.map(type => <option key={type.descriptor.key} value={type.descriptor.key}
            disabled={type.status !== 'active'}>
            {type.descriptor.label} ({type.descriptor.key})
            {type.status !== 'active' ? ' — không hoạt động' : ''}
          </option>)}
        </select>
      </label>
      <label className="block text-xs font-semibold">
        Category
        <select className="mt-1 w-full" aria-label="List category" value={config.category_id ?? ''}
          onChange={event => update({ category_id: event.target.value ? Number(event.target.value) : undefined })}>
          <option value="">Tất cả category</option>
          {config.category_id !== undefined && !categories.some(term => term.id === config.category_id)
            && <option value={config.category_id}>ID {config.category_id} (đã mất)</option>}
          {categories.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
        </select>
      </label>
      <label className="flex items-center gap-2 text-xs font-semibold">
        <input aria-label="List include descendants" type="checkbox" disabled={!config.category_id}
          checked={config.include_descendants}
          onChange={event => update({ include_descendants: event.target.checked })} />
        Bao gồm category con
      </label>
      <label className="block text-xs font-semibold">
        Tag
        <select className="mt-1 w-full" aria-label="List tag" value={config.tag_id ?? ''}
          onChange={event => update({ tag_id: event.target.value ? Number(event.target.value) : undefined })}>
          <option value="">Tất cả tag</option>
          {config.tag_id !== undefined && !tags.some(term => term.id === config.tag_id)
            && <option value={config.tag_id}>ID {config.tag_id} (đã mất)</option>}
          {tags.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
        </select>
      </label>
      <label className="block text-xs font-semibold">
        Thứ tự
        <select className="mt-1 w-full" aria-label="List sort" value={config.sort}
          onChange={event => update({ sort: event.target.value as ListSort })}>
          <option value="id_desc">Mới nhất trước (id_desc)</option>
          <option value="id_asc">Cũ nhất trước (id_asc)</option>
        </select>
      </label>
      <label className="block text-xs font-semibold">
        Số mục/trang (1–20)
        <input className="mt-1 w-full" aria-label="List page size" type="number"
          min={1} max={20} step={1} value={config.page_size}
          onChange={event => {
            const value = Number(event.target.value);
            if (Number.isInteger(value) && (1 <= value && value <= 20)) update({ page_size: value });
          }} />
      </label>
      {(missingType || missingCategory || missingTag) &&
        <p role="status" className="text-xs text-amber-900">
          CPT hoặc category/tag tham chiếu không còn tồn tại. List hiển thị rỗng; cấu hình sẽ được giữ khi Save.
        </p>}
      {lookupError && <div className="space-y-2">
        <p role="alert" className="text-xs text-rose-700">{lookupError}</p>
        <button type="button" className="btn-secondary !py-1.5 text-xs"
          onClick={() => { setLookupError(''); setReload(value => value + 1); }}>
          Tải lại danh sách nguồn
        </button>
      </div>}
    </fieldset>
  );
}
