import { __, _x, useI18n } from '../../i18n/index.tsx';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  createContentType,
  effectiveContentTypeSettings,
  listContentTypes,
  purgeContentEntry,
  queryRetainedTrashTable,
  updateContentType,
  type ContentPublicationPolicy,
  type ContentSupport,
  type ContentTypeDefinitionWithSettings,
  type ContentTypeSettings,
  type OwnedContentTypeWithSettings,
} from '../../api/content.ts';
import type { EntryTableResult, EntryTableRow } from './types.ts';
import { ApiClientError, requestJson } from '../../api/client.ts';
import { listTaxonomyDefinitions, type TaxonomyDefinition } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import {
  ContentTypeForm, isContentTypeCreationDraftDirty,
  type ContentTypeCreationDraft,
} from './ContentTypeForm.tsx';
import { EditorLayoutConfigurator } from './EditorLayoutConfigurator.tsx';
import { FieldGroupsManager } from './FieldGroupsManager.tsx';
import {
  AdminAlert,
  AdminBadge,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
  type AdminTabItem,
} from '../../components/AdminPrimitives.tsx';

type ContentTypeTabId = 'overview' | 'taxonomies' | 'field_groups' | 'layout';

interface ContentTypeSchemaSettingsProps {
  /** Passed by App from its authenticated server session; RAM recovery is off without these. */
  user?: AuthUser | null;
  siteInstanceId?: string;
  selectedTypeKey: string | null;
  onSelectTypeKey: (typeKey: string | null) => void;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onContentTypesChanged: () => void;
  onDirtyChange?: (dirty: boolean) => void;
}

interface ConfiguredTaxonomies {
  taxonomies: string[];
  effective: string[];
  has_override: boolean;
  /** Read under the same backend snapshot as taxonomies for safe CAS. */
  type_version: number;
}

interface SavedTaxonomyDraft {
  /** Version and exact membership from the moment the user began editing. */
  baseVersion: number;
  baseline: string[];
  selected: string[];
  stale: boolean;
}

interface SavedCptSettingsDraft {
  /** Version of the server CPT before the user's unsaved edits. */
  baseVersion: number;
  baselineLabel: string;
  baselineSettings: ContentTypeSettings;
  label: string;
  settings: ContentTypeSettings;
  stale: boolean;
}

// Module-level RAM only, never browser storage. A 401 removes the component
// without clearing this map; re-login to another site/account/role cannot read
// another identity's drafts. Include CPT key to safely switch CPTs and return.
const taxonomyDraftMemory = new Map<string, SavedTaxonomyDraft>();
const settingsDraftMemory = new Map<string, SavedCptSettingsDraft>();
const scopeKey = (siteId: string | undefined, user: AuthUser | null | undefined, typeKey: string): string | null =>
  siteId && siteId !== 'default-site' && user?.id && user.role === 'admin' && typeKey
    ? JSON.stringify([siteId, user.id, user.role, typeKey]) : null;
// A newly created CPT has no type key yet. Its entire incomplete form lives in
// transient RAM, isolated by verified site/account/role, never localStorage.
const createCptDraftMemory = new Map<string, ContentTypeCreationDraft>();
const sameKeys = (left: string[], right: string[]): boolean =>
  [...left].sort().join('\u0000') === [...right].sort().join('\u0000');
const copySettings = (value: ContentTypeSettings): ContentTypeSettings =>
  ({ ...value, supports: [...value.supports] });
const settingsDiffer = (
  label: string, value: ContentTypeSettings, baselineLabel: string, baseline: ContentTypeSettings,
): boolean => label !== baselineLabel || value.public !== baseline.public ||
  value.has_archive !== baseline.has_archive || value.publication !== baseline.publication ||
  !sameKeys(value.supports, baseline.supports);

// This worker owns one TSX file only. Prime has equivalent helpers in its newer
// api/content.ts; use the canonical routes here without changing shared APIs.
function getConfiguredTaxonomies(typeKey: string, signal?: AbortSignal): Promise<ConfiguredTaxonomies> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}/configured`, {
    signal, defaultErrorMessage: __("Could not load configured taxonomies."),
  });
}

function putTaxonomyEligibility(typeKey: string, version: number, taxonomies: string[], token: string): Promise<OwnedContentTypeWithSettings> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}`, {
    method: 'PUT', csrfToken: token, ifMatchVersion: version, contentType: 'application/json',
    body: JSON.stringify({ taxonomies, expected_version: version }),
    defaultErrorMessage: __("Could not update assigned taxonomies."),
  });
}

interface LoadedTaxonomyState {
  identity: string;
  available: TaxonomyDefinition[];
  applied: string[];
  baseVersion: number;
  stale: boolean;
}

