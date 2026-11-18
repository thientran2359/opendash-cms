import { useEffect, useMemo, useRef, useState } from 'react';
import {
  changeContentEntryPublication, deleteContentEntry, getEffectiveContentTypeTaxonomies,
  queryContentEntryTable, getEffectiveEntryListConfig, runEntryProviderAction,
} from '../../api/content.ts';
import {
  AdminColumnCell, type EffectiveListConfig, type ProviderColumn,
} from '../extensions/AdminContributionRenderer.tsx';
import { ApiClientError } from '../../api/client.ts';
import { listAllTaxonomyTerms, listTaxonomyDefinitions, type TaxonomyDefinition, type TaxonomyTerm } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import type { EntryTableQuery, EntryTableResult, EntryTableRow } from './types.ts';

interface Props {
  typeKey: string;
  user: AuthUser;
  siteInstanceId: string;
  authGeneration: number;
  csrfToken: string | null;
  routeHash: string;
  onNavigateHash: (hash: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

const control = 'min-w-0 max-w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-800';
const action = 'rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50';
const statuses = ['all', 'draft', 'published'] as const;
type BulkAction = 'published' | 'draft' | 'delete';
type TaxonomyTableQuery = EntryTableQuery & { taxonomy_key?: string; term_id?: number };

function rootHash(typeKey: string): string {
  return typeKey === 'post' ? '#/posts' : typeKey === 'page' ? '#/pages' : `#/cpt/${encodeURIComponent(typeKey)}`;
}

function boundedInt(raw: string | null, fallback: number, min: number, max: number): number {
  if (!raw || !/^\d+$/.test(raw)) return fallback;
  const value = Number(raw);
  return Number.isSafeInteger(value) && value >= min && value <= max ? value : fallback;
}

export function parseEntryTableHash(hash: string): TaxonomyTableQuery {
  const p = new URLSearchParams(hash.split('?')[1] ?? '');
  const query: TaxonomyTableQuery = {};
  const q = p.get('q')?.trim();
  if (q && [...q].length <= 200 && !/[\u0000-\u001f\u007f]/.test(q)) query.q = q;
  const status = p.get('status');
  if (status === 'draft' || status === 'published') query.status = status;
  const category = boundedInt(p.get('category_id'), 0, 1, Number.MAX_SAFE_INTEGER);
  const tag = boundedInt(p.get('tag_id'), 0, 1, Number.MAX_SAFE_INTEGER);
  if (category) query.category_id = category;
  if (tag) query.tag_id = tag;
  if (p.has('taxonomy_key')) query.taxonomy_key = p.get('taxonomy_key') ?? '';
  if (p.has('term_id')) query.term_id = Number(p.get('term_id'));
  if (p.get('include_descendants') === 'true') query.include_descendants = true;
  const month = p.get('updated_month');
  if (month && /^\d{4}-(0[1-9]|1[0-2])$/.test(month) && month.slice(0, 4) !== '0000') {
    query.updated_month = month;
  }
  const sort = p.get('sort');
  if (sort === 'title' || sort === 'id' || sort === 'created_at' || sort === 'updated_at') query.sort = sort;
  if (p.get('order') === 'asc') query.order = 'asc';
  const limit = boundedInt(p.get('limit'), 20, 1, 100);
  if (limit !== 20) query.limit = limit;
  const offset = boundedInt(p.get('offset'), 0, 0, 1000000);
  if (offset) query.offset = offset;
  return query;
}

export function entryTableHash(base: string, filter: TaxonomyTableQuery): string {
  const p = new URLSearchParams();
  if (filter.q) p.set('q', filter.q);
  if (filter.status && filter.status !== 'all') p.set('status', filter.status);
  if (filter.category_id) p.set('category_id', String(filter.category_id));
  if (filter.tag_id) p.set('tag_id', String(filter.tag_id));
  if (filter.taxonomy_key !== undefined) p.set('taxonomy_key', filter.taxonomy_key);
  if (filter.term_id !== undefined) p.set('term_id', String(filter.term_id));
  if (filter.include_descendants) p.set('include_descendants', 'true');
  if (filter.updated_month) p.set('updated_month', filter.updated_month);
  if (filter.sort && filter.sort !== 'updated_at') p.set('sort', filter.sort);
  if (filter.order && filter.order !== 'desc') p.set('order', filter.order);
  if (filter.limit && filter.limit !== 20) p.set('limit', String(filter.limit));
  if (filter.offset) p.set('offset', String(filter.offset));
  return `${base}${p.size ? `?${p.toString()}` : ''}`;
}

/** Never render a user-supplied URL with a protocol or scheme-relative origin. */
function safePublicPath(path: string | null): string | null {
  return path && path.startsWith('/') && !path.startsWith('//') && !/[\\\u0000-\u001f\u007f]/.test(path)
    ? path : null;
}

function readableError(cause: unknown): string {
  if (cause instanceof ApiClientError && cause.status === 412) return '412 — Mục đã thay đổi. Vui lòng tải lại danh sách để lấy phiên bản mới.';
  if (cause instanceof ApiClientError && cause.status === 403) return '403 — Không có quyền thực hiện thao tác này.';
  if (cause instanceof ApiClientError && cause.status === 401) return '401 — Phiên đăng nhập không còn hiệu lực.';
  return cause instanceof Error ? cause.message : 'Yêu cầu không hoàn tất.';
}

export function EntryListPage({
  typeKey, user, siteInstanceId, authGeneration, csrfToken, routeHash, onNavigateHash, onApiError,
}: Props) {
  const base = rootHash(typeKey);
  const filters = useMemo(() => parseEntryTableHash(routeHash), [routeHash]);
  const queryHash = entryTableHash(base, filters);
  const accountScope = JSON.stringify([siteInstanceId, user.id, user.role, authGeneration, typeKey]);
  const accountScopeRef = useRef(accountScope);
  accountScopeRef.current = accountScope;
  const scope = JSON.stringify([accountScope, queryHash]);
  const scopeRef = useRef(scope);
  scopeRef.current = scope;
  const aliveRef = useRef(true);
  useEffect(() => {
    aliveRef.current = true;
    return () => { aliveRef.current = false; };
  }, []);
  const [dataState, setDataState] = useState<{ scope: string; value: EntryTableResult } | null>(null);
  const [loadingScope, setLoadingScope] = useState<string | null>(null);
  const [errorState, setErrorState] = useState<{ scope: string; message: string } | null>(null);
  const [retry, setRetry] = useState(0);
  const [termsState, setTermsState] = useState<{
    accountScope: string; category: TaxonomyTerm[]; tag: TaxonomyTerm[];
    categoryEnabled: boolean; tagEnabled: boolean;
    definitions: TaxonomyDefinition[];
    byKind: Record<string, TaxonomyTerm[]>;
  } | null>(null);
  const [termLoadErrors, setTermLoadErrors] = useState<string[]>([]);
  const [providerState, setProviderState] = useState<{
    accountScope: string; config: EffectiveListConfig;
  } | null>(null);
  const [providerError, setProviderError] = useState('');
  const [providerBusy, setProviderBusy] = useState(false);
  const providerBusyRef = useRef(false);
  const [providerNotice, setProviderNotice] = useState('');
  const [searchInput, setSearchInput] = useState(filters.q ?? '');
  const [taxonomyChoice, setTaxonomyChoice] = useState(filters.taxonomy_key ?? '');
  const [selected, setSelected] = useState<Set<number>>(() => new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const bulkBusyRef = useRef(false);
  const [bulkResults, setBulkResults] = useState<{ id: number; message: string; ok: boolean }[]>([]);
  const isBuiltIn = typeKey === 'post' || typeKey === 'page';
  const data = dataState?.scope === scope ? dataState.value : null;
  const error = errorState?.scope === scope ? errorState.message : '';
  const loading = loadingScope === scope || (!data && !error);
  const terms = termsState?.accountScope === accountScope ? termsState : null;
  const providerColumns = providerState?.accountScope === accountScope
    ? providerState.config.columns : [];

  const updateFilter = (patch: Partial<TaxonomyTableQuery>, resetPage = true) => {
    if (bulkBusyRef.current) return;
    const next = { ...filters, ...patch };
    if (resetPage) delete next.offset;
    const nextHash = entryTableHash(base, next);
    if (nextHash !== routeHash) onNavigateHash(nextHash);
  };

  useEffect(() => {
    setSearchInput(filters.q ?? '');
    setSelected(new Set());
    setBulkResults([]);
  }, [scope]);

  // A taxonomy choice alone is not a valid server filter. Keep it local until
  // a term is chosen, so the server never receives an incomplete pair.
  useEffect(() => { setTaxonomyChoice(filters.taxonomy_key ?? ''); }, [typeKey]);

  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    setDataState(null);
    setErrorState(null);
    setLoadingScope(scope);
    void queryContentEntryTable(typeKey, filters, controller.signal)
      .then(value => {
        if (!active || scopeRef.current !== scope) return;
        setDataState({ scope, value });
        setErrorState(null);
      })
      .catch(cause => {
        if (!active || controller.signal.aborted || scopeRef.current !== scope) return;
        setErrorState({ scope, message: readableError(cause) });
        if (cause instanceof ApiClientError && cause.status === 401) onApiError(cause, 'Phiên đăng nhập hết hạn.');
      })
      .finally(() => { if (active && scopeRef.current === scope) setLoadingScope(null); });
    return () => { active = false; controller.abort(); };
  }, [scope, retry]);

  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    setTermsState(null);
    setTermLoadErrors([]);
    void (async () => {
      try {
        const [keys, allDefinitions] = await Promise.all([
          getEffectiveContentTypeTaxonomies(typeKey, controller.signal),
          listTaxonomyDefinitions(controller.signal),
        ]);
        const enabledKeys = new Set(keys);
        const definitions = allDefinitions.filter(def => enabledKeys.has(def.key)
          && def.provider_status === 'active' && def.declaration_state === 'declared');
        // Do not enumerate terms for every custom taxonomy on every table
        // visit (a CPT can have 64). Load only the legacy filters and the
        // currently selected custom taxonomy, with independent failures.
        const requested = new Set(['category', 'tag', filters.taxonomy_key || taxonomyChoice]);
        const wanted = definitions.filter(def => requested.has(def.key));
        const responses = await Promise.allSettled(wanted.map(def =>
          listAllTaxonomyTerms(def.key, controller.signal)));
        const byKind: Record<string, TaxonomyTerm[]> = {};
        const errors: string[] = [];
        for (const [index, result] of responses.entries()) {
          if (result.status === 'fulfilled') byKind[wanted[index].key] = result.value;
          else if (!controller.signal.aborted) {
            errors.push(`${wanted[index].label}: ${readableError(result.reason)}`);
            if (result.reason instanceof ApiClientError && result.reason.status === 401) {
              onApiError(result.reason, 'Phiên đăng nhập hết hạn khi tải taxonomy.');
            }
          }
        }
        if (active) setTermsState({
          accountScope, definitions, byKind,
          category: byKind.category ?? [], tag: byKind.tag ?? [],
          categoryEnabled: definitions.some(def => def.key === 'category'),
          tagEnabled: definitions.some(def => def.key === 'tag'),
        });
        if (active) setTermLoadErrors(errors);
      } catch (cause) {
        if (!active || controller.signal.aborted) return;
        setTermsState({ accountScope, category: [], tag: [], categoryEnabled: false, tagEnabled: false,
          definitions: [], byKind: {} });
        onApiError(cause, 'Không thể tải bộ lọc taxonomy.');
      }
    })();
    return () => { active = false; controller.abort(); };
  }, [accountScope, filters.taxonomy_key, taxonomyChoice, retry]);

  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    setProviderState(null);
    setProviderError('');
    void getEffectiveEntryListConfig(typeKey, controller.signal)
      .then(config => {
        if (!active || controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setProviderState({ accountScope, config });
      })
      .catch(cause => {
        if (!active || controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setProviderError(readableError(cause));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, 'Phiên đăng nhập hết hạn khi tải cột provider.');
        }
      });
    return () => { active = false; controller.abort(); };
  }, [accountScope, retry]);

  useEffect(() => {
    if (data && data.total > 0 && (filters.offset ?? 0) >= data.total) {
      const limit = filters.limit ?? 20;
      updateFilter({ offset: Math.floor((data.total - 1) / limit) * limit }, false);
    }
  }, [scope, data?.total]);

  const currentRows = data?.items ?? [];
  const currentIds = currentRows.map(row => row.id);
  const selectedRows = currentRows.filter(row => selected.has(row.id));
  const allSelected = currentIds.length > 0 && currentIds.every(id => selected.has(id));
  const pageSize = filters.limit ?? 20;
  const offset = filters.offset ?? 0;

  const performBulk = async (kind: BulkAction) => {
    if (!csrfToken || bulkBusyRef.current || !selectedRows.length) return;
    if (kind !== 'delete' && !isBuiltIn) return;
    const noun = kind === 'delete' ? 'xóa vĩnh viễn'
      : kind === 'published' ? 'xuất bản' : 'chuyển về nháp';
    if (!window.confirm(`Bạn có chắc muốn ${noun} ${selectedRows.length} mục đã chọn? Mỗi mục được xử lý riêng, có thể xung đột phiên bản.`)) return;
    const startingScope = scope;
    const captured = [...selectedRows];
    bulkBusyRef.current = true;
    setBulkBusy(true);
    setBulkResults([]);
    const reports: { id: number; message: string; ok: boolean }[] = [];
    try {
      for (const row of captured) {
        // No subsequent mutation may start if an account or CPT changed.
        if (!aliveRef.current || scopeRef.current !== startingScope) break;
        try {
          if (kind === 'delete') {
            await deleteContentEntry(typeKey, row.id, row.version, csrfToken);
          } else {
            await changeContentEntryPublication(typeKey, row.id, row.version, kind, csrfToken);
          }
          reports.push({ id: row.id, message: `#${row.id}: ${noun} thành công.`, ok: true });
        } catch (cause) {
          reports.push({ id: row.id, message: `#${row.id}: ${readableError(cause)}`, ok: false });
          if (cause instanceof ApiClientError && cause.status === 401) {
            onApiError(cause, 'Phiên đăng nhập hết hạn khi cập nhật hàng loạt.');
            break;
          }
        }
      }
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      const succeeded = new Set(reports.filter(report => report.ok).map(report => report.id));
      setSelected(prev => new Set([...prev].filter(id => !succeeded.has(id))));
      setBulkResults(reports);
      setRetry(prev => prev + 1); // Always re-query real server counts, even on mixed results.
    } finally {
      bulkBusyRef.current = false;
      setBulkBusy(false);
    }
  };

  const performProviderAction = async (column: ProviderColumn, row: EntryTableRow) => {
    if (providerBusyRef.current || bulkBusyRef.current || !csrfToken || !column.action_id ||
      column.owner_kind !== 'plugin') return;
    if (!window.confirm(`Chạy action ${column.label} cho ${row.title} (#${row.id})? Action có thể thay đổi dữ liệu.`)) return;
    const startingScope = scope;
    providerBusyRef.current = true;
    setProviderBusy(true);
    setProviderNotice('');
    try {
      const response = await runEntryProviderAction(
        typeKey, row, column.owner_id, column.action_id, csrfToken,
      );
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      setProviderNotice(response.success ? `Đã chạy action cho #${row.id}.` : 'Action không thành công.');
      setRetry(old => old + 1);
    } catch (cause) {
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      setProviderNotice(`#${row.id}: ${readableError(cause)}`);
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, 'Phiên đăng nhập hết hạn khi chạy action.');
    } finally {
      providerBusyRef.current = false;
      if (aliveRef.current) setProviderBusy(false);
    }
  };

  const toggleRow = (id: number, checked: boolean) => {
    if (bulkBusyRef.current) return;
    setSelected(prev => {
      const next = new Set(prev);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };
  const sortBy = (column: NonNullable<EntryTableQuery['sort']>) => {
    const nextOrder = (filters.sort ?? 'updated_at') === column && (filters.order ?? 'desc') === 'desc'
      ? 'asc' : 'desc';
    updateFilter({ sort: column, order: nextOrder });
  };
  const editSuffix = queryHash.includes('?') ? queryHash.slice(queryHash.indexOf('?')) : '';
  const termLabels = new Map(Object.values(terms?.byKind ?? {}).flat()
    .map(term => [term.id, term.name] as const));
  const namesFor = (row: EntryTableRow, kind: string) => {
    const known = terms?.byKind[kind] ?? [];
    const ids = new Set(known.map(term => term.id));
    return row.term_ids.filter(id => ids.has(id)).map(id => termLabels.get(id) ?? `#${id}`).join(', ') || '—';
  };
  const customDefinitions = terms?.definitions.filter(def => def.key !== 'category' && def.key !== 'tag') ?? [];
  const selectedTaxonomy = filters.taxonomy_key || taxonomyChoice;
  const chosenDefinition = terms?.definitions.find(def => def.key === selectedTaxonomy);

  return (
    <section className="w-full min-w-0 space-y-5" data-testid={`entry-list-${typeKey}`} data-type-key={typeKey}>
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">Nội dung / Danh sách</p>
          <h2 className="mt-2 text-2xl font-bold text-slate-900">
            {typeKey === 'post' ? 'Bài viết' : typeKey === 'page' ? 'Trang' : `Nội dung: ${typeKey}`}
          </h2>
          <p className="mt-1 text-sm text-slate-600">Tìm kiếm và lọc trên toàn bộ dữ liệu, không chỉ trang đang xem.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button type="button" className="od-primary-action" disabled={bulkBusy}
            onClick={() => onNavigateHash(`${base}/new${editSuffix}`)}>+ Thêm mới</button>
          <button type="button" className={action} disabled={bulkBusy || loading}
            onClick={() => setRetry(prev => prev + 1)}>Tải lại</button>
        </div>
      </header>

      <div className="flex flex-wrap gap-2" aria-label="Lọc trạng thái" role="group">
        {statuses.map(status => {
          const count = data?.status_counts[status] ?? 0;
          const label = status === 'all' ? 'Tất cả' : status === 'draft' ? 'Bản nháp' : 'Đã xuất bản';
          return <button key={status} type="button" className={action}
            aria-pressed={(filters.status ?? 'all') === status}
            disabled={bulkBusy}
            onClick={() => updateFilter({ status })}>{label} ({count})</button>;
        })}
      </div>

      <form className="flex flex-wrap items-end gap-2" role="search"
        onSubmit={event => {
          event.preventDefault();
          const q = searchInput.trim().slice(0, 200);
          updateFilter({ q: q || undefined });
        }}>
        <label htmlFor={`entry-table-search-${typeKey}`} className="min-w-[180px] flex-1 text-sm font-medium text-slate-700">
          Tìm tiêu đề hoặc slug
          <input id={`entry-table-search-${typeKey}`} type="search" className={`${control} mt-1 block w-full`}
            value={searchInput} maxLength={200} disabled={bulkBusy}
            onChange={event => setSearchInput(event.target.value)}
            placeholder="Nhập từ khóa…" />
        </label>
        <button type="submit" className="od-primary-action" disabled={bulkBusy}>Tìm kiếm</button>
      </form>

      <div className="grid min-w-0 gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {terms?.categoryEnabled && <label className="min-w-0 text-sm font-medium text-slate-700">
          Danh mục
          <select aria-label="Lọc danh mục" className={`${control} mt-1 block w-full`}
            value={filters.category_id ?? ''} disabled={bulkBusy}
            onChange={event => updateFilter({
              category_id: event.target.value ? Number(event.target.value) : undefined,
              taxonomy_key: undefined, term_id: undefined,
              include_descendants: false,
            })}>
            <option value="">Mọi danh mục</option>
            {terms.category.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
          </select>
        </label>}
        {terms?.tagEnabled && <label className="min-w-0 text-sm font-medium text-slate-700">
          Thẻ
          <select aria-label="Lọc thẻ" className={`${control} mt-1 block w-full`}
            value={filters.tag_id ?? ''} disabled={bulkBusy}
            onChange={event => updateFilter({ tag_id: event.target.value ? Number(event.target.value) : undefined,
              taxonomy_key: undefined, term_id: undefined, include_descendants: false })}>
            <option value="">Mọi thẻ</option>
            {terms.tag.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
          </select>
        </label>}
        {customDefinitions.length > 0 && <label className="min-w-0 text-sm font-medium text-slate-700">
          Taxonomy tùy chỉnh
          <select aria-label="Chọn taxonomy để lọc" className={`${control} mt-1 block w-full`}
            disabled={bulkBusy} value={selectedTaxonomy}
            onChange={event => {
              setTaxonomyChoice(event.target.value);
              updateFilter({ taxonomy_key: undefined,
                term_id: undefined, category_id: undefined, tag_id: undefined, include_descendants: false });
            }}>
            <option value="">Tất cả taxonomy</option>
            {customDefinitions.map(def => <option key={def.key} value={def.key}>{def.label}</option>)}
          </select>
        </label>}
        {chosenDefinition && <label className="min-w-0 text-sm font-medium text-slate-700">
          {chosenDefinition.label}
          <select aria-label={`Lọc theo ${chosenDefinition.label}`} className={`${control} mt-1 block w-full`}
            disabled={bulkBusy} value={filters.term_id ?? ''}
            onChange={event => {
              setTaxonomyChoice(selectedTaxonomy);
              updateFilter({ taxonomy_key: event.target.value ? selectedTaxonomy : undefined,
                term_id: event.target.value ? Number(event.target.value) : undefined,
                include_descendants: false });
            }}>
            <option value="">Chọn term</option>
            {(terms?.byKind[chosenDefinition.key] ?? []).map(term =>
              <option key={term.id} value={term.id}>{term.name}</option>)}
          </select>
        </label>}
        <label className="min-w-0 text-sm font-medium text-slate-700">
          Tháng cập nhật (UTC)
          <input type="month" aria-label="Lọc tháng cập nhật" className={`${control} mt-1 block w-full`}
            value={filters.updated_month ?? ''} disabled={bulkBusy}
            onChange={event => updateFilter({ updated_month: event.target.value || undefined })} />
        </label>
        <label className="min-w-0 text-sm font-medium text-slate-700">
          Mỗi trang
          <select aria-label="Số mục mỗi trang" className={`${control} mt-1 block w-full`}
            value={pageSize} disabled={bulkBusy}
            onChange={event => updateFilter({ limit: Number(event.target.value) })}>
            {[10, 20, 50, 100].map(size => <option key={size} value={size}>{size}</option>)}
          </select>
        </label>
      </div>
      {termLoadErrors.length > 0 && <p role="alert" className="text-sm text-amber-800">
        Không tải được một số term: {termLoadErrors.join('; ')}. Các bộ lọc khác vẫn sử dụng được.
      </p>}
      {filters.category_id && terms?.categoryEnabled && <label className="flex items-center gap-2 text-sm text-slate-700">
        <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
          onChange={event => updateFilter({ include_descendants: event.target.checked })} />
        Gồm danh mục con
      </label>}
      {filters.term_id && chosenDefinition?.hierarchical && <label className="flex items-center gap-2 text-sm text-slate-700">
        <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
          onChange={event => updateFilter({ include_descendants: event.target.checked })} />
        Gồm các term con của {chosenDefinition.label}
      </label>}
      <div className="flex flex-wrap items-center gap-3 text-sm">
        <button className={action} disabled={bulkBusy} type="button"
          onClick={() => { setSearchInput(''); setTaxonomyChoice(''); onNavigateHash(base); }}>Xóa tất cả bộ lọc</button>
        <span className="text-slate-600" role="status">{data ? `Tổng: ${data.total} mục phù hợp` : 'Đang lấy số lượng…'}</span>
      </div>

      <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Thao tác hàng loạt">
        <span className="text-sm text-slate-600">{selectedRows.length} mục đã chọn trên trang</span>
        {isBuiltIn && <>
          <button className={action} type="button" disabled={bulkBusy || !csrfToken || !selectedRows.length}
            onClick={() => void performBulk('published')}>Xuất bản đã chọn</button>
          <button className={action} type="button" disabled={bulkBusy || !csrfToken || !selectedRows.length}
            onClick={() => void performBulk('draft')}>Chuyển về nháp</button>
        </>}
        <button className={action} type="button" disabled={bulkBusy || !csrfToken || !selectedRows.length}
          onClick={() => void performBulk('delete')}>Xóa đã chọn</button>
        {bulkBusy && <span role="status" className="text-sm">Đang xử lý riêng từng mục…</span>}
      </div>
      {bulkResults.length > 0 && (
        <div className="rounded-lg border border-slate-200 bg-white p-3" aria-label="Kết quả hàng loạt">
          <p className="mb-2 text-sm font-semibold">Kết quả: {bulkResults.filter(row => row.ok).length} thành công / {bulkResults.length} đã thử</p>
          <ul className="space-y-1 text-sm">
            {bulkResults.map(result => <li key={result.id} data-bulk-id={result.id}
              role={result.ok ? 'status' : 'alert'}
              className={result.ok ? 'text-emerald-800' : 'text-rose-800'}>{result.message}</li>)}
          </ul>
        </div>
      )}

      {providerNotice && <p role="status" data-testid="provider-action-result"
        className="text-sm text-slate-700">{providerNotice}</p>}
      {providerError && <div role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">
        Không thể tải cột mở rộng: {providerError}
        <button type="button" className="ml-2 underline" onClick={() => setRetry(n => n + 1)}>Thử lại</button>
      </div>}

      {error && <div role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">
        {error} <button type="button" className="underline" onClick={() => setRetry(prev => prev + 1)}>Thử lại</button>
      </div>}
      {loading && <p role="status" className="text-sm text-slate-600">Đang tải danh sách từ máy chủ…</p>}
      <div className="w-full min-w-0 max-w-full overflow-x-auto rounded-xl border border-slate-200 bg-white" aria-busy={loading}>
        <table className="w-full min-w-[780px] table-auto text-left text-sm" data-testid="entry-table">
          <caption className="sr-only">Bảng nội dung {typeKey}; tìm kiếm, trạng thái và phân trang từ máy chủ</caption>
          <thead className="border-b border-slate-200 bg-slate-50 text-slate-700">
            <tr>
              <th scope="col" className="w-10 px-3 py-3">
                <input type="checkbox" aria-label="Chọn tất cả trên trang" checked={allSelected}
                  disabled={!currentRows.length || bulkBusy || loading}
                  onChange={event => setSelected(prev => {
                    const next = new Set(prev);
                    currentIds.forEach(id => { if (event.target.checked) next.add(id); else next.delete(id); });
                    return next;
                  })} />
              </th>
              <th scope="col" className="min-w-[160px] px-3 py-3">
                <button type="button" data-testid="sort-title" className="text-left font-semibold underline-offset-2 hover:underline"
                  disabled={bulkBusy} onClick={() => sortBy('title')}
                  aria-label="Sắp xếp theo tiêu đề">Tiêu đề {(filters.sort ?? 'updated_at') === 'title' ? filters.order === 'asc' ? '↑' : '↓' : ''}</button>
              </th>
              <th scope="col" className="px-3 py-3">Trạng thái</th>
              {terms?.categoryEnabled && <th scope="col" className="px-3 py-3">Danh mục</th>}
              {terms?.tagEnabled && <th scope="col" className="px-3 py-3">Thẻ</th>}
              {customDefinitions.map(def => <th key={def.key} scope="col" className="px-3 py-3">{def.label}</th>)}
              {providerColumns.map(column => <th key={column.key} scope="col"
                className="min-w-[120px] px-3 py-3">{column.label}</th>)}
              <th scope="col" className="min-w-[135px] px-3 py-3">
                <button type="button" data-testid="sort-updated" className="font-semibold underline-offset-2 hover:underline"
                  disabled={bulkBusy} aria-label="Sắp xếp theo ngày cập nhật"
                  onClick={() => sortBy('updated_at')}>
                  Ngày cập nhật {(filters.sort ?? 'updated_at') === 'updated_at' ? (filters.order === 'asc' ? '↑' : '↓') : ''}
                </button>
              </th>
              <th scope="col" className="px-3 py-3">Hành động</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {!loading && !error && currentRows.length === 0 && <tr>
              <td colSpan={5 + (terms?.definitions.length ?? 0) + providerColumns.length}
                className="px-4 py-8 text-center text-slate-600">
                {data?.total === 0 ? 'Không tìm thấy nội dung phù hợp. Có thể xóa bộ lọc để xem lại.' : 'Không có mục trên trang này.'}
              </td>
            </tr>}
            {currentRows.map(row => {
              const path = row.publication_status === 'published' ? safePublicPath(row.public_url) : null;
              return <tr key={row.id} data-entry-id={row.id} className="align-top">
                <td className="px-3 py-3"><input type="checkbox" aria-label={`Chọn mục #${row.id}`}
                  checked={selected.has(row.id)} disabled={bulkBusy}
                  onChange={event => toggleRow(row.id, event.target.checked)} /></td>
                <td className="break-words px-3 py-3">
                  <strong className="block font-semibold text-slate-900">{row.title}</strong>
                  <span className="break-all text-xs text-slate-500">/{row.slug} · #{row.id} · v{row.version}</span>
                </td>
                <td className="px-3 py-3">{row.publication_status === 'published' ? 'Đã xuất bản' : 'Bản nháp'}</td>
                {terms?.categoryEnabled && <td className="break-words px-3 py-3">{namesFor(row, 'category')}</td>}
                {terms?.tagEnabled && <td className="break-words px-3 py-3">{namesFor(row, 'tag')}</td>}
                {customDefinitions.map(def => <td key={def.key} className="break-words px-3 py-3">{namesFor(row, def.key)}</td>)}
                {providerColumns.map(column => <td key={column.key} className="break-words px-3 py-3">
                  <AdminColumnCell column={column} row={row} busy={providerBusy || bulkBusy}
                    onRun={(col, entry) => void performProviderAction(col, entry)} />
                </td>)}
                <td className="whitespace-nowrap px-3 py-3">{row.updated_at.slice(0, 16)} UTC</td>
                <td className="px-3 py-3">
                  <div className="flex flex-wrap gap-2">
                    <button type="button" className="text-sky-700 hover:underline" disabled={bulkBusy}
                      aria-label={`Chỉnh sửa ${row.title}, ID ${row.id}`}
                      onClick={() => onNavigateHash(`${base}/${row.id}/edit${editSuffix}`)}>Chỉnh sửa</button>
                    {path && <a href={path} target="_blank" rel="noopener noreferrer" className="text-sky-700 hover:underline"
                      aria-label={`Xem công khai ${row.title}`}>Xem</a>}
                  </div>
                </td>
              </tr>;
            })}
          </tbody>
        </table>
      </div>
      <nav aria-label="Phân trang nội dung" className="flex flex-wrap items-center justify-between gap-3 text-sm text-slate-700">
        <span data-testid="entry-table-page-info">{data ? `Hiển thị ${data.total ? offset + 1 : 0}–${Math.min(offset + currentRows.length, data.total)} / ${data.total}` : 'Chưa tải trang'}</span>
        <div className="flex items-center gap-2">
          <button type="button" className={action} disabled={bulkBusy || loading || offset === 0}
            onClick={() => updateFilter({ offset: Math.max(0, offset - pageSize) }, false)}>Trang trước</button>
          <span>Trang {Math.floor(offset / pageSize) + 1}{data ? ` / ${Math.max(1, Math.ceil(data.total / pageSize))}` : ''}</span>
          <button type="button" className={action}
            disabled={bulkBusy || loading || !data || offset + pageSize >= data.total || offset + pageSize > 1000000}
            onClick={() => updateFilter({ offset: offset + pageSize }, false)}>Trang sau</button>
        </div>
      </nav>
    </section>
  );
}
