import { formatNumber } from '../../i18n/format.ts';
import { tStatic, useI18n } from '../../i18n/index.tsx';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  changeContentEntryPublication, getEffectiveContentTypeTaxonomies,
  purgeContentEntry, queryContentEntryTable, getEffectiveEntryListConfig,
  restoreContentEntry, runEntryProviderAction, contentEntryPreviewPath, trashContentEntry,
} from '../../api/content.ts';
import {
  AdminColumnCell, type EffectiveListConfig, type ProviderColumn,
} from '../extensions/AdminContributionRenderer.tsx';
import { ApiClientError } from '../../api/client.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { mediaThumbnailUrl } from '../../api/media.ts';
import {
  listAllTaxonomyTerms, listTaxonomyDefinitions, resolveTaxonomyTermLabels,
  type TaxonomyDefinition, type TaxonomyTerm, type TaxonomyTermLabel,
} from '../../api/taxonomy.ts';
import type { AuthUser, PluginLifecycleConfirmation as PluginLifecycleConfirmationDto } from '../../types.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { PluginLifecycleConfirmation } from '../plugins/PluginLifecycleConfirmation.tsx';
import type { EntryTableQuery, EntryTableResult, EntryTableRow } from './types.ts';

interface Props {
  typeKey: string;
  user: AuthUser;
  siteInstanceId: string;
  authGeneration: number;
  csrfToken: string | null;
  routeHash: string;
  localeState?: AdminLocaleState;
  onNavigateHash: (hash: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

const control =
  'h-9 min-w-0 max-w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent disabled:opacity-50';
const sortButton =
  'inline-flex min-h-6 w-full items-center gap-1 text-left text-xs font-semibold uppercase tracking-wider text-zinc-600 ' +
  'hover:text-zinc-950 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-od-accent';
const statuses = ['all', 'draft', 'published', 'trash'] as const;
const TERM_RESOLVE_BATCH = 100;
const TERM_RESOLVE_CONCURRENCY = 2;
const MAX_BULK_ITEMS = 100;
type BulkAction = 'published' | 'draft' | 'trash';
type BulkItemStatus = 'succeeded' | 'stale' | 'failed' | 'uncertain' | 'unattempted';
interface BulkResultItem {
  id: number;
  status: BulkItemStatus;
  message: string;
}
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
  if (status === 'draft' || status === 'published' || status === 'trash') query.status = status;
  const locale = p.get('locale')?.trim();
  if (locale && locale !== 'all') query.locale = locale;
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
  if (
    sort === 'title' ||
    sort === 'id' ||
    sort === 'created_at' ||
    sort === 'updated_at' ||
    sort === 'trashed_at'
  ) {
    query.sort = sort;
  }
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
  if (filter.locale && filter.locale !== 'all') p.set('locale', filter.locale);
  if (filter.status !== 'trash') {
    if (filter.category_id) p.set('category_id', String(filter.category_id));
    if (filter.tag_id) p.set('tag_id', String(filter.tag_id));
    if (filter.taxonomy_key !== undefined) p.set('taxonomy_key', filter.taxonomy_key);
    if (filter.term_id !== undefined) p.set('term_id', String(filter.term_id));
    if (filter.include_descendants) p.set('include_descendants', 'true');
    if (filter.updated_month) p.set('updated_month', filter.updated_month);
  }
  const defaultSort = filter.status === 'trash' ? 'trashed_at' : 'updated_at';
  if (filter.sort && filter.sort !== defaultSort) p.set('sort', filter.sort);
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
  if (cause instanceof ApiClientError && cause.status === 412) return tStatic("feature.content.entrylistpage.412_the_item_changed_reload_the_list_to_get");
  if (cause instanceof ApiClientError && cause.status === 403) return tStatic("feature.content.entrylistpage.403_you_do_not_have_permission_to_perform_this");
  if (cause instanceof ApiClientError && cause.status === 401) return tStatic("feature.content.entrylistpage.401_your_session_is_no_longer_valid");
  return cause instanceof Error ? cause.message : tStatic("feature.content.entrylistpage.the_request_did_not_complete");
}

export function EntryListPage({
  typeKey, user, siteInstanceId, authGeneration, csrfToken, routeHash,
  localeState = EMPTY_ADMIN_LOCALE_STATE, onNavigateHash, onApiError,
}: Props) {
  useI18n();
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
  const [pendingProviderConfirmation, setPendingProviderConfirmation] = useState<{
    pluginId: string;
    actionId: string;
    rowId: number;
    confirmation: PluginLifecycleConfirmationDto;
  } | null>(null);
  const [searchInput, setSearchInput] = useState(filters.q ?? '');
  const [taxonomyChoice, setTaxonomyChoice] = useState(filters.taxonomy_key ?? '');
  const [selected, setSelected] = useState<Set<number>>(() => new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkCancelling, setBulkCancelling] = useState(false);
  const bulkBusyRef = useRef(false);
  const bulkCancelRef = useRef(false);
  const bulkAbortRef = useRef<AbortController | null>(null);
  const [bulkResults, setBulkResults] = useState<BulkResultItem[]>([]);
  const [bulkBlockMessage, setBulkBlockMessage] = useState('');
  const [bulkBatchMeta, setBulkBatchMeta] = useState<{
    kind: BulkAction;
    scope: string;
    reconciledAfterRetry: number;
  } | null>(null);
  const [dataRevision, setDataRevision] = useState(0);
  const isBuiltIn = typeKey === 'post' || typeKey === 'page';
  const isTrashView = filters.status === 'trash';
  const data = dataState?.scope === scope ? dataState.value : null;
  const error = errorState?.scope === scope ? errorState.message : '';
  const loading = loadingScope === scope || (!data && !error);
  const terms = termsState?.accountScope === accountScope ? termsState : null;
  const resolved = resolvedState?.scope === scope ? resolvedState : null;
  const providerColumns = !isTrashView && providerState?.accountScope === accountScope
    ? providerState.config.columns : [];

  const updateFilter = (patch: Partial<TaxonomyTableQuery>, resetPage = true) => {
    if (bulkBusyRef.current) return;
    const next = { ...filters, ...patch };
    if (patch.status === 'trash') {
      delete next.category_id;
      delete next.tag_id;
      delete next.taxonomy_key;
      delete next.term_id;
      delete next.include_descendants;
      delete next.updated_month;
      if (!patch.sort && (!next.sort || next.sort === 'updated_at')) {
        next.sort = 'trashed_at';
      }
    } else if (patch.status && next.sort === 'trashed_at') {
      next.sort = 'updated_at';
    }
    if (resetPage) delete next.offset;
    const nextHash = entryTableHash(base, next);
    if (nextHash !== routeHash) onNavigateHash(nextHash);
  };

  useEffect(() => {
    if (!filters.locale || localeState.allLocales.length === 0) return;
    if (localeState.allLocales.some(locale => locale.code === filters.locale)) return;
    const next = { ...filters };
    delete next.locale;
    delete next.offset;
    const nextHash = entryTableHash(base, next);
    if (nextHash !== routeHash) onNavigateHash(nextHash);
  }, [base, filters, localeState.allLocales, routeHash, onNavigateHash]);

  useEffect(() => {
    bulkCancelRef.current = true;
    bulkAbortRef.current?.abort();
    setSearchInput(filters.q ?? '');
    setSelected(new Set());
    setBulkResults([]);
    setBulkBlockMessage('');
    setBulkBatchMeta(null);
    setPendingProviderConfirmation(null);
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
        setDataRevision(rev => rev + 1);
        setErrorState(null);
      })
      .catch(cause => {
        if (!active || controller.signal.aborted || scopeRef.current !== scope) return;
        setErrorState({ scope, message: readableError(cause) });
        if (cause instanceof ApiClientError && cause.status === 401) onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired"));
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
        onApiError(cause, tStatic("feature.content.entrylistpage.could_not_load_taxonomy_filters"));
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
        onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired_while_loading_taxonomy_filters"));
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
              onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired_while_loading_term_names"));
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
          onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired_while_loading_provider_columns"));
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

  const runSafeSelectedBatch = async (kind: BulkAction, targetRows: EntryTableRow[], skipConfirm = false) => {
    if (!csrfToken || bulkBusyRef.current || !targetRows.length || isTrashView) return;
    if (kind !== 'trash' && !isBuiltIn) return;
    const bounded = targetRows.slice(0, MAX_BULK_ITEMS);
    const locales = new Set(bounded.map(row => row.locale ?? ''));
    if (locales.size > 1) {
      setBulkBlockMessage(tStatic("entry_lifecycle.bulk_mixed_locale_blocked"));
      return;
    }
    const noun = kind === 'trash'
      ? tStatic("entry_lifecycle.move_to_trash")
      : kind === 'published'
        ? tStatic("feature.content.entrylistpage.publish")
        : tStatic("feature.content.entrylistpage.move_to_draft");
    if (!skipConfirm) {
      const confirmPrompt = kind === 'trash'
        ? tStatic("entry_lifecycle.bulk_confirm_move_to_trash", { value1: formatNumber(bounded.length) })
        : tStatic("feature.content.entrylistpage.are_you_sure_you_want_to_value_value_selected", {
            value1: noun,
            value2: formatNumber(bounded.length),
          });
      if (!window.confirm(confirmPrompt)) return;
    }
    const startingScope = scope;
    const captured = [...bounded];
    bulkBusyRef.current = true;
    bulkCancelRef.current = false;
    setBulkBusy(true);
    setBulkCancelling(false);
    setBulkBlockMessage('');
    setBulkResults([]);
    setBulkBatchMeta(null);
    const reports: BulkResultItem[] = [];
    const beforeRefreshRevision = dataRevision;
    try {
      for (let idx = 0; idx < captured.length; idx += 1) {
        const row = captured[idx];
        if (!aliveRef.current || scopeRef.current !== startingScope || bulkCancelRef.current) {
          for (let rem = idx; rem < captured.length; rem += 1) {
            reports.push({
              id: captured[rem].id,
              status: 'unattempted',
              message: `#${captured[rem].id}: ${tStatic("entry_lifecycle.bulk_unattempted_message")}`,
            });
          }
          break;
        }
        const controller = new AbortController();
        bulkAbortRef.current = controller;
        try {
          if (kind === 'trash') {
            await trashContentEntry(typeKey, row.id, row.version, csrfToken, controller.signal);
          } else {
            await changeContentEntryPublication(typeKey, row.id, row.version, kind, csrfToken);
          }
          reports.push({
            id: row.id,
            status: 'succeeded',
            message: tStatic("feature.content.entrylistpage.value_value_succeeded", { value1: row.id, value2: noun }),
          });
        } catch (cause) {
          if (cause instanceof ApiClientError) {
            if (cause.status === 412) {
              reports.push({ id: row.id, status: 'stale', message: `#${row.id}: ${readableError(cause)}` });
            } else if (cause.status === 401) {
              reports.push({ id: row.id, status: 'failed', message: `#${row.id}: ${readableError(cause)}` });
              onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired_during_the_bulk_update"));
              for (let rem = idx + 1; rem < captured.length; rem += 1) {
                reports.push({
                  id: captured[rem].id,
                  status: 'unattempted',
                  message: `#${captured[rem].id}: ${tStatic("entry_lifecycle.bulk_unattempted_message")}`,
                });
              }
              break;
            } else if (cause.status === 503) {
              reports.push({
                id: row.id,
                status: 'failed',
                message: `#${row.id}: ${tStatic("entry_lifecycle.bulk_maintenance_stopped")}`,
              });
              for (let rem = idx + 1; rem < captured.length; rem += 1) {
                reports.push({
                  id: captured[rem].id,
                  status: 'unattempted',
                  message: `#${captured[rem].id}: ${tStatic("entry_lifecycle.bulk_unattempted_message")}`,
                });
              }
              break;
            } else {
              reports.push({ id: row.id, status: 'failed', message: `#${row.id}: ${readableError(cause)}` });
            }
          } else {
            // Network drop or in-flight cancellation: server commit state is uncertain.
            reports.push({
              id: row.id,
              status: 'uncertain',
              message: `#${row.id}: ${tStatic("entry_lifecycle.bulk_uncertain_message")}`,
            });
            if (bulkCancelRef.current || !aliveRef.current || scopeRef.current !== startingScope) {
              for (let rem = idx + 1; rem < captured.length; rem += 1) {
                reports.push({
                  id: captured[rem].id,
                  status: 'unattempted',
                  message: `#${captured[rem].id}: ${tStatic("entry_lifecycle.bulk_unattempted_message")}`,
                });
              }
              break;
            }
          }
        } finally {
          if (bulkAbortRef.current === controller) bulkAbortRef.current = null;
        }
      }
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      const succeeded = new Set(reports.filter(report => report.status === 'succeeded').map(report => report.id));
      setSelected(prev => new Set([...prev].filter(id => !succeeded.has(id))));
      setBulkResults(reports);
      setBulkBatchMeta({ kind, scope: startingScope, reconciledAfterRetry: beforeRefreshRevision });
      setRetry(prev => prev + 1); // Always reconcile authoritative server state & versions.
    } finally {
      bulkBusyRef.current = false;
      bulkAbortRef.current = null;
      if (aliveRef.current) {
        setBulkBusy(false);
        setBulkCancelling(false);
      }
    }
  };

  const performBulk = async (kind: BulkAction) => runSafeSelectedBatch(kind, selectedRows, false);

  const retryRemainingBulk = () => {
    if (!bulkBatchMeta || bulkBatchMeta.scope !== scope || bulkBusyRef.current) return;
    if (dataRevision <= bulkBatchMeta.reconciledAfterRetry) return;
    const retryIds = new Set(
      bulkResults
        .filter(item => item.status !== 'succeeded')
        .map(item => item.id),
    );
    // Reconcile against freshly loaded authoritative rows (never replay succeeded rows
    // or rows already transitioned/absent on the server).
    const freshRows = currentRows.filter(row => {
      if (!retryIds.has(row.id)) return false;
      if (bulkBatchMeta.kind === 'published' && row.publication_status === 'published') return false;
      if (bulkBatchMeta.kind === 'draft' && row.publication_status === 'draft') return false;
      return true;
    });
    if (!freshRows.length) {
      setBulkResults([]);
      setBulkBatchMeta(null);
      return;
    }
    void runSafeSelectedBatch(bulkBatchMeta.kind, freshRows, false);
  };

  const performProviderAction = async (column: ProviderColumn, row: EntryTableRow) => {
    if (providerBusyRef.current || bulkBusyRef.current || !csrfToken || !column.action_id ||
      column.owner_kind !== 'plugin') return;
    if (!window.confirm(tStatic("feature.content.entrylistpage.run_action_value_for_value_value_this_action_may", { value1: column.label, value2: row.title, value3: row.id }))) return;
    const startingScope = scope;
    providerBusyRef.current = true;
    setProviderBusy(true);
    setProviderNotice('');
    setPendingProviderConfirmation(null);
    try {
      const response = await runEntryProviderAction(
        typeKey, row, column.owner_id, column.action_id, csrfToken,
      );
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      setProviderNotice(response.success ? tStatic("feature.content.entrylistpage.action_completed_for_value", { value1: row.id }) : tStatic("feature.content.entrylistpage.the_action_failed"));
      setRetry(old => old + 1);
    } catch (cause) {
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      if (cause instanceof ApiClientError && cause.confirmation) {
        setPendingProviderConfirmation({
          pluginId: column.owner_id,
          actionId: column.action_id,
          rowId: row.id,
          confirmation: cause.confirmation,
        });
      } else {
        setProviderNotice(`#${row.id}: ${readableError(cause)}`);
        if (cause instanceof ApiClientError && cause.status === 401)
          onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired_while_running_the_action"));
      }
    } finally {
      providerBusyRef.current = false;
      if (aliveRef.current) setProviderBusy(false);
    }
  };

  const toggleRow = (id: number, checked: boolean) => {
    if (bulkBusyRef.current) return;
    setBulkBlockMessage('');
    setSelected(prev => {
      const next = new Set(prev);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };
  const sortBy = (column: NonNullable<EntryTableQuery['sort']>) => {
    const defaultSort = isTrashView ? 'trashed_at' : 'updated_at';
    const nextOrder = (filters.sort ?? defaultSort) === column && (filters.order ?? 'desc') === 'desc'
      ? 'asc' : 'desc';
    updateFilter({ sort: column, order: nextOrder });
  };
  const trashSingleRow = async (row: EntryTableRow) => {
    if (bulkBusyRef.current || !csrfToken || row.capabilities?.trash === false) return;
    if (!window.confirm(tStatic("entry_lifecycle.confirm_move_to_trash_row", { value1: row.title, value2: row.id }))) return;
    const startingScope = scope;
    bulkBusyRef.current = true;
    setBulkBusy(true);
    try {
      await trashContentEntry(typeKey, row.id, row.version, csrfToken);
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      setSelected(prev => {
        const next = new Set(prev);
        next.delete(row.id);
        return next;
      });
      setRetry(prev => prev + 1);
    } catch (cause) {
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      if (cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, tStatic("feature.content.entrylistpage.your_session_expired_while_deleting_the_item"));
      } else {
        setBulkResults([{ id: row.id, status: 'failed', message: `#${row.id}: ${readableError(cause)}` }]);
      }
    } finally {
      bulkBusyRef.current = false;
      if (aliveRef.current) setBulkBusy(false);
    }
  };

  const restoreSingleRow = async (row: EntryTableRow) => {
    if (bulkBusyRef.current || !csrfToken || row.capabilities?.restore === false) return;
    const needsPublicConfirm = Boolean(row.capabilities?.immediate_public_restore);
    if (
      needsPublicConfirm &&
      !window.confirm(tStatic("entry_lifecycle.confirm_public_restore_row", { value1: row.title, value2: row.id }))
    ) {
      return;
    }
    const startingScope = scope;
    bulkBusyRef.current = true;
    setBulkBusy(true);
    try {
      await restoreContentEntry(typeKey, row.id, row.version, csrfToken, {
        confirmPublicRestore: needsPublicConfirm,
      });
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      setRetry(prev => prev + 1);
    } catch (cause) {
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      if (cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, tStatic("api.content.restore_the_entry"));
      } else {
        setBulkResults([{ id: row.id, status: 'failed', message: `#${row.id}: ${readableError(cause)}` }]);
      }
    } finally {
      bulkBusyRef.current = false;
      if (aliveRef.current) setBulkBusy(false);
    }
  };

  const purgeSingleRow = async (row: EntryTableRow) => {
    if (bulkBusyRef.current || !csrfToken || user.role !== 'admin' || row.capabilities?.purge === false) return;
    if (!window.confirm(tStatic("entry_lifecycle.confirm_purge_row", { value1: row.title, value2: row.id }))) return;
    const startingScope = scope;
    bulkBusyRef.current = true;
    setBulkBusy(true);
    try {
      await purgeContentEntry(typeKey, row.id, row.version, csrfToken);
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      setRetry(prev => prev + 1);
    } catch (cause) {
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      if (cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, tStatic("api.content.purge_the_entry"));
      } else {
        setBulkResults([{ id: row.id, status: 'failed', message: `#${row.id}: ${readableError(cause)}` }]);
      }
    } finally {
      bulkBusyRef.current = false;
      if (aliveRef.current) setBulkBusy(false);
    }
  };

  const editSuffix = queryHash.includes('?') ? queryHash.slice(queryHash.indexOf('?')) : '';
  const renderTermsCell = (row: EntryTableRow, kind: string) => {
    if (!row.term_ids.length) return <span className="text-zinc-400">—</span>;
    if (!resolved) return <span className="text-xs text-zinc-400">{tStatic("reading.loading")}</span>;
    const matched = row.term_ids
      .map(id => {
        const term = resolved.labels.get(id);
        return term?.kind === kind ? { id, name: term.name } : null;
      })
      .filter((item): item is { id: number; name: string } => item !== null);
    if (!matched.length) {
      return <span className="text-zinc-400">{resolved.failed ? tStatic("feature.content.entrylistpage.could_not_load") : '—'}</span>;
    }
    return (
      <span className="inline-flex flex-wrap items-center gap-x-1 gap-y-0.5">
        {matched.map((term, idx) => (
          <span key={term.id} className="inline-flex items-center">
            <button
              type="button"
              disabled={bulkBusy}
              onClick={() => {
                if (kind === 'category') {
                  updateFilter({
                    category_id: term.id,
                    taxonomy_key: undefined,
                    term_id: undefined,
                    include_descendants: false,
                  });
                } else if (kind === 'tag') {
                  updateFilter({
                    tag_id: term.id,
                    taxonomy_key: undefined,
                    term_id: undefined,
                    include_descendants: Boolean(filters.category_id && filters.include_descendants),
                  });
                } else {
                  setTaxonomyChoice(kind);
                  updateFilter({
                    taxonomy_key: kind,
                    term_id: term.id,
                    category_id: undefined,
                    tag_id: undefined,
                    include_descendants: false,
                  });
                }
              }}
              className="text-left text-zinc-700 hover:text-od-accent hover:underline disabled:opacity-50"
              title={tStatic("feature.content.entrylistpage.filter_by_value", { value1: term.name })}
            >
              {term.name}
            </button>
            {idx < matched.length - 1 && <span className="text-zinc-400">,</span>}
          </span>
        ))}
      </span>
    );
  };
  const selectedOptionLabel = (kind: string, id: number): string => {
    const term = resolved?.labels.get(id);
    return term?.kind === kind ? term.name : `#${id}`;
  };
  const customDefinitions = terms?.definitions.filter(def => def.key !== 'category' && def.key !== 'tag') ?? [];
  const selectedTaxonomy = filters.taxonomy_key || taxonomyChoice;
  const chosenDefinition = terms?.definitions.find(def => def.key === selectedTaxonomy);

  const captionId = `entry-table-caption-${typeKey}`;
  const activeSort = filters.sort ?? (isTrashView ? 'trashed_at' : 'updated_at');
  const activeOrder = filters.order ?? 'desc';
  const ariaSortFor = (field: 'title' | 'updated_at' | 'trashed_at'): 'ascending' | 'descending' | undefined =>
    activeSort === field ? (activeOrder === 'asc' ? 'ascending' : 'descending') : undefined;
  const bulkRemainingRetryCount = useMemo(() => {
    if (!bulkBatchMeta || bulkBatchMeta.scope !== scope || dataRevision <= bulkBatchMeta.reconciledAfterRetry) {
      return 0;
    }
    const retryIds = new Set(bulkResults.filter(item => item.status !== 'succeeded').map(item => item.id));
    return currentRows.filter(row => {
      if (!retryIds.has(row.id)) return false;
      if (bulkBatchMeta.kind === 'published' && row.publication_status === 'published') return false;
      if (bulkBatchMeta.kind === 'draft' && row.publication_status === 'draft') return false;
      return true;
    }).length;
  }, [bulkBatchMeta, scope, dataRevision, bulkResults, currentRows]);

  return (
    <section className="w-full min-w-0 space-y-3.5" data-testid={`entry-list-${typeKey}`} data-type-key={typeKey}>
      <div className="flex flex-wrap items-center justify-between gap-2.5">
        <div className="flex flex-wrap items-center gap-1.5" aria-label={tStatic("feature.content.entrylistpage.filter_status")} role="group">
          {statuses.map(status => {
            const count = status === 'trash'
              ? (data?.status_counts.trash ?? 0)
              : (data?.status_counts[status] ?? 0);
            const label = status === 'all'
              ? tStatic("feature.media.mediapicker.all")
              : status === 'draft'
                ? tStatic("feature.content.entrylistpage.draft")
                : status === 'published'
                  ? tStatic("reading.published")
                  : tStatic("entry_lifecycle.tab_trash");
            const active = (filters.status ?? 'all') === status;
            return (
              <AdminButton
                key={status}
                variant={active ? 'primary' : 'secondary'}
                size="sm"
                aria-pressed={active}
                data-testid={`entry-status-tab-${status}`}
                disabled={bulkBusy}
                onClick={() => updateFilter({ status })}
              >
                {label} <span className={active ? 'opacity-80' : 'text-zinc-500'}>({formatNumber(count)})</span>
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
            {tStatic("reading.reload")}
          </AdminButton>
          <AdminButton
            variant="primary"
            size="sm"
            disabled={bulkBusy}
            onClick={() => onNavigateHash(`${base}/new${editSuffix}`)}
          >
            {tStatic("feature.content.entrylistpage.add_new")}
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
            <label htmlFor={`entry-table-search-${typeKey}`} className="flex min-w-0 flex-1 flex-col gap-1 text-xs font-medium text-zinc-600">
              <span>{tStatic("feature.content.entrylistpage.search_title_or_slug")}</span>
              <input id={`entry-table-search-${typeKey}`} type="search" className={`${control} w-full`}
                value={searchInput} maxLength={200} disabled={bulkBusy}
                onChange={event => setSearchInput(event.target.value)}
                placeholder={tStatic("feature.content.entrylistpage.enter_keywords")} />
            </label>
            <AdminButton type="submit" variant="secondary" disabled={bulkBusy}>{tStatic("feature.content.entrylistpage.search")}</AdminButton>
          </form>

          {localeState.hasMultipleLocales && (
            <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
              <span>{tStatic("feature.content.entrylistpage.language")}</span>
              <select
                aria-label={tStatic("feature.content.entrylistpage.filter_language")}
                data-testid="entry-locale-filter"
                className={`${control} w-full`}
                value={filters.locale ?? 'all'}
                disabled={bulkBusy}
                onChange={(event) =>
                  updateFilter({
                    locale: event.target.value === 'all' ? undefined : event.target.value,
                  })
                }
              >
                <option value="all">{tStatic("feature.content.entrylistpage.all_languages")}</option>
                {localeState.allLocales.map((loc) => (
                  <option key={loc.code} value={loc.code}>
                    {loc.label} ({loc.code})
                  </option>
                ))}
              </select>
            </label>
          )}

          {!isTrashView && terms?.categoryEnabled && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{tStatic("feature.content.entrylistpage.category")}</span>
            <select aria-label={tStatic("feature.content.entrylistpage.filter_category")} className={`${control} w-full`}
              value={filters.category_id ?? ''} disabled={bulkBusy}
              onFocus={() => loadFilterOptions('category')}
              onChange={event => updateFilter({
                category_id: event.target.value ? Number(event.target.value) : undefined,
                taxonomy_key: undefined, term_id: undefined,
                include_descendants: false,
              })}>
              <option value="">{tStatic("feature.content.entrylistpage.all_categories")}</option>
              {filters.category_id && !terms.byKind.category?.some(term => term.id === filters.category_id) &&
                <option value={filters.category_id}>{selectedOptionLabel('category', filters.category_id)}</option>}
              {terms.byKind.category?.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms.byKind.category && <option disabled>{tStatic("feature.content.entrylistpage.tap_to_load_categories")}</option>}
            </select>
          </label>}

          {!isTrashView && terms?.tagEnabled && <label className="flex min-w-[130px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{tStatic("shell.tags")}</span>
            <select aria-label={tStatic("feature.content.entrylistpage.filter_tags")} className={`${control} w-full`}
              value={filters.tag_id ?? ''} disabled={bulkBusy}
              onFocus={() => loadFilterOptions('tag')}
              onChange={event => updateFilter({ tag_id: event.target.value ? Number(event.target.value) : undefined,
                taxonomy_key: undefined, term_id: undefined,
                // A tag is an AND filter, not a reset of the selected category
                // subtree. Preserve descendants only when a category is active.
                include_descendants: Boolean(filters.category_id && filters.include_descendants) })}>
              <option value="">{tStatic("feature.content.entrylistpage.all_tags")}</option>
              {filters.tag_id && !terms.byKind.tag?.some(term => term.id === filters.tag_id) &&
                <option value={filters.tag_id}>{selectedOptionLabel('tag', filters.tag_id)}</option>}
              {terms.byKind.tag?.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms.byKind.tag && <option disabled>{tStatic("feature.content.entrylistpage.tap_to_load_tags")}</option>}
            </select>
          </label>}

          {!isTrashView && customDefinitions.length > 0 && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{tStatic("feature.common.ui_taxonomy")}</span>
            <select aria-label={tStatic("feature.content.entrylistpage.choose_taxonomy_to_filter")} className={`${control} w-full`}
              disabled={bulkBusy} value={selectedTaxonomy}
              onChange={event => {
                setTaxonomyChoice(event.target.value);
                updateFilter({ taxonomy_key: undefined,
                  term_id: undefined, category_id: undefined, tag_id: undefined, include_descendants: false });
              }}>
              <option value="">{tStatic("feature.content.entrylistpage.all_taxonomies")}</option>
              {customDefinitions.map(def => <option key={def.key} value={def.key}>{def.label}</option>)}
            </select>
          </label>}

          {!isTrashView && chosenDefinition && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{chosenDefinition.label}</span>
            <select aria-label={tStatic("feature.content.entrylistpage.filter_by_value", { value1: chosenDefinition.label })} className={`${control} w-full`}
              disabled={bulkBusy} value={filters.term_id ?? ''}
              onFocus={() => loadFilterOptions(chosenDefinition.key)}
              onChange={event => {
                setTaxonomyChoice(selectedTaxonomy);
                updateFilter({ taxonomy_key: event.target.value ? selectedTaxonomy : undefined,
                  term_id: event.target.value ? Number(event.target.value) : undefined,
                  include_descendants: false });
              }}>
              <option value="">{tStatic("feature.content.entrylistpage.choose_term")}</option>
              {filters.term_id && !terms?.byKind[chosenDefinition.key]?.some(term => term.id === filters.term_id) &&
                <option value={filters.term_id}>{selectedOptionLabel(chosenDefinition.key, filters.term_id)}</option>}
              {(terms?.byKind[chosenDefinition.key] ?? []).map(term =>
                <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms?.byKind[chosenDefinition.key] && <option disabled>{tStatic("feature.content.entrylistpage.tap_to_load_terms")}</option>}
            </select>
          </label>}

          {!isTrashView && <label className="flex min-w-[145px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{tStatic("feature.content.entrylistpage.month_utc")}</span>
            <input type="month" aria-label={tStatic("feature.content.entrylistpage.filter_by_update_month")} className={`${control} w-full`}
              value={filters.updated_month ?? ''} disabled={bulkBusy}
              onChange={event => updateFilter({ updated_month: event.target.value || undefined })} />
          </label>}

          <label className="flex w-[88px] flex-col gap-1 text-xs font-medium text-zinc-600">
            <span>{tStatic("feature.content.entrylistpage.per_page")}</span>
            <select aria-label={tStatic("feature.content.entrylistpage.items_per_page")} className={`${control} w-full`}
              value={pageSize} disabled={bulkBusy}
              onChange={event => updateFilter({ limit: Number(event.target.value) })}>
              {[10, 20, 50, 100].map(size => <option key={size} value={size}>{size}</option>)}
            </select>
          </label>
        </div>

        {!isTrashView && ((filters.category_id && terms?.categoryEnabled) || (filters.term_id && chosenDefinition?.hierarchical)) ? (
          <div className="flex flex-wrap items-center gap-4 border-t border-zinc-100 pt-2.5">
            {filters.category_id && terms?.categoryEnabled && <label className="flex items-center gap-2 text-xs text-zinc-700">
              <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
                onChange={event => updateFilter({ include_descendants: event.target.checked })} />
              {tStatic("feature.content.entrylistpage.include_child_categories")}
            </label>}
            {filters.term_id && chosenDefinition?.hierarchical && <label className="flex items-center gap-2 text-xs text-zinc-700">
              <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
                onChange={event => updateFilter({ include_descendants: event.target.checked })} />
              {tStatic("feature.content.entrylistpage.include_child_terms_of")}{' '}{chosenDefinition.label}
            </label>}
          </div>
        ) : null}

        <div className="flex flex-wrap items-center justify-between gap-2.5 border-t border-zinc-100 pt-2.5">
          {!isTrashView ? (
            <div className="flex flex-wrap items-center gap-2" role="group" aria-label={tStatic("feature.content.entrylistpage.bulk_actions")}>
              <span className="text-xs font-medium text-zinc-500">{formatNumber(selectedRows.length)} {tStatic("feature.content.entrylistpage.selected")}</span>
              {isBuiltIn && <>
                <AdminButton variant="secondary" size="sm" disabled={bulkBusy || !csrfToken || !selectedRows.length}
                  onClick={() => void performBulk('published')}>{tStatic("feature.content.entrylistpage.publish_selected")}</AdminButton>
                <AdminButton variant="secondary" size="sm" disabled={bulkBusy || !csrfToken || !selectedRows.length}
                  onClick={() => void performBulk('draft')}>{tStatic("feature.content.entrylistpage.move_to_draft_2")}</AdminButton>
              </>}
              <AdminButton variant="danger" size="sm" data-testid="bulk-move-to-trash-btn"
                disabled={bulkBusy || !csrfToken || !selectedRows.length}
                onClick={() => void performBulk('trash')}>{tStatic("entry_lifecycle.move_selected_to_trash")}</AdminButton>
              {bulkBusy && (
                <>
                  <span role="status" className="text-xs text-zinc-600">{tStatic("feature.content.entrylistpage.processing_each_item_separately")}</span>
                  <AdminButton
                    variant="secondary"
                    size="sm"
                    data-testid="bulk-cancel-btn"
                    disabled={bulkCancelling}
                    onClick={() => {
                      bulkCancelRef.current = true;
                      setBulkCancelling(true);
                      bulkAbortRef.current?.abort();
                    }}
                  >
                    {bulkCancelling ? tStatic("entry_lifecycle.bulk_cancelling") : tStatic("entry_lifecycle.bulk_cancel")}
                  </AdminButton>
                </>
              )}
            </div>
          ) : <div />}
          <div className="flex flex-wrap items-center gap-2.5 text-xs">
            <span className="text-zinc-500" role="status">{data ? tStatic("feature.content.entrylistpage.total_value_items", { value1: formatNumber(data.total) }) : tStatic("feature.content.entrylistpage.loading_counts")}</span>
            <AdminButton variant="ghost" size="sm" disabled={bulkBusy}
              onClick={() => { setSearchInput(''); setTaxonomyChoice(''); onNavigateHash(base); }}>
              {tStatic("feature.content.entrylistpage.clear_filters")}
            </AdminButton>
          </div>
        </div>
      </div>

      {bulkBlockMessage && <AdminAlert tone="warning" role="alert" data-testid="bulk-block-alert">{bulkBlockMessage}</AdminAlert>}

      {pendingProviderConfirmation && (
        <PluginLifecycleConfirmation
          pluginId={pendingProviderConfirmation.pluginId}
          actionId={pendingProviderConfirmation.actionId}
          confirmation={pendingProviderConfirmation.confirmation}
          csrfToken={csrfToken}
          onResolved={() => {
            const rowId = pendingProviderConfirmation.rowId;
            setPendingProviderConfirmation(null);
            setProviderNotice(tStatic("feature.content.entrylistpage.action_completed_for_value", { value1: rowId }));
            setRetry(old => old + 1);
          }}
          onCancelled={() => {
            setPendingProviderConfirmation(null);
            setProviderNotice(tStatic("plugin_lifecycle.cancelled_notice"));
          }}
          onFailed={message => {
            const rowId = pendingProviderConfirmation.rowId;
            setPendingProviderConfirmation(null);
            setProviderNotice(`#${rowId}: ${message}`);
          }}
        />
      )}

      {termLoadErrors.length > 0 && <AdminAlert tone="warning" role="alert">
        {tStatic("feature.content.entrylistpage.could_not_load_some_terms")}{' '}{termLoadErrors.join('; ')}{tStatic("feature.content.entrylistpage.other_filters_remain_available")}
      </AdminAlert>}
      {resolved?.failed && <AdminAlert tone="warning" role="alert">
        {tStatic("feature.content.entrylistpage.could_not_load_some_term_names_on_this_page")}
      </AdminAlert>}

      {bulkResults.length > 0 && (() => {
        const counts = {
          succeeded: bulkResults.filter(r => r.status === 'succeeded').length,
          stale: bulkResults.filter(r => r.status === 'stale').length,
          failed: bulkResults.filter(r => r.status === 'failed').length,
          uncertain: bulkResults.filter(r => r.status === 'uncertain').length,
          unattempted: bulkResults.filter(r => r.status === 'unattempted').length,
        };
        return (
          <div className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3" aria-label={tStatic("feature.content.entrylistpage.bulk_results")} data-testid="bulk-results-panel">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-xs font-semibold text-zinc-900">
                {tStatic("entry_lifecycle.bulk_summary", {
                  succeeded: formatNumber(counts.succeeded),
                  stale: formatNumber(counts.stale),
                  failed: formatNumber(counts.failed),
                  uncertain: formatNumber(counts.uncertain),
                  unattempted: formatNumber(counts.unattempted),
                })}
              </p>
              {bulkRemainingRetryCount > 0 && (
                <AdminButton
                  variant="secondary"
                  size="sm"
                  data-testid="bulk-reconcile-retry-btn"
                  disabled={bulkBusy || loading || !csrfToken}
                  onClick={retryRemainingBulk}
                >
                  {tStatic("entry_lifecycle.bulk_reconcile_retry", { value1: formatNumber(bulkRemainingRetryCount) })}
                </AdminButton>
              )}
            </div>
            {bulkRemainingRetryCount > 0 && (
              <p className="text-xs text-zinc-600">{tStatic("entry_lifecycle.bulk_reconciling_notice")}</p>
            )}
            <ul className="space-y-1 text-xs">
              {bulkResults.map(result => (
                <li
                  key={result.id}
                  data-bulk-id={result.id}
                  data-bulk-status={result.status}
                  role={result.status === 'succeeded' ? 'status' : 'alert'}
                  className={
                    result.status === 'succeeded'
                      ? 'text-emerald-700'
                      : result.status === 'uncertain' || result.status === 'stale'
                        ? 'text-amber-800'
                        : result.status === 'unattempted'
                          ? 'text-zinc-600'
                          : 'text-rose-700'
                  }
                >
                  {result.message}
                </li>
              ))}
            </ul>
          </div>
        );
      })()}

      {providerNotice && <AdminAlert tone="info" data-testid="provider-action-result">{providerNotice}</AdminAlert>}
      {providerError && <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
        <span>{tStatic("feature.content.entrylistpage.could_not_load_extension_columns")}{' '}{providerError}</span>
        <AdminButton variant="ghost" size="sm" onClick={() => setRetry(n => n + 1)}>{tStatic("feature.dashboard.dashboardpage.retry")}</AdminButton>
      </AdminAlert>}

      {error && <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
        <span>{error}</span>
        <AdminButton variant="ghost" size="sm" onClick={() => setRetry(prev => prev + 1)}>{tStatic("feature.dashboard.dashboardpage.retry")}</AdminButton>
      </AdminAlert>}

      {loading && <p role="status" className="text-xs text-zinc-500">{tStatic("feature.content.entrylistpage.loading_list_from_server")}</p>}

      <div
        role="region"
        aria-labelledby={captionId}
        tabIndex={0}
        className="w-full min-w-0 max-w-full overflow-x-auto rounded-xl border border-zinc-200 bg-white focus-visible:outline-2 focus-visible:outline-od-accent"
        aria-busy={loading}
      >
        <table className="w-full min-w-[780px] table-auto text-left text-sm" data-testid="entry-table">
          <caption id={captionId} className="sr-only">{tStatic("feature.content.entrylistpage.content_table")}{' '}{typeKey}{tStatic("feature.content.entrylistpage.server_side_search_status_and_pagination")}</caption>
          <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold uppercase tracking-wider text-zinc-600">
            <tr>
              {!isTrashView && (
                <th scope="col" className="w-10 px-3 py-2.5">
                  <input type="checkbox" aria-label={tStatic("feature.content.entrylistpage.select_all_on_page")} checked={allSelected}
                    disabled={!currentRows.length || bulkBusy || loading}
                    onChange={event => setSelected(prev => {
                      const next = new Set(prev);
                      currentIds.forEach(id => { if (event.target.checked) next.add(id); else next.delete(id); });
                      return next;
                    })} />
                </th>
              )}
              {!isTrashView && <th scope="col" className="w-14 px-2 py-2.5 text-center">{tStatic("feature.media.mediapicker.images")}</th>}
              <th scope="col" aria-sort={ariaSortFor('title')} className="min-w-[220px] px-3 py-2.5">
                <button type="button" data-testid="sort-title" className={sortButton}
                  disabled={bulkBusy} onClick={() => sortBy('title')}>
                  <span>{tStatic("feature.contact.contactpanel.title")}</span>
                  {activeSort === 'title' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                </button>
              </th>
              <th scope="col" className="whitespace-nowrap px-3 py-2.5">{tStatic("multilingual.status")}</th>
              {!isTrashView && terms?.categoryEnabled && <th scope="col" className="px-3 py-2.5">{tStatic("feature.content.entrylistpage.category")}</th>}
              {!isTrashView && terms?.tagEnabled && <th scope="col" className="px-3 py-2.5">{tStatic("shell.tags")}</th>}
              {!isTrashView && customDefinitions.map(def => <th key={def.key} scope="col" className="px-3 py-2.5">{def.label}</th>)}
              {!isTrashView && providerColumns.map(column => <th key={column.key} scope="col"
                className="min-w-[120px] px-3 py-2.5">{column.label}</th>)}
              {isTrashView ? (
                <th scope="col" aria-sort={ariaSortFor('trashed_at')} className="min-w-[140px] whitespace-nowrap px-3 py-2.5">
                  <button type="button" data-testid="sort-trashed" className={sortButton}
                    disabled={bulkBusy} onClick={() => sortBy('trashed_at')}>
                    <span>{tStatic("entry_lifecycle.trashed_date")}</span>
                    {activeSort === 'trashed_at' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                  </button>
                </th>
              ) : (
                <th scope="col" aria-sort={ariaSortFor('updated_at')} className="min-w-[140px] whitespace-nowrap px-3 py-2.5">
                  <button type="button" data-testid="sort-updated" className={sortButton}
                    disabled={bulkBusy} onClick={() => sortBy('updated_at')}>
                    <span>{tStatic("feature.content.entrylistpage.updated_date")}</span>
                    {activeSort === 'updated_at' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                  </button>
                </th>
              )}
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100">
            {!loading && !error && currentRows.length === 0 && <tr>
              <td colSpan={isTrashView ? 3 : 5 + (terms?.definitions.length ?? 0) + providerColumns.length}
                className="px-4 py-8 text-center text-sm text-zinc-500">
                {data?.total === 0 ? tStatic("feature.content.entrylistpage.no_matching_content_clear_filters_to_see_more") : tStatic("feature.content.entrylistpage.no_items_on_this_page")}
              </td>
            </tr>}
            {currentRows.map(row => {
              const path = !isTrashView && row.publication_status === 'published' ? safePublicPath(row.public_url) : null;
              const thumbId = !isTrashView && typeof row.fields?.featured_image === 'string' ? row.fields.featured_image.trim() : '';
              const readingBlocked = row.blocked_reasons?.trash === 'reading_page_assigned';
              const restoreLocaleBlocked = row.blocked_reasons?.restore === 'locale_disabled';
              return <tr key={row.id} data-entry-id={row.id} className="align-middle transition-colors hover:bg-zinc-50/70">
                {!isTrashView && (
                  <td className="px-3 py-2.5"><input type="checkbox" aria-label={tStatic("feature.content.entrylistpage.select_item_value", { value1: row.id })}
                    checked={selected.has(row.id)} disabled={bulkBusy}
                    onChange={event => toggleRow(row.id, event.target.checked)} /></td>
                )}
                {!isTrashView && (
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
                )}
                <td className="break-words px-3 py-2.5">
                  <div className="leading-snug">
                    {isTrashView ? (
                      <span className="inline text-left text-sm font-semibold text-zinc-900">
                        {row.title}
                      </span>
                    ) : (
                      <button
                        type="button"
                        disabled={bulkBusy}
                        onClick={() => onNavigateHash(`${base}/${row.id}/edit${editSuffix}`)}
                        className="inline text-left text-sm font-semibold text-zinc-950 hover:text-od-accent"
                      >
                        {row.title}
                      </button>
                    )}
                    <span className="ml-1.5 inline-block align-middle rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-[11px] font-medium text-zinc-500">
                      #{row.id}
                    </span>
                    {isTrashView && (
                      <span className="ml-1.5 inline-block align-middle font-mono text-[11px] text-zinc-500">
                        /{row.slug}
                      </span>
                    )}
                    {localeState.hasMultipleLocales && row.locale && (
                      <span className="ml-1.5 inline-block align-middle">
                        <AdminBadge tone="neutral">{row.locale}</AdminBadge>
                      </span>
                    )}
                  </div>
                  {isTrashView ? (
                    <div className="mt-1 space-y-1">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                        {csrfToken && (
                          <button
                            type="button"
                            data-testid={`entry-restore-btn-${row.id}`}
                            className="font-medium text-od-accent hover:underline disabled:opacity-50"
                            disabled={bulkBusy || row.capabilities?.restore === false}
                            onClick={() => void restoreSingleRow(row)}
                          >
                            {tStatic("entry_lifecycle.restore")}
                          </button>
                        )}
                        {csrfToken && user.role === 'admin' && row.capabilities?.purge !== false && (
                          <>
                            <span className="text-zinc-300" aria-hidden="true">·</span>
                            <button
                              type="button"
                              data-testid={`entry-purge-btn-${row.id}`}
                              className="font-medium text-rose-600 hover:text-rose-700 hover:underline disabled:opacity-50"
                              disabled={bulkBusy}
                              onClick={() => void purgeSingleRow(row)}
                            >
                              {tStatic("entry_lifecycle.purge_permanently")}
                            </button>
                          </>
                        )}
                      </div>
                      {restoreLocaleBlocked && (
                        <p className="text-[11px] text-amber-800">
                          {tStatic("entry_lifecycle.locale_disabled_restore_blocked")}
                        </p>
                      )}
                    </div>
                  ) : (
                    <div className="mt-1 space-y-1">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                        <button
                          type="button"
                          className="font-medium text-od-accent hover:underline"
                          disabled={bulkBusy}
                          aria-label={tStatic("feature.content.entrylistpage.edit_value_id_value", { value1: row.title, value2: row.id })}
                          onClick={() => onNavigateHash(`${base}/${row.id}/edit${editSuffix}`)}
                        >
                          {tStatic("shell.edit")}
                        </button>
                        {path && (
                          <>
                            <span className="text-zinc-300" aria-hidden="true">·</span>
                            <a
                              href={path}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="font-medium text-zinc-600 hover:text-zinc-950 hover:underline"
                              aria-label={tStatic("feature.content.entrylistpage.view_public_value", { value1: row.title })}
                            >
                              {tStatic("feature.common.view")}
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
                              aria-label={tStatic("feature.content.entrylistpage.preview_saved_draft_value", { value1: row.title })}
                            >
                              {tStatic("feature.menus.menuspanel.preview")}
                            </a>
                          </>
                        )}
                        {csrfToken && (
                          <>
                            <span className="text-zinc-300" aria-hidden="true">·</span>
                            <button
                              type="button"
                              data-testid={`entry-trash-btn-${row.id}`}
                              className="font-medium text-rose-600 hover:text-rose-700 hover:underline disabled:cursor-not-allowed disabled:opacity-50"
                              disabled={bulkBusy || row.capabilities?.trash === false}
                              title={readingBlocked ? tStatic("entry_lifecycle.reading_page_blocked_hint") : undefined}
                              aria-label={tStatic("feature.content.entrylistpage.delete_value_id_value", { value1: row.title, value2: row.id })}
                              onClick={() => void trashSingleRow(row)}
                            >
                              {tStatic("entry_lifecycle.move_to_trash")}
                            </button>
                          </>
                        )}
                      </div>
                      {readingBlocked && (
                        <p className="text-[11px] text-amber-800" data-testid={`reading-blocked-hint-${row.id}`}>
                          {tStatic("entry_lifecycle.reading_page_blocked_hint")}{' '}
                          {user.role === 'admin' && (
                            <button
                              type="button"
                              data-testid={`reading-settings-link-${row.id}`}
                              onClick={() => onNavigateHash('#/settings/reading')}
                              className="font-medium text-od-accent underline"
                            >
                              {tStatic("entry_lifecycle.open_reading_settings")}
                            </button>
                          )}
                        </p>
                      )}
                    </div>
                  )}
                </td>
                <td className="whitespace-nowrap px-3 py-2.5">
                  <AdminBadge tone={row.publication_status === 'published' ? 'success' : 'neutral'}>
                    {row.publication_status === 'published' ? tStatic("reading.published") : tStatic("feature.content.entrylistpage.draft")}
                  </AdminBadge>
                </td>
                {!isTrashView && terms?.categoryEnabled && <td className="break-words px-3 py-2.5 text-zinc-700">{renderTermsCell(row, 'category')}</td>}
                {!isTrashView && terms?.tagEnabled && <td className="break-words px-3 py-2.5 text-zinc-700">{renderTermsCell(row, 'tag')}</td>}
                {!isTrashView && customDefinitions.map(def => <td key={def.key} className="break-words px-3 py-2.5 text-zinc-700">{renderTermsCell(row, def.key)}</td>)}
                {!isTrashView && providerColumns.map(column => <td key={column.key} className="break-words px-3 py-2.5">
                  <AdminColumnCell column={column} row={row} busy={providerBusy || bulkBusy}
                    onRun={(col, entry) => void performProviderAction(col, entry)} />
                </td>)}
                <td className="whitespace-nowrap px-3 py-2.5 text-xs tabular-nums text-zinc-600">
                  {isTrashView
                    ? (row.trashed_at ? `${row.trashed_at.slice(0, 16)} UTC` : '—')
                    : `${row.updated_at.slice(0, 16)} UTC`}
                </td>
              </tr>;
            })}
          </tbody>
        </table>
      </div>
      <nav aria-label={tStatic("feature.content.entrylistpage.content_pagination")} className="flex flex-wrap items-center justify-between gap-3 text-xs text-zinc-600">
        <span data-testid="entry-table-page-info">{data ? tStatic("feature.content.entrylistpage.showing_value_value_value", { value1: formatNumber(data.total ? offset + 1 : 0), value2: formatNumber(Math.min(offset + currentRows.length, data.total)), value3: formatNumber(data.total) }) : tStatic("feature.content.entrylistpage.page_not_loaded")}</span>
        <div className="flex items-center gap-2">
          <AdminButton variant="secondary" size="sm" disabled={bulkBusy || loading || offset === 0}
            onClick={() => updateFilter({ offset: Math.max(0, offset - pageSize) }, false)}>{tStatic("feature.content.contentworkspace.previous_page")}</AdminButton>
          <span className="tabular-nums">{tStatic("feature.common.page_number", { page: formatNumber(Math.floor(offset / pageSize) + 1) })}{data ? ` / ${formatNumber(Math.max(1, Math.ceil(data.total / pageSize)))}` : ''}</span>
          <AdminButton variant="secondary" size="sm"
            disabled={bulkBusy || loading || !data || offset + pageSize >= data.total || offset + pageSize > 1000000}
            onClick={() => updateFilter({ offset: offset + pageSize }, false)}>{tStatic("feature.common.next_page")}</AdminButton>
        </div>
      </nav>
    </section>
  );
}
