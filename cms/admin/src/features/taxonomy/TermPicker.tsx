import { useEffect, useId, useMemo, useRef, useState } from 'react';
import {
  listAllTaxonomyTerms,
  type EntryTermFilters,
  type TaxonomyKind,
  type TaxonomyTerm,
} from '../../api/taxonomy.ts';

interface TermsProps {
  userId: number | null;
  disabled?: boolean;
  onApiError: (error: unknown, fallback: string) => void;
}

interface TermPickerProps extends TermsProps {
  selectedIds: number[];
  onChange: (ids: number[]) => void;
}

interface TermFiltersProps extends TermsProps {
  value: EntryTermFilters;
  onChange: (value: EntryTermFilters) => void;
}

const fieldClass = 'block w-full min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600 disabled:opacity-50';
const maxTerms: Record<TaxonomyKind, number> = { category: 32, tag: 64 };

function sortTerms(a: TaxonomyTerm, b: TaxonomyTerm): number {
  return a.name.localeCompare(b.name, 'vi') || a.id - b.id;
}

/** Order categories by parent without hiding an orphan if the tree changed mid-fetch. */
function categoryTree(terms: TaxonomyTerm[]): Array<{ term: TaxonomyTerm; depth: number }> {
  const byParent = new Map<number | null, TaxonomyTerm[]>();
  const known = new Set(terms.map(term => term.id));
  for (const term of terms) {
    const parent = term.parent_id !== null && known.has(term.parent_id) ? term.parent_id : null;
    byParent.set(parent, [...(byParent.get(parent) ?? []), term]);
  }
  for (const siblings of byParent.values()) siblings.sort(sortTerms);
  const result: Array<{ term: TaxonomyTerm; depth: number }> = [];
  const visited = new Set<number>();
  const visit = (parentId: number | null, depth: number) => {
    for (const term of byParent.get(parentId) ?? []) {
      if (visited.has(term.id)) continue;
      visited.add(term.id);
      result.push({ term, depth });
      visit(term.id, depth + 1);
    }
  };
  visit(null, 0);
  for (const term of [...terms].sort(sortTerms)) {
    if (visited.has(term.id)) continue;
    visited.add(term.id);
    result.push({ term, depth: 0 });
    visit(term.id, 1);
  }
  return result;
}

