import { getEffectiveBcp47, tStatic, useI18n } from '../../i18n/index.tsx';
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { listContentTypes } from '../../api/content.ts';
import type { OwnedContentTypeWithSettings } from '../../api/content.ts';
import { ApiClientError } from '../../api/client.ts';
import {
  createTaxonomyDefinition, deleteTaxonomyDefinition, getTaxonomyDefinition,
  listTaxonomyDefinitions, updateTaxonomyDefinition, updateTaxonomyEligibility,
  type TaxonomyDefinition,
} from '../../api/taxonomy.ts';
import {
  AdminAlert,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
} from '../../components/AdminPrimitives.tsx';

import {
  type TaxonomyDefsTabId, type Props, type FormValues, type DefinitionsDraft,
  definitionDrafts, draftScope, keysEqual,   getConfiguredTaxonomies, selectedKeys, blank, fromDefinition,
  fieldClass, buttonClass, failure, termsHash,
} from './taxonomyDefinitionsHelpers.ts';

export function TaxonomyDefinitionsPage({ user, siteInstanceId, csrfToken, onApiError, onDirtyChange, onNavigateHash }: Props) {
  useI18n();
  const scope = draftScope(siteInstanceId, user);
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);
  const recoveredRef = useRef<DefinitionsDraft | null>(scope ? definitionDrafts.get(scope) ?? null : null);
  const recovered = recoveredRef.current;
  const [definitions, setDefinitions] = useState<TaxonomyDefinition[]>([]);
  const [types, setTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [activeKey, setActiveKey] = useState<string>(recovered?.activeKey ?? '');
  const [form, setForm] = useState<FormValues>(recovered?.form ?? blank);
  const [baseline, setBaseline] = useState<FormValues>(recovered?.baseline ?? blank);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [conflict, setConflict] = useState(recovered?.conflict ?? false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [typeKey, setTypeKey] = useState(recovered?.typeKey ?? '');
  const [eligible, setEligible] = useState<string[]>(recovered?.eligible ?? []);
  const [eligibilityBaseline, setEligibilityBaseline] = useState<string[]>(recovered?.eligibilityBaseline ?? []);
  const [eligibilityTypeVersion, setEligibilityTypeVersion] = useState<number | null>(recovered?.typeVersion ?? null);
  const [eligibilityLoading, setEligibilityLoading] = useState(false);
  // One publication fence for effect loads, explicit reload and rebase. A
  // slower earlier GET must not replace newer server baseline or user edits.
  const eligibilityRequestEpochRef = useRef(0);
  const [eligibilityConflict, setEligibilityConflict] = useState(recovered?.eligibilityConflict ?? false);
  const [configuredEffective, setConfiguredEffective] = useState<string[]>([]);
  const [activeTab, setActiveTab] = useState<TaxonomyDefsTabId>('definitions');
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const reportDirtyRef = useRef(onDirtyChange);
  reportDirtyRef.current = onDirtyChange;
  const active = definitions.find(definition => definition.key === activeKey) ?? null;
  const isNew = activeKey === '__new__';
  const missingDefinition = activeKey !== '' && !isNew && !active && !loading;
  const coreCategoryOrTag = active?.owner.kind === 'core' && (active.key === 'category' || active.key === 'tag');
  const canEdit = user?.role === 'admin' && (isNew || active?.owner.kind === 'user' || coreCategoryOrTag);
  const canDelete = user?.role === 'admin' && active?.owner.kind === 'user';
  const configDirty = (isNew && JSON.stringify(form) !== JSON.stringify(blank()))
    || (activeKey !== '' && !isNew && JSON.stringify(form) !== JSON.stringify(baseline));
  const eligibleDirty = !keysEqual(eligible, eligibilityBaseline);
  const dirty = configDirty || eligibleDirty;
  const selectedType = types.find(item => item.descriptor.key === typeKey);
  // A site-admin can change the taxonomy *override* for a theme/plugin CPT;
  // this does not grant ownership of its provider-managed schema/definition.
  const typeEditable = user?.role === 'admin' && typeKey !== 'section' && !!selectedType;
  const eligibilityVersionMismatch = eligibilityTypeVersion !== null && !!selectedType
    && selectedType.version !== eligibilityTypeVersion;
  const ordered = useMemo(() => [...definitions].sort((a, b) => a.label.localeCompare(b.label, getEffectiveBcp47()) || a.key.localeCompare(b.key)), [definitions]);

  useEffect(() => { reportDirtyRef.current?.(dirty); }, [dirty]);
  useEffect(() => () => reportDirtyRef.current?.(false), []);

  useEffect(() => {
    // Never overwrite a recovered snapshot with loading placeholders.
    if (!scope || loading || eligibilityLoading) return;
    if (dirty) definitionDrafts.set(scope, {
      activeKey, form: { ...form }, baseline: { ...baseline },
      definitionVersion: conflict
        ? recoveredRef.current?.definitionVersion ?? active?.version ?? null
        : active?.version ?? recoveredRef.current?.definitionVersion ?? null,
      conflict,
      typeKey, eligible: [...eligible], eligibilityBaseline: [...eligibilityBaseline],
      typeVersion: eligibilityTypeVersion,
      eligibilityConflict,
    });
    else definitionDrafts.delete(scope);
  }, [scope, dirty, loading, eligibilityLoading, activeKey, form, baseline, active?.version, conflict,
    typeKey, eligible, eligibilityBaseline, eligibilityTypeVersion, eligibilityConflict]);

  useEffect(() => () => {
    // Explicit route navigation has its own dirty confirmation in App. A 401
    // leaves this hash untouched, so it retains the account-scoped draft.
    if (scope && window.location.hash.split('?')[0] !== '#/taxonomy/definitions') definitionDrafts.delete(scope);
  }, [scope]);

  const reload = async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const [nextDefinitions, nextTypes] = await Promise.all([listTaxonomyDefinitions(signal), listContentTypes()]);
      if (signal?.aborted) return;
      setDefinitions(nextDefinitions);
      setTypes(nextTypes);
      setError('');
      const remembered = recoveredRef.current;
      if (remembered) {
        if (remembered.activeKey && remembered.activeKey !== '__new__') {
          const latest = nextDefinitions.find(item => item.key === remembered.activeKey);
          if (!latest || latest.version !== remembered.definitionVersion) setConflict(true);
        }
        // listContentTypes is for labels/selector only. It cannot provide a
        // version for the taxonomy snapshot's If-Match token.
        const latestType = nextTypes.find(item => item.descriptor.key === remembered.typeKey);
        if (!keysEqual(remembered.eligible, remembered.eligibilityBaseline) &&
            (!latestType || latestType.version !== remembered.typeVersion)) setEligibilityConflict(true);
      }
      if (!typeKey) setTypeKey(nextTypes.find(item => item.descriptor.key !== 'section')?.descriptor.key ?? '');
    } catch (cause) {
      if (signal?.aborted) return;
      setError(failure(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_the_taxonomy_list")));
      onErrorRef.current(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_the_taxonomy_list"));
    } finally { if (!signal?.aborted) setLoading(false); }
  };

  useEffect(() => {
    const controller = new AbortController();
    void reload(controller.signal);
    return () => controller.abort();
  }, [user?.id, user?.role]);

  useEffect(() => {
    const requestEpoch = ++eligibilityRequestEpochRef.current;
    const controller = new AbortController();
    setEligibilityLoading(true);
    const remembered = recoveredRef.current?.typeKey === typeKey ? recoveredRef.current : null;
    if (!remembered) {
      setEligibilityConflict(false);
      setEligible([]);
      setEligibilityBaseline([]);
      setEligibilityTypeVersion(null);
    }
    if (!typeKey || !user) { setEligibilityLoading(false); return () => controller.abort(); }
    void getConfiguredTaxonomies(typeKey, controller.signal).then(value => {
      if (controller.signal.aborted || !mountedRef.current ||
          requestEpoch !== eligibilityRequestEpochRef.current) return;
      setConfiguredEffective(value.effective);
      if (remembered) {
        // Preserve the user's input, including a stale schema CAS, until they
        // explicitly rebase or discard it. Never silently re-apply a new token.
        setEligible(remembered.eligible);
        setEligibilityBaseline(remembered.eligibilityBaseline);
        const rememberedDirty = !keysEqual(remembered.eligible, remembered.eligibilityBaseline);
        setEligibilityTypeVersion(rememberedDirty ? remembered.typeVersion : value.type_version);
        if (rememberedDirty && (remembered.typeVersion !== value.type_version ||
            !keysEqual(remembered.eligibilityBaseline, selectedKeys(value)))) setEligibilityConflict(true);
      } else {
        setEligible(selectedKeys(value)); setEligibilityBaseline(selectedKeys(value));
        setEligibilityTypeVersion(value.type_version);
      }
    }).catch(cause => {
      if (controller.signal.aborted || !mountedRef.current ||
          requestEpoch !== eligibilityRequestEpochRef.current) return;
      setEligibilityConflict(true);
      setError(failure(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_content_type_taxonomies")));
      onErrorRef.current(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_content_type_taxonomies"));
    }).finally(() => {
      if (!controller.signal.aborted && mountedRef.current &&
          requestEpoch === eligibilityRequestEpochRef.current) setEligibilityLoading(false);
    });
    return () => controller.abort();
  }, [typeKey, user?.id, user?.role]);

  const choose = (next: string) => {
    if (next === activeKey) return;
    if (configDirty && !window.confirm(tStatic("feature.taxonomy.taxonomydefinitionspage.discard_changes_to_the_current_definition_and_switch_to"))) return;
    recoveredRef.current = null;
    const definition = definitions.find(item => item.key === next);
    setActiveKey(next);
    setForm(definition ? fromDefinition(definition) : blank());
    setBaseline(definition ? fromDefinition(definition) : blank());
    setError(''); setNotice(''); setConflict(false);
  };

  const rebaseDefinition = async () => {
    if (!activeKey || isNew || busy) return;
    setBusy(true); setError('');
    try {
      const latest = await getTaxonomyDefinition(activeKey);
      if (!mountedRef.current) return;
      const server = fromDefinition(latest);
      const changed = (Object.keys(form) as Array<keyof FormValues>)
        .filter(field => field !== 'key' && !(latest.owner.kind === 'core' && field === 'hierarchical') &&
          form[field] !== baseline[field]);
      const overlaps = changed.filter(field => server[field] !== baseline[field]);
      if (overlaps.length && !window.confirm(
        tStatic("feature.taxonomy.taxonomydefinitionspage.the_server_also_changed_value_keep_current_values_and", { value1: overlaps.join(', ') }),
      )) return;
      const next = { ...server };
      for (const field of changed) {
        if (field === 'label') next.label = form.label;
        if (field === 'hierarchical') next.hierarchical = form.hierarchical;
        if (field === 'public') next.public = form.public;
      }
      setDefinitions(previous => [...previous.filter(item => item.key !== activeKey), latest]);
      setBaseline(server); setForm(next); setConflict(false);
      setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.reviewed_vvalue_edited_fields_are_retained_review_before_saving", { value1: latest.version }));
      recoveredRef.current = null;
    } catch (cause) {
      if (!mountedRef.current) return;
      setError(failure(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_latest_taxonomy_version")));
      onErrorRef.current(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_latest_taxonomy_version"));
    } finally { setBusy(false); }
  };

  const rebaseEligibility = async () => {
    if (!typeKey || busy || eligibilityLoading) return;
    const requestEpoch = ++eligibilityRequestEpochRef.current;
    setBusy(true); setError('');
    try {
      // The configured endpoint supplies the version AND associated keys from
      // one SQLite snapshot. Never pair keys with a concurrently read list
      // version: that can silently overwrite someone else's new membership.
      const configured = await getConfiguredTaxonomies(typeKey);
      if (!mountedRef.current || requestEpoch !== eligibilityRequestEpochRef.current) return;
      const previous = new Set(eligibilityBaseline);
      const added = eligible.filter(key => !previous.has(key));
      const removed = new Set(eligibilityBaseline.filter(key => !eligible.includes(key)));
      const configuredKeys = selectedKeys(configured);
      const next = [...new Set([...configuredKeys.filter(key => !removed.has(key)), ...added])];
      const nextTypes = await listContentTypes();
      if (!mountedRef.current || requestEpoch !== eligibilityRequestEpochRef.current) return;
      const latestType = nextTypes.find(item => item.descriptor.key === typeKey);
      if (!latestType) {
        setEligibilityConflict(true);
        setError(tStatic("feature.taxonomy.taxonomydefinitionspage.content_type_no_longer_exists_selections_are_retained")); return;
      }
      if (latestType.version !== configured.type_version) {
        setEligibilityConflict(true);
        setError(tStatic("feature.taxonomy.taxonomydefinitionspage.the_cpt_list_version_differs_from_the_taxonomy_snapshot"));
        return;
      }
      setTypes(nextTypes);
      setConfiguredEffective(configured.effective);
      setEligibilityBaseline(configuredKeys);
      setEligible(next);
      setEligibilityTypeVersion(configured.type_version);
      setEligibilityConflict(false);
      setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.reviewed_the_latest_taxonomy_configuration_your_changed_selections_are"));
      recoveredRef.current = null;
    } catch (cause) {
      if (!mountedRef.current || requestEpoch !== eligibilityRequestEpochRef.current) return;
      setError(failure(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_review_taxonomy_configuration")));
      onErrorRef.current(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_review_taxonomy_configuration"));
    } finally { setBusy(false); }
  };

  const saveDefinition = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!csrfToken || !canEdit || busy || loading || conflict) return;
    const label = form.label.trim();
    if (!label || [...label].length > 100 || /[\u0000-\u001f\u007f]/.test(label)) {
      setError(tStatic("feature.taxonomy.taxonomydefinitionspage.taxonomy_name_must_be_1_to_100_characters_without")); return;
    }
    if (isNew && (form.key.length > 64 || !/^[a-z][a-z0-9_]*$/.test(form.key))) {
      setError(tStatic("feature.taxonomy.taxonomydefinitionspage.taxonomy_key_must_be_a_valid_lowercase_identifier")); return;
    }
    const changes = {
      ...(label !== baseline.label ? { label } : {}),
      ...(active?.owner.kind !== 'core' && form.hierarchical !== baseline.hierarchical
        ? { hierarchical: form.hierarchical } : {}),
      ...(form.public !== baseline.public ? { public: form.public } : {}),
    };
    if (!isNew && Object.keys(changes).length === 0) {
      setForm(baseline);
      setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.no_changes_to_save"));
      return;
    }
    setBusy(true); setError(''); setNotice('');
    try {
      const updated = isNew
        ? await createTaxonomyDefinition({ key: form.key, label, hierarchical: form.hierarchical, public: form.public }, csrfToken)
        : await updateTaxonomyDefinition(active!.key, active!.version, changes, csrfToken);
      if (!mountedRef.current) return;
      setActiveKey(updated.key); setBaseline(fromDefinition(updated)); setForm(fromDefinition(updated));
      setDefinitions(previous => [...previous.filter(item => item.key !== updated.key), updated]);
      setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.saved_taxonomy_value_vvalue", { value1: updated.key, value2: updated.version }));
      setConflict(false);
      recoveredRef.current = null;
      if (scope) {
        if (eligibleDirty) definitionDrafts.set(scope, {
          activeKey: updated.key, form: fromDefinition(updated), baseline: fromDefinition(updated),
          definitionVersion: updated.version, conflict: false,
          typeKey, eligible: [...eligible], eligibilityBaseline: [...eligibilityBaseline],
          typeVersion: eligibilityTypeVersion, eligibilityConflict,
        });
        else definitionDrafts.delete(scope);
      }
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 412) setConflict(true);
      setError(failure(cause, tStatic("feature.content.contenttypeschemasettings.could_not_save_taxonomies")));
      onErrorRef.current(cause, tStatic("feature.content.contenttypeschemasettings.could_not_save_taxonomies"));
    } finally { setBusy(false); }
  };

  const remove = async () => {
    if (!active || !canDelete || !csrfToken || busy) return;
    if (!window.confirm(tStatic("feature.taxonomy.taxonomydefinitionspage.delete_taxonomy_value_value_existing_terms_associations_and_active", { value1: active.label, value2: active.key }))) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await deleteTaxonomyDefinition(active.key, active.version, csrfToken);
      if (!mountedRef.current) return;
      setDefinitions(previous => previous.filter(item => item.key !== active.key));
      setActiveKey(''); setForm(blank()); setBaseline(blank());
      setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.deleted_taxonomy_value", { value1: active.key }));
      recoveredRef.current = null;
      if (scope) {
        if (eligibleDirty) definitionDrafts.set(scope, {
          activeKey: '', form: blank(), baseline: blank(), definitionVersion: null, conflict: false,
          typeKey, eligible: [...eligible], eligibilityBaseline: [...eligibilityBaseline],
          typeVersion: eligibilityTypeVersion, eligibilityConflict,
        });
        else definitionDrafts.delete(scope);
      }
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 412) setConflict(true);
      setError(failure(cause, tStatic("api.taxonomy.delete_the_taxonomy")));
      onErrorRef.current(cause, tStatic("api.taxonomy.delete_the_taxonomy"));
    } finally { setBusy(false); }
  };

  const saveEligibility = async () => {
    if (!selectedType || !csrfToken || !typeEditable || busy || eligibilityLoading || eligibilityConflict ||
      eligibilityTypeVersion === null || eligibilityVersionMismatch || !eligibleDirty) return;
    setBusy(true); setError(''); setNotice('');
    try {
      // Read again just before PUT; the version and membership must both
      // match the draft's atomic baseline. The following If-Match still fences
      // a concurrent writer between this GET and the PUT.
      const latest = await getConfiguredTaxonomies(typeKey);
      if (!mountedRef.current) return;
      if (latest.type_version !== eligibilityTypeVersion ||
          !keysEqual(selectedKeys(latest), eligibilityBaseline)) {
        setEligibilityConflict(true);
        setError(tStatic("feature.taxonomy.taxonomydefinitionspage.schema_eligibility_changed_since_editing_began_retain_selections_and"));
        return;
      }
      const submitted = [...eligible];
      const updated = await updateTaxonomyEligibility(typeKey, latest.type_version, submitted, csrfToken) as OwnedContentTypeWithSettings;
      if (!mountedRef.current) return;
      // Mutation response carries the new authoritative CPT version. Avoid a
      // second, independent list/configured read that might straddle a write.
      setTypes(previous => previous.map(item => item.descriptor.key === typeKey ? updated : item));
      setEligible(submitted); setEligibilityBaseline(submitted);
      setEligibilityTypeVersion(updated.version);
      setEligibilityConflict(false);
      // A successful explicit override resolves to its active/declared keys.
      // The pre-PUT `latest.effective` describes the old selection instead.
      setConfiguredEffective(submitted.filter(key => definitions.some(item =>
        item.key === key && item.provider_status === 'active' && item.declaration_state === 'declared')));
      setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.updated_applicable_taxonomies_for_value", { value1: typeKey }));
      recoveredRef.current = null;
      if (scope) {
        if (configDirty) definitionDrafts.set(scope, {
          activeKey, form: { ...form }, baseline: { ...baseline },
          definitionVersion: active?.version ?? null, conflict,
          typeKey, eligible: [...submitted], eligibilityBaseline: [...submitted],
          typeVersion: updated.version,
          eligibilityConflict: false,
        });
        else definitionDrafts.delete(scope);
      }
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 412) setEligibilityConflict(true);
      setError(failure(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_update_applicable_taxonomies")));
      onErrorRef.current(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_update_applicable_taxonomies"));
    } finally { setBusy(false); }
  };

  if (!user) return null;
  return <section className="w-full min-w-0 space-y-5" data-testid="taxonomy-definitions-page">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">{tStatic("feature.taxonomy.taxonomydefinitionspage.content_configuration")}</p>
        <h2 className="mt-1 text-2xl font-bold">{tStatic("feature.common.ui_taxonomy")}</h2>
        <p className="text-sm text-slate-600">{tStatic("feature.taxonomy.taxonomydefinitionspage.create_grouping_types_manage_terms_and_select_applicable_content")}</p>
      </div>
      <button className={buttonClass} type="button" disabled={busy || loading} onClick={() => {
        if (dirty && !window.confirm(tStatic("feature.taxonomy.taxonomydefinitionspage.discard_unsaved_changes_and_reload"))) return;
        const requestEpoch = ++eligibilityRequestEpochRef.current;
        if (scope) definitionDrafts.delete(scope);
        recoveredRef.current = null;
        setConflict(false); setEligibilityConflict(false); setActiveKey(''); setForm(blank()); setBaseline(blank());
        void reload();
        if (typeKey) { setEligibilityLoading(true); void getConfiguredTaxonomies(typeKey).then(configured => {
          if (!mountedRef.current || requestEpoch !== eligibilityRequestEpochRef.current) return;
          setEligible(selectedKeys(configured)); setEligibilityBaseline(selectedKeys(configured));
          setEligibilityTypeVersion(configured.type_version);
          setConfiguredEffective(configured.effective);
        }).catch(cause => {
          if (!mountedRef.current || requestEpoch !== eligibilityRequestEpochRef.current) return;
          setEligibilityConflict(true);
          setError(failure(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_reload_taxonomies")));
          onErrorRef.current(cause, tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_reload_taxonomies"));
        }).finally(() => {
          if (mountedRef.current && requestEpoch === eligibilityRequestEpochRef.current)
            setEligibilityLoading(false);
        }); }
      }}>{tStatic("reading.reload")}</button>
    </header>
    {error && <AdminAlert tone="error">{error}</AdminAlert>}
    {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
    {missingDefinition && <AdminAlert tone="warning" role="alert">
      {tStatic("feature.common.ui_taxonomy")} {activeKey} {tStatic("feature.taxonomy.taxonomydefinitionspage.was_deleted_or_is_no_longer_accessible_your_input")}
    </AdminAlert>}
    {loading && <p role="status" className="text-xs text-zinc-500">{tStatic("feature.taxonomy.taxonomydefinitionspage.loading_taxonomies")}</p>}
    <AdminTabs
      tabs={[
        {
          id: 'definitions',
          label: tStatic('taxonomy_defs.tab_definitions'),
          badge: `${definitions.length}/64`,
          dirty: configDirty || conflict,
        },
        {
          id: 'eligibility',
          label: tStatic('taxonomy_defs.tab_eligibility'),
          dirty: eligibleDirty || eligibilityConflict || eligibilityVersionMismatch,
        },
      ]}
      activeId={activeTab}
      onChange={setActiveTab}
      ariaLabel={tStatic('taxonomy_defs.tabs_label')}
      testId="taxonomy-definitions-tabs"
      idPrefix="taxonomy-defs"
    />
    <AdminTabPanel
      active={activeTab === 'definitions'}
      id="taxonomy-defs-panel-definitions"
      labelledBy="taxonomy-defs-tab-definitions"
    >
      <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
        <div className="min-w-0 space-y-2">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 className="text-sm font-semibold text-zinc-950">{tStatic("feature.taxonomy.taxonomydefinitionspage.definitions")}{definitions.length}/64)</h3>
            {user.role === 'admin' && <AdminButton variant="primary" size="sm" disabled={busy || definitions.length >= 64}
              onClick={() => choose('__new__')}>{tStatic("feature.taxonomy.taxonomydefinitionspage.add_taxonomy")}</AdminButton>}
          </div>
          {ordered.map(item => <div key={item.key} className={`rounded-xl border p-3.5 transition-colors ${activeKey === item.key ? 'border-od-accent bg-blue-50/50' : 'border-zinc-200 bg-white'}`}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="min-w-0">
                <p className="break-words text-sm font-semibold text-zinc-950">{item.label} <span className="font-normal text-zinc-500">({item.key})</span></p>
                <p className="mt-0.5 text-xs text-zinc-500">{item.hierarchical ? tStatic("feature.content.contenttypeschemasettings.hierarchical") : tStatic("feature.taxonomy.taxonomydefinitionspage.flat")} · {item.public ? tStatic("feature.editor.panelregionrenderer.public") : tStatic("feature.editor.panelregionrenderer.private")} · {item.owner.kind} · v{item.version}</p>
                {item.provider_status !== 'active' || item.declaration_state !== 'declared' ?
                  <p className="mt-0.5 text-xs text-amber-800">{tStatic("feature.common.ui_provider")} {item.provider_status}{tStatic("feature.taxonomy.taxonomydefinitionspage.declaration")}{' '}{item.declaration_state}{tStatic("feature.taxonomy.taxonomydefinitionspage.data_is_retained")}</p> : null}
              </div>
              <div className="flex gap-1.5">
                <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => choose(item.key)}>{tStatic("feature.taxonomy.taxonomydefinitionspage.configuration")}</AdminButton>
                <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => {
                  if (dirty && !window.confirm(tStatic("feature.taxonomy.taxonomydefinitionspage.there_are_unsaved_changes_leave_this_page"))) return;
                  onNavigateHash(termsHash(item.key));
                }}>{tStatic("feature.common.ui_term")}</AdminButton>
              </div>
            </div>
          </div>)}
        </div>
        <div className="min-w-0 space-y-4">
          {(isNew || active || missingDefinition) && <form onSubmit={event => void saveDefinition(event)} className="space-y-4 rounded-xl border border-zinc-200 bg-white p-4">
            <h3 className="text-sm font-semibold text-zinc-950">{isNew ? tStatic("feature.taxonomy.taxonomydefinitionspage.create_definition") : tStatic("feature.taxonomy.taxonomydefinitionspage.edit_value", { value1: active?.key ?? activeKey })}</h3>
            <label className="block text-xs font-medium text-zinc-700">{tStatic("feature.taxonomy.taxonomydefinitionspage.key")}
              <input className={fieldClass} value={form.key} maxLength={64} required disabled={!isNew || busy || !canEdit}
                onChange={event => setForm(previous => ({ ...previous, key: event.target.value }))}/>
              <span className="mt-1 block text-xs font-normal text-zinc-500">{tStatic("feature.taxonomy.taxonomydefinitionspage.use_lowercase_letters_digits_and_underscores_the_key_cannot")}</span>
            </label>
            <label className="block text-xs font-medium text-zinc-700">{tStatic("feature.content.contenttypeform.display_name")}
              <input className={fieldClass} value={form.label} maxLength={100} required disabled={busy || !canEdit}
                onChange={event => setForm(previous => ({ ...previous, label: event.target.value }))}/>
            </label>
            <label className="flex items-center gap-2 text-sm text-zinc-800"><input type="checkbox" checked={form.hierarchical}
              disabled={busy || !canEdit || active?.owner.kind === 'core'} onChange={event => setForm(previous => ({ ...previous, hierarchical: event.target.checked }))}/>
              {tStatic("feature.taxonomy.taxonomydefinitionspage.allow_parent_child_hierarchy_up_to_8_levels")}</label>
            <label className="flex items-center gap-2 text-sm text-zinc-800"><input type="checkbox" checked={form.public}
              disabled={busy || !canEdit} onChange={event => setForm(previous => ({ ...previous, public: event.target.checked }))}/>
              {tStatic("feature.taxonomy.taxonomydefinitionspage.allow_public_visibility")}</label>
            {active && active.owner.kind !== 'user' && <p className="text-xs text-zinc-500">{active.owner.kind === 'core'
              ? tStatic("feature.taxonomy.taxonomydefinitionspage.core_taxonomies_retain_their_key_and_hierarchy_mode")
              : tStatic("feature.taxonomy.taxonomydefinitionspage.the_definition_is_provider_managed_only_the_provider_can")}</p>}
            {conflict && <AdminAlert tone="warning" role="alert" className="space-y-2">
              <p>{tStatic("feature.taxonomy.taxonomydefinitionspage.version_is_stale_your_input_is_retained_review_the")}</p>
              <AdminButton variant="secondary" size="sm" disabled={busy || !active}
                onClick={() => void rebaseDefinition()}>{tStatic("feature.taxonomy.taxonomydefinitionspage.review_latest_version_and_retain_edited_fields")}</AdminButton>
            </AdminAlert>}
            <div className="flex flex-wrap gap-2">
              <AdminButton type="submit" variant="primary" disabled={busy || !csrfToken || !canEdit || conflict || (!isNew && !configDirty)}>
                {busy ? tStatic("reading.saving") : tStatic("feature.taxonomy.taxonomydefinitionspage.save_definition")}</AdminButton>
              {canDelete && <AdminButton variant="danger" disabled={busy || !csrfToken || conflict} onClick={() => void remove()}>{tStatic("multilingual.delete")}</AdminButton>}
              {missingDefinition && <AdminButton variant="secondary" onClick={() => choose('')}>
                {tStatic("feature.taxonomy.taxonomydefinitionspage.discard_input_whose_save_target_no_longer_exists")}
              </AdminButton>}
            </div>
          </form>}
        </div>
      </div>
    </AdminTabPanel>
    <AdminTabPanel
      active={activeTab === 'eligibility'}
      id="taxonomy-defs-panel-eligibility"
      labelledBy="taxonomy-defs-tab-eligibility"
    >
      <div className="max-w-2xl space-y-3 rounded-xl border border-zinc-200 bg-white p-5">
        <h3 className="text-sm font-semibold text-zinc-950">{tStatic("feature.taxonomy.taxonomydefinitionspage.associate_taxonomies_with_content_types")}</h3>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{tStatic("feature.taxonomy.taxonomydefinitionspage.content_type")}</span>
          <select className="h-9 w-full max-w-md rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 disabled:opacity-50" value={typeKey} disabled={busy || eligibilityLoading || loading}
            onChange={event => {
              if (eligibleDirty && !window.confirm(tStatic("feature.taxonomy.taxonomydefinitionspage.discard_unsaved_taxonomy_changes"))) return;
              recoveredRef.current = null;
              setTypeKey(event.target.value); setError('');
            }}>
            {types.filter(item => item.descriptor.key !== 'section').map(item =>
              <option key={item.descriptor.key} value={item.descriptor.key}>{item.descriptor.label} ({item.descriptor.key})</option>)}
          </select>
        </label>
        {eligibilityLoading && <p role="status" className="text-xs text-zinc-500">{tStatic("feature.taxonomy.taxonomydefinitionspage.loading_eligibility_configuration")}</p>}
        <div className="grid gap-2.5 sm:grid-cols-2">
          {!eligibilityLoading && definitions.map(item => <label key={item.key} className="flex items-start gap-2 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3 text-sm text-zinc-800 hover:border-zinc-300">
            <input type="checkbox" className="mt-1" checked={eligible.includes(item.key)}
              disabled={!typeEditable || busy || eligibilityLoading || eligibilityConflict}
              onChange={event => setEligible(previous => event.target.checked
                ? [...previous, item.key] : previous.filter(key => key !== item.key))}/>
            <span>{item.label} ({item.key})
              {(item.provider_status !== 'active' || item.declaration_state !== 'declared') &&
                <span className="block text-xs text-amber-700">{tStatic("feature.taxonomy.taxonomydefinitionspage.provider_inactive_or_declaration_withdrawn_configured_associations_are_retained")}</span>}
            </span>
          </label>)}
        </div>
        {(eligibilityConflict || eligibilityVersionMismatch) && <AdminAlert tone="warning" role="alert" className="space-y-2">
          <p>{tStatic("feature.taxonomy.taxonomydefinitionspage.cpt_version_differs_from_the_taxonomy_snapshot_or_the")}</p>
          <AdminButton variant="secondary" size="sm" disabled={busy || eligibilityLoading}
            onClick={() => void rebaseEligibility()}>{tStatic("feature.taxonomy.taxonomydefinitionspage.review_latest_schema_and_retain_changed_selections")}</AdminButton>
        </AdminAlert>}
        {eligible.some(key => !definitions.some(item => item.key === key)) &&
          <p role="status" className="text-xs text-amber-700">{tStatic("feature.taxonomy.taxonomydefinitionspage.configured_taxonomies_are_missing_from_the_list_they_are")}</p>}
        {definitions.some(item => eligible.includes(item.key) && !configuredEffective.includes(item.key)) &&
          <p role="status" className="text-xs text-amber-700">{tStatic("feature.taxonomy.taxonomydefinitionspage.inactive_taxonomies_do_not_appear_in_the_picker_but")}</p>}
        <AdminButton variant="primary" onClick={() => void saveEligibility()}
          disabled={!typeEditable || !csrfToken || !eligibleDirty || busy || eligibilityLoading ||
            eligibilityConflict || eligibilityTypeVersion === null || eligibilityVersionMismatch}>{tStatic("feature.taxonomy.taxonomydefinitionspage.save_applicable_types")}</AdminButton>
        {!typeEditable && typeKey && <p className="text-xs text-zinc-500">{tStatic("feature.taxonomy.taxonomydefinitionspage.only_admins_can_edit_applicable_taxonomies_for_valid_cpts")}</p>}
      </div>
    </AdminTabPanel>
  </section>;
}