export const ContentTypeSchemaSettings: React.FC<ContentTypeSchemaSettingsProps> = ({
  user,
  siteInstanceId,
  selectedTypeKey,
  onSelectTypeKey,
  csrfToken,
  onApiError,
  onContentTypesChanged,
  onDirtyChange,
}) => {
  useI18n();
  const createScope = scopeKey(siteInstanceId, user, '__new_cpt__');
  const latestCreateScopeRef = useRef(createScope);
  latestCreateScopeRef.current = createScope;
  const [contentTypes, setContentTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [createOpenState, setCreateOpenState] = useState(() => ({
    scope: createScope, open: Boolean(createScope && createCptDraftMemory.has(createScope)),
  }));
  const showCreateForm = createOpenState.scope === createScope ? createOpenState.open
    : Boolean(createScope && createCptDraftMemory.has(createScope));
  const [createDirtyState, setCreateDirtyState] = useState(() => ({
    scope: createScope, dirty: Boolean(createScope && createCptDraftMemory.has(createScope)),
  }));
  const createDirty = createDirtyState.scope === createScope ? createDirtyState.dirty
    : Boolean(createScope && createCptDraftMemory.has(createScope));
  const [editLabel, setEditLabel] = useState('');
  const [editSettings, setEditSettings] = useState<ContentTypeSettings>(
    effectiveContentTypeSettings('post'),
  );
  const [draftIdentity, setDraftIdentity] = useState('');
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsStale, setSettingsStale] = useState(false);
  const [settingsError, setSettingsError] = useState('');
  const [createNoticeState, setCreateNoticeState] = useState<{
    scope: string | null; message: string;
  } | null>(null);
  const createNotice = createNoticeState?.scope === createScope ? createNoticeState.message : '';
  const [revision, setRevision] = useState(0);
  const [activeTab, setActiveTab] = useState<ContentTypeTabId>('overview');
  const [fieldGroupCount, setFieldGroupCount] = useState<number>(0);
  const [layoutVersion, setLayoutVersion] = useState<number | null>(null);
  const [taxonomyState, setTaxonomyState] = useState<LoadedTaxonomyState | null>(null);
  const [taxonomyDraft, setTaxonomyDraft] = useState<string[]>([]);
  const [taxonomyBusy, setTaxonomyBusy] = useState(false);
  const [taxonomyError, setTaxonomyError] = useState('');
  const [taxonomyNotice, setTaxonomyNotice] = useState('');
  const [retainedTrash, setRetainedTrash] = useState<{ key: string; data: EntryTableResult } | null>(null);
  const [retainedTrashOffset, setRetainedTrashOffset] = useState(0);
  const [retainedTrashLoading, setRetainedTrashLoading] = useState(false);
  const [retainedTrashError, setRetainedTrashError] = useState('');
  const [retainedTrashProviderActive, setRetainedTrashProviderActive] = useState(false);
  const [retainedTrashBusyId, setRetainedTrashBusyId] = useState<number | null>(null);
  const [retainedTrashReloadTick, setRetainedTrashReloadTick] = useState(0);
  const latestScopeRef = useRef<string | null>(null);
  const latestTypeRef = useRef<string>('');
  const mountedRef = useRef(true);
  const createFormEpochRef = useRef(0);
  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  const onCreateDraftChange = useCallback((snapshot: ContentTypeCreationDraft) => {
    if (!mountedRef.current || latestCreateScopeRef.current !== createScope) return;
    const dirty = isContentTypeCreationDraftDirty(snapshot);
    if (createScope) {
      if (dirty) createCptDraftMemory.set(createScope, snapshot);
      else createCptDraftMemory.delete(createScope);
    }
    setCreateDirtyState({ scope: createScope, dirty });
  }, [createScope]);

  const onCreateFormDirty = useCallback((dirty: boolean) => {
    if (!mountedRef.current || latestCreateScopeRef.current !== createScope) return;
    setCreateDirtyState(previous => previous.scope === createScope && previous.dirty === dirty
      ? previous : { scope: createScope, dirty });
  }, [createScope]);

  const toggleCreateForm = () => {
    if (!showCreateForm) {
      setCreateNoticeState(null);
      createFormEpochRef.current += 1;
      setCreateOpenState({ scope: createScope, open: true });
      return;
    }
    if (createDirty && !window.confirm(__("Discard the unsaved new content type form?"))) return;
    createFormEpochRef.current += 1;
    // Explicit close/discard is destructive; a 401 unmount deliberately does
    // not call this handler so same-account reauthentication can restore.
    if (createScope) createCptDraftMemory.delete(createScope);
    setCreateDirtyState({ scope: createScope, dirty: false });
    setCreateOpenState({ scope: createScope, open: false });
  };

  const load = async (isCancelled?: () => boolean) => {
    try {
      const list = await listContentTypes();
      if (isCancelled?.()) return;
      setContentTypes(list);
      if (!selectedTypeKey && list.length > 0) {
        onSelectTypeKey(list[0].descriptor.key);
      }
    } catch (err) {
      if (isCancelled?.()) return;
      onApiError(err, __("Could not load the content type list."));
    }
  };

  useEffect(() => {
    let cancelled = false;
    void load(() => cancelled);
    return () => {
      cancelled = true;
    };
  }, [revision]);

  const activeType = contentTypes.find(ct => ct.descriptor.key === selectedTypeKey) ?? null;
  const activeKey = activeType?.descriptor.key ?? '';
  const currentIdentity = activeType ? `${activeKey}:${activeType.version}` : '';
  const selectedScope = scopeKey(siteInstanceId, user, activeKey);
  latestScopeRef.current = selectedScope;
  latestTypeRef.current = activeKey;
  const isBuiltin = activeKey === 'post' || activeKey === 'page';
  const isPattern = activeKey === 'pattern';
  const isDormantOrdinaryCpt = Boolean(
    activeType &&
    user?.role === 'admin' &&
    activeKey !== 'pattern' &&
    activeKey !== 'section' &&
    (activeType.status !== 'active' || activeType.declaration_state === 'withdrawn'),
  );

  useEffect(() => {
    setRetainedTrashOffset(0);
    setRetainedTrashProviderActive(false);
    setRetainedTrashError('');
  }, [activeKey]);

  useEffect(() => {
    if (!isDormantOrdinaryCpt || !activeKey) {
      setRetainedTrash(null);
      setRetainedTrashLoading(false);
      setRetainedTrashError('');
      setRetainedTrashProviderActive(false);
      return;
    }
    let alive = true;
    const controller = new AbortController();
    setRetainedTrashLoading(true);
    setRetainedTrashError('');
    setRetainedTrashProviderActive(false);
    void queryRetainedTrashTable(activeKey, { limit: 20, offset: retainedTrashOffset }, controller.signal)
      .then(data => {
        if (!alive || controller.signal.aborted) return;
        setRetainedTrash({ key: activeKey, data });
      })
      .catch(cause => {
        if (!alive || controller.signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, __("Could not load retained Trash entries."));
        }
        if (
          cause instanceof ApiClientError &&
          cause.status === 409 &&
          (cause.errorCode === 'entry.provider_active' || cause.message.includes('provider_active'))
        ) {
          setRetainedTrashProviderActive(true);
          setRevision(r => r + 1);
          onContentTypesChanged();
        }
        setRetainedTrashError(cause instanceof Error ? cause.message : __("Could not load retained Trash entries."));
      })
      .finally(() => {
        if (alive) setRetainedTrashLoading(false);
      });
    return () => {
      alive = false;
      controller.abort();
    };
  }, [isDormantOrdinaryCpt, activeKey, retainedTrashOffset, retainedTrashReloadTick]);

  const handlePurgeRetainedRow = async (row: EntryTableRow) => {
    if (!csrfToken || !activeKey || retainedTrashBusyId !== null) return;
    if (
      !window.confirm(
        _x("Permanently delete \"{{value1}}\" (#{{value2}})? This cannot be undone.", "entry_lifecycle.confirm_purge_row", { value1: row.title, value2: row.id }),
      )
    ) {
      return;
    }
    setRetainedTrashBusyId(row.id);
    setRetainedTrashError('');
    try {
      await purgeContentEntry(activeKey, row.id, row.version, csrfToken, { retainedTrash: true });
      if (!mountedRef.current) return;
      setRetainedTrashReloadTick(n => n + 1);
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, __("Could not permanently delete the entry."));
      }
      if (
        cause instanceof ApiClientError &&
        cause.status === 409 &&
        (cause.errorCode === 'entry.provider_active' || cause.message.includes('provider_active'))
      ) {
        setRetainedTrashProviderActive(true);
        setRevision(r => r + 1);
        onContentTypesChanged();
      }
      setRetainedTrashError(cause instanceof Error ? cause.message : __("Could not permanently delete the entry."));
    } finally {
      if (mountedRef.current) setRetainedTrashBusyId(null);
    }
  };
  const canEditSettings = Boolean(activeType && (
    activeType.owner.kind === 'user'
    || (activeType.owner.kind === 'core' && isBuiltin)
  ));
  // Site-owned CPT taxonomy associations are independent of provider-owned
  // schema editing. Admin may override plugin/theme CPT eligibility without
  // modifying the provider's schema; Pattern and Editors remain restricted.
  const canEditTaxonomies = Boolean(activeType && !isPattern && user?.role === 'admin');
  const canonicalSettings = activeType
    ? effectiveContentTypeSettings(activeKey, activeType.descriptor.settings)
    : null;
  const settingsChanged = draftIdentity.startsWith(`${activeKey}:`) && canonicalSettings !== null &&
    settingsDiffer(editLabel, editSettings, activeType?.descriptor.label ?? '', canonicalSettings);
  const taxonomyLoaded = taxonomyState?.identity === currentIdentity ? taxonomyState : null;
  const taxonomyChanged = Boolean(taxonomyLoaded && !sameKeys(taxonomyDraft, taxonomyLoaded.applied));
  const settingsDirtyRef = useRef(false);
  settingsDirtyRef.current = Boolean(settingsChanged || taxonomyChanged);

  const persistSettingsInput = (nextLabel: string, nextSettings: ContentTypeSettings) => {
    if (!selectedScope || !activeType || !canonicalSettings) return;
    const previous = settingsDraftMemory.get(selectedScope);
    const changed = settingsDiffer(nextLabel, nextSettings, activeType.descriptor.label, canonicalSettings);
    // This synchronous guard also protects a create POST that resolves in the
    // same turn as an unrelated settings input, before React has re-rendered.
    settingsDirtyRef.current = changed || settingsStale || taxonomyChanged;
    if (!changed && !settingsStale) {
      settingsDraftMemory.delete(selectedScope);
      return;
    }
    settingsDraftMemory.set(selectedScope, {
      baseVersion: previous?.baseVersion ?? activeType.version,
      baselineLabel: previous?.baselineLabel ?? activeType.descriptor.label,
      baselineSettings: copySettings(previous?.baselineSettings ?? canonicalSettings),
      label: nextLabel, settings: copySettings(nextSettings),
      stale: previous?.stale ?? settingsStale,
    });
  };
  const changeEditLabel = (label: string) => {
    persistSettingsInput(label, editSettings);
    setEditLabel(label); setSettingsError('');
  };
  const changeEditSettings = (patch: Partial<ContentTypeSettings>) => {
    const next = { ...editSettings, ...patch };
    persistSettingsInput(editLabel, next);
    setEditSettings(next); setSettingsError('');
  };

  const rememberTaxonomyDraft = (state: LoadedTaxonomyState, next: string[], scope = selectedScope) => {
    if (!scope) return;
    if (!sameKeys(next, state.applied) || state.stale) {
      taxonomyDraftMemory.set(scope, {
        baseVersion: state.baseVersion, baseline: [...state.applied],
        selected: [...next], stale: state.stale,
      });
    } else taxonomyDraftMemory.delete(scope);
  };

  // Keep the draft even if React unmounts the screen immediately on 401. The
  // checkbox handlers also persist synchronously before any route transition.
  useEffect(() => {
    if (taxonomyLoaded) rememberTaxonomyDraft(taxonomyLoaded, taxonomyDraft);
  }, [selectedScope, taxonomyLoaded, taxonomyDraft]);

  useEffect(() => {
    onDirtyChange?.(createDirty || settingsChanged || taxonomyChanged);
    return () => onDirtyChange?.(false);
  }, [createDirty, settingsChanged, taxonomyChanged, onDirtyChange]);

  useEffect(() => {
    if (!currentIdentity || !activeKey || isPattern) {
      setTaxonomyState(null);
      setTaxonomyError('');
      return;
    }
    let active = true;
    const controller = new AbortController();
    setTaxonomyState(null);
    setTaxonomyError('');
    setTaxonomyNotice('');
    void Promise.all([
      listTaxonomyDefinitions(controller.signal),
      getConfiguredTaxonomies(activeKey, controller.signal),
    ]).then(([available, applied]) => {
      if (!active || controller.signal.aborted) return;
      const remembered = selectedScope ? taxonomyDraftMemory.get(selectedScope) : undefined;
      if (remembered) {
        const stale = remembered.stale || remembered.baseVersion !== applied.type_version ||
          activeType!.version !== applied.type_version ||
          !sameKeys(remembered.baseline, applied.taxonomies);
        setTaxonomyDraft([...remembered.selected]);
        setTaxonomyState({ identity: currentIdentity, available,
          applied: [...remembered.baseline], baseVersion: remembered.baseVersion, stale });
        if (stale) setTaxonomyNotice(__("Your selection was recovered, but the CPT or taxonomy configuration has changed on the server. Review it before saving."));
        else setTaxonomyNotice(__("Recovered this account's unsaved taxonomy selection."));
      } else {
        setTaxonomyDraft([...applied.taxonomies]);
        setTaxonomyState({ identity: currentIdentity, available,
          applied: [...applied.taxonomies], baseVersion: applied.type_version,
          stale: activeType!.version !== applied.type_version });
        if (activeType!.version !== applied.type_version) {
          setTaxonomyNotice(__("The CPT changed while loading. Review the latest version before saving."));
        }
      }
    }).catch(error => {
      if (!active || controller.signal.aborted) return;
      setTaxonomyError(error instanceof Error ? error.message : __("Could not load taxonomy associations."));
      onApiError(error, __("Could not load taxonomy associations."));
    });
    return () => { active = false; controller.abort(); };
  }, [activeKey, currentIdentity, revision, isPattern, selectedScope]);

  const handleUpdateTaxonomies = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!activeType || !taxonomyLoaded || !canEditTaxonomies || isPattern || taxonomyBusy
      || savingSettings || !csrfToken || taxonomyLoaded.stale || !taxonomyChanged) return;
    if (settingsChanged && !window.confirm(
      __("Saving taxonomies reloads the CPT version. Unsaved general configuration changes may be discarded. Continue?"),
    )) return;
    const initialScope = selectedScope;
    const initialType = activeKey;
    const chosen = [...taxonomyDraft];
    rememberTaxonomyDraft(taxonomyLoaded, chosen);
    setTaxonomyBusy(true);
    setTaxonomyError('');
    try {
      const updated = await putTaxonomyEligibility(activeKey, taxonomyLoaded.baseVersion, chosen, csrfToken);
      if (!mountedRef.current) return;
      if (initialScope) taxonomyDraftMemory.delete(initialScope);
      if (latestScopeRef.current !== initialScope || latestTypeRef.current !== initialType) return;
      setContentTypes(previous => previous.map(item => item.descriptor.key === activeKey ? updated : item));
      setTaxonomyDraft(chosen);
      setTaxonomyState({ identity: `${activeKey}:${updated.version}`, available: taxonomyLoaded.available,
        applied: chosen, baseVersion: updated.version, stale: false });
      setTaxonomyNotice(__("Applicable taxonomies saved successfully."));
      setRevision(prev => prev + 1);
      onContentTypesChanged();
    } catch (error) {
      if (!mountedRef.current) return;
      if (error instanceof ApiClientError && error.status === 412 && initialScope) {
        taxonomyDraftMemory.set(initialScope, {
          baseVersion: taxonomyLoaded.baseVersion, baseline: [...taxonomyLoaded.applied],
          selected: chosen, stale: true,
        });
      }
      if (latestScopeRef.current !== initialScope || latestTypeRef.current !== initialType) return;
      if (error instanceof ApiClientError && error.status === 412) {
        setTaxonomyState(previous => previous && previous.identity === currentIdentity
          ? { ...previous, stale: true } : previous);
      }
      const suffix = error instanceof ApiClientError && error.status === 412
        ? __("412 — CPT schema changed. Your selection is retained; review the server version before saving again.")
        : error instanceof ApiClientError && error.status === 403
          ? __("403 — You do not have permission to update taxonomies.")
          : error instanceof Error ? error.message : __("Could not save taxonomies.");
      setTaxonomyError(suffix);
      if (error instanceof ApiClientError && error.status === 401) onApiError(error, suffix);
    } finally {
      setTaxonomyBusy(false);
    }
  };

  const rebaseTaxonomyDraft = async () => {
    if (!taxonomyLoaded || !activeKey || taxonomyBusy || savingSettings) return;
    if (settingsChanged && !window.confirm(
      __("Reviewing taxonomies reloads the latest CPT version and discards unsaved general configuration. Continue?"),
    )) return;
    const startingScope = selectedScope;
    const startingType = activeKey;
    const previousSelection = [...taxonomyDraft];
    const previousBaseline = [...taxonomyLoaded.applied];
    setTaxonomyBusy(true);
    setTaxonomyError('');
    try {
      const [freshTypes, configured] = await Promise.all([
        listContentTypes(), getConfiguredTaxonomies(activeKey),
      ]);
      if (!mountedRef.current) return;
      if (latestScopeRef.current !== startingScope || latestTypeRef.current !== startingType) return;
      const fresh = freshTypes.find(item => item.descriptor.key === activeKey);
      if (!fresh) {
        setTaxonomyError(__("The content type was deleted. Your draft is retained but cannot be saved."));
        return;
      }
      if (fresh.version !== configured.type_version) {
        // These two HTTP calls may themselves straddle a concurrent edit.
        // Never pair pre-edit associations with the post-edit CAS version.
        setTaxonomyState(previous => previous ? { ...previous, stale: true } : previous);
        setTaxonomyError(__("The CPT changed during review. Your input is retained; try again to obtain a consistent version."));
        return;
      }
      const selectedBefore = new Set(previousSelection);
      const baselineBefore = new Set(previousBaseline);
      // Apply just this user's membership delta onto the fresh configured
      // selection, preserving concurrent changes to all other taxonomy keys.
      const newlySelected = previousSelection.filter(key => !baselineBefore.has(key));
      const removed = new Set(previousBaseline.filter(key => !selectedBefore.has(key)));
      const merged = [...new Set([
        ...configured.taxonomies.filter(key => !removed.has(key)), ...newlySelected,
      ])].sort();
      const nextState: LoadedTaxonomyState = {
        identity: `${activeKey}:${configured.type_version}`, available: taxonomyLoaded.available,
        applied: [...configured.taxonomies], baseVersion: configured.type_version, stale: false,
      };
      // Update cache BEFORE changing schema identity, so the re-fetch effect
      // can safely hydrate it without overwriting the draft.
      rememberTaxonomyDraft(nextState, merged, startingScope);
      setContentTypes(freshTypes);
      setTaxonomyState(nextState);
      setTaxonomyDraft(merged);
      setTaxonomyNotice(__("Loaded the latest CPT version and reapplied your changed selections. Review before saving."));
    } catch (cause) {
      if (!mountedRef.current) return;
      if (latestScopeRef.current !== startingScope || latestTypeRef.current !== startingType) return;
      setTaxonomyError(cause instanceof Error ? cause.message : __("Could not review the latest configuration."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, __("Your session expired while reviewing taxonomies."));
    } finally {
      setTaxonomyBusy(false);
    }
  };

  const discardTaxonomyDraft = () => {
    if (!taxonomyLoaded || taxonomyBusy) return;
    if (selectedScope) taxonomyDraftMemory.delete(selectedScope);
    // Force a server reload rather than reverting to a potentially stale
    // version. Clearing state first prevents effects from re-saving the draft.
    setTaxonomyState(null);
    setTaxonomyDraft([]);
    setTaxonomyError('');
    setTaxonomyNotice(__("Discarded unsaved taxonomy selections; loading the server configuration."));
    setRevision(old => old + 1);
  };

  useEffect(() => {
    if (activeType) {
      const saved = selectedScope ? settingsDraftMemory.get(selectedScope) : null;
      if (saved) {
        // 401 unmount must not throw away incomplete settings changes. A CPT
        // version change cannot silently borrow a new CAS token for old input.
        setDraftIdentity(`${activeType.descriptor.key}:${saved.baseVersion}`);
        setEditLabel(saved.label);
        setEditSettings({ ...saved.settings, supports: [...saved.settings.supports] });
        const stale = saved.stale || saved.baseVersion !== activeType.version;
        setSettingsStale(stale);
        if (stale) setSettingsError(__("The CPT version changed. Your input is retained; review the latest version before saving."));
      } else {
        setDraftIdentity(`${activeType.descriptor.key}:${activeType.version}`);
        setEditLabel(activeType.descriptor.label);
        setEditSettings(effectiveContentTypeSettings(activeType.descriptor.key, activeType.descriptor.settings));
        setSettingsStale(false);
        setSettingsError('');
      }
    }
  }, [activeType?.descriptor.key, activeType?.version, selectedScope]);

  const handleCreate = async (def: ContentTypeDefinitionWithSettings): Promise<boolean> => {
    if (!csrfToken) return false;
    // The post-create navigation must not discard unrelated edits in the
    // currently selected CPT. Refuse before the POST rather than prompting
    // after the server has already committed the new CPT.
    if (settingsChanged || taxonomyChanged) {
      setSettingsError(__("Save or discard the current CPT/taxonomy changes before creating a new content type."));
      return false;
    }
    const requestScope = createScope;
    const requestFormEpoch = createFormEpochRef.current;
    const submittedSnapshot = requestScope ? createCptDraftMemory.get(requestScope) : undefined;
    try {
      const created = await createContentType(def, csrfToken);
      // A detached create response must not erase a newer draft typed after a
      // form close/reopen or a different account's login.
      const sameSubmittedDraft = !requestScope || createCptDraftMemory.get(requestScope) === submittedSnapshot;
      if (requestScope && sameSubmittedDraft)
        createCptDraftMemory.delete(requestScope);
      if (!mountedRef.current || latestCreateScopeRef.current !== requestScope ||
          createFormEpochRef.current !== requestFormEpoch) return false;
      setRevision(r => r + 1);
      onContentTypesChanged();
      // A second Admin edit or new form input can occur while the POST is in
      // flight. Never clear that newer draft or bypass App's dirty guard after
      // the server has committed the create. Preserve all in-progress edits
      // and make the successful server mutation explicit in the UI.
      if (!sameSubmittedDraft || settingsDirtyRef.current) {
        if (sameSubmittedDraft) {
          setCreateDirtyState({ scope: requestScope, dirty: false });
          setCreateOpenState({ scope: requestScope, open: false });
        }
        const message = __("Created CPT {{value1}}, but other changes remain unsaved. Your input is retained; save or discard changes before switching screens.", { value1: created.descriptor.key });
        setSettingsError(message);
        // Provider-owned CPTs have no editable schema form; show the receipt
        // outside that branch so the post-201 success cannot be invisible.
        setCreateNoticeState({ scope: requestScope, message });
        return sameSubmittedDraft;
      }
      setCreateDirtyState({ scope: requestScope, dirty: false });
      setCreateOpenState({ scope: requestScope, open: false });
      // App.mayLeave consults schemaDirtyRef synchronously. React has not yet
      // committed the state change above, so clear only the completed create
      // draft now, before navigating to the newly committed CPT.
      onDirtyChange?.(false);
      setCreateNoticeState({ scope: requestScope, message: __("Created CPT {{value1}}.", { value1: created.descriptor.key }) });
      onSelectTypeKey(created.descriptor.key);
      return true;
    } catch (cause) {
      if (mountedRef.current && latestCreateScopeRef.current === requestScope &&
          cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, __("Your session expired while creating the CPT; unsaved input is retained."));
      }
      throw cause;
    }
  };

  const toggleSupport = (support: ContentSupport, enabled: boolean) => {
    changeEditSettings({ supports: enabled
      ? Array.from(new Set([...editSettings.supports, support]))
      : editSettings.supports.filter(item => item !== support) });
  };

  const handleUpdateSettings = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!activeType || !canEditSettings || draftIdentity !== currentIdentity
      || settingsStale || !csrfToken || savingSettings || taxonomyBusy) return;
    const requestScope = selectedScope;
    const savedAtSubmit = requestScope ? settingsDraftMemory.get(requestScope) : undefined;
    setSavingSettings(true);
    setSettingsError('');
    try {
      // Saving CPT settings also changes the schema CAS. Preserve any taxonomy
      // selection under its original version, so the next load requires rebase.
      if (taxonomyLoaded && taxonomyChanged) rememberTaxonomyDraft(taxonomyLoaded, taxonomyDraft);
      await updateContentType(
        activeType.descriptor.key,
        activeType.version,
        {
          ...activeType.descriptor,
          label: activeType.owner.kind === 'user' ? editLabel.trim() : activeType.descriptor.label,
          settings: {
            ...editSettings,
            publication: isBuiltin ? 'draft_publish' : editSettings.publication,
          },
        },
        csrfToken,
      );
      if (requestScope && settingsDraftMemory.get(requestScope) === savedAtSubmit)
        settingsDraftMemory.delete(requestScope);
      if (!mountedRef.current || latestScopeRef.current !== requestScope) return;
      setSettingsStale(false);
      setRevision(r => r + 1);
      onContentTypesChanged();
    } catch (err) {
      if (!mountedRef.current || latestScopeRef.current !== requestScope) return;
      if (err instanceof ApiClientError && err.status === 412) {
        setSettingsStale(true);
        if (requestScope) {
          const saved = settingsDraftMemory.get(requestScope);
          if (saved) settingsDraftMemory.set(requestScope, { ...saved, stale: true });
        }
        setSettingsError(__("The configuration changed elsewhere (412). Your input is retained; load the latest version before saving again."));
      } else if (err instanceof ApiClientError && err.status === 409) {
        setSettingsError(__("Configuration conflict (409): {{value1}}. Your current input is retained.", { value1: err.message }));
      } else {
        onApiError(err, __("Could not update the content type."));
      }
    } finally {
      if (mountedRef.current && latestScopeRef.current === requestScope) setSavingSettings(false);
    }
  };

  const rebaseSettingsDraft = async () => {
    if (!activeType || !settingsStale || savingSettings || !selectedScope) return;
    const requestScope = selectedScope;
    setSavingSettings(true);
    try {
      const types = await listContentTypes();
      if (!mountedRef.current || latestScopeRef.current !== requestScope) return;
      const fresh = types.find(item => item.descriptor.key === activeKey);
      if (!fresh) {
        setSettingsError(__("The CPT was deleted; your input is retained for copying."));
        return;
      }
      const saved = settingsDraftMemory.get(requestScope);
      if (!saved) {
        setSettingsError(__("There is no CPT draft to review; use Reload from server."));
        return;
      }
      const newSettings = effectiveContentTypeSettings(activeKey, fresh.descriptor.settings);
      const prev = saved.baselineSettings;
      const changed = {
        label: saved.label !== saved.baselineLabel,
        public: saved.settings.public !== prev.public,
        has_archive: saved.settings.has_archive !== prev.has_archive,
        publication: saved.settings.publication !== prev.publication,
        supports: !sameKeys(saved.settings.supports, prev.supports),
      };
      const conflicts = [
        ...(changed.label && fresh.descriptor.label !== saved.baselineLabel ? ['label'] : []),
        ...(changed.public && newSettings.public !== prev.public ? ['public'] : []),
        ...(changed.has_archive && newSettings.has_archive !== prev.has_archive ? ['has_archive'] : []),
        ...(changed.publication && newSettings.publication !== prev.publication ? ['publication'] : []),
        ...(changed.supports && !sameKeys(newSettings.supports, prev.supports) ? ['supports'] : []),
      ];
      if (conflicts.length && !window.confirm(
        __("The server also changed {{value1}}. Keep your current values when reviewing the latest CPT?", { value1: conflicts.join(', ') }),
      )) return;
      const mergedSettings: ContentTypeSettings = {
        public: changed.public ? saved.settings.public : newSettings.public,
        has_archive: changed.has_archive ? saved.settings.has_archive : newSettings.has_archive,
        publication: changed.publication ? saved.settings.publication : newSettings.publication,
        supports: [...(changed.supports ? saved.settings.supports : newSettings.supports)],
      };
      const mergedLabel = changed.label ? saved.label : fresh.descriptor.label;
      settingsDraftMemory.set(requestScope, {
        baseVersion: fresh.version, baselineLabel: fresh.descriptor.label,
        baselineSettings: copySettings(newSettings), label: mergedLabel,
        settings: copySettings(mergedSettings), stale: false,
      });
      setContentTypes(types);
      setEditLabel(mergedLabel); setEditSettings(mergedSettings);
      setDraftIdentity(`${activeKey}:${fresh.version}`);
      setSettingsStale(false);
      setSettingsError(__("Reviewed the latest CPT version; your changes are retained. Review before saving."));
    } catch (cause) {
      if (!mountedRef.current || latestScopeRef.current !== requestScope) return;
      setSettingsError(cause instanceof Error ? cause.message : __("Could not review the CPT version."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, __("Your session expired while reviewing the CPT configuration."));
    } finally {
      if (mountedRef.current && latestScopeRef.current === requestScope) setSavingSettings(false);
    }
  };

  return (
    <section className="space-y-6" data-testid="content-type-schema-settings">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{__("Structure / Content types")}</p>
          <h2 className="mt-1 text-xl font-bold tracking-tight text-zinc-950">{__("Content types & fields")}</h2>
          <p className="mt-1 text-sm text-zinc-600">
            {__("Configure schema, extension field groups and Single Edit layout for each content type.")}
          </p>
        </div>
        <AdminButton variant={showCreateForm ? 'secondary' : 'primary'} size="md" onClick={toggleCreateForm}>
          {showCreateForm ? __("Close CPT creation form") : __("+ Create content type")}
        </AdminButton>
      </div>

      {createNotice && <AdminAlert tone="info">{createNotice}</AdminAlert>}

      {showCreateForm && (
        <div className="rounded-xl border border-zinc-200 bg-zinc-50/70 p-4">
          <ContentTypeForm
            key={createScope ?? 'unverified-identity'}
            disabled={!csrfToken}
            onCreate={handleCreate}
            initialDraft={createScope ? createCptDraftMemory.get(createScope) ?? null : null}
            onDraftChange={onCreateDraftChange}
            onDirtyChange={onCreateFormDirty}
          />
          {createDirty && <p role="status" className="mt-3 text-xs text-zinc-600">
            {createScope
              ? __("Unsaved CPT input is retained in memory by site and account when your session expires. Reloading the browser clears the draft.")
              : __("The site/account could not be verified for draft recovery; save before leaving this screen.")}
          </p>}
          <div className="mt-3">
            <AdminButton variant="secondary" size="sm" onClick={toggleCreateForm}>
              {__("Close form / discard unsaved input")}
            </AdminButton>
          </div>
        </div>
      )}

      <div className="flex flex-wrap gap-2 border-b border-zinc-200 pb-3">
        {contentTypes.map(ct => {
          const active = selectedTypeKey === ct.descriptor.key;
          return (
            <button
              key={ct.descriptor.key}
              type="button"
              aria-pressed={active}
              disabled={taxonomyBusy || savingSettings}
              onClick={() => {
                if (ct.descriptor.key === selectedTypeKey) return;
                onSelectTypeKey(ct.descriptor.key);
              }}
              className={
                'inline-flex h-8 items-center rounded-md px-3 text-xs font-medium transition-colors ' +
                (active
                  ? 'bg-od-accent text-white shadow-2xs'
                  : 'border border-zinc-300 bg-white text-zinc-700 hover:border-zinc-400 hover:bg-zinc-50')
              }
            >
              {ct.descriptor.label} ({ct.descriptor.key})
            </button>
          );
        })}
      </div>

      {activeType && (() => {
        const cptTabs: AdminTabItem<ContentTypeTabId>[] = [
          {
            id: 'overview',
            label: __("Overview & schema"),
            dirty: settingsChanged || settingsStale,
          },
          {
            id: 'taxonomies',
            label: _x("Taxonomies", "content_types.tab_taxonomies"),
            badge: isPattern ? undefined : taxonomyLoaded ? taxonomyDraft.length : undefined,
            dirty: Boolean(taxonomyChanged || taxonomyLoaded?.stale),
          },
          {
            id: 'field_groups',
            label: __("Field groups"),
            badge: fieldGroupCount,
          },
          {
            id: 'layout',
            label: __("Editor layout"),
            badge: layoutVersion !== null ? `v${layoutVersion}` : undefined,
          },
        ];

        const clearAllTaxonomies = () => {
          if (!taxonomyLoaded || !canEditTaxonomies || taxonomyBusy || savingSettings) return;
          setTaxonomyDraft([]);
          rememberTaxonomyDraft(taxonomyLoaded, []);
          setTaxonomyNotice('');
        };

        return (
          <div className="space-y-4">
            <div className="rounded-xl border border-zinc-200 bg-white px-5 pt-5 pb-0">
              <div className="flex flex-wrap items-start justify-between gap-3 pb-4">
                <div>
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-lg font-semibold text-zinc-950">
                      {activeType.descriptor.label}
                    </h3>
                    <AdminBadge tone="neutral">v{activeType.version}</AdminBadge>
                    {canonicalSettings && (
                      <AdminBadge tone={canonicalSettings.public ? 'success' : 'neutral'}>
                        {canonicalSettings.public
                          ? __("Public")
                          : __("Private")}
                      </AdminBadge>
                    )}
                    {canonicalSettings?.has_archive && (
                      <AdminBadge tone="neutral">{__("Archive")}</AdminBadge>
                    )}
                  </div>
                  <p className="mt-1 text-xs text-zinc-500">
                    {__("Key")}: <code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-zinc-700">{activeType.descriptor.key}</code> · {__("· Owner:")}{' '}{activeType.owner.kind}
                  </p>
                </div>
                <div className="text-xs text-zinc-600">
                  {__("Base fields")} ({activeType.descriptor.fields.length}):{' '}
                  <span className="font-mono text-zinc-700">
                    {activeType.descriptor.fields.map(f => f.key).join(', ') || _x("None", "feature.content")}
                  </span>
                </div>
              </div>

              <AdminTabs
                tabs={cptTabs}
                activeId={activeTab}
                onChange={setActiveTab}
                ariaLabel={__("Content type configuration sections")}
                testId="content-type-tabs"
                idPrefix="content-type"
                className="border-b-0"
              />
            </div>

            <AdminTabPanel
              active={activeTab === 'overview'}
              id="content-type-panel-overview"
              labelledBy="content-type-tab-overview"
            >
              <div className="rounded-xl border border-zinc-200 bg-white p-5">
                <h4 className="text-sm font-semibold text-zinc-950">
                  {__("Schema information:")}{' '}{activeType.descriptor.label}
                </h4>
                {canEditSettings ? (
                  <form onSubmit={handleUpdateSettings} className="mt-4 space-y-4" aria-label={__("Content type configuration")}>
                    {activeType.owner.kind === 'user' && (
                      <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
                        <span>{__("Display name")}</span>
                        <input type="text" value={editLabel} required maxLength={100}
                          disabled={!csrfToken || savingSettings} onChange={event => changeEditLabel(event.target.value)}
                          className="h-9 w-full max-w-md rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent" />
                      </label>
                    )}
                    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
                      <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                        <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Public visibility")}</legend>
                        <label className="flex items-center gap-2 text-sm text-zinc-800">
                          <input type="checkbox" checked={editSettings.public}
                            disabled={!csrfToken || savingSettings}
                            onChange={event => changeEditSettings({ public: event.target.checked })} />
                          {__("Public content (public)")}
                        </label>
                        <label className="flex items-center gap-2 text-sm text-zinc-800">
                          <input type="checkbox" checked={editSettings.has_archive}
                            disabled={!csrfToken || savingSettings}
                            onChange={event => changeEditSettings({ has_archive: event.target.checked })} />
                          {__("Enable archive (has_archive)")}
                        </label>
                        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
                          <span>{__("Publishing workflow")}</span>
                          <select className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                            value={editSettings.publication} disabled={!csrfToken || savingSettings || isBuiltin}
                            onChange={event => changeEditSettings({
                              publication: event.target.value as ContentPublicationPolicy,
                            })}>
                            <option value="immediate">{__("Save publishes immediately (immediate)")}</option>
                            <option value="draft_publish">{__("Save draft / Publish (draft_publish)")}</option>
                          </select>
                        </label>
                        {isBuiltin && <p className="text-xs text-zinc-500">{__("Posts and pages always use Save draft / Publish.")}</p>}
                      </fieldset>

                      <div className="space-y-4">
                        <fieldset className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                          <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{__("Editor screen features")}</legend>
                          <label className="flex items-center gap-2 text-sm text-zinc-800">
                            <input type="checkbox" checked={editSettings.supports.includes('editor')}
                              disabled={!csrfToken || savingSettings}
                              onChange={event => toggleSupport('editor', event.target.checked)} />
                            {__("Editor (editor)")}
                          </label>
                          <label className="flex items-center gap-2 text-sm text-zinc-800">
                            <input type="checkbox" checked={editSettings.supports.includes('featured_image')}
                              disabled={!csrfToken || savingSettings}
                              onChange={event => toggleSupport('featured_image', event.target.checked)} />
                            {__("Featured image (featured_image)")}
                          </label>
                          <label className="flex items-center gap-2 text-sm text-zinc-800">
                            <input type="checkbox" checked={editSettings.supports.includes('builder')}
                              disabled={!csrfToken || savingSettings}
                              onChange={event => toggleSupport('builder', event.target.checked)} />
                            {__("Visual Builder (builder)")}
                          </label>
                          <p className="text-xs text-zinc-500">{__("Enable Builder to design individual entries. Disabling a feature does not delete saved text, images or Builder designs.")}</p>
                        </fieldset>

                        <div className="rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                          <div className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
                            {__("Base fields")} ({activeType.descriptor.fields.length})
                          </div>
                          {activeType.descriptor.fields.length === 0 ? (
                            <p className="mt-1.5 text-xs text-zinc-500">{_x("None", "feature.content")}</p>
                          ) : (
                            <div className="mt-2 flex flex-wrap gap-1.5">
                              {activeType.descriptor.fields.map(field => (
                                <span
                                  key={field.key}
                                  className="inline-flex items-center gap-1 rounded-md border border-zinc-200 bg-white px-2 py-1 text-xs text-zinc-700"
                                >
                                  <span className="font-medium text-zinc-900">{field.label}</span>
                                  <code className="font-mono text-[11px] text-zinc-500">{field.key}</code>
                                </span>
                              ))}
                            </div>
                          )}
                        </div>
                      </div>
                    </div>

                    {settingsError && <AdminAlert tone="error">{settingsError}</AdminAlert>}
                    {settingsStale && <AdminAlert tone="warning" role="alert" className="space-y-2">
                      <p>{__("The CPT version changed. Your input is retained but cannot be saved with the old CAS token.")}</p>
                      <AdminButton variant="secondary" size="sm"
                        disabled={savingSettings || taxonomyBusy} onClick={() => void rebaseSettingsDraft()}>
                        {__("Review latest CPT and keep current changes")}
                      </AdminButton>
                    </AdminAlert>}
                    <div className="flex flex-wrap items-center gap-2">
                      <AdminButton type="submit" variant="primary" size="md"
                        disabled={!settingsChanged || settingsStale || !csrfToken || savingSettings || taxonomyBusy}>
                        {savingSettings ? __("Saving...") : __("Save configuration")}
                      </AdminButton>
                      <AdminButton variant="secondary" size="md" disabled={savingSettings || taxonomyBusy}
                        onClick={() => {
                          if (selectedScope) settingsDraftMemory.delete(selectedScope);
                          setSettingsStale(false);
                          if (activeType && canonicalSettings) {
                            setEditLabel(activeType.descriptor.label);
                            setEditSettings(copySettings(canonicalSettings));
                            setDraftIdentity(currentIdentity);
                          }
                          setRevision(r => r + 1);
                          setSettingsError('');
                        }}>
                        {__("Reload from server (discard edits)")}
                      </AdminButton>
                    </div>
                  </form>
                ) : (
                  <p className="mt-3 text-sm text-zinc-600">
                    {isPattern ? __("Pattern is a private content type and its public configuration cannot be changed.")
                      : __("This configuration is provider-owned and read-only here.")}
                  </p>
                )}
              </div>

              {isDormantOrdinaryCpt && (
                <section
                  className="mt-4 rounded-xl border border-amber-200 bg-white p-5 space-y-3"
                  data-testid="retained-trash-maintenance"
                  aria-label={__("Retained Trash Maintenance")}
                >
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div>
                      <h4 className="text-sm font-semibold text-zinc-950">
                        {__("Retained Trash Maintenance")}
                        {retainedTrash?.key === activeKey ? ` (${retainedTrash.data.total})` : ''}
                      </h4>
                      <p className="mt-1 text-xs text-zinc-600">
                        {__("Metadata-only view for trashed entries retained under this inactive or withdrawn content type. Administrators may permanently delete retained Trash rows here.")}
                      </p>
                      <p className="mt-1 text-xs text-amber-800">
                        {__("To restore entries instead of purging them, re-enable the content type provider first.")}
                      </p>
                    </div>
                    <AdminButton
                      variant="secondary"
                      size="sm"
                      disabled={retainedTrashLoading || retainedTrashBusyId !== null}
                      onClick={() => setRetainedTrashReloadTick(n => n + 1)}
                    >
                      {__("Reload")}
                    </AdminButton>
                  </div>

                  {retainedTrashProviderActive && (
                    <AdminAlert tone="info" className="flex flex-wrap items-center justify-between gap-2">
                      <span>{__("This content type is now active. Manage its entries in the ordinary Trash view.")}</span>
                      <a
                        href={activeKey === 'post' ? '#/posts?status=trash' : activeKey === 'page' ? '#/pages?status=trash' : `#/cpt/${encodeURIComponent(activeKey)}?status=trash`}
                        className="font-medium text-od-accent underline"
                      >
                        {__("Open Trash")}
                      </a>
                    </AdminAlert>
                  )}

                  {retainedTrashError && <AdminAlert tone="error">{retainedTrashError}</AdminAlert>}

                  {retainedTrashLoading && (
                    <p role="status" className="text-xs text-zinc-500">
                      {__("Loading retained Trash entries...")}
                    </p>
                  )}

                  {!retainedTrashLoading && retainedTrash?.key === activeKey && retainedTrash.data.items.length === 0 && !retainedTrashError && (
                    <p className="rounded-lg border border-dashed border-zinc-200 p-4 text-xs text-zinc-500">
                      {__("No retained Trash entries for this content type.")}
                    </p>
                  )}

                  {retainedTrash?.key === activeKey && retainedTrash.data.items.length > 0 && (
                    <div className="space-y-3">
                      <div className="overflow-x-auto rounded-lg border border-zinc-200">
                        <table className="w-full table-auto text-left text-xs">
                          <thead className="border-b border-zinc-200 bg-zinc-50 text-[11px] font-semibold uppercase tracking-wider text-zinc-600">
                            <tr>
                              <th className="px-3 py-2">{__("Title")}</th>
                              <th className="px-3 py-2">{__("Language")}</th>
                              <th className="px-3 py-2">{__("Status")}</th>
                              <th className="px-3 py-2">{__("Trashed date")}</th>
                              <th className="px-3 py-2 text-right">{__("Delete")}</th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-zinc-100">
                            {retainedTrash.data.items.map(row => (
                              <tr key={row.id} data-retained-trash-id={row.id}>
                                <td className="px-3 py-2">
                                  <span className="font-medium text-zinc-900">{row.title}</span>{' '}
                                  <span className="font-mono text-zinc-500">#{row.id} · /{row.slug}</span>
                                </td>
                                <td className="px-3 py-2 font-mono text-zinc-700">{row.locale ?? '—'}</td>
                                <td className="px-3 py-2">
                                  <AdminBadge tone="neutral">
                                    {row.publication_status === 'published'
                                      ? __("Published")
                                      : __("Draft")}
                                  </AdminBadge>
                                </td>
                                <td className="px-3 py-2 font-mono text-zinc-600">
                                  {row.trashed_at ? `${row.trashed_at.slice(0, 16)} UTC` : '—'}
                                </td>
                                <td className="px-3 py-2 text-right">
                                  <AdminButton
                                    variant="danger"
                                    size="sm"
                                    disabled={!csrfToken || retainedTrashBusyId !== null}
                                    data-testid={`retained-trash-purge-${row.id}`}
                                    onClick={() => void handlePurgeRetainedRow(row)}
                                  >
                                    {__("Delete permanently")}
                                  </AdminButton>
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                      {retainedTrash.data.total > 20 && (
                        <div className="flex items-center justify-between text-xs text-zinc-600">
                          <span>
                            {retainedTrashOffset + 1}–{Math.min(retainedTrashOffset + retainedTrash.data.items.length, retainedTrash.data.total)} / {retainedTrash.data.total}
                          </span>
                          <div className="flex gap-2">
                            <AdminButton
                              variant="secondary"
                              size="sm"
                              disabled={retainedTrashLoading || retainedTrashOffset === 0}
                              onClick={() => setRetainedTrashOffset(o => Math.max(0, o - 20))}
                            >
                              {__("Previous page")}
                            </AdminButton>
                            <AdminButton
                              variant="secondary"
                              size="sm"
                              disabled={retainedTrashLoading || retainedTrashOffset + 20 >= retainedTrash.data.total}
                              onClick={() => setRetainedTrashOffset(o => o + 20)}
                            >
                              {__("Next page")}
                            </AdminButton>
                          </div>
                        </div>
                      )}
                    </div>
                  )}
                </section>
              )}
            </AdminTabPanel>

            <AdminTabPanel
              active={activeTab === 'taxonomies'}
              id="content-type-panel-taxonomies"
              labelledBy="content-type-tab-taxonomies"
            >
              <section className="rounded-xl border border-zinc-200 bg-white p-5" aria-label={__("Taxonomies applicable to the content type")}>
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div>
                    <h3 className="text-base font-semibold text-zinc-950">{__("Applicable taxonomies")}</h3>
                    <p className="mt-1 text-sm text-zinc-600">
                      {__("Select taxonomies that may be associated with")}{' '}{activeType.descriptor.label}{__(". Clearing a selection does not delete saved associations.")}
                    </p>
                  </div>
                  {!isPattern && canEditTaxonomies && taxonomyLoaded && taxonomyDraft.length > 0 && (
                    <AdminButton
                      variant="ghost"
                      size="sm"
                      disabled={taxonomyBusy || savingSettings}
                      onClick={clearAllTaxonomies}
                    >
                      {__("Clear all")}
                    </AdminButton>
                  )}
                </div>
                {isPattern ? <p className="mt-3 text-sm text-zinc-600">{__("Patterns do not support taxonomies.")}</p>
                  : !taxonomyLoaded ? <p role="status" className="mt-3 text-sm text-zinc-600">{_x("Loading applicable taxonomies…", "feature.content")}</p>
                    : <form className="mt-3.5 space-y-3.5" onSubmit={handleUpdateTaxonomies}>
                        <div className="grid gap-2.5 sm:grid-cols-2">
                          {taxonomyLoaded.available.filter(def =>
                            (def.provider_status === 'active' && def.declaration_state === 'declared')
                            || taxonomyDraft.includes(def.key)).map(def => (
                            <label key={def.key} className="flex items-start gap-2.5 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3 text-sm text-zinc-700 hover:border-zinc-300">
                              <input type="checkbox" className="mt-1" checked={taxonomyDraft.includes(def.key)}
                                disabled={!canEditTaxonomies || !csrfToken || taxonomyBusy || savingSettings
                                  || (!taxonomyDraft.includes(def.key) &&
                                    (def.provider_status !== 'active' || def.declaration_state !== 'declared'))}
                                onChange={event => {
                                  const next = event.target.checked
                                    ? Array.from(new Set([...taxonomyDraft, def.key]))
                                    : taxonomyDraft.filter(value => value !== def.key);
                                  setTaxonomyDraft(next);
                                  rememberTaxonomyDraft(taxonomyLoaded, next);
                                  setTaxonomyNotice('');
                                }} />
                              <span><strong className="block font-medium text-zinc-900">{def.label}</strong>
                                <code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-xs text-zinc-700">{def.key}</code> · <span className="text-xs text-zinc-500">{def.hierarchical ? __("Hierarchical") : __("Non-hierarchical")}</span>
                                {(def.provider_status !== 'active' || def.declaration_state !== 'declared')
                                  && <span className="mt-0.5 block text-xs text-amber-700">{__("The provider is inactive or its declaration was withdrawn; saved selections are retained.")}</span>}
                              </span>
                            </label>
                          ))}
                          {taxonomyDraft.filter(key => !taxonomyLoaded.available.some(def => def.key === key)).map(key => (
                            <label key={key} className="flex items-start gap-2.5 rounded-lg border border-amber-300 bg-amber-50/50 p-3 text-sm text-amber-950">
                              <input type="checkbox" checked disabled={!canEditTaxonomies || taxonomyBusy || savingSettings}
                                onChange={() => {
                                  const next = taxonomyDraft.filter(value => value !== key);
                                  setTaxonomyDraft(next);
                                  rememberTaxonomyDraft(taxonomyLoaded, next);
                                }} />
                              <span>{key} {__("— Taxonomy no longer exists in the current list. Clear the selection to remove the saved key.")}</span>
                            </label>
                          ))}
                        </div>
                        {!canEditTaxonomies && <p className="text-xs text-zinc-500">{__("Only administrators can change taxonomies; patterns do not support them.")}</p>}
                        {taxonomyChanged && <p className="text-xs text-zinc-500" role="status">
                          {selectedScope
                            ? __("Unsaved taxonomy selections are kept in this session's memory by account, site and content type when you navigate or sign in again. Reloading the page clears the draft.")
                            : __("Unsaved selections will be lost when leaving the screen or when your session expires; drafts can only be retained when the site and account are identified.")}
                        </p>}
                        {taxonomyNotice && <AdminAlert tone="info">{taxonomyNotice}</AdminAlert>}
                        {taxonomyLoaded.stale && <AdminAlert tone="warning" className="space-y-2">
                          <p role="alert">{__("The CPT version/schema or taxonomy configuration changed. Your selection is retained; do not save with the old CAS token.")}</p>
                          <AdminButton variant="secondary" size="sm"
                            disabled={taxonomyBusy || savingSettings || !csrfToken}
                            onClick={() => void rebaseTaxonomyDraft()}>
                            {__("Review server version and keep current selections")}
                          </AdminButton>
                        </AdminAlert>}
                        {taxonomyError && <AdminAlert tone="error">{taxonomyError}</AdminAlert>}
                        <div className="flex flex-wrap gap-2">
                          <AdminButton type="submit" variant="primary" size="md"
                            disabled={!canEditTaxonomies || !taxonomyChanged || taxonomyBusy || savingSettings || !csrfToken || taxonomyLoaded.stale}>
                            {taxonomyBusy ? __("Saving…") : __("Save applicable taxonomies")}
                          </AdminButton>
                          <AdminButton variant="secondary" size="md"
                            disabled={taxonomyBusy || savingSettings} onClick={discardTaxonomyDraft}>
                            {__("Discard unsaved selections")}
                          </AdminButton>
                        </div>
                      </form>}
                {!taxonomyLoaded && taxonomyError && <AdminAlert tone="error" className="mt-3">{taxonomyError}</AdminAlert>}
              </section>
            </AdminTabPanel>

            <AdminTabPanel
              active={activeTab === 'field_groups'}
              id="content-type-panel-field_groups"
              labelledBy="content-type-tab-field_groups"
            >
              <FieldGroupsManager
                typeKey={activeType.descriptor.key}
                csrfToken={csrfToken}
                onApiError={onApiError}
                onChanged={() => setRevision(r => r + 1)}
                onCountChange={setFieldGroupCount}
              />
            </AdminTabPanel>

            <AdminTabPanel
              active={activeTab === 'layout'}
              id="content-type-panel-layout"
              labelledBy="content-type-tab-layout"
            >
              <EditorLayoutConfigurator
                typeKey={activeType.descriptor.key}
                baseFields={activeType.descriptor.fields}
                csrfToken={csrfToken}
                onApiError={onApiError}
                onVersionChange={setLayoutVersion}
              />
            </AdminTabPanel>
          </div>
        );
      })()}
    </section>
  );
};