function useTermOptions(userId: number | null, onApiError: TermsProps['onApiError']) {
  const [categories, setCategories] = useState<TaxonomyTerm[]>([]);
  const [tags, setTags] = useState<TaxonomyTerm[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;

  useEffect(() => {
    if (userId === null) {
      setCategories([]);
      setTags([]);
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    setLoading(true);
    setError('');
    Promise.all([
      listAllTaxonomyTerms('category', controller.signal),
      listAllTaxonomyTerms('tag', controller.signal),
    ])
      .then(([nextCategories, nextTags]) => {
        if (controller.signal.aborted) return;
        setCategories(nextCategories);
        setTags(nextTags);
      })
      .catch(cause => {
        if (controller.signal.aborted) return;
        setError(cause instanceof Error ? cause.message : 'Không thể tải category/tag.');
        onErrorRef.current(cause, 'Không thể tải category/tag.');
      })
      .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
  }, [userId, revision]);

  return { categories, tags, loading, error, reload: () => setRevision(value => value + 1) };
}

/** Native checkboxes preserve keyboard navigation and leave unknown saved IDs removable. */
export function TermPicker({ userId, disabled, selectedIds, onChange, onApiError }: TermPickerProps) {
  const { categories, tags, loading, error, reload } = useTermOptions(userId, onApiError);
  const searchId = useId();
  const [search, setSearch] = useState('');
  const [selectionError, setSelectionError] = useState('');
  const allTerms = useMemo(() => [...categories, ...tags], [categories, tags]);
  const known = useMemo(() => new Map(allTerms.map(term => [term.id, term])), [allTerms]);
  const selected = useMemo(() => new Set(selectedIds), [selectedIds]);
  const categoryCount = selectedIds.filter(id => known.get(id)?.kind === 'category').length;
  const tagCount = selectedIds.filter(id => known.get(id)?.kind === 'tag').length;
  const unknownIds = selectedIds.filter(id => !known.has(id));
  const normalizedSearch = search.trim().toLocaleLowerCase('vi');

  const toggle = (id: number, checked: boolean) => {
    const term = known.get(id);
    if (checked && term) {
      const count = term.kind === 'category' ? categoryCount : tagCount;
      if (count >= maxTerms[term.kind]) {
        setSelectionError(`Tối đa ${maxTerms[term.kind]} ${term.kind === 'category' ? 'category' : 'tag'} cho một nội dung.`);
        return;
      }
    }
    setSelectionError('');
    onChange([...new Set(checked ? [...selectedIds, id] : selectedIds.filter(value => value !== id))]
      .sort((a, b) => a - b));
  };

  const groups: Array<{ kind: TaxonomyKind; title: string; terms: Array<{ term: TaxonomyTerm; depth: number }>; count: number }> = [
    { kind: 'category', title: 'Danh mục', terms: categoryTree(categories), count: categoryCount },
    { kind: 'tag', title: 'Thẻ', terms: [...tags].sort(sortTerms).map(term => ({ term, depth: 0 })), count: tagCount },
  ];

  return (
    <section className="min-w-0 space-y-3" aria-label="Gắn danh mục và thẻ">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-slate-600">Chọn tối đa 32 danh mục và 64 thẻ.</p>
        <button type="button" className="btn-secondary" onClick={reload} disabled={disabled || loading}>
          Tải lại danh mục/thẻ
        </button>
      </div>
      <div>
        <label htmlFor={searchId} className="mb-1 block text-sm font-medium text-slate-700">Tìm danh mục hoặc thẻ</label>
        <input id={searchId} type="search" value={search} onChange={event => setSearch(event.target.value)}
          placeholder="Tìm theo tên hoặc slug" disabled={disabled} className={fieldClass} />
      </div>
      {loading && <p role="status" className="text-sm text-slate-600">Đang tải category/tag…</p>}
      {error && (
        <div role="alert" className="space-y-2 text-sm text-red-700">
          <p>{error} Các ID đã chọn vẫn được giữ trong bản nháp.</p>
          <button type="button" className="btn-secondary" onClick={reload} disabled={disabled}>Thử tải lại</button>
        </div>
      )}
      {selectionError && <p role="alert" className="text-sm text-red-700">{selectionError}</p>}
      <div className="grid min-w-0 grid-cols-1 gap-3 lg:grid-cols-2">
        {groups.map(group => {
          const visible = group.terms.filter(({ term }) =>
            !normalizedSearch || term.name.toLocaleLowerCase('vi').includes(normalizedSearch) ||
            term.slug.toLocaleLowerCase('vi').includes(normalizedSearch));
          return (
            <fieldset key={group.kind} disabled={disabled} className="min-w-0 rounded-lg border border-slate-200 p-3">
              <legend className="px-1 text-sm font-semibold text-slate-800">
                {group.title} ({group.count}/{maxTerms[group.kind]})
              </legend>
              <div className="max-h-56 space-y-1 overflow-y-auto">
                {visible.map(({ term, depth }) => (
                  <label key={term.id} className="flex min-w-0 cursor-pointer items-start gap-2 rounded px-1 py-1 text-sm hover:bg-slate-50">
                    <input type="checkbox" className="mt-1 accent-blue-600" checked={selected.has(term.id)}
                      onChange={event => toggle(term.id, event.target.checked)}
                      disabled={disabled || (!selected.has(term.id) && group.count >= maxTerms[group.kind])} />
                    <span className="min-w-0 break-words" style={{ paddingLeft: `${Math.min(depth, 8) * 10}px` }}>
                      <span>{term.name}</span> <span className="text-slate-500">({term.slug})</span>
                    </span>
                  </label>
                ))}
                {!loading && visible.length === 0 && (
                  <p className="text-sm text-slate-500">{normalizedSearch ? 'Không có mục khớp tìm kiếm.' : 'Chưa có mục nào.'}</p>
                )}
              </div>
            </fieldset>
          );
        })}
      </div>
      {unknownIds.length > 0 && (
        <fieldset disabled={disabled} className="min-w-0 rounded-lg border border-amber-300 p-3">
          <legend className="px-1 text-sm font-medium text-amber-900">
            {loading ? 'ID đã chọn (đang tải danh sách)' : 'ID đã lưu chưa tìm thấy'}
          </legend>
          <p className="mb-2 text-xs text-amber-900">
            {loading ? 'Lựa chọn của bạn được giữ nguyên trong khi tải.' : 'Mục có thể đã bị xóa hoặc không còn truy cập được. Bỏ chọn nếu không cần liên kết này.'}
          </p>
          <div className="flex flex-wrap gap-3">
            {unknownIds.map(id => (
              <label key={id} className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked onChange={() => toggle(id, false)} disabled={disabled} />
                ID #{id}
              </label>
            ))}
          </div>
        </fieldset>
      )}
      <p className="text-xs text-slate-600">Chỉ gắn mục đã có. Mục chưa lưu sẽ không đổi dữ liệu trên website cho tới khi bạn bấm Lưu.</p>
    </section>
  );
}

/** Same filter controls for Post, Page and CPT; the API applies them before LIMIT/OFFSET. */
export function TermFilters({ userId, disabled, value, onChange, onApiError }: TermFiltersProps) {
  const { categories, tags, loading, error, reload } = useTermOptions(userId, onApiError);
  const categoryId = useId();
  const tagId = useId();
  const descendantId = useId();
  const ordered = useMemo(() => categoryTree(categories), [categories]);
  const missingCategory = value.categoryId != null && !categories.some(term => term.id === value.categoryId);
  const missingTag = value.tagId != null && !tags.some(term => term.id === value.tagId);
  return (
    <div className="min-w-0 space-y-2" aria-label="Bộ lọc danh mục và thẻ">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs text-slate-600">Lọc kết quả trên máy chủ trước khi phân trang.</span>
        <button type="button" className="btn-secondary" onClick={reload} disabled={disabled || loading}>
          Tải lại danh mục/thẻ
        </button>
      </div>
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
        <label htmlFor={categoryId} className="block min-w-0 text-sm font-medium text-slate-700">
          Danh mục
          <select id={categoryId} className={`mt-1 ${fieldClass}`} value={value.categoryId ?? ''}
            disabled={disabled || loading} onChange={event => onChange({
              ...value, categoryId: event.target.value ? Number(event.target.value) : null,
              includeDescendants: event.target.value ? value.includeDescendants : false,
            })}>
            <option value="">Tất cả danh mục</option>
            {missingCategory && <option value={value.categoryId!}>ID #{value.categoryId} (không còn trong danh sách)</option>}
            {ordered.map(({ term, depth }) => (
              <option key={term.id} value={term.id}>{'— '.repeat(Math.min(depth, 8))}{term.name} ({term.slug})</option>
            ))}
          </select>
        </label>
        <label htmlFor={tagId} className="block min-w-0 text-sm font-medium text-slate-700">
          Thẻ
          <select id={tagId} className={`mt-1 ${fieldClass}`} value={value.tagId ?? ''}
            disabled={disabled || loading} onChange={event => onChange({
              ...value, tagId: event.target.value ? Number(event.target.value) : null,
            })}>
            <option value="">Tất cả thẻ</option>
            {missingTag && <option value={value.tagId!}>ID #{value.tagId} (không còn trong danh sách)</option>}
            {[...tags].sort(sortTerms).map(term => <option key={term.id} value={term.id}>{term.name} ({term.slug})</option>)}
          </select>
        </label>
      </div>
      <label htmlFor={descendantId} className="flex cursor-pointer items-center gap-2 text-sm text-slate-700">
        <input id={descendantId} type="checkbox" checked={Boolean(value.includeDescendants && value.categoryId != null)}
          disabled={disabled || value.categoryId == null} onChange={event => onChange({ ...value, includeDescendants: event.target.checked })} />
        Bao gồm danh mục con
      </label>
      {loading && <p role="status" className="text-xs text-slate-500">Đang tải bộ lọc…</p>}
      {error && (
        <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-red-700">
          {error} <button type="button" className="btn-secondary" onClick={reload} disabled={disabled}>Thử tải lại</button>
        </div>
      )}
    </div>
  );
}
