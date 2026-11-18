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
    signal, defaultErrorMessage: 'Không thể tải taxonomy đã cấu hình.',
  });
}

function putTaxonomyEligibility(typeKey: string, version: number, taxonomies: string[], token: string): Promise<OwnedContentTypeWithSettings> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}`, {
    method: 'PUT', csrfToken: token, ifMatchVersion: version, contentType: 'application/json',
    body: JSON.stringify({ taxonomies, expected_version: version }),
    defaultErrorMessage: 'Không thể cập nhật taxonomy áp dụng.',
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
    if (createDirty && !window.confirm('Bỏ bản nhập tạo loại nội dung mới chưa lưu?')) return;
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
      onApiError(err, 'Không thể tải danh sách loại nội dung.');
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
  const isSection = activeKey === 'section';
  const canEditSettings = Boolean(activeType && (
    activeType.owner.kind === 'user'
    || (activeType.owner.kind === 'core' && isBuiltin)
  ));
  // Site-owned CPT taxonomy associations are independent of provider-owned
  // schema editing. Admin may override plugin/theme CPT eligibility without
  // modifying the provider's schema; Section and Editors remain restricted.
  const canEditTaxonomies = Boolean(activeType && !isSection && user?.role === 'admin');
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
    if (!currentIdentity || !activeKey || isSection) {
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
        if (stale) setTaxonomyNotice('Bản chọn đã khôi phục, nhưng CPT hoặc cấu hình taxonomy trên máy chủ đã thay đổi. Hãy đối chiếu trước khi lưu.');
        else setTaxonomyNotice('Đã khôi phục lựa chọn taxonomy chưa lưu của tài khoản này.');
      } else {
        setTaxonomyDraft([...applied.taxonomies]);
        setTaxonomyState({ identity: currentIdentity, available,
          applied: [...applied.taxonomies], baseVersion: applied.type_version,
          stale: activeType!.version !== applied.type_version });
        if (activeType!.version !== applied.type_version) {
          setTaxonomyNotice('CPT đã thay đổi trong lúc tải dữ liệu. Hãy đối chiếu phiên bản mới trước khi lưu.');
        }
      }
    }).catch(error => {
      if (!active || controller.signal.aborted) return;
      setTaxonomyError(error instanceof Error ? error.message : 'Không thể tải liên kết taxonomy.');
      onApiError(error, 'Không thể tải liên kết taxonomy.');
    });
    return () => { active = false; controller.abort(); };
  }, [activeKey, currentIdentity, revision, isSection, selectedScope]);

  const handleUpdateTaxonomies = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!activeType || !taxonomyLoaded || !canEditTaxonomies || isSection || taxonomyBusy
      || savingSettings || !csrfToken || taxonomyLoaded.stale || !taxonomyChanged) return;
    if (settingsChanged && !window.confirm(
      'Lưu taxonomy sẽ tải lại phiên bản CPT. Phần cấu hình chung chưa lưu có thể bị bỏ. Tiếp tục?',
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
      setTaxonomyNotice('Đã lưu taxonomy áp dụng thành công.');
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
        ? '412 — Schema CPT đã thay đổi. Lựa chọn đang nhập được giữ; đối chiếu với máy chủ trước khi lưu lại.'
        : error instanceof ApiClientError && error.status === 403
          ? '403 — Bạn không có quyền cập nhật taxonomy.'
          : error instanceof Error ? error.message : 'Không thể lưu taxonomy.';
      setTaxonomyError(suffix);
      if (error instanceof ApiClientError && error.status === 401) onApiError(error, suffix);
    } finally {
      setTaxonomyBusy(false);
    }
  };

  const rebaseTaxonomyDraft = async () => {
    if (!taxonomyLoaded || !activeKey || taxonomyBusy || savingSettings) return;
    if (settingsChanged && !window.confirm(
      'Đối chiếu taxonomy sẽ tải phiên bản CPT mới và bỏ phần cấu hình chung chưa lưu. Tiếp tục?',
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
        setTaxonomyError('Loại nội dung đã bị xóa. Bản nháp vẫn được giữ nhưng không thể lưu.');
        return;
      }
      if (fresh.version !== configured.type_version) {
        // These two HTTP calls may themselves straddle a concurrent edit.
        // Never pair pre-edit associations with the post-edit CAS version.
        setTaxonomyState(previous => previous ? { ...previous, stale: true } : previous);
        setTaxonomyError('CPT thay đổi trong lúc đối chiếu. Bản nhập vẫn được giữ; hãy thử lại để lấy cùng một phiên bản.');
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
      setTaxonomyNotice('Đã lấy phiên bản CPT mới và áp dụng lại các lựa chọn bạn đã sửa. Kiểm tra trước khi lưu.');
    } catch (cause) {
      if (!mountedRef.current) return;
      if (latestScopeRef.current !== startingScope || latestTypeRef.current !== startingType) return;
      setTaxonomyError(cause instanceof Error ? cause.message : 'Không thể đối chiếu cấu hình mới.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, 'Phiên đăng nhập đã hết hạn khi đối chiếu taxonomy.');
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
    setTaxonomyNotice('Đã bỏ lựa chọn taxonomy chưa lưu; đang tải cấu hình máy chủ.');
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
        if (stale) setSettingsError('Phiên bản CPT đã thay đổi. Bản nhập được giữ; đối chiếu phiên bản trước khi lưu.');
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
      setSettingsError('Hãy lưu hoặc hủy các thay đổi CPT/taxonomy hiện tại trước khi tạo loại nội dung mới.');
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
        const message = `Đã tạo CPT ${created.descriptor.key}, nhưng có thay đổi khác chưa lưu. Bản nhập được giữ; hãy lưu hoặc bỏ thay đổi trước khi chuyển màn hình.`;
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
      setCreateNoticeState({ scope: requestScope, message: `Đã tạo CPT ${created.descriptor.key}.` });
      onSelectTypeKey(created.descriptor.key);
      return true;
    } catch (cause) {
      if (mountedRef.current && latestCreateScopeRef.current === requestScope &&
          cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, 'Phiên đăng nhập đã hết hạn khi tạo CPT; bản nhập chưa lưu được giữ.');
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
        setSettingsError('Cấu hình đã thay đổi ở nơi khác (412). Bản nhập vẫn được giữ; tải phiên bản mới trước khi lưu lại.');
      } else if (err instanceof ApiClientError && err.status === 409) {
        setSettingsError(`Xung đột cấu hình (409): ${err.message}. Cấu hình đang nhập vẫn được giữ.`);
      } else {
        onApiError(err, 'Không thể cập nhật loại nội dung.');
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
        setSettingsError('CPT đã bị xóa; bản nhập vẫn được giữ để sao chép.');
        return;
      }
      const saved = settingsDraftMemory.get(requestScope);
      if (!saved) {
        setSettingsError('Không còn bản nhập CPT để đối chiếu; hãy dùng Tải lại từ máy chủ.');
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
        `Máy chủ cũng thay đổi ${conflicts.join(', ')}. Giữ các giá trị bạn đang nhập khi đối chiếu CPT mới?`,
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
      setSettingsError('Đã đối chiếu phiên bản CPT mới; các thay đổi đang nhập được giữ. Kiểm tra trước khi lưu.');
    } catch (cause) {
      if (!mountedRef.current || latestScopeRef.current !== requestScope) return;
      setSettingsError(cause instanceof Error ? cause.message : 'Không thể đối chiếu phiên bản CPT.');
      if (cause instanceof ApiClientError && cause.status === 401)
        onApiError(cause, 'Phiên đăng nhập hết hạn khi đối chiếu cấu hình CPT.');
    } finally {
      if (mountedRef.current && latestScopeRef.current === requestScope) setSavingSettings(false);
    }
  };

  return (
    <section className="space-y-6" data-testid="content-type-schema-settings">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Loại nội dung &amp; Trường</h2>
          <p className="text-sm text-slate-600">
            Cấu hình schema, nhóm trường mở rộng và bố cục Single Edit cho từng loại nội dung.
          </p>
        </div>
        <button
          type="button"
          onClick={toggleCreateForm}
          className="rounded-lg bg-slate-900 px-3.5 py-2 text-sm font-semibold text-white hover:bg-slate-800"
        >
          {showCreateForm ? 'Đóng form tạo CPT' : '+ Tạo loại nội dung mới'}
        </button>
      </div>

      {createNotice && <p role="status" className="rounded border border-sky-200 bg-sky-50 p-3 text-sm text-sky-900">
        {createNotice}
      </p>}

      {showCreateForm && (
        <div className="rounded-xl border border-slate-200 bg-slate-50 p-4">
          <ContentTypeForm
            key={createScope ?? 'unverified-identity'}
            disabled={!csrfToken}
            onCreate={handleCreate}
            initialDraft={createScope ? createCptDraftMemory.get(createScope) ?? null : null}
            onDraftChange={onCreateDraftChange}
            onDirtyChange={onCreateFormDirty}
          />
          {createDirty && <p role="status" className="mt-3 text-xs text-slate-600">
            {createScope
              ? 'Bản nhập CPT chưa lưu được giữ trong RAM theo website và tài khoản khi hết phiên đăng nhập. Tải lại trình duyệt sẽ xóa nháp.'
              : 'Không xác minh được website/tài khoản để khôi phục nháp; hãy lưu trước khi rời màn hình.'}
          </p>}
          <button type="button" className="mt-3 rounded border border-slate-300 px-3 py-2 text-sm"
            onClick={toggleCreateForm}>Đóng form / bỏ bản nhập chưa lưu</button>
        </div>
      )}

      <div className="flex flex-wrap gap-2 border-b border-slate-200 pb-3">
        {contentTypes.map(ct => (
          <button
            key={ct.descriptor.key}
            type="button"
            aria-pressed={selectedTypeKey === ct.descriptor.key}
            disabled={taxonomyBusy || savingSettings}
            onClick={() => {
              if (ct.descriptor.key === selectedTypeKey) return;
              onSelectTypeKey(ct.descriptor.key);
            }}
            className={
              'rounded-lg px-3 py-1.5 text-sm font-medium ' +
              (selectedTypeKey === ct.descriptor.key
                ? 'bg-slate-900 text-white'
                : 'border border-slate-200 bg-white text-slate-700 hover:bg-slate-50')
            }
          >
            {ct.descriptor.label} ({ct.descriptor.key})
          </button>
        ))}
      </div>

      {activeType && (
        <div className="space-y-8">
          <div className="rounded-xl border border-slate-200 bg-white p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h3 className="text-base font-semibold text-slate-900">
                  Thông tin Schema: {activeType.descriptor.label}
                </h3>
                <p className="text-xs text-slate-500">
                  Key: <code>{activeType.descriptor.key}</code> · Chủ sở hữu: {activeType.owner.kind} · v{activeType.version}
                </p>
              </div>
            </div>
            <div className="mt-3 text-xs text-slate-600">
              Base fields ({activeType.descriptor.fields.length}):{' '}
              {activeType.descriptor.fields.map(f => f.key).join(', ') || 'Chưa có'}
            </div>
            {canEditSettings ? (
              <form onSubmit={handleUpdateSettings} className="mt-5 space-y-4" aria-label="Cấu hình loại nội dung">
                {activeType.owner.kind === 'user' && (
                  <label className="block text-sm font-medium text-slate-800">
                    Tên hiển thị
                    <input type="text" value={editLabel} required maxLength={100}
                      disabled={!csrfToken || savingSettings} onChange={event => changeEditLabel(event.target.value)}
                      className="mt-1 block w-full max-w-md rounded border border-slate-300 px-3 py-2" />
                  </label>
                )}
                <fieldset className="space-y-3 rounded-lg border border-slate-200 p-3">
                  <legend className="px-1 text-sm font-semibold">Hiển thị công khai</legend>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.public}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => changeEditSettings({ public: event.target.checked })} />
                    Công khai nội dung (public)
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.has_archive}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => changeEditSettings({ has_archive: event.target.checked })} />
                    Cho phép trang danh sách (has_archive)
                  </label>
                  <label className="block text-sm font-medium">
                    Cơ chế xuất bản
                    <select className="mt-1 block w-full max-w-md rounded border border-slate-300 px-3 py-2"
                      value={editSettings.publication} disabled={!csrfToken || savingSettings || isBuiltin}
                      onChange={event => changeEditSettings({
                        publication: event.target.value as ContentPublicationPolicy,
                      })}>
                      <option value="immediate">Lưu là xuất bản (immediate)</option>
                      <option value="draft_publish">Lưu nháp / Xuất bản (draft_publish)</option>
                    </select>
                  </label>
                  {isBuiltin && <p className="text-xs text-slate-600">Post và Page luôn dùng Lưu nháp / Xuất bản.</p>}
                </fieldset>
                <fieldset className="space-y-2 rounded-lg border border-slate-200 p-3">
                  <legend className="px-1 text-sm font-semibold">Thành phần màn hình chỉnh sửa</legend>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.supports.includes('editor')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('editor', event.target.checked)} />
                    Trình soạn thảo (editor)
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked={editSettings.supports.includes('featured_image')}
                      disabled={!csrfToken || savingSettings}
                      onChange={event => toggleSupport('featured_image', event.target.checked)} />
                    Ảnh đại diện (featured_image)
                  </label>
                  <p className="text-xs text-slate-600">Tắt thành phần không xóa dữ liệu hay ảnh đã lưu.</p>
                </fieldset>
                {settingsError && <p className="text-sm text-rose-700" role="alert">{settingsError}</p>}
                {settingsStale && <div role="alert" className="space-y-2 text-sm text-amber-800">
                  <p>Phiên bản CPT đã thay đổi. Bản nhập được giữ nhưng không thể lưu bằng CAS cũ.</p>
                  <button type="button" className="rounded border border-amber-400 px-3 py-2"
                    disabled={savingSettings || taxonomyBusy} onClick={() => void rebaseSettingsDraft()}>
                    Đối chiếu CPT mới, giữ thay đổi đang nhập
                  </button>
                </div>}
                <div className="flex flex-wrap items-center gap-2">
                  <button type="submit" disabled={!settingsChanged || settingsStale || !csrfToken || savingSettings || taxonomyBusy}
                    className="rounded bg-slate-800 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50">
                    {savingSettings ? 'Đang lưu...' : 'Lưu cấu hình'}
                  </button>
                  <button type="button" disabled={savingSettings || taxonomyBusy}
                    className="rounded border border-slate-300 px-3 py-2 text-sm text-slate-700"
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
                    Tải lại từ máy chủ (bỏ chỉnh sửa)
                  </button>
                </div>
              </form>
            ) : (
              <p className="mt-4 text-sm text-slate-600">
                {isSection ? 'Section là loại nội dung riêng tư và không cho phép thay đổi cấu hình công khai.'
                  : 'Cấu hình thuộc quyền sở hữu của nhà cung cấp; chỉ có thể xem tại đây.'}
              </p>
            )}
          </div>

          <section className="rounded-xl border border-slate-200 bg-white p-4" aria-label="Taxonomy áp dụng cho loại nội dung">
            <h3 className="text-base font-semibold text-slate-900">Taxonomy áp dụng</h3>
            <p className="mt-1 text-sm text-slate-600">
              Chọn taxonomy được phép gắn vào {activeType.descriptor.label}. Bỏ chọn không xóa liên kết đã lưu.
            </p>
            {isSection ? <p className="mt-3 text-sm text-slate-600">Section không hỗ trợ taxonomy.</p>
              : !taxonomyLoaded ? <p role="status" className="mt-3 text-sm text-slate-600">Đang tải taxonomy áp dụng…</p>
                : <form className="mt-3 space-y-3" onSubmit={handleUpdateTaxonomies}>
                    <div className="grid gap-2 sm:grid-cols-2">
                      {taxonomyLoaded.available.filter(def =>
                        (def.provider_status === 'active' && def.declaration_state === 'declared')
                        || taxonomyDraft.includes(def.key)).map(def => (
                        <label key={def.key} className="flex items-start gap-2 rounded border border-slate-200 p-3 text-sm text-slate-700">
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
                          <span><strong className="block">{def.label}</strong>
                            <code className="text-xs">{def.key}</code> · {def.hierarchical ? 'Phân cấp' : 'Không phân cấp'}
                            {(def.provider_status !== 'active' || def.declaration_state !== 'declared')
                              && <span className="block text-xs text-amber-700">Provider không hoạt động hoặc khai báo đã rút; lựa chọn đã lưu được giữ lại.</span>}
                          </span>
                        </label>
                      ))}
                      {taxonomyDraft.filter(key => !taxonomyLoaded.available.some(def => def.key === key)).map(key => (
                        <label key={key} className="flex items-start gap-2 rounded border border-amber-300 p-3 text-sm">
                          <input type="checkbox" checked disabled={!canEditTaxonomies || taxonomyBusy || savingSettings}
                            onChange={() => {
                              const next = taxonomyDraft.filter(value => value !== key);
                              setTaxonomyDraft(next);
                              rememberTaxonomyDraft(taxonomyLoaded, next);
                            }} />
                          <span>{key} — Taxonomy không còn trong danh sách hiện tại. Bỏ chọn để gỡ khóa đã lưu.</span>
                        </label>
                      ))}
                    </div>
                    {!canEditTaxonomies && <p className="text-xs text-slate-600">Chỉ quản trị viên có thể thay đổi taxonomy; Section không hỗ trợ.</p>}
                    {taxonomyChanged && <p className="text-xs text-slate-600" role="status">
                      {selectedScope
                        ? 'Bản chọn taxonomy chưa lưu được giữ trong bộ nhớ phiên này theo tài khoản, website và loại nội dung khi bạn chuyển màn hình hoặc đăng nhập lại. Tải lại trang sẽ xóa bản nháp.'
                        : 'Bản chọn chưa lưu sẽ bị mất khi rời màn hình hoặc hết phiên đăng nhập; chỉ lưu được bản nháp khi xác định đủ website và tài khoản.'}
                    </p>}
                    {taxonomyNotice && <p role="status" className="text-sm text-amber-800">{taxonomyNotice}</p>}
                    {taxonomyLoaded.stale && <div className="space-y-2 rounded border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950">
                      <p role="alert">Version/schema CPT hoặc cấu hình taxonomy đã đổi. Bản chọn vẫn được giữ; không lưu với CAS cũ.</p>
                      <button type="button" className="rounded border border-amber-500 px-3 py-2 font-medium"
                        disabled={taxonomyBusy || savingSettings || !csrfToken}
                        onClick={() => void rebaseTaxonomyDraft()}>
                        Đối chiếu với máy chủ, giữ lựa chọn đang sửa
                      </button>
                    </div>}
                    {taxonomyError && <p role="alert" className="text-sm text-rose-700">{taxonomyError}</p>}
                    <div className="flex flex-wrap gap-2">
                      <button type="submit" className="rounded bg-slate-800 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"
                        disabled={!canEditTaxonomies || !taxonomyChanged || taxonomyBusy || savingSettings || !csrfToken || taxonomyLoaded.stale}>
                        {taxonomyBusy ? 'Đang lưu…' : 'Lưu taxonomy áp dụng'}
                      </button>
                      <button type="button" className="rounded border border-slate-300 px-3 py-2 text-sm"
                        disabled={taxonomyBusy || savingSettings} onClick={discardTaxonomyDraft}>
                        Hủy lựa chọn chưa lưu
                      </button>
                    </div>
                  </form>}
            {!taxonomyLoaded && taxonomyError && <p role="alert" className="mt-3 text-sm text-rose-700">{taxonomyError}</p>}
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
