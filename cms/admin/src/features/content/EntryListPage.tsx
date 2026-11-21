import { useEffect, useMemo, useRef, useState } from 'react';
import {
  changeContentEntryPublication, deleteContentEntry, getEffectiveContentTypeTaxonomies,
  queryContentEntryTable, getEffectiveEntryListConfig, runEntryProviderAction, contentEntryPreviewPath,
} from '../../api/content.ts';
import {
  AdminColumnCell, type EffectiveListConfig, type ProviderColumn,
} from '../extensions/AdminContributionRenderer.tsx';
import { ApiClientError } from '../../api/client.ts';
import { mediaThumbnailUrl } from '../../api/media.ts';
import {
  listAllTaxonomyTerms, listTaxonomyDefinitions, resolveTaxonomyTermLabels,
  type TaxonomyDefinition, type TaxonomyTerm, type TaxonomyTermLabel,
} from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
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

const control =
  'h-9 min-w-0 max-w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent disabled:opacity-50';
const sortButton =
  'inline-flex min-h-6 w-full items-center gap-1 text-left text-xs font-semibold uppercase tracking-wider text-zinc-600 ' +
  'hover:text-zinc-950 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-od-accent';
const statuses = ['all', 'draft', 'published'] as const;
const TERM_RESOLVE_BATCH = 100;
const TERM_RESOLVE_CONCURRENCY = 2;
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
export function safePublicPath(path: string | null | undefined): string | null {
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
    accountScope: string;
    categoryEnabled: boolean; tagEnabled: boolean;
    definitions: TaxonomyDefinition[];
    byKind: Record<string, TaxonomyTerm[]>;
  } | null>(null);
  const [termLoadErrors, setTermLoadErrors] = useState<string[]>([]);
  const optionRequestsRef = useRef(new Map<string, AbortController>());
  const [resolvedState, setResolvedState] = useState<{
    scope: string; labels: Map<number, TaxonomyTermLabel>; failed: boolean;
  } | null>(null);
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
  const resolved = resolvedState?.scope === scope ? resolvedState : null;
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
    optionRequestsRef.current.forEach(request => request.abort());
    optionRequestsRef.current.clear();
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
        if (!active || controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        // Defining a column never triggers a full paginated term catalog walk.
        // Option catalogs are fetched only when the user opens a filter.
        setTermsState({
          accountScope, definitions, byKind: {},
          categoryEnabled: definitions.some(def => def.key === 'category'),
          tagEnabled: definitions.some(def => def.key === 'tag'),
        });
      } catch (cause) {
        if (!active || controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setTermsState({ accountScope, categoryEnabled: false, tagEnabled: false,
          definitions: [], byKind: {} });
        onApiError(cause, 'Không thể tải bộ lọc taxonomy.');
      }
    })();
    return () => {
      active = false;
      controller.abort();
      optionRequestsRef.current.forEach(request => request.abort());
      optionRequestsRef.current.clear();
    };
  }, [accountScope, retry]);

  const loadFilterOptions = (kind: string) => {
    if (!terms || !terms.definitions.some(def => def.key === kind) ||
      Object.prototype.hasOwnProperty.call(terms.byKind, kind) ||
      optionRequestsRef.current.has(kind)) return;
    const controller = new AbortController();
    optionRequestsRef.current.set(kind, controller);
    void listAllTaxonomyTerms(kind, controller.signal).then(found => {
      if (controller.signal.aborted || accountScopeRef.current !== accountScope) return;
      setTermsState(previous => previous?.accountScope === accountScope
        ? { ...previous, byKind: { ...previous.byKind, [kind]: found } } : previous);
      setTermLoadErrors(previous => previous.filter(error => !error.startsWith(`${kind}: `)));
    }).catch(cause => {
      if (controller.signal.aborted || accountScopeRef.current !== accountScope) return;
      if (cause instanceof ApiClientError && cause.status === 401) {
        controller.abort();
        onApiError(cause, 'Phiên đăng nhập hết hạn khi tải bộ lọc taxonomy.');
      } else {
        setTermLoadErrors(previous => [...previous.filter(error => !error.startsWith(`${kind}: `)),
          `${kind}: ${readableError(cause)}`]);
      }
    }).finally(() => {
      if (optionRequestsRef.current.get(kind) === controller) optionRequestsRef.current.delete(kind);
    });
  };

  // Only IDs in the current server-paginated rows are eligible for label lookup.
  // One request resolves at most 100 IDs; at most two requests run at a time.
  useEffect(() => {
    let active = true;
    const controller = new AbortController();
    setResolvedState(null);
    if (!data) return () => { active = false; controller.abort(); };
    const ids = [...new Set(data.items.flatMap(row => row.term_ids))]
      .filter(id => Number.isSafeInteger(id) && id > 0);
    if (!ids.length) {
      setResolvedState({ scope, labels: new Map(), failed: false });
      return () => { active = false; controller.abort(); };
    }
    const batches: number[][] = [];
    for (let i = 0; i < ids.length; i += TERM_RESOLVE_BATCH) {
      batches.push(ids.slice(i, i + TERM_RESOLVE_BATCH));
    }
    let nextBatch = 0;
    let failed = false;
    let authReported = false;
    const labels = new Map<number, TaxonomyTermLabel>();
    const load = async () => {
      while (active && !controller.signal.aborted && scopeRef.current === scope) {
        const batch = batches[nextBatch++];
        if (!batch) return;
        try {
          const found = await resolveTaxonomyTermLabels(batch, controller.signal);
          if (!active || controller.signal.aborted || scopeRef.current !== scope) return;
          for (const term of found) if (batch.includes(term.id)) labels.set(term.id, term);
        } catch (cause) {
          if (!active || controller.signal.aborted || scopeRef.current !== scope) return;
          if (cause instanceof ApiClientError && cause.status === 401) {
            if (!authReported) {
              authReported = true;
              controller.abort();
              onApiError(cause, 'Phiên đăng nhập hết hạn khi tải tên term.');
            }
            return;
          }
          failed = true;
        }
      }
    };
    void Promise.all(Array.from({
      length: Math.min(TERM_RESOLVE_CONCURRENCY, batches.length),
    }, load)).then(() => {
      if (active && !controller.signal.aborted && scopeRef.current === scope) {
        setResolvedState({ scope, labels, failed });
      }
    });
    return () => { active = false; controller.abort(); };
  }, [scope, data]);

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
  const namesFor = (row: EntryTableRow, kind: string) => {
    if (!row.term_ids.length) return '—';
    if (!resolved) return 'Đang tải…';
    const names = row.term_ids.map(id => resolved.labels.get(id))
      .filter((term): term is TaxonomyTermLabel => term?.kind === kind)
      .map(term => term.name);
    return names.join(', ') || (resolved.failed ? 'Không tải được' : '—');
  };
  const selectedOptionLabel = (kind: string, id: number): string => {
    const term = resolved?.labels.get(id);
    return term?.kind === kind ? term.name : `#${id}`;
  };
  const customDefinitions = terms?.definitions.filter(def => def.key !== 'category' && def.key !== 'tag') ?? [];
  const selectedTaxonomy = filters.taxonomy_key || taxonomyChoice;
  const chosenDefinition = terms?.definitions.find(def => def.key === selectedTaxonomy);

  const captionId = `entry-table-caption-${typeKey}`;
  const activeSort = filters.sort ?? 'updated_at';
  const activeOrder = filters.order ?? 'desc';
  const ariaSortFor = (field: 'title' | 'updated_at'): 'ascending' | 'descending' | undefined =>
    activeSort === field ? (activeOrder === 'asc' ? 'ascending' : 'descending') : undefined;

  return (
    <section className="w-full min-w-0 space-y-3.5" data-testid={`entry-list-${typeKey}`} data-type-key={typeKey}>
      <div className="flex flex-wrap items-center justify-between gap-2.5">
        <div className="flex flex-wrap items-center gap-1.5" aria-label="Lọc trạng thái" role="group">
          {statuses.map(status => {
            const count = data?.status_counts[status] ?? 0;
            const label = status === 'all' ? 'Tất cả' : status === 'draft' ? 'Bản nháp' : 'Đã xuất bản';
            const active = (filters.status ?? 'all') === status;
            return (
              <AdminButton
                key={status}
                variant={active ? 'primary' : 'secondary'}
                size="sm"
                aria-pressed={active}
                disabled={bulkBusy}
                onClick={() => updateFilter({ status })}
              >
                {label} <span className={active ? 'opacity-80' : 'text-zinc-500'}>({count})</span>
              </AdminButton>
            );
          })}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <AdminButton
            variant="secondary"
            size="sm"
            disabled={bulkBusy || loading}
            onClick={() => setRetry(prev => prev + 1)}
          >
            Tải lại
          </AdminButton>
          <AdminButton
            variant="primary"
            size="sm"
            disabled={bulkBusy}
            onClick={() => onNavigateHash(`${base}/new${editSuffix}`)}
          >
            + Thêm mới
          </AdminButton>
        </div>
      </div>

      <div className="space-y-3 rounded-xl border border-zinc-200 bg-white p-3.5">
        <div className="flex flex-wrap items-end gap-2.5">
          <form className="flex min-w-[220px] flex-1 items-end gap-2" role="search"
            onSubmit={event => {
              event.preventDefault();
              const q = searchInput.trim().slice(0, 200);
              updateFilter({ q: q || undefined });
            }}>
            <label htmlFor={`entry-table-search-${typeKey}`} className="min-w-0 flex-1 text-xs font-medium text-zinc-600">
              Tìm tiêu đề hoặc slug
              <input id={`entry-table-search-${typeKey}`} type="search" className={`${control} mt-1 block w-full`}
                value={searchInput} maxLength={200} disabled={bulkBusy}
                onChange={event => setSearchInput(event.target.value)}
                placeholder="Nhập từ khóa…" />
            </label>
            <AdminButton type="submit" variant="secondary" disabled={bulkBusy}>Tìm kiếm</AdminButton>
          </form>

          {terms?.categoryEnabled && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>Danh mục</span>
            <select aria-label="Lọc danh mục" className={`${control} w-full`}
              value={filters.category_id ?? ''} disabled={bulkBusy}
              onFocus={() => loadFilterOptions('category')}
              onChange={event => updateFilter({
                category_id: event.target.value ? Number(event.target.value) : undefined,
                taxonomy_key: undefined, term_id: undefined,
                include_descendants: false,
              })}>
              <option value="">Mọi danh mục</option>
              {filters.category_id && !terms.byKind.category?.some(term => term.id === filters.category_id) &&
                <option value={filters.category_id}>{selectedOptionLabel('category', filters.category_id)}</option>}
              {terms.byKind.category?.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms.byKind.category && <option disabled>Chạm để tải danh mục…</option>}
            </select>
          </label>}

          {terms?.tagEnabled && <label className="flex min-w-[130px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>Thẻ</span>
            <select aria-label="Lọc thẻ" className={`${control} w-full`}
              value={filters.tag_id ?? ''} disabled={bulkBusy}
              onFocus={() => loadFilterOptions('tag')}
              onChange={event => updateFilter({ tag_id: event.target.value ? Number(event.target.value) : undefined,
                taxonomy_key: undefined, term_id: undefined,
                // A tag is an AND filter, not a reset of the selected category
                // subtree. Preserve descendants only when a category is active.
                include_descendants: Boolean(filters.category_id && filters.include_descendants) })}>
              <option value="">Mọi thẻ</option>
              {filters.tag_id && !terms.byKind.tag?.some(term => term.id === filters.tag_id) &&
                <option value={filters.tag_id}>{selectedOptionLabel('tag', filters.tag_id)}</option>}
              {terms.byKind.tag?.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms.byKind.tag && <option disabled>Chạm để tải thẻ…</option>}
            </select>
          </label>}

          {customDefinitions.length > 0 && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>Taxonomy</span>
            <select aria-label="Chọn taxonomy để lọc" className={`${control} w-full`}
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

          {chosenDefinition && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{chosenDefinition.label}</span>
            <select aria-label={`Lọc theo ${chosenDefinition.label}`} className={`${control} w-full`}
              disabled={bulkBusy} value={filters.term_id ?? ''}
              onFocus={() => loadFilterOptions(chosenDefinition.key)}
              onChange={event => {
                setTaxonomyChoice(selectedTaxonomy);
                updateFilter({ taxonomy_key: event.target.value ? selectedTaxonomy : undefined,
                  term_id: event.target.value ? Number(event.target.value) : undefined,
                  include_descendants: false });
              }}>
              <option value="">Chọn term</option>
              {filters.term_id && !terms?.byKind[chosenDefinition.key]?.some(term => term.id === filters.term_id) &&
                <option value={filters.term_id}>{selectedOptionLabel(chosenDefinition.key, filters.term_id)}</option>}
              {(terms?.byKind[chosenDefinition.key] ?? []).map(term =>
                <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms?.byKind[chosenDefinition.key] && <option disabled>Chạm để tải term…</option>}
            </select>
          </label>}

          <label className="flex min-w-[145px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>Tháng (UTC)</span>
            <input type="month" aria-label="Lọc tháng cập nhật" className={`${control} w-full`}
              value={filters.updated_month ?? ''} disabled={bulkBusy}
              onChange={event => updateFilter({ updated_month: event.target.value || undefined })} />
          </label>

          <label className="flex w-[88px] flex-col gap-1 text-xs font-medium text-zinc-600">
            <span>Mỗi trang</span>
            <select aria-label="Số mục mỗi trang" className={`${control} w-full`}
              value={pageSize} disabled={bulkBusy}
              onChange={event => updateFilter({ limit: Number(event.target.value) })}>
              {[10, 20, 50, 100].map(size => <option key={size} value={size}>{size}</option>)}
            </select>
          </label>
        </div>

        {(filters.category_id && terms?.categoryEnabled) || (filters.term_id && chosenDefinition?.hierarchical) ? (
          <div className="flex flex-wrap items-center gap-4 border-t border-zinc-100 pt-2.5">
            {filters.category_id && terms?.categoryEnabled && <label className="flex items-center gap-2 text-xs text-zinc-700">
              <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
                onChange={event => updateFilter({ include_descendants: event.target.checked })} />
              Gồm danh mục con
            </label>}
            {filters.term_id && chosenDefinition?.hierarchical && <label className="flex items-center gap-2 text-xs text-zinc-700">
              <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
                onChange={event => updateFilter({ include_descendants: event.target.checked })} />
              Gồm các term con của {chosenDefinition.label}
            </label>}
          </div>
        ) : null}

        <div className="flex flex-wrap items-center justify-between gap-2.5 border-t border-zinc-100 pt-2.5">
          <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Thao tác hàng loạt">
            <span className="text-xs font-medium text-zinc-500">{selectedRows.length} đã chọn</span>
            {isBuiltIn && <>
              <AdminButton variant="secondary" size="sm" disabled={bulkBusy || !csrfToken || !selectedRows.length}
                onClick={() => void performBulk('published')}>Xuất bản đã chọn</AdminButton>
              <AdminButton variant="secondary" size="sm" disabled={bulkBusy || !csrfToken || !selectedRows.length}
                onClick={() => void performBulk('draft')}>Chuyển về nháp</AdminButton>
            </>}
            <AdminButton variant="danger" size="sm" disabled={bulkBusy || !csrfToken || !selectedRows.length}
              onClick={() => void performBulk('delete')}>Xóa đã chọn</AdminButton>
            {bulkBusy && <span role="status" className="text-xs text-zinc-600">Đang xử lý riêng từng mục…</span>}
          </div>
          <div className="flex flex-wrap items-center gap-2.5 text-xs">
            <span className="text-zinc-500" role="status">{data ? `Tổng: ${data.total} mục` : 'Đang lấy số lượng…'}</span>
            <AdminButton variant="ghost" size="sm" disabled={bulkBusy}
              onClick={() => { setSearchInput(''); setTaxonomyChoice(''); onNavigateHash(base); }}>
              Xóa bộ lọc
            </AdminButton>
          </div>
        </div>
      </div>

      {termLoadErrors.length > 0 && <AdminAlert tone="warning" role="alert">
        Không tải được một số term: {termLoadErrors.join('; ')}. Các bộ lọc khác vẫn sử dụng được.
      </AdminAlert>}
      {resolved?.failed && <AdminAlert tone="warning" role="alert">
        Không tải được tên của một số term trên trang. Thử Tải lại để hiển thị đầy đủ.
      </AdminAlert>}

      {bulkResults.length > 0 && (
        <div className="rounded-lg border border-zinc-200 bg-white p-3" aria-label="Kết quả hàng loạt">
          <p className="mb-1.5 text-xs font-semibold text-zinc-900">Kết quả: {bulkResults.filter(row => row.ok).length} thành công / {bulkResults.length} đã thử</p>
          <ul className="space-y-1 text-xs">
            {bulkResults.map(result => <li key={result.id} data-bulk-id={result.id}
              role={result.ok ? 'status' : 'alert'}
              className={result.ok ? 'text-emerald-700' : 'text-rose-700'}>{result.message}</li>)}
          </ul>
        </div>
      )}

      {providerNotice && <AdminAlert tone="info" data-testid="provider-action-result">{providerNotice}</AdminAlert>}
      {providerError && <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
        <span>Không thể tải cột mở rộng: {providerError}</span>
        <AdminButton variant="ghost" size="sm" onClick={() => setRetry(n => n + 1)}>Thử lại</AdminButton>
      </AdminAlert>}

      {error && <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
        <span>{error}</span>
        <AdminButton variant="ghost" size="sm" onClick={() => setRetry(prev => prev + 1)}>Thử lại</AdminButton>
      </AdminAlert>}

      {loading && <p role="status" className="text-xs text-zinc-500">Đang tải danh sách từ máy chủ…</p>}

      <div
        role="region"
        aria-labelledby={captionId}
        tabIndex={0}
        className="w-full min-w-0 max-w-full overflow-x-auto rounded-xl border border-zinc-200 bg-white focus-visible:outline-2 focus-visible:outline-od-accent"
        aria-busy={loading}
      >
        <table className="w-full min-w-[780px] table-auto text-left text-sm" data-testid="entry-table">
          <caption id={captionId} className="sr-only">Bảng nội dung {typeKey}; tìm kiếm, trạng thái và phân trang từ máy chủ</caption>
          <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold uppercase tracking-wider text-zinc-600">
            <tr>
              <th scope="col" className="w-10 px-3 py-2.5">
                <input type="checkbox" aria-label="Chọn tất cả trên trang" checked={allSelected}
                  disabled={!currentRows.length || bulkBusy || loading}
                  onChange={event => setSelected(prev => {
                    const next = new Set(prev);
                    currentIds.forEach(id => { if (event.target.checked) next.add(id); else next.delete(id); });
                    return next;
                  })} />
              </th>
              <th scope="col" className="w-14 px-2 py-2.5 text-center">Ảnh</th>
              <th scope="col" aria-sort={ariaSortFor('title')} className="min-w-[220px] px-3 py-2.5">
                <button type="button" data-testid="sort-title" className={sortButton}
                  disabled={bulkBusy} onClick={() => sortBy('title')}>
                  <span>Tiêu đề</span>
                  {activeSort === 'title' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                </button>
              </th>
              <th scope="col" className="whitespace-nowrap px-3 py-2.5">Trạng thái</th>
              {terms?.categoryEnabled && <th scope="col" className="px-3 py-2.5">Danh mục</th>}
              {terms?.tagEnabled && <th scope="col" className="px-3 py-2.5">Thẻ</th>}
              {customDefinitions.map(def => <th key={def.key} scope="col" className="px-3 py-2.5">{def.label}</th>)}
              {providerColumns.map(column => <th key={column.key} scope="col"
                className="min-w-[120px] px-3 py-2.5">{column.label}</th>)}
              <th scope="col" aria-sort={ariaSortFor('updated_at')} className="min-w-[140px] whitespace-nowrap px-3 py-2.5">
                <button type="button" data-testid="sort-updated" className={sortButton}
                  disabled={bulkBusy} onClick={() => sortBy('updated_at')}>
                  <span>Ngày cập nhật</span>
                  {activeSort === 'updated_at' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                </button>
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100">
            {!loading && !error && currentRows.length === 0 && <tr>
              <td colSpan={5 + (terms?.definitions.length ?? 0) + providerColumns.length}
                className="px-4 py-8 text-center text-sm text-zinc-500">
                {data?.total === 0 ? 'Không tìm thấy nội dung phù hợp. Có thể xóa bộ lọc để xem lại.' : 'Không có mục trên trang này.'}
              </td>
            </tr>}
            {currentRows.map(row => {
              const path = row.publication_status === 'published' ? safePublicPath(row.public_url) : null;
              const thumbId = typeof row.fields?.featured_image === 'string' ? row.fields.featured_image.trim() : '';
              return <tr key={row.id} data-entry-id={row.id} className="align-middle transition-colors hover:bg-zinc-50/70">
                <td className="px-3 py-2.5"><input type="checkbox" aria-label={`Chọn mục #${row.id}`}
                  checked={selected.has(row.id)} disabled={bulkBusy}
                  onChange={event => toggleRow(row.id, event.target.checked)} /></td>
                <td className="w-14 px-2 py-2.5">
                  {thumbId ? (
                    <img
                      src={mediaThumbnailUrl(thumbId)}
                      alt={row.title}
                      loading="lazy"
                      className="mx-auto h-10 w-10 rounded-md border border-zinc-200 bg-zinc-50 object-cover"
                    />
                  ) : (
                    <div
                      className="mx-auto flex h-10 w-10 items-center justify-center rounded-md border border-dashed border-zinc-200 bg-zinc-50 text-zinc-300"
                      aria-hidden="true"
                    >
                      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                        <rect x="3" y="4" width="18" height="16" rx="2" />
                        <circle cx="8.5" cy="9.5" r="1.5" />
                        <path d="m4 17 5-5 4 3 3-4 4 5" />
                      </svg>
                    </div>
                  )}
                </td>
                <td className="break-words px-3 py-2.5">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <button
                      type="button"
                      disabled={bulkBusy}
                      onClick={() => onNavigateHash(`${base}/${row.id}/edit${editSuffix}`)}
                      className="text-left text-sm font-semibold text-zinc-950 hover:text-od-accent"
                    >
                      {row.title}
                    </button>
                    <span className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-[11px] font-medium text-zinc-500">
                      #{row.id}
                    </span>
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-1 text-xs">
                    <button
                      type="button"
                      className="font-medium text-od-accent hover:underline"
                      disabled={bulkBusy}
                      aria-label={`Chỉnh sửa ${row.title}, ID ${row.id}`}
                      onClick={() => onNavigateHash(`${base}/${row.id}/edit${editSuffix}`)}
                    >
                      Chỉnh sửa
                    </button>
                    {path && (
                      <>
                        <span className="text-zinc-300" aria-hidden="true">·</span>
                        <a
                          href={path}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="font-medium text-zinc-600 hover:text-zinc-950 hover:underline"
                          aria-label={`Xem công khai ${row.title}`}
                        >
                          Xem
                        </a>
                      </>
                    )}
                    {row.publication_status === 'draft' && (
                      <>
                        <span className="text-zinc-300" aria-hidden="true">·</span>
                        <a
                          href={contentEntryPreviewPath(typeKey, row.id)}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="font-medium text-zinc-600 hover:text-zinc-950 hover:underline"
                          aria-label={`Xem trước bản nháp đã lưu ${row.title}`}
                        >
                          Xem trước
                        </a>
                      </>
                    )}
                    <span className="text-zinc-300" aria-hidden="true">·</span>
                    <span className="break-all font-mono text-[11px] text-zinc-400">/{row.slug}</span>
                  </div>
                </td>
                <td className="whitespace-nowrap px-3 py-2.5">
                  <AdminBadge tone={row.publication_status === 'published' ? 'success' : 'neutral'}>
                    {row.publication_status === 'published' ? 'Đã xuất bản' : 'Bản nháp'}
                  </AdminBadge>
                </td>
                {terms?.categoryEnabled && <td className="break-words px-3 py-2.5 text-zinc-700">{namesFor(row, 'category')}</td>}
                {terms?.tagEnabled && <td className="break-words px-3 py-2.5 text-zinc-700">{namesFor(row, 'tag')}</td>}
                {customDefinitions.map(def => <td key={def.key} className="break-words px-3 py-2.5 text-zinc-700">{namesFor(row, def.key)}</td>)}
                {providerColumns.map(column => <td key={column.key} className="break-words px-3 py-2.5">
                  <AdminColumnCell column={column} row={row} busy={providerBusy || bulkBusy}
                    onRun={(col, entry) => void performProviderAction(col, entry)} />
                </td>)}
                <td className="whitespace-nowrap px-3 py-2.5 text-xs tabular-nums text-zinc-600">{row.updated_at.slice(0, 16)} UTC</td>
              </tr>;
            })}
          </tbody>
        </table>
      </div>
      <nav aria-label="Phân trang nội dung" className="flex flex-wrap items-center justify-between gap-3 text-xs text-zinc-600">
        <span data-testid="entry-table-page-info">{data ? `Hiển thị ${data.total ? offset + 1 : 0}–${Math.min(offset + currentRows.length, data.total)} / ${data.total}` : 'Chưa tải trang'}</span>
        <div className="flex items-center gap-2">
          <AdminButton variant="secondary" size="sm" disabled={bulkBusy || loading || offset === 0}
            onClick={() => updateFilter({ offset: Math.max(0, offset - pageSize) }, false)}>Trang trước</AdminButton>
          <span className="tabular-nums">Trang {Math.floor(offset / pageSize) + 1}{data ? ` / ${Math.max(1, Math.ceil(data.total / pageSize))}` : ''}</span>
          <AdminButton variant="secondary" size="sm"
            disabled={bulkBusy || loading || !data || offset + pageSize >= data.total || offset + pageSize > 1000000}
            onClick={() => updateFilter({ offset: offset + pageSize }, false)}>Trang sau</AdminButton>
        </div>
      </nav>
    </section>
  );
}
