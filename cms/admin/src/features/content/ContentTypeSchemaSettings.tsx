import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  createContentType,
  effectiveContentTypeSettings,
  listContentTypes,
  updateContentType,
  type ContentPublicationPolicy,
  type ContentSupport,
  type ContentTypeDefinitionWithSettings,
  type ContentTypeSettings,
  type OwnedContentTypeWithSettings,
} from '../../api/content.ts';
import { ApiClientError, requestJson } from '../../api/client.ts';
import { listTaxonomyDefinitions, type TaxonomyDefinition } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import {
  ContentTypeForm, isContentTypeCreationDraftDirty,
  type ContentTypeCreationDraft,
} from './ContentTypeForm.tsx';
import { EditorLayoutConfigurator } from './EditorLayoutConfigurator.tsx';
import { FieldGroupsManager } from './FieldGroupsManager.tsx';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

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
    signal, defaultErrorMessage: tStatic("feature.content.contenttypeschemasettings.could_not_load_configured_taxonomies"),
  });
}

function putTaxonomyEligibility(typeKey: string, version: number, taxonomies: string[], token: string): Promise<OwnedContentTypeWithSettings> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}`, {
    method: 'PUT', csrfToken: token, ifMatchVersion: version, contentType: 'application/json',
    body: JSON.stringify({ taxonomies, expected_version: version }),
    defaultErrorMessage: tStatic("api.content.update_assigned_taxonomies"),
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
  const [taxonomyState, setTaxonomyState] = useState<LoadedTaxonomyState | null>(null);
  const [taxonomyDraft, setTaxonomyDraft] = useState<string[]>([]);
  const [taxonomyBusy, setTaxonomyBusy] = useState(false);
  const [taxonomyError, setTaxonomyError] = useState('');
  const [taxonomyNotice, setTaxonomyNotice] = useState('');
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
    if (createDirty && !window.confirm(tStatic("feature.content.contenttypeschemasettings.discard_the_unsaved_new_content_type_form"))) return;
    createFormEpochRef.current += 1;
    // Explicit close/discard is destructive; a 401 unmount deliberately does
    // not call this handler so same-account reauthentication can restore.
    if (createScope) createCptDraftMemory.delete(createScope);
    setCreateDirtyState({ scope: createScope, dirty: false });
    setCreateOpenState({ scope: createScope, open: false });
  };

  const load = async () => {
    try {
      const list = await listContentTypes();
      setContentTypes(list);
      if (!selectedTypeKey && list.length > 0) {
        onSelectTypeKey(list[0].descriptor.key);
      }
    } catch (err) {
      onApiError(err, tStatic("feature.content.contenttypeschemasettings.could_not_load_the_content_type_list"));
    }
  };

  useEffect(() => {
    void load();
  }, [revision]);

  const activeType = contentTypes.find(ct => ct.descriptor.key === selectedTypeKey) ?? null;
  const activeKey = activeType?.descriptor.key ?? '';
  const currentIdentity = activeType ? `${activeKey}:${activeType.version}` : '';
  const selectedScope = scopeKey(siteInstanceId, user, activeKey);
  latestScopeRef.current = selectedScope;
  latestTypeRef.current = activeKey;
  const isBuiltin = activeKey === 'post' || activeKey === 'page';
  const isPattern = activeKey === 'pattern';
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
        if (stale) setTaxonomyNotice(tStatic("feature.content.contenttypeschemasettings.your_selection_was_recovered_but_the_cpt_or_taxonomy"));
        else setTaxonomyNotice(tStatic("feature.content.contenttypeschemasettings.recovered_this_account_s_unsaved_taxonomy_selection"));
      } else {
        setTaxonomyDraft([...applied.taxonomies]);
        setTaxonomyState({ identity: currentIdentity, available,
          applied: [...applied.taxonomies], baseVersion: applied.type_version,
          stale: activeType!.version !== applied.type_version });
        if (activeType!.version !== applied.type_version) {
          setTaxonomyNotice(tStatic("feature.content.contenttypeschemasettings.the_cpt_changed_while_loading_review_the_latest_version"));
        }
      }
    }).catch(error => {
      if (!active || controller.signal.aborted) return;
      setTaxonomyError(error instanceof Error ? error.message : tStatic("feature.content.contenttypeschemasettings.could_not_load_taxonomy_associations"));
      onApiError(error, tStatic("feature.content.contenttypeschemasettings.could_not_load_taxonomy_associations"));
    });
    return () => { active = false; controller.abort(); };
  }, [activeKey, currentIdentity, revision, isPattern, selectedScope]);

  const handleUpdateTaxonomies = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!activeType || !taxonomyLoaded || !canEditTaxonomies || isPattern || taxonomyBusy
      || savingSettings || !csrfToken || taxonomyLoaded.stale || !taxonomyChanged) return;
    if (settingsChanged && !window.confirm(
      tStatic("feature.content.contenttypeschemasettings.saving_taxonomies_reloads_the_cpt_version_unsaved_general_configuration"),
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
      setTaxonomyNotice(tStatic("feature.content.contenttypeschemasettings.applicable_taxonomies_saved_successfully"));
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
        ? tStatic("feature.content.contenttypeschemasettings.412_cpt_schema_changed_your_selection_is_retained_review")
        : error instanceof ApiClientError && error.status === 403
          ? tStatic("feature.content.contenttypeschemasettings.403_you_do_not_have_permission_to_update_taxonomies")
          : error instanceof Error ? error.message : tStatic("feature.content.contenttypeschemasettings.could_not_save_taxonomies");
      setTaxonomyError(suffix);
      if (error instanceof ApiClientError && error.status === 401) onApiError(error, suffix);
    } finally {
      setTaxonomyBusy(false);
    }
  };

  const rebaseTaxonomyDraft = async () => {
    if (!taxonomyLoaded || !activeKey || taxonomyBusy || savingSettings) return;
    if (settingsChanged && !window.confirm(
      tStatic("feature.content.contenttypeschemasettings.reviewing_taxonomies_reloads_the_latest_cpt_version_and_discards"),
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
        setTaxonomyError(tStatic("feature.content.contenttypeschemasettings.the_content_type_was_deleted_your_draft_is_retained"));
        return;
      }
      if (fresh.version !== configured.type_version) {
        // These two HTTP calls may themselves straddle a concurrent edit.
        // Never pair pre-edit associations with the post-edit CAS version.
        setTaxonomyState(previous => previous ? { ...previous, stale: true } : previous);
        setTaxonomyError(tStatic("feature.content.contenttypeschemasettings.the_cpt_changed_during_review_your_input_is_retained"));
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
      setTaxonomyNotice(tStatic("feature.content.contenttypeschemasettings.loaded_the_latest_cpt_version_and_reapplied_your_changed"));
    } catch (cause) {
      if (!mountedRef.current) return;
      if (latestScopeRef.current !== startingScope || latestTypeRef.current !== startingType) return;
      setTaxonomyError(cause instanceof Error ? cause.message : tStatic("feature.content.contenttypeschemasettings.could_not_review_the_latest_configuration"));
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, tStatic("feature.content.contenttypeschemasettings.your_session_expired_while_reviewing_taxonomies"));
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
    setTaxonomyNotice(tStatic("feature.content.contenttypeschemasettings.discarded_unsaved_taxonomy_selections_loading_the_server_configuration"));
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
        if (stale) setSettingsError(tStatic("feature.content.contenttypeschemasettings.the_cpt_version_changed_your_input_is_retained_review"));
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
      setSettingsError(tStatic("feature.content.contenttypeschemasettings.save_or_discard_the_current_cpt_taxonomy_changes_before"));
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
        const message = tStatic("feature.content.contenttypeschemasettings.created_cpt_value_but_other_changes_remain_unsaved_your", { value1: created.descriptor.key });
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
      setCreateNoticeState({ scope: requestScope, message: tStatic("feature.content.contenttypeschemasettings.created_cpt_value", { value1: created.descriptor.key }) });
      onSelectTypeKey(created.descriptor.key);
      return true;
    } catch (cause) {
      if (mountedRef.current && latestCreateScopeRef.current === requestScope &&
          cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, tStatic("feature.content.contenttypeschemasettings.your_session_expired_while_creating_the_cpt_unsaved_input"));
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
        setSettingsError(tStatic("feature.content.contenttypeschemasettings.the_configuration_changed_elsewhere_412_your_input_is_retained"));
      } else if (err instanceof ApiClientError && err.status === 409) {
        setSettingsError(tStatic("feature.content.contenttypeschemasettings.configuration_conflict_409_value_your_current_input_is_retained", { value1: err.message }));
      } else {
        onApiError(err, tStatic("api.content.update_the_content_type"));
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
        setSettingsError(tStatic("feature.content.contenttypeschemasettings.the_cpt_was_deleted_your_input_is_retained_for"));
        return;
      }
      const saved = settingsDraftMemory.get(requestScope);
      if (!saved) {
        setSettingsError(tStatic("feature.content.contenttypeschemasettings.there_is_no_cpt_draft_to_review_use_reload"));
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
        tStatic("feature.content.contenttypeschemasettings.the_server_also_changed_value_keep_your_current_values", { value1: conflicts.join(', ') }),
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
      setSettingsError(tStatic("feature.content.contenttypeschemasettings.reviewed_the_latest_cpt_version_your_changes_are_retained"));
    } catch (cause) {
      if (!mountedRef.current || latestScopeRef.current !== requestScope) return;
      setSettingsError(cause instanceof Error ? cause.message : tStatic("feature.content.contenttypeschemasettings.could_not_review_the_cpt_version"));
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, tStatic("feature.content.contenttypeschemasettings.your_session_expired_while_reviewing_the_cpt_configuration"));
    } finally {
      if (mountedRef.current && latestScopeRef.current === requestScope) setSavingSettings(false);
    }
  };

  return (
    <section className="space-y-6" data-testid="content-type-schema-settings">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{tStatic("feature.content.contenttypeschemasettings.structure_content_types")}</p>
          <h2 className="mt-1 text-xl font-bold tracking-tight text-zinc-950">{tStatic("shell.content_types")}</h2>
          <p className="mt-1 text-sm text-zinc-600">
            {tStatic("feature.content.contenttypeschemasettings.configure_schema_extension_field_groups_and_single_edit_layout")}
          </p>
        </div>
        <AdminButton variant={showCreateForm ? 'secondary' : 'primary'} size="md" onClick={toggleCreateForm}>
          {showCreateForm ? tStatic("feature.content.contenttypeschemasettings.close_cpt_creation_form") : tStatic("feature.content.contenttypeschemasettings.create_content_type")}
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
              ? tStatic("feature.content.contenttypeschemasettings.unsaved_cpt_input_is_retained_in_memory_by_site")
              : tStatic("feature.content.contenttypeschemasettings.the_site_account_could_not_be_verified_for_draft")}
          </p>}
          <div className="mt-3">
            <AdminButton variant="secondary" size="sm" onClick={toggleCreateForm}>
              {tStatic("feature.content.contenttypeschemasettings.close_form_discard_unsaved_input")}
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

      {activeType && (
        <div className="space-y-6">
          <div className="rounded-xl border border-zinc-200 bg-white p-5">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-100 pb-3">
              <div>
                <h3 className="text-base font-semibold text-zinc-950">
                  {tStatic("feature.content.contenttypeschemasettings.schema_information")}{' '}{activeType.descriptor.label}
                </h3>
                <p className="mt-0.5 text-xs text-zinc-500">
                  {tStatic("feature.common.key")}: <code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-zinc-700">{activeType.descriptor.key}</code> {tStatic("feature.content.contenttypeschemasettings.owner")}{' '}{activeType.owner.kind}
                </p>
              </div>
              <AdminBadge tone="neutral">v{activeType.version}</AdminBadge>
            </div>
            <div className="mt-3 text-xs text-zinc-600">
              {tStatic("feature.common.base_fields")} ({activeType.descriptor.fields.length}):{' '}
              {activeType.descriptor.fields.map(f => f.key).join(', ') || tStatic("feature.content.contenttypeschemasettings.none")}
            </div>
            {canEditSettings ? (
              <form onSubmit={handleUpdateSettings} className="mt-5 space-y-4" aria-label={tStatic("feature.content.contenttypeschemasettings.content_type_configuration")}>
                {activeType.owner.kind === 'user' && (
                  <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
                    <span>{tStatic("multilingual.label")}</span>
                    <input type="text" value={editLabel} required maxLength={100}
                      disabled={!csrfToken || savingSettings} onChange={event => changeEditLabel(event.target.value)}
                      className="h-9 w-full max-w-md rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent" />
                  </label>
                )}
                <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                  <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{tStatic("feature.content.contenttypeschemasettings.public_visibility")}</legend>
                  <label className="flex items-center gap-2 text-sm text-zinc-800">
                    <input type="checkbox" checked={editSettings.public}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => changeEditSettings({ public: event.target.checked })} />
                    {tStatic("feature.content.contenttypeschemasettings.public_content_public")}
                  </label>
                  <label className="flex items-center gap-2 text-sm text-zinc-800">
                    <input type="checkbox" checked={editSettings.has_archive}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => changeEditSettings({ has_archive: event.target.checked })} />
                    {tStatic("feature.content.contenttypeform.enable_archive_has_archive")}
                  </label>
                  <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
                    <span>{tStatic("feature.content.contenttypeform.publishing_workflow")}</span>
                    <select className="h-9 w-full max-w-md rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                      value={editSettings.publication} disabled={!csrfToken || savingSettings || isBuiltin}
                      onChange={event => changeEditSettings({
                        publication: event.target.value as ContentPublicationPolicy,
                      })}>
                      <option value="immediate">{tStatic("feature.content.contenttypeform.save_publishes_immediately_immediate")}</option>
                      <option value="draft_publish">{tStatic("feature.content.contenttypeform.save_draft_publish_draft_publish")}</option>
                    </select>
                  </label>
                  {isBuiltin && <p className="text-xs text-zinc-500">{tStatic("feature.content.contenttypeschemasettings.posts_and_pages_always_use_save_draft_publish")}</p>}
                </fieldset>
                <fieldset className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3.5">
                  <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-zinc-700">{tStatic("feature.content.contenttypeform.editor_screen_features")}</legend>
                  <label className="flex items-center gap-2 text-sm text-zinc-800">
                    <input type="checkbox" checked={editSettings.supports.includes('editor')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('editor', event.target.checked)} />
                    {tStatic("feature.content.contenttypeform.editor_editor")}
                  </label>
                  <label className="flex items-center gap-2 text-sm text-zinc-800">
                    <input type="checkbox" checked={editSettings.supports.includes('featured_image')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('featured_image', event.target.checked)} />
                    {tStatic("feature.content.contenttypeform.featured_image_featured_image")}
                  </label>
                  <label className="flex items-center gap-2 text-sm text-zinc-800">
                    <input type="checkbox" checked={editSettings.supports.includes('builder')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('builder', event.target.checked)} />
                    {tStatic("feature.common.visual_builder")}
                  </label>
                  <p className="text-xs text-zinc-500">{tStatic("feature.content.contenttypeschemasettings.enable_builder_to_design_individual_entries_disabling_a_feature")}</p>
                </fieldset>
                {settingsError && <AdminAlert tone="error">{settingsError}</AdminAlert>}
                {settingsStale && <AdminAlert tone="warning" role="alert" className="space-y-2">
                  <p>{tStatic("feature.content.contenttypeschemasettings.the_cpt_version_changed_your_input_is_retained_but")}</p>
                  <AdminButton variant="secondary" size="sm"
                    disabled={savingSettings || taxonomyBusy} onClick={() => void rebaseSettingsDraft()}>
                    {tStatic("feature.content.contenttypeschemasettings.review_latest_cpt_and_keep_current_changes")}
                  </AdminButton>
                </AdminAlert>}
                <div className="flex flex-wrap items-center gap-2">
                  <AdminButton type="submit" variant="primary" size="md"
                    disabled={!settingsChanged || settingsStale || !csrfToken || savingSettings || taxonomyBusy}>
                    {savingSettings ? tStatic("feature.users.userprofilepanel.saving") : tStatic("feature.content.contenttypeschemasettings.save_configuration")}
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
                    {tStatic("feature.content.contenttypeschemasettings.reload_from_server_discard_edits")}
                  </AdminButton>
                </div>
              </form>
            ) : (
              <p className="mt-4 text-sm text-zinc-600">
                {isPattern ? tStatic("feature.content.contenttypeschemasettings.pattern_is_a_private_content_type_and_its_public")
                  : tStatic("feature.content.contenttypeschemasettings.this_configuration_is_provider_owned_and_read_only_here")}
              </p>
            )}
          </div>

          <section className="rounded-xl border border-zinc-200 bg-white p-5" aria-label={tStatic("feature.content.contenttypeschemasettings.taxonomies_applicable_to_the_content_type")}>
            <h3 className="text-base font-semibold text-zinc-950">{tStatic("feature.content.contenttypeschemasettings.applicable_taxonomies")}</h3>
            <p className="mt-1 text-sm text-zinc-600">
              {tStatic("feature.content.contenttypeschemasettings.select_taxonomies_that_may_be_associated_with")}{' '}{activeType.descriptor.label}{tStatic("feature.content.contenttypeschemasettings.clearing_a_selection_does_not_delete_saved_associations")}
            </p>
            {isPattern ? <p className="mt-3 text-sm text-zinc-600">{tStatic("feature.content.contenttypeschemasettings.patterns_do_not_support_taxonomies")}</p>
              : !taxonomyLoaded ? <p role="status" className="mt-3 text-sm text-zinc-600">{tStatic("feature.content.contenttypeschemasettings.loading_applicable_taxonomies")}</p>
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
                            <code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-xs text-zinc-700">{def.key}</code> · <span className="text-xs text-zinc-500">{def.hierarchical ? tStatic("feature.content.contenttypeschemasettings.hierarchical") : tStatic("feature.content.contenttypeschemasettings.non_hierarchical")}</span>
                            {(def.provider_status !== 'active' || def.declaration_state !== 'declared')
                              && <span className="mt-0.5 block text-xs text-amber-700">{tStatic("feature.content.contenttypeschemasettings.the_provider_is_inactive_or_its_declaration_was_withdrawn")}</span>}
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
                          <span>{key} {tStatic("feature.content.contenttypeschemasettings.taxonomy_no_longer_exists_in_the_current_list_clear")}</span>
                        </label>
                      ))}
                    </div>
                    {!canEditTaxonomies && <p className="text-xs text-zinc-500">{tStatic("feature.content.contenttypeschemasettings.only_administrators_can_change_taxonomies_patterns_do_not_support")}</p>}
                    {taxonomyChanged && <p className="text-xs text-zinc-500" role="status">
                      {selectedScope
                        ? tStatic("feature.content.contenttypeschemasettings.unsaved_taxonomy_selections_are_kept_in_this_session_s")
                        : tStatic("feature.content.contenttypeschemasettings.unsaved_selections_will_be_lost_when_leaving_the_screen")}
                    </p>}
                    {taxonomyNotice && <AdminAlert tone="info">{taxonomyNotice}</AdminAlert>}
                    {taxonomyLoaded.stale && <AdminAlert tone="warning" className="space-y-2">
                      <p role="alert">{tStatic("feature.content.contenttypeschemasettings.the_cpt_version_schema_or_taxonomy_configuration_changed_your")}</p>
                      <AdminButton variant="secondary" size="sm"
                        disabled={taxonomyBusy || savingSettings || !csrfToken}
                        onClick={() => void rebaseTaxonomyDraft()}>
                        {tStatic("feature.content.contenttypeschemasettings.review_server_version_and_keep_current_selections")}
                      </AdminButton>
                    </AdminAlert>}
                    {taxonomyError && <AdminAlert tone="error">{taxonomyError}</AdminAlert>}
                    <div className="flex flex-wrap gap-2">
                      <AdminButton type="submit" variant="primary" size="md"
                        disabled={!canEditTaxonomies || !taxonomyChanged || taxonomyBusy || savingSettings || !csrfToken || taxonomyLoaded.stale}>
                        {taxonomyBusy ? tStatic("reading.saving") : tStatic("feature.content.contenttypeschemasettings.save_applicable_taxonomies")}
                      </AdminButton>
                      <AdminButton variant="secondary" size="md"
                        disabled={taxonomyBusy || savingSettings} onClick={discardTaxonomyDraft}>
                        {tStatic("feature.content.contenttypeschemasettings.discard_unsaved_selections")}
                      </AdminButton>
                    </div>
                  </form>}
            {!taxonomyLoaded && taxonomyError && <AdminAlert tone="error" className="mt-3">{taxonomyError}</AdminAlert>}
          </section>

          <FieldGroupsManager
            typeKey={activeType.descriptor.key}
            csrfToken={csrfToken}
            onApiError={onApiError}
            onChanged={() => setRevision(r => r + 1)}
          />

          <EditorLayoutConfigurator
            typeKey={activeType.descriptor.key}
            baseFields={activeType.descriptor.fields}
            csrfToken={csrfToken}
            onApiError={onApiError}
          />
        </div>
      )}
    </section>
  );
};
