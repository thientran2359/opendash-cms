import { getEffectiveBcp47, __, useI18n } from '../../i18n/index.tsx';
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
    const currentPath = window.location.hash.split('?')[0];
    if (scope && currentPath !== '#/content-structure/taxonomies' && currentPath !== '#/taxonomy/definitions') {
      definitionDrafts.delete(scope);
    }
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
      setError(failure(cause, __("Could not load the taxonomy list.")));
      onErrorRef.current(cause, __("Could not load the taxonomy list."));
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
      setError(failure(cause, __("Could not load content type taxonomies.")));
      onErrorRef.current(cause, __("Could not load content type taxonomies."));
    }).finally(() => {
      if (!controller.signal.aborted && mountedRef.current &&
          requestEpoch === eligibilityRequestEpochRef.current) setEligibilityLoading(false);
    });
    return () => controller.abort();
  }, [typeKey, user?.id, user?.role]);

  const choose = (next: string) => {
    if (next === activeKey) return;
    if (configDirty && !window.confirm(__("Discard changes to the current definition and switch to another item?"))) return;
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
        __("The server also changed {{value1}}. Keep current values and try saving against the latest version?", { value1: overlaps.join(', ') }),
      )) return;
      const next = { ...server };
      for (const field of changed) {
        if (field === 'label') next.label = form.label;
        if (field === 'hierarchical') next.hierarchical = form.hierarchical;
        if (field === 'public') next.public = form.public;
      }
      setDefinitions(previous => [...previous.filter(item => item.key !== activeKey), latest]);
      setBaseline(server); setForm(next); setConflict(false);
      setNotice(__("Reviewed v{{value1}}. Edited fields are retained; review before saving.", { value1: latest.version }));
      recoveredRef.current = null;
    } catch (cause) {
      if (!mountedRef.current) return;
      setError(failure(cause, __("Could not load latest taxonomy version.")));
      onErrorRef.current(cause, __("Could not load latest taxonomy version."));
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
        setError(__("Content type no longer exists; selections are retained.")); return;
      }
      if (latestType.version !== configured.type_version) {
        setEligibilityConflict(true);
        setError(__("The CPT list version differs from the taxonomy snapshot. Selections are retained; review again to avoid losing associations."));
        return;
      }
      setTypes(nextTypes);
      setConfiguredEffective(configured.effective);
      setEligibilityBaseline(configuredKeys);
      setEligible(next);
      setEligibilityTypeVersion(configured.type_version);
      setEligibilityConflict(false);
      setNotice(__("Reviewed the latest taxonomy configuration; your changed selections are reapplied. Review before saving."));
      recoveredRef.current = null;
    } catch (cause) {
      if (!mountedRef.current || requestEpoch !== eligibilityRequestEpochRef.current) return;
      setError(failure(cause, __("Could not review taxonomy configuration.")));
      onErrorRef.current(cause, __("Could not review taxonomy configuration."));
    } finally { setBusy(false); }
  };

  const saveDefinition = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!csrfToken || !canEdit || busy || loading || conflict) return;
    const label = form.label.trim();
    if (!label || [...label].length > 100 || /[\u0000-\u001f\u007f]/.test(label)) {
      setError(__("Taxonomy name must be 1 to 100 characters without control characters.")); return;
    }
    if (isNew && (form.key.length > 64 || !/^[a-z][a-z0-9_]*$/.test(form.key))) {
      setError(__("Taxonomy key must be a valid lowercase identifier.")); return;
    }
    const changes = {
      ...(label !== baseline.label ? { label } : {}),
      ...(active?.owner.kind !== 'core' && form.hierarchical !== baseline.hierarchical
        ? { hierarchical: form.hierarchical } : {}),
      ...(form.public !== baseline.public ? { public: form.public } : {}),
    };
    if (!isNew && Object.keys(changes).length === 0) {
      setForm(baseline);
      setNotice(__("No changes to save."));
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
      setNotice(__("Saved taxonomy {{value1}} (v{{value2}}).", { value1: updated.key, value2: updated.version }));
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
      setError(failure(cause, __("Could not save taxonomies.")));
      onErrorRef.current(cause, __("Could not save taxonomies."));
    } finally { setBusy(false); }
  };

  const remove = async () => {
    if (!active || !canDelete || !csrfToken || busy) return;
    if (!window.confirm(__("Delete taxonomy {{value1}} ({{value2}})? Existing terms, associations and active providers will block deletion.", { value1: active.label, value2: active.key }))) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await deleteTaxonomyDefinition(active.key, active.version, csrfToken);
      if (!mountedRef.current) return;
      setDefinitions(previous => previous.filter(item => item.key !== active.key));
      setActiveKey(''); setForm(blank()); setBaseline(blank());
      setNotice(__("Deleted taxonomy {{value1}}.", { value1: active.key }));
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
      setError(failure(cause, __("Could not delete the taxonomy.")));
      onErrorRef.current(cause, __("Could not delete the taxonomy."));
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
        setError(__("Schema/eligibility changed since editing began. Retain selections and review before saving."));
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
      setNotice(__("Updated applicable taxonomies for {{value1}}.", { value1: typeKey }));
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
      setError(failure(cause, __("Could not update applicable taxonomies.")));
      onErrorRef.current(cause, __("Could not update applicable taxonomies."));
    } finally { setBusy(false); }
  };

  if (!user) return null;
  return <section className="w-full min-w-0 space-y-5" data-testid="taxonomy-definitions-page">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">{__("Content / Configuration")}</p>
        <h2 className="mt-1 text-2xl font-bold">{__("Taxonomy")}</h2>
        <p className="text-sm text-slate-600">{__("Create grouping types, manage terms and select applicable content types.")}</p>
      </div>
      <button className={buttonClass} type="button" disabled={busy || loading} onClick={() => {
        if (dirty && !window.confirm(__("Discard unsaved changes and reload?"))) return;
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
          setError(failure(cause, __("Could not reload taxonomies.")));
          onErrorRef.current(cause, __("Could not reload taxonomies."));
        }).finally(() => {
          if (mountedRef.current && requestEpoch === eligibilityRequestEpochRef.current)
            setEligibilityLoading(false);
        }); }
      }}>{__("Reload")}</button>
    </header>
    {error && <AdminAlert tone="error">{error}</AdminAlert>}
    {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
    {missingDefinition && <AdminAlert tone="warning" role="alert">
      {__("Taxonomy")} {activeKey} {__("was deleted or is no longer accessible. Your input remains visible below for copying; it cannot be saved to the missing definition.")}
    </AdminAlert>}
    {loading && <p role="status" className="text-xs text-zinc-500">{__("Loading taxonomies…")}</p>}
    <AdminTabs
      tabs={[
        {
          id: 'definitions',
          label: __("Definitions"),
          badge: `${definitions.length}/64`,
          dirty: configDirty || conflict,
        },
        {
          id: 'eligibility',
          label: __("Content type eligibility"),
          dirty: eligibleDirty || eligibilityConflict || eligibilityVersionMismatch,
        },
      ]}
      activeId={activeTab}
      onChange={setActiveTab}
      ariaLabel={__("Taxonomy configuration sections")}
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
            <h3 className="text-sm font-semibold text-zinc-950">{__("Definitions (")}{definitions.length}/64)</h3>
            {user.role === 'admin' && <AdminButton variant="primary" size="sm" disabled={busy || definitions.length >= 64}
              onClick={() => choose('__new__')}>{__("+ Add taxonomy")}</AdminButton>}
          </div>
          {ordered.map(item => <div key={item.key} className={`rounded-xl border p-3.5 transition-colors ${activeKey === item.key ? 'border-od-accent bg-blue-50/50' : 'border-zinc-200 bg-white'}`}>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="min-w-0">
                <p className="break-words text-sm font-semibold text-zinc-950">{item.label} <span className="font-normal text-zinc-500">({item.key})</span></p>
                <p className="mt-0.5 text-xs text-zinc-500">{item.hierarchical ? __("Hierarchical") : __("Flat")} · {item.public ? __("Public") : __("Private")} · {item.owner.kind} · v{item.version}</p>
                {item.provider_status !== 'active' || item.declaration_state !== 'declared' ?
                  <p className="mt-0.5 text-xs text-amber-800">{__("Provider")} {item.provider_status}{__(", declaration")}{' '}{item.declaration_state}{__("; data is retained.")}</p> : null}
              </div>
              <div className="flex gap-1.5">
                <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => choose(item.key)}>{__("Configuration")}</AdminButton>
                <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => {
                  if (dirty && !window.confirm(__("There are unsaved changes. Leave this page?"))) return;
                  onNavigateHash(termsHash(item.key));
                }}>{__("Term")}</AdminButton>
              </div>
            </div>
          </div>)}
        </div>
        <div className="min-w-0 space-y-4">
          {(isNew || active || missingDefinition) && <form onSubmit={event => void saveDefinition(event)} className="space-y-4 rounded-xl border border-zinc-200 bg-white p-4">
            <h3 className="text-sm font-semibold text-zinc-950">{isNew ? __("Create definition") : __("Edit {{value1}}", { value1: active?.key ?? activeKey })}</h3>
            <label className="block text-xs font-medium text-zinc-700">{__("Key *")}
              <input className={fieldClass} value={form.key} maxLength={64} required disabled={!isNew || busy || !canEdit}
                onChange={event => setForm(previous => ({ ...previous, key: event.target.value }))}/>
              <span className="mt-1 block text-xs font-normal text-zinc-500">{__("Use lowercase letters, digits and underscores; the key cannot change after creation.")}</span>
            </label>
            <label className="block text-xs font-medium text-zinc-700">{__("Display name *")}
              <input className={fieldClass} value={form.label} maxLength={100} required disabled={busy || !canEdit}
                onChange={event => setForm(previous => ({ ...previous, label: event.target.value }))}/>
            </label>
            <label className="flex items-center gap-2 text-sm text-zinc-800"><input type="checkbox" checked={form.hierarchical}
              disabled={busy || !canEdit || active?.owner.kind === 'core'} onChange={event => setForm(previous => ({ ...previous, hierarchical: event.target.checked }))}/>
              {__("Allow parent/child hierarchy (up to 8 levels)")}</label>
            <label className="flex items-center gap-2 text-sm text-zinc-800"><input type="checkbox" checked={form.public}
              disabled={busy || !canEdit} onChange={event => setForm(previous => ({ ...previous, public: event.target.checked }))}/>
              {__("Allow public visibility")}</label>
            {active && active.owner.kind !== 'user' && <p className="text-xs text-zinc-500">{active.owner.kind === 'core'
              ? __("Core taxonomies retain their key and hierarchy mode.")
              : __("The definition is provider-managed. Only the provider can edit or delete it.")}</p>}
            {conflict && <AdminAlert tone="warning" role="alert" className="space-y-2">
              <p>{__("Version is stale. Your input is retained; review the latest version before saving.")}</p>
              <AdminButton variant="secondary" size="sm" disabled={busy || !active}
                onClick={() => void rebaseDefinition()}>{__("Review latest version and retain edited fields")}</AdminButton>
            </AdminAlert>}
            <div className="flex flex-wrap gap-2">
              <AdminButton type="submit" variant="primary" disabled={busy || !csrfToken || !canEdit || conflict || (!isNew && !configDirty)}>
                {busy ? __("Saving…") : __("Save definition")}</AdminButton>
              {canDelete && <AdminButton variant="danger" disabled={busy || !csrfToken || conflict} onClick={() => void remove()}>{__("Delete")}</AdminButton>}
              {missingDefinition && <AdminButton variant="secondary" onClick={() => choose('')}>
                {__("Discard input whose save target no longer exists")}
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
        <h3 className="text-sm font-semibold text-zinc-950">{__("Associate taxonomies with content types")}</h3>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{__("Content type")}</span>
          <select className="h-9 w-full max-w-md rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 disabled:opacity-50" value={typeKey} disabled={busy || eligibilityLoading || loading}
            onChange={event => {
              if (eligibleDirty && !window.confirm(__("Discard unsaved taxonomy changes?"))) return;
              recoveredRef.current = null;
              setTypeKey(event.target.value); setError('');
            }}>
            {types.filter(item => item.descriptor.key !== 'section').map(item =>
              <option key={item.descriptor.key} value={item.descriptor.key}>{item.descriptor.label} ({item.descriptor.key})</option>)}
          </select>
        </label>
        {eligibilityLoading && <p role="status" className="text-xs text-zinc-500">{__("Loading eligibility configuration…")}</p>}
        <div className="grid gap-2.5 sm:grid-cols-2">
          {!eligibilityLoading && definitions.map(item => <label key={item.key} className="flex items-start gap-2 rounded-lg border border-zinc-200 bg-zinc-50/40 p-3 text-sm text-zinc-800 hover:border-zinc-300">
            <input type="checkbox" className="mt-1" checked={eligible.includes(item.key)}
              disabled={!typeEditable || busy || eligibilityLoading || eligibilityConflict}
              onChange={event => setEligible(previous => event.target.checked
                ? [...previous, item.key] : previous.filter(key => key !== item.key))}/>
            <span>{item.label} ({item.key})
              {(item.provider_status !== 'active' || item.declaration_state !== 'declared') &&
                <span className="block text-xs text-amber-700">{__("Provider inactive or declaration withdrawn; configured associations are retained.")}</span>}
            </span>
          </label>)}
        </div>
        {(eligibilityConflict || eligibilityVersionMismatch) && <AdminAlert tone="warning" role="alert" className="space-y-2">
          <p>{__("CPT version differs from the taxonomy snapshot or the schema changed. Selections are retained; review before saving.")}</p>
          <AdminButton variant="secondary" size="sm" disabled={busy || eligibilityLoading}
            onClick={() => void rebaseEligibility()}>{__("Review latest schema and retain changed selections")}</AdminButton>
        </AdminAlert>}
        {eligible.some(key => !definitions.some(item => item.key === key)) &&
          <p role="status" className="text-xs text-amber-700">{__("Configured taxonomies are missing from the list. They are retained to avoid unintentionally losing associations.")}</p>}
        {definitions.some(item => eligible.includes(item.key) && !configuredEffective.includes(item.key)) &&
          <p role="status" className="text-xs text-amber-700">{__("Inactive taxonomies do not appear in the picker, but configuration is retained for when the provider becomes active again.")}</p>}
        <AdminButton variant="primary" onClick={() => void saveEligibility()}
          disabled={!typeEditable || !csrfToken || !eligibleDirty || busy || eligibilityLoading ||
            eligibilityConflict || eligibilityTypeVersion === null || eligibilityVersionMismatch}>{__("Save applicable types")}</AdminButton>
        {!typeEditable && typeKey && <p className="text-xs text-zinc-500">{__("Only admins can edit applicable taxonomies for valid CPTs (except Section).")}</p>}
      </div>
    </AdminTabPanel>
  </section>;
}
