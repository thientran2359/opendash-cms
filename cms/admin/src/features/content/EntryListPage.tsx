import { formatNumber } from '../../i18n/format.ts';
import { __, _x, useI18n } from '../../i18n/index.tsx';
import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import {
  effectiveContentTypeSettings,
  getEffectiveContentTypeTaxonomies,
  listContentTypes,
  purgeContentEntry, queryContentEntryTable, getEffectiveEntryListConfig,
  restoreContentEntry, runEntryProviderAction, contentEntryPreviewPath, trashContentEntry,
  type ContentTypeSettings,
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
import { editorDraftStore } from '../editor/EditorDraftStore.ts';
import { PluginLifecycleConfirmation } from '../plugins/PluginLifecycleConfirmation.tsx';
import {
  buildBulkConfirmationPrompt,
  captureBulkRows,
  reconcileBulkResults,
  runSequentialBulk,
  validateBulkSelection,
  type BulkActionSpec,
  type BulkResultItem,
} from './bulkEntryActions.ts';
import { QuickEditForm } from './QuickEditForm.tsx';
import {
  buildHierarchicalDisplayRows,
  EntryListColumnCell,
  entryTableHash,
  parseEntryTableHash,
  readableEntryTableError as readableError,
  resolveListColumnLabel,
  safePublicPath,
  type TaxonomyTableQuery,
} from './EntryListColumns.tsx';
import type { EntryTableQuery, EntryTableResult, EntryTableRow, TaxonomyDeltaOp } from './types.ts';

export { entryTableHash, parseEntryTableHash, safePublicPath };

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

function rootHash(typeKey: string): string {
  return typeKey === 'post' ? '#/posts' : typeKey === 'page' ? '#/pages' : `#/cpt/${encodeURIComponent(typeKey)}`;
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
  const [typeSettingsState, setTypeSettingsState] = useState<{
    accountScope: string; settings: ContentTypeSettings;
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
    spec: BulkActionSpec;
    scope: string;
    reconciledAfterRetry: number;
  } | null>(null);
  const [bulkTaxOpen, setBulkTaxOpen] = useState(false);
  const [bulkTaxKey, setBulkTaxKey] = useState('');
  const [bulkTaxOp, setBulkTaxOp] = useState<TaxonomyDeltaOp>('add');
  const [bulkTaxTermIds, setBulkTaxTermIds] = useState<number[]>([]);
  const [bulkTaxCatalog, setBulkTaxCatalog] = useState<Record<string, TaxonomyTerm[]>>({});
  const [bulkTaxLoading, setBulkTaxLoading] = useState(false);
  const [dataRevision, setDataRevision] = useState(0);
  const [quickEditRowId, setQuickEditRowId] = useState<number | null>(null);
  const quickEditDirtyRef = useRef(false);
  const [quickEditNotice, setQuickEditNotice] = useState('');
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
    if (quickEditRowId !== null && quickEditDirtyRef.current) {
      if (!window.confirm(__("Discard unsaved Quick Edit changes?"))) return;
      editorDraftStore.deleteQuickEdit(typeKey, quickEditRowId);
      quickEditDirtyRef.current = false;
      setQuickEditRowId(null);
    }
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
    setBulkTaxOpen(false);
    setBulkTaxKey('');
    setBulkTaxOp('add');
    setBulkTaxTermIds([]);
    setPendingProviderConfirmation(null);
    setQuickEditRowId(null);
    quickEditDirtyRef.current = false;
    setQuickEditNotice('');
  }, [scope]);

  // Auto-reopen an authorized quarantined/active Quick Edit draft for this type after re-login.
  useEffect(() => {
    if (!data || isTrashView || quickEditRowId !== null) return;
    const saved = editorDraftStore.findQuickEditForType(siteInstanceId, user.id, typeKey);
    if (saved && data.items.some(item => item.id === saved.entry_id)) {
      setQuickEditRowId(saved.entry_id);
    }
  }, [data, isTrashView, quickEditRowId, siteInstanceId, user.id, typeKey]);

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
        if (cause instanceof ApiClientError && cause.status === 401) onApiError(cause, __("Your session expired."));
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
        onApiError(cause, __("Could not load taxonomy filters."));
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
        onApiError(cause, __("Your session expired while loading taxonomy filters."));
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
              onApiError(cause, __("Your session expired while loading term names."));
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
    void Promise.all([
      getEffectiveEntryListConfig(typeKey, controller.signal),
      listContentTypes().catch(() => null),
    ])
      .then(([config, typesList]) => {
        if (!active || controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setProviderState({ accountScope, config });
        const matched = typesList?.find(item => item.descriptor.key === typeKey);
        setTypeSettingsState({
          accountScope,
          settings: effectiveContentTypeSettings(typeKey, matched?.descriptor.settings),
        });
      })
      .catch(cause => {
        if (!active || controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setProviderError(readableError(cause));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, __("Your session expired while loading provider columns."));
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
  const supportsDraftPublish =
    isBuiltIn ||
    currentRows.some(row => Boolean(row.capabilities?.publish || row.capabilities?.unpublish));

  const selectedLocaleCheck = useMemo(() => validateBulkSelection(selectedRows), [selectedRows]);
  const bulkTaxCatalogKey =
    bulkTaxKey && selectedLocaleCheck.ok ? `${bulkTaxKey}:${selectedLocaleCheck.locale}` : '';

  useEffect(() => {
    if (!bulkTaxOpen || !bulkTaxKey || !selectedLocaleCheck.ok) return;
    if (Object.prototype.hasOwnProperty.call(bulkTaxCatalog, bulkTaxCatalogKey)) return;
    const controller = new AbortController();
    setBulkTaxLoading(true);
    void listAllTaxonomyTerms(bulkTaxKey, controller.signal, selectedLocaleCheck.locale || null)
      .then(found => {
        if (controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setBulkTaxCatalog(prev => ({ ...prev, [bulkTaxCatalogKey]: found }));
      })
      .catch(cause => {
        if (controller.signal.aborted || accountScopeRef.current !== accountScope) return;
        setBulkBlockMessage(readableError(cause));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, __("Could not load terms."));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setBulkTaxLoading(false);
      });
    return () => controller.abort();
  }, [bulkTaxOpen, bulkTaxKey, bulkTaxCatalogKey, selectedLocaleCheck, accountScope]);

  const runSafeSelectedBatch = async (
    spec: BulkActionSpec,
    targetRows: EntryTableRow[],
    skipConfirm = false,
  ) => {
    if (!csrfToken || bulkBusyRef.current || !targetRows.length) return;
    if ((spec.kind === 'published' || spec.kind === 'draft') && !supportsDraftPublish) return;
    if (spec.kind === 'purge' && user.role !== 'admin') return;
    const check = validateBulkSelection(targetRows);
    if (!check.ok) {
      setBulkBlockMessage(check.message);
      return;
    }
    const captured = captureBulkRows(targetRows);
    const effectiveSpec: BulkActionSpec =
      spec.kind === 'restore'
        ? {
            kind: 'restore',
            confirmPublicRestore: captured.some(row => row.immediate_public_restore),
          }
        : spec;

    if (!skipConfirm) {
      const confirmPrompt = buildBulkConfirmationPrompt(
        effectiveSpec,
        captured.length,
        check.locale,
      );
      if (!window.confirm(confirmPrompt)) return;
    }

    const startingScope = scope;
    bulkBusyRef.current = true;
    bulkCancelRef.current = false;
    setBulkBusy(true);
    setBulkCancelling(false);
    setBulkBlockMessage('');
    setBulkResults([]);
    setBulkBatchMeta(null);
    const beforeRefreshRevision = dataRevision;
    try {
      const outcome = await runSequentialBulk({
        typeKey,
        spec: effectiveSpec,
        capturedRows: captured,
        csrfToken,
        startingScope,
        getCurrentScope: () => scopeRef.current,
        isAlive: () => aliveRef.current,
        isCancelled: () => bulkCancelRef.current,
        setActiveAbort: controller => {
          bulkAbortRef.current = controller;
        },
        formatError: readableError,
        onUnauthorized: cause =>
          onApiError(
            cause,
            __("Your session expired during the bulk update."),
          ),
      });
      if (!aliveRef.current || scopeRef.current !== startingScope) return;
      const succeeded = new Set(
        outcome.reports.filter(report => report.status === 'succeeded').map(report => report.id),
      );
      setSelected(prev => new Set([...prev].filter(id => !succeeded.has(id))));
      setBulkResults(outcome.reports);
      setBulkBatchMeta({
        spec: effectiveSpec,
        scope: startingScope,
        reconciledAfterRetry: beforeRefreshRevision,
      });
      if (effectiveSpec.kind === 'taxonomy') {
        setBulkTaxOpen(false);
        setBulkTaxTermIds([]);
      }
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

  const performBulk = async (spec: BulkActionSpec) =>
    runSafeSelectedBatch(spec, selectedRows, false);

  const performBulkTaxonomy = async () => {
    if (!bulkTaxKey) return;
    if ((bulkTaxOp === 'add' || bulkTaxOp === 'remove') && bulkTaxTermIds.length === 0) {
      setBulkBlockMessage(__("Choose at least one term for Add or Remove."));
      return;
    }
    const defLabel = terms?.definitions.find(d => d.key === bulkTaxKey)?.label ?? bulkTaxKey;
    await runSafeSelectedBatch(
      {
        kind: 'taxonomy',
        taxonomyKey: bulkTaxKey,
        taxonomyLabel: defLabel,
        operation: bulkTaxOp,
        termIds: [...bulkTaxTermIds],
        schemaToken: data?.schema_token ?? '',
      },
      selectedRows,
      false,
    );
  };

  const reconciledBulk = useMemo(() => {
    if (
      !bulkBatchMeta ||
      bulkBatchMeta.scope !== scope ||
      dataRevision <= bulkBatchMeta.reconciledAfterRetry
    ) {
      return null;
    }
    return reconcileBulkResults(bulkBatchMeta.spec, bulkResults, currentRows);
  }, [bulkBatchMeta, scope, dataRevision, bulkResults, currentRows]);

  const retryRemainingBulk = () => {
    if (!bulkBatchMeta || !reconciledBulk || bulkBusyRef.current) return;
    if (!reconciledBulk.retryableRows.length) {
      setBulkResults(reconciledBulk.annotatedResults);
      return;
    }
    const nextSpec: BulkActionSpec =
      bulkBatchMeta.spec.kind === 'taxonomy'
        ? { ...bulkBatchMeta.spec, schemaToken: data?.schema_token ?? bulkBatchMeta.spec.schemaToken }
        : bulkBatchMeta.spec;
    void runSafeSelectedBatch(nextSpec, reconciledBulk.retryableRows, false);
  };

  const performProviderAction = async (column: ProviderColumn, row: EntryTableRow) => {
    if (providerBusyRef.current || bulkBusyRef.current || !csrfToken || !column.action_id ||
      column.owner_kind !== 'plugin') return;
    if (!window.confirm(__("Run action {{value1}} for {{value2}} (#{{value3}})? This action may change data.", { value1: column.label, value2: row.title, value3: row.id }))) return;
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
      setProviderNotice(response.success ? __("Action completed for #{{value1}}.", { value1: row.id }) : __("The action failed."));
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
          onApiError(cause, __("Your session expired while running the action."));
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
    if (!window.confirm(__("Move \"{{value1}}\" (#{{value2}}) to Trash? You can restore it later from Trash.", { value1: row.title, value2: row.id }))) return;
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
        onApiError(cause, __("Your session expired while deleting the item."));
      } else {
        setBulkResults([{ id: row.id, status: 'failed', beforeVersion: row.version, message: `#${row.id}: ${readableError(cause)}` }]);
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
      !window.confirm(__("Restore \"{{value1}}\" (#{{value2}})? This content type uses immediate publication and will become public immediately.", { value1: row.title, value2: row.id }))
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
        onApiError(cause, __("Could not restore the entry from Trash."));
      } else {
        setBulkResults([{ id: row.id, status: 'failed', beforeVersion: row.version, message: `#${row.id}: ${readableError(cause)}` }]);
      }
    } finally {
      bulkBusyRef.current = false;
      if (aliveRef.current) setBulkBusy(false);
    }
  };

  const purgeSingleRow = async (row: EntryTableRow) => {
    if (bulkBusyRef.current || !csrfToken || user.role !== 'admin' || row.capabilities?.purge === false) return;
    if (!window.confirm(_x("Permanently delete \"{{value1}}\" (#{{value2}})? This cannot be undone.", "entry_lifecycle.confirm_purge_row", { value1: row.title, value2: row.id }))) return;
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
        onApiError(cause, __("Could not permanently delete the entry."));
      } else {
        setBulkResults([{ id: row.id, status: 'failed', beforeVersion: row.version, message: `#${row.id}: ${readableError(cause)}` }]);
      }
    } finally {
      bulkBusyRef.current = false;
      if (aliveRef.current) setBulkBusy(false);
    }
  };

  const editSuffix = queryHash.includes('?') ? queryHash.slice(queryHash.indexOf('?')) : '';
  const renderTermsCell = (row: EntryTableRow, kind: string) => {
    if (!row.term_ids.length) return <span className="text-zinc-400">—</span>;
    if (!resolved) return <span className="text-xs text-zinc-400">{__("Loading…")}</span>;
    const matched = row.term_ids
      .map(id => {
        const term = resolved.labels.get(id);
        return term?.kind === kind ? { id, name: term.name } : null;
      })
      .filter((item): item is { id: number; name: string } => item !== null);
    if (!matched.length) {
      return <span className="text-zinc-400">{resolved.failed ? __("Could not load") : '—'}</span>;
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
              title={__("Filter by {{value1}}", { value1: term.name })}
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
  const ariaSortFor = (field: 'title' | 'updated_at' | 'trashed_at' | 'menu_order'): 'ascending' | 'descending' | undefined =>
    activeSort === field ? (activeOrder === 'asc' ? 'ascending' : 'descending') : undefined;
  const activeTypeSettings = typeSettingsState?.accountScope === accountScope
    ? typeSettingsState.settings
    : null;
  const listColumns = !isTrashView ? (activeTypeSettings?.list_columns ?? []) : [];
  const showHierarchyTree = Boolean(
    !isTrashView &&
      activeTypeSettings?.hierarchical &&
      !filters.q &&
      !filters.category_id &&
      !filters.tag_id &&
      !filters.term_id,
  );
  const displayRows = useMemo(
    () => buildHierarchicalDisplayRows(currentRows, showHierarchyTree),
    [currentRows, showHierarchyTree],
  );
  const rowTitleById = useMemo(
    () => new Map(currentRows.map(row => [row.id, row.title])),
    [currentRows],
  );
  const bulkRemainingRetryCount = reconciledBulk?.retryableRows.length ?? 0;
  const displayBulkResults = reconciledBulk?.annotatedResults ?? bulkResults;
  const unresolvedUncertainCount = reconciledBulk?.unresolvedUncertainIds.length ?? 0;

  return (
    <section className="w-full min-w-0 space-y-3.5" data-testid={`entry-list-${typeKey}`} data-type-key={typeKey}>
      <div className="flex flex-wrap items-center justify-between gap-2.5">
        <div className="flex flex-wrap items-center gap-1.5" aria-label={__("Filter status")} role="group">
          {statuses.map(status => {
            const count = status === 'trash'
              ? (data?.status_counts.trash ?? 0)
              : (data?.status_counts[status] ?? 0);
            const label = status === 'all'
              ? __("All")
              : status === 'draft'
                ? __("Draft")
                : status === 'published'
                  ? __("Published")
                  : __("Trash");
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
            {__("Reload")}
          </AdminButton>
          <AdminButton
            variant="primary"
            size="sm"
            disabled={bulkBusy}
            onClick={() => onNavigateHash(`${base}/new${editSuffix}`)}
          >
            {__("+ Add new")}
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
              <span>{__("Search title or slug")}</span>
              <input id={`entry-table-search-${typeKey}`} type="search" className={`${control} w-full`}
                value={searchInput} maxLength={200} disabled={bulkBusy}
                onChange={event => setSearchInput(event.target.value)}
                placeholder={__("Enter keywords…")} />
            </label>
            <AdminButton type="submit" variant="secondary" disabled={bulkBusy}>{__("Search")}</AdminButton>
          </form>

          {localeState.hasMultipleLocales && (
            <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
              <span>{__("Language")}</span>
              <select
                aria-label={__("Filter language")}
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
                <option value="all">{__("All languages")}</option>
                {localeState.allLocales.map((loc) => (
                  <option key={loc.code} value={loc.code}>
                    {loc.label} ({loc.code})
                  </option>
                ))}
              </select>
            </label>
          )}

          {!isTrashView && terms?.categoryEnabled && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{_x("Category", "feature.content")}</span>
            <select aria-label={__("Filter category")} className={`${control} w-full`}
              value={filters.category_id ?? ''} disabled={bulkBusy}
              onFocus={() => loadFilterOptions('category')}
              onChange={event => updateFilter({
                category_id: event.target.value ? Number(event.target.value) : undefined,
                taxonomy_key: undefined, term_id: undefined,
                include_descendants: false,
              })}>
              <option value="">{_x("All categories", "feature.content")}</option>
              {filters.category_id && !terms.byKind.category?.some(term => term.id === filters.category_id) &&
                <option value={filters.category_id}>{selectedOptionLabel('category', filters.category_id)}</option>}
              {terms.byKind.category?.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms.byKind.category && <option disabled>{__("Tap to load categories…")}</option>}
            </select>
          </label>}

          {!isTrashView && terms?.tagEnabled && <label className="flex min-w-[130px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{__("Tags")}</span>
            <select aria-label={__("Filter tags")} className={`${control} w-full`}
              value={filters.tag_id ?? ''} disabled={bulkBusy}
              onFocus={() => loadFilterOptions('tag')}
              onChange={event => updateFilter({ tag_id: event.target.value ? Number(event.target.value) : undefined,
                taxonomy_key: undefined, term_id: undefined,
                // A tag is an AND filter, not a reset of the selected category
                // subtree. Preserve descendants only when a category is active.
                include_descendants: Boolean(filters.category_id && filters.include_descendants) })}>
              <option value="">{_x("All tags", "feature.content")}</option>
              {filters.tag_id && !terms.byKind.tag?.some(term => term.id === filters.tag_id) &&
                <option value={filters.tag_id}>{selectedOptionLabel('tag', filters.tag_id)}</option>}
              {terms.byKind.tag?.map(term => <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms.byKind.tag && <option disabled>{__("Tap to load tags…")}</option>}
            </select>
          </label>}

          {!isTrashView && customDefinitions.length > 0 && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{__("Taxonomy")}</span>
            <select aria-label={__("Choose taxonomy to filter")} className={`${control} w-full`}
              disabled={bulkBusy} value={selectedTaxonomy}
              onChange={event => {
                setTaxonomyChoice(event.target.value);
                updateFilter({ taxonomy_key: undefined,
                  term_id: undefined, category_id: undefined, tag_id: undefined, include_descendants: false });
              }}>
              <option value="">{__("All taxonomies")}</option>
              {customDefinitions.map(def => <option key={def.key} value={def.key}>{def.label}</option>)}
            </select>
          </label>}

          {!isTrashView && chosenDefinition && <label className="flex min-w-[140px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{chosenDefinition.label}</span>
            <select aria-label={__("Filter by {{value1}}", { value1: chosenDefinition.label })} className={`${control} w-full`}
              disabled={bulkBusy} value={filters.term_id ?? ''}
              onFocus={() => loadFilterOptions(chosenDefinition.key)}
              onChange={event => {
                setTaxonomyChoice(selectedTaxonomy);
                updateFilter({ taxonomy_key: event.target.value ? selectedTaxonomy : undefined,
                  term_id: event.target.value ? Number(event.target.value) : undefined,
                  include_descendants: false });
              }}>
              <option value="">{__("Choose term")}</option>
              {filters.term_id && !terms?.byKind[chosenDefinition.key]?.some(term => term.id === filters.term_id) &&
                <option value={filters.term_id}>{selectedOptionLabel(chosenDefinition.key, filters.term_id)}</option>}
              {(terms?.byKind[chosenDefinition.key] ?? []).map(term =>
                <option key={term.id} value={term.id}>{term.name}</option>)}
              {!terms?.byKind[chosenDefinition.key] && <option disabled>{__("Tap to load terms…")}</option>}
            </select>
          </label>}

          {!isTrashView && <label className="flex min-w-[145px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-600 sm:flex-none">
            <span>{__("Month (UTC)")}</span>
            <input type="month" aria-label={__("Filter by update month")} className={`${control} w-full`}
              value={filters.updated_month ?? ''} disabled={bulkBusy}
              onChange={event => updateFilter({ updated_month: event.target.value || undefined })} />
          </label>}

          <label className="flex w-[88px] flex-col gap-1 text-xs font-medium text-zinc-600">
            <span>{__("Per page")}</span>
            <select aria-label={__("Items per page")} className={`${control} w-full`}
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
              {__("Include child categories")}
            </label>}
            {filters.term_id && chosenDefinition?.hierarchical && <label className="flex items-center gap-2 text-xs text-zinc-700">
              <input type="checkbox" checked={Boolean(filters.include_descendants)} disabled={bulkBusy}
                onChange={event => updateFilter({ include_descendants: event.target.checked })} />
              {__("Include child terms of")}{' '}{chosenDefinition.label}
            </label>}
          </div>
        ) : null}

        <div className="flex flex-wrap items-center justify-between gap-2.5 border-t border-zinc-100 pt-2.5">
          {!isTrashView ? (
            <div className="flex flex-wrap items-center gap-2" role="group" aria-label={__("Bulk actions")}>
              <span className="text-xs font-medium text-zinc-500">{formatNumber(selectedRows.length)} {__("selected")}</span>
              {supportsDraftPublish && <>
                <AdminButton variant="secondary" size="sm" data-testid="bulk-publish-btn" disabled={bulkBusy || !csrfToken || !selectedRows.length}
                  onClick={() => void performBulk({ kind: 'published' })}>{__("Publish selected")}</AdminButton>
                <AdminButton variant="secondary" size="sm" data-testid="bulk-draft-btn" disabled={bulkBusy || !csrfToken || !selectedRows.length}
                  onClick={() => void performBulk({ kind: 'draft' })}>{__("Move to draft")}</AdminButton>
              </>}
              {(terms?.definitions.length ?? 0) > 0 && (
                <AdminButton
                  variant={bulkTaxOpen ? 'primary' : 'secondary'}
                  size="sm"
                  data-testid="bulk-taxonomy-toggle-btn"
                  disabled={bulkBusy || !csrfToken || !selectedRows.length}
                  onClick={() => {
                    if (!selectedLocaleCheck.ok) {
                      setBulkBlockMessage(selectedLocaleCheck.message);
                      return;
                    }
                    setBulkBlockMessage('');
                    setBulkTaxOpen(prev => !prev);
                  }}
                >
                  {__("Bulk taxonomy")}
                </AdminButton>
              )}
              <AdminButton variant="danger" size="sm" data-testid="bulk-move-to-trash-btn"
                disabled={bulkBusy || !csrfToken || !selectedRows.length}
                onClick={() => void performBulk({ kind: 'trash' })}>{__("Move selected to Trash")}</AdminButton>
              {bulkBusy && (
                <>
                  <span role="status" className="text-xs text-zinc-600">{__("Processing each item separately…")}</span>
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
                    {bulkCancelling ? __("Cancelling...") : _x("Cancel", "entry_lifecycle.bulk_cancel")}
                  </AdminButton>
                </>
              )}
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-2" role="group" aria-label={__("Bulk actions")}>
              <span className="text-xs font-medium text-zinc-500">{formatNumber(selectedRows.length)} {__("selected")}</span>
              <AdminButton
                variant="secondary"
                size="sm"
                data-testid="bulk-restore-btn"
                disabled={bulkBusy || !csrfToken || !selectedRows.length}
                onClick={() => void performBulk({ kind: 'restore', confirmPublicRestore: false })}
              >
                {__("Restore selected")}
              </AdminButton>
              {user.role === 'admin' && (
                <AdminButton
                  variant="danger"
                  size="sm"
                  data-testid="bulk-purge-btn"
                  disabled={bulkBusy || !csrfToken || !selectedRows.length}
                  onClick={() => void performBulk({ kind: 'purge' })}
                >
                  {__("Delete selected permanently")}
                </AdminButton>
              )}
              {bulkBusy && (
                <>
                  <span role="status" className="text-xs text-zinc-600">{__("Processing each item separately…")}</span>
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
                    {bulkCancelling ? __("Cancelling...") : _x("Cancel", "entry_lifecycle.bulk_cancel")}
                  </AdminButton>
                </>
              )}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2.5 text-xs">
            <span className="text-zinc-500" role="status">{data ? __("Total: {{value1}} items", { value1: formatNumber(data.total) }) : __("Loading counts…")}</span>
            <AdminButton variant="ghost" size="sm" disabled={bulkBusy}
              onClick={() => { setSearchInput(''); setTaxonomyChoice(''); onNavigateHash(base); }}>
              {__("Clear filters")}
            </AdminButton>
          </div>
        </div>

        {!isTrashView && bulkTaxOpen && selectedRows.length > 0 && selectedLocaleCheck.ok && (
          <div
            data-testid="bulk-taxonomy-panel"
            className="space-y-2.5 rounded-lg border border-zinc-200 bg-zinc-50 p-3 text-xs"
          >
            <div className="flex flex-wrap items-end gap-2.5">
              <label className="flex min-w-[150px] flex-col gap-1 font-medium text-zinc-700">
                <span>{__("Taxonomy")}</span>
                <select
                  data-testid="bulk-taxonomy-key"
                  className={control}
                  value={bulkTaxKey}
                  disabled={bulkBusy}
                  onChange={e => {
                    setBulkTaxKey(e.target.value);
                    setBulkTaxTermIds([]);
                  }}
                >
                  <option value="">{__("No taxonomy change")}</option>
                  {(terms?.definitions ?? []).map(def => (
                    <option key={def.key} value={def.key}>{def.label}</option>
                  ))}
                </select>
              </label>

              {bulkTaxKey && (
                <label className="flex min-w-[160px] flex-col gap-1 font-medium text-zinc-700">
                  <span>{__("Operation")}</span>
                  <select
                    data-testid="bulk-taxonomy-op"
                    className={control}
                    value={bulkTaxOp}
                    disabled={bulkBusy}
                    onChange={e => setBulkTaxOp(e.target.value as TaxonomyDeltaOp)}
                  >
                    <option value="add">{__("Add terms")}</option>
                    <option value="remove">{__("Remove terms")}</option>
                    <option value="replace">{__("Replace taxonomy terms")}</option>
                  </select>
                </label>
              )}

              {bulkTaxKey && (
                <AdminButton
                  variant="primary"
                  size="sm"
                  data-testid="bulk-taxonomy-apply-btn"
                  disabled={bulkBusy || !csrfToken || bulkTaxLoading}
                  onClick={() => void performBulkTaxonomy()}
                >
                  {__("Apply taxonomy to selected")}
                </AdminButton>
              )}
            </div>

            {bulkTaxKey && (
              <div className="space-y-1.5">
                <p className="font-medium text-zinc-600">
                  {__("Terms ({{value1}})", { value1: selectedLocaleCheck.locale || 'default' })}
                </p>
                {bulkTaxLoading ? (
                  <p className="text-zinc-500">{__("Loading terms for this language...")}</p>
                ) : (bulkTaxCatalog[bulkTaxCatalogKey]?.length ?? 0) === 0 ? (
                  <p className="text-zinc-500">
                    {__("No terms exist for this taxonomy in language {{value1}}.", { value1: selectedLocaleCheck.locale || 'default' })}
                  </p>
                ) : (
                  <div className="flex max-h-32 flex-wrap gap-2 overflow-y-auto rounded border border-zinc-200 bg-white p-2" data-testid="bulk-taxonomy-terms">
                    {(bulkTaxCatalog[bulkTaxCatalogKey] ?? []).map(term => {
                      const checked = bulkTaxTermIds.includes(term.id);
                      return (
                        <label
                          key={term.id}
                          className="inline-flex items-center gap-1.5 rounded border border-zinc-200 bg-zinc-50 px-2 py-1 text-xs text-zinc-800"
                        >
                          <input
                            type="checkbox"
                            data-testid={`bulk-taxonomy-term-${term.id}`}
                            checked={checked}
                            disabled={bulkBusy}
                            onChange={e => {
                              setBulkTaxTermIds(prev =>
                                e.target.checked
                                  ? [...prev, term.id]
                                  : prev.filter(id => id !== term.id),
                              );
                            }}
                          />
                          <span>{term.name}</span>
                        </label>
                      );
                    })}
                  </div>
                )}
                {bulkTaxOp === 'replace' && (
                  <p className="text-[11px] text-zinc-500">{__("Leave all terms unchecked with Replace to clear only this taxonomy.")}</p>
                )}
              </div>
            )}
          </div>
        )}
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
            setProviderNotice(__("Action completed for #{{value1}}.", { value1: rowId }));
            setRetry(old => old + 1);
          }}
          onCancelled={() => {
            setPendingProviderConfirmation(null);
            setProviderNotice(__("Confirmation cancelled. No content was changed."));
          }}
          onFailed={message => {
            const rowId = pendingProviderConfirmation.rowId;
            setPendingProviderConfirmation(null);
            setProviderNotice(`#${rowId}: ${message}`);
          }}
        />
      )}

      {termLoadErrors.length > 0 && <AdminAlert tone="warning" role="alert">
        {__("Could not load some terms:")}{' '}{termLoadErrors.join('; ')}{__(". Other filters remain available.")}
      </AdminAlert>}
      {resolved?.failed && <AdminAlert tone="warning" role="alert">
        {__("Could not load some term names on this page. Try Reload to display them all.")}
      </AdminAlert>}

      {displayBulkResults.length > 0 && (() => {
        const counts = {
          succeeded: displayBulkResults.filter(r => r.status === 'succeeded').length,
          stale: displayBulkResults.filter(r => r.status === 'stale').length,
          failed: displayBulkResults.filter(r => r.status === 'failed').length,
          uncertain: displayBulkResults.filter(r => r.status === 'uncertain').length,
          unattempted: displayBulkResults.filter(r => r.status === 'unattempted').length,
        };
        return (
          <div className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3" aria-label={__("Bulk results")} data-testid="bulk-results-panel">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-xs font-semibold text-zinc-900">
                {__("Succeeded: {{succeeded}} · Stale (412): {{stale}} · Failed: {{failed}} · Uncertain: {{uncertain}} · Unattempted: {{unattempted}}", {
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
                  {__("Retry remaining ({{value1}})", { value1: formatNumber(bulkRemainingRetryCount) })}
                </AdminButton>
              )}
            </div>
            {unresolvedUncertainCount > 0 && (
              <p className="text-xs font-medium text-amber-800" data-testid="bulk-unresolved-uncertain-notice">
                {__("{{value1}} uncertain item(s) changed on the server and require manual verification before retrying.", { value1: formatNumber(unresolvedUncertainCount) })}
              </p>
            )}
            {bulkRemainingRetryCount > 0 && (
              <p className="text-xs text-zinc-600">{__("Refreshed server state. Review remaining rows before retrying; succeeded rows will not be replayed.")}</p>
            )}
            <ul className="space-y-1 text-xs">
              {displayBulkResults.map(result => (
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
      {quickEditNotice && <AdminAlert tone="success" data-testid="quick-edit-saved-notice">{quickEditNotice}</AdminAlert>}
      {!isTrashView && quickEditRowId !== null && !currentRows.some(r => r.id === quickEditRowId) && (() => {
        const detachedDraft = editorDraftStore.getQuickEdit(typeKey, quickEditRowId);
        const recoverable = JSON.stringify(
          detachedDraft?.patch ?? {},
          null,
          2,
        );
        return (
          <div
            role="alert"
            data-testid="quick-edit-trashed-guard"
            className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 p-3.5 text-xs text-amber-950"
          >
            <p className="font-semibold">{__("Entry moved to Trash (409)")}</p>
            <p>{__("This entry can no longer be saved from Quick Edit. Copy any unsaved text below or restore it from Trash first.")}</p>
            {detachedDraft && (
              <label className="block space-y-1">
                <span className="font-medium">{__("Recoverable unsaved edits")}</span>
                <textarea
                  readOnly
                  rows={5}
                  data-testid="quick-edit-recoverable-text"
                  value={recoverable}
                  className="w-full rounded border border-amber-300 bg-white p-2 font-mono text-xs text-zinc-900"
                />
              </label>
            )}
            <div className="flex flex-wrap gap-2">
              <AdminButton
                type="button"
                size="sm"
                variant="primary"
                onClick={() => onNavigateHash(`${base}?status=trash`)}
              >
                {__("Open Trash")}
              </AdminButton>
              <AdminButton
                type="button"
                size="sm"
                variant="secondary"
                data-testid="quick-edit-cancel-btn"
                onClick={() => {
                  if (quickEditDirtyRef.current && !window.confirm(__("Discard unsaved Quick Edit changes?"))) {
                    return;
                  }
                  editorDraftStore.deleteQuickEdit(typeKey, quickEditRowId);
                  quickEditDirtyRef.current = false;
                  setQuickEditRowId(null);
                }}
              >
                {_x("Cancel", "quick_edit.cancel")}
              </AdminButton>
            </div>
          </div>
        );
      })()}
      {providerError && <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
        <span>{__("Could not load extension columns:")}{' '}{providerError}</span>
        <AdminButton variant="ghost" size="sm" onClick={() => setRetry(n => n + 1)}>{_x("Retry", "feature.dashboard")}</AdminButton>
      </AdminAlert>}

      {error && <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
        <span>{error}</span>
        <AdminButton variant="ghost" size="sm" onClick={() => setRetry(prev => prev + 1)}>{_x("Retry", "feature.dashboard")}</AdminButton>
      </AdminAlert>}

      {loading && <p role="status" className="text-xs text-zinc-500">{__("Loading list from server…")}</p>}

      <div
        role="region"
        aria-labelledby={captionId}
        tabIndex={0}
        className="w-full min-w-0 max-w-full overflow-x-auto rounded-xl border border-zinc-200 bg-white focus-visible:outline-2 focus-visible:outline-od-accent"
        aria-busy={loading}
      >
        <table className="w-full min-w-[780px] table-auto text-left text-sm" data-testid="entry-table">
          <caption id={captionId} className="sr-only">{__("Content table")}{' '}{typeKey}{__("; server-side search, status and pagination")}</caption>
          <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold uppercase tracking-wider text-zinc-600">
            <tr>
              <th scope="col" className="w-10 px-3 py-2.5">
                <input type="checkbox" aria-label={__("Select all on page")} checked={allSelected}
                  disabled={!currentRows.length || bulkBusy || loading}
                  onChange={event => setSelected(prev => {
                    const next = new Set(prev);
                    currentIds.forEach(id => { if (event.target.checked) next.add(id); else next.delete(id); });
                    return next;
                  })} />
              </th>
              {!isTrashView && <th scope="col" className="w-14 px-2 py-2.5 text-center">{__("Images")}</th>}
              <th scope="col" aria-sort={ariaSortFor('title')} className="min-w-[220px] px-3 py-2.5">
                <button type="button" data-testid="sort-title" className={sortButton}
                  disabled={bulkBusy} onClick={() => sortBy('title')}>
                  <span>{__("Title")}</span>
                  {activeSort === 'title' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                </button>
              </th>
              <th scope="col" className="whitespace-nowrap px-3 py-2.5">{__("Status")}</th>
              {!isTrashView && terms?.categoryEnabled && <th scope="col" className="px-3 py-2.5">{_x("Category", "feature.content")}</th>}
              {!isTrashView && terms?.tagEnabled && <th scope="col" className="px-3 py-2.5">{__("Tags")}</th>}
              {!isTrashView && customDefinitions.map(def => <th key={def.key} scope="col" className="px-3 py-2.5">{def.label}</th>)}
              {!isTrashView && listColumns.map((col, idx) => (
                <th key={`listcol-${col.kind}-${col.key}-${idx}`} scope="col"
                  aria-sort={col.kind === 'menu_order' ? ariaSortFor('menu_order') : undefined}
                  className="min-w-[100px] px-3 py-2.5">
                  {col.kind === 'menu_order' ? (
                    <button type="button" className={sortButton} disabled={bulkBusy} onClick={() => sortBy('menu_order')}>
                      <span>{resolveListColumnLabel(col, terms?.definitions ?? [])}</span>
                      {activeSort === 'menu_order' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                    </button>
                  ) : (
                    resolveListColumnLabel(col, terms?.definitions ?? [])
                  )}
                </th>
              ))}
              {!isTrashView && providerColumns.map(column => <th key={column.key} scope="col"
                className="min-w-[120px] px-3 py-2.5">{column.label}</th>)}
              {isTrashView ? (
                <th scope="col" aria-sort={ariaSortFor('trashed_at')} className="min-w-[140px] whitespace-nowrap px-3 py-2.5">
                  <button type="button" data-testid="sort-trashed" className={sortButton}
                    disabled={bulkBusy} onClick={() => sortBy('trashed_at')}>
                    <span>{__("Trashed date")}</span>
                    {activeSort === 'trashed_at' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                  </button>
                </th>
              ) : (
                <th scope="col" aria-sort={ariaSortFor('updated_at')} className="min-w-[140px] whitespace-nowrap px-3 py-2.5">
                  <button type="button" data-testid="sort-updated" className={sortButton}
                    disabled={bulkBusy} onClick={() => sortBy('updated_at')}>
                    <span>{__("Updated date")}</span>
                    {activeSort === 'updated_at' && <span aria-hidden="true">{activeOrder === 'asc' ? '↑' : '↓'}</span>}
                  </button>
                </th>
              )}
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100">
            {!loading && !error && currentRows.length === 0 && <tr>
              <td colSpan={isTrashView ? 4 : 5 + (terms?.definitions.length ?? 0) + listColumns.length + providerColumns.length}
                className="px-4 py-8 text-center text-sm text-zinc-500">
                {data?.total === 0 ? __("No matching content. Clear filters to see more.") : __("No items on this page.")}
              </td>
            </tr>}
            {displayRows.map(({ row, depth }) => {
              const path = !isTrashView && row.publication_status === 'published' ? safePublicPath(row.public_url) : null;
              const thumbId = !isTrashView && typeof row.fields?.featured_image === 'string' ? row.fields.featured_image.trim() : '';
              const readingBlocked = row.blocked_reasons?.trash === 'reading_page_assigned';
              const restoreLocaleBlocked = row.blocked_reasons?.restore === 'locale_disabled';
              const totalCols = isTrashView ? 4 : 5 + (terms?.definitions.length ?? 0) + listColumns.length + providerColumns.length;
              const isQuickEditing = !isTrashView && quickEditRowId === row.id;
              return <Fragment key={row.id}><tr data-entry-id={row.id} className="align-middle transition-colors hover:bg-zinc-50/70">
                <td className="px-3 py-2.5"><input type="checkbox" aria-label={__("Select item #{{value1}}", { value1: row.id })}
                  checked={selected.has(row.id)} disabled={bulkBusy}
                  onChange={event => toggleRow(row.id, event.target.checked)} /></td>
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
                  <div className="leading-snug" style={depth > 0 ? { paddingLeft: `${depth * 18}px` } : undefined}>
                    {depth > 0 && (
                      <span className="mr-1 text-zinc-400 select-none" aria-hidden="true">
                        {'— '.repeat(depth)}
                      </span>
                    )}
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
                            {__("Restore")}
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
                              {__("Delete permanently")}
                            </button>
                          </>
                        )}
                      </div>
                      {restoreLocaleBlocked && (
                        <p className="text-[11px] text-amber-800">
                          {__("Cannot restore while the entry's language is disabled.")}
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
                          aria-label={__("Edit {{value1}}, ID {{value2}}", { value1: row.title, value2: row.id })}
                          onClick={() => onNavigateHash(`${base}/${row.id}/edit${editSuffix}`)}
                        >
                          {_x("Edit", "shell.edit")}
                        </button>
                        {csrfToken && row.capabilities?.quick_edit !== false && (
                          <>
                            <span className="text-zinc-300" aria-hidden="true">·</span>
                            <button
                              type="button"
                              data-testid={`entry-quick-edit-btn-${row.id}`}
                              className="font-medium text-zinc-700 hover:text-od-accent hover:underline disabled:opacity-50"
                              disabled={bulkBusy}
                              aria-expanded={isQuickEditing}
                              onClick={() => {
                                if (quickEditRowId === row.id) {
                                  if (quickEditDirtyRef.current && !window.confirm(__("Discard unsaved Quick Edit changes?"))) {
                                    return;
                                  }
                                  editorDraftStore.deleteQuickEdit(typeKey, row.id);
                                  quickEditDirtyRef.current = false;
                                  setQuickEditRowId(null);
                                  return;
                                }
                                if (quickEditRowId !== null && quickEditDirtyRef.current) {
                                  if (!window.confirm(__("Discard unsaved Quick Edit changes?"))) {
                                    return;
                                  }
                                  editorDraftStore.deleteQuickEdit(typeKey, quickEditRowId);
                                  quickEditDirtyRef.current = false;
                                }
                                setQuickEditNotice('');
                                setQuickEditRowId(row.id);
                              }}
                            >
                              {__("Quick Edit")}
                            </button>
                          </>
                        )}
                        {path && (
                          <>
                            <span className="text-zinc-300" aria-hidden="true">·</span>
                            <a
                              href={path}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="font-medium text-zinc-600 hover:text-zinc-950 hover:underline"
                              aria-label={__("View public {{value1}}", { value1: row.title })}
                            >
                              {__("View")}
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
                              aria-label={__("Preview saved draft {{value1}}", { value1: row.title })}
                            >
                              {__("Preview")}
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
                              title={readingBlocked ? __("Assigned in Reading Settings (Homepage or Posts page); reassign before moving to Trash.") : undefined}
                              aria-label={__("Delete {{value1}}, ID {{value2}}", { value1: row.title, value2: row.id })}
                              onClick={() => void trashSingleRow(row)}
                            >
                              {__("Move to Trash")}
                            </button>
                          </>
                        )}
                      </div>
                      {readingBlocked && (
                        <p className="text-[11px] text-amber-800" data-testid={`reading-blocked-hint-${row.id}`}>
                          {__("Assigned in Reading Settings (Homepage or Posts page); reassign before moving to Trash.")}{' '}
                          {user.role === 'admin' && (
                            <button
                              type="button"
                              data-testid={`reading-settings-link-${row.id}`}
                              onClick={() => onNavigateHash('#/settings/reading')}
                              className="font-medium text-od-accent underline"
                            >
                              {__("Reading Settings")}
                            </button>
                          )}
                        </p>
                      )}
                    </div>
                  )}
                </td>
                <td className="whitespace-nowrap px-3 py-2.5">
                  <AdminBadge tone={row.publication_status === 'published' ? 'success' : 'neutral'}>
                    {row.publication_status === 'published' ? __("Published") : __("Draft")}
                  </AdminBadge>
                </td>
                {!isTrashView && terms?.categoryEnabled && <td className="break-words px-3 py-2.5 text-zinc-700">{renderTermsCell(row, 'category')}</td>}
                {!isTrashView && terms?.tagEnabled && <td className="break-words px-3 py-2.5 text-zinc-700">{renderTermsCell(row, 'tag')}</td>}
                {!isTrashView && customDefinitions.map(def => <td key={def.key} className="break-words px-3 py-2.5 text-zinc-700">{renderTermsCell(row, def.key)}</td>)}
                {!isTrashView && listColumns.map((col, idx) => (
                  <td key={`listcell-${row.id}-${col.kind}-${col.key}-${idx}`} className="break-words px-3 py-2.5"><EntryListColumnCell column={col} row={row} rowTitleById={rowTitleById} renderTermsCell={renderTermsCell} /></td>
                ))}
                {!isTrashView && providerColumns.map(column => <td key={column.key} className="break-words px-3 py-2.5">
                  <AdminColumnCell column={column} row={row} busy={providerBusy || bulkBusy}
                    onRun={(col, entry) => void performProviderAction(col, entry)} />
                </td>)}
                <td className="whitespace-nowrap px-3 py-2.5 text-xs tabular-nums text-zinc-600">
                  {isTrashView
                    ? (row.trashed_at ? `${row.trashed_at.slice(0, 16).replace('T', ' ')} UTC` : '—')
                    : `${row.updated_at.slice(0, 16).replace('T', ' ')} UTC`}
                </td>
              </tr>
              {isQuickEditing && (
                <tr data-testid={`quick-edit-row-${row.id}`} className="bg-zinc-50/70">
                  <td colSpan={totalCols} className="p-3">
                    <QuickEditForm
                      typeKey={typeKey}
                      row={row}
                      schemaToken={data?.schema_token ?? ''}
                      taxonomies={terms?.definitions ?? []}
                      user={user}
                      siteInstanceId={siteInstanceId}
                      authGeneration={authGeneration}
                      csrfToken={csrfToken}
                      editHref={`${base}/${row.id}/edit${editSuffix}`}
                      trashHref={`${base}?status=trash`}
                      onSaved={updated => {
                        setQuickEditRowId(null);
                        quickEditDirtyRef.current = false;
                        setQuickEditNotice(
                          __("Saved Quick Edit changes for #{{value1}}.", { value1: updated.id }),
                        );
                        setRetry(prev => prev + 1);
                      }}
                      onCancel={() => {
                        setQuickEditRowId(null);
                        quickEditDirtyRef.current = false;
                      }}
                      onRefreshServerRow={async () => {
                        const fresh = await queryContentEntryTable(typeKey, filters);
                        if (aliveRef.current && scopeRef.current === scope) {
                          setDataState({ scope, value: fresh });
                          setDataRevision(rev => rev + 1);
                        }
                        return {
                          row: fresh.items.find(item => item.id === row.id) ?? null,
                          schemaToken: fresh.schema_token ?? '',
                        };
                      }}
                      onNavigateHash={onNavigateHash}
                      onApiError={onApiError}
                      onDirtyChange={dirty => {
                        quickEditDirtyRef.current = dirty;
                      }}
                    />
                  </td>
                </tr>
              )}
              </Fragment>;
            })}
          </tbody>
        </table>
      </div>
      <nav aria-label={__("Content pagination")} className="flex flex-wrap items-center justify-between gap-3 text-xs text-zinc-600">
        <span data-testid="entry-table-page-info">{data ? __("Showing {{value1}}–{{value2}} / {{value3}}", { value1: formatNumber(data.total ? offset + 1 : 0), value2: formatNumber(Math.min(offset + currentRows.length, data.total)), value3: formatNumber(data.total) }) : __("Page not loaded")}</span>
        <div className="flex items-center gap-2">
          <AdminButton variant="secondary" size="sm" disabled={bulkBusy || loading || offset === 0}
            onClick={() => updateFilter({ offset: Math.max(0, offset - pageSize) }, false)}>{__("Previous page")}</AdminButton>
          <span className="tabular-nums">{__("Page {{page}}", { page: formatNumber(Math.floor(offset / pageSize) + 1) })}{data ? ` / ${formatNumber(Math.max(1, Math.ceil(data.total / pageSize)))}` : ''}</span>
          <AdminButton variant="secondary" size="sm"
            disabled={bulkBusy || loading || !data || offset + pageSize >= data.total || offset + pageSize > 1000000}
            onClick={() => updateFilter({ offset: offset + pageSize }, false)}>{__("Next page")}</AdminButton>
        </div>
      </nav>
    </section>
  );
}
