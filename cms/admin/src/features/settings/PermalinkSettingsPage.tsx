import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { useI18n, __, _x } from '../../i18n/index.tsx';
import { ApiClientError, type PermalinkConflictItem } from '../../api/client.ts';
import {
  getPermalinkSettings, savePermalinkSettings,
  type PermalinkSettings, type PermalinkSettingsDraft,
} from '../../api/settings.ts';
import { effectiveContentTypeSettings, listContentTypes, queryContentEntryTable, type OwnedContentTypeWithSettings } from '../../api/content.ts';
import { listTaxonomyDefinitions, type TaxonomyDefinition } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import {
  AdminAlert,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
} from '../../components/AdminPrimitives.tsx';

type PermalinkTabId = 'posts' | 'cpts' | 'taxonomies';

interface Props {
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  onApiError?: (error: unknown) => void;
  onDirtyChange?: (dirty: boolean) => void;
}

const PRESETS = [
  { value: '/%postname%', label: () => __("Post name (/%postname%)") },
  { value: '/blog/%postname%', label: () => __("Blog prefix (/blog/%postname%)") },
  { value: '/%category%/%postname%', label: () => __("Category and post name (/%category%/%postname%)") },
  { value: '/archives/%post_id%', label: () => __("Numeric archive (/archives/%post_id%)") },
] as const;
const CUSTOM = 'custom';
const TOKENS = ['%category%', '%postname%', '%post_id%'] as const;
type DraftRecord = { baselineVersion: number; draft: PermalinkSettingsDraft };
const pendingDrafts = new Map<string, DraftRecord>();

function draftFor(saved: PermalinkSettings): PermalinkSettingsDraft {
  return {
    post_structure: saved.post_structure,
    content_type_bases: { ...saved.content_type_bases },
    taxonomy_bases: { ...saved.taxonomy_bases },
  };
}

function sameBases(left: Record<string, string | null>, right: Record<string, string | null>): boolean {
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of keys) if (left[key] !== right[key]) return false;
  return true;
}

function sameDraft(left: PermalinkSettingsDraft, right: PermalinkSettingsDraft): boolean {
  return left.post_structure === right.post_structure
    && sameBases(left.content_type_bases, right.content_type_bases)
    && sameBases(left.taxonomy_bases, right.taxonomy_bases);
}

function baseChoice(bases: Record<string, string | null>, name: string): 'inherit' | 'system' | 'custom' {
  if (!Object.prototype.hasOwnProperty.call(bases, name)) return 'inherit';
  return bases[name] === null ? 'system' : 'custom';
}

/** Insert category before a terminal post token; switch mutually exclusive
 * terminal tokens instead of producing a structure the server would reject. */
function withPostToken(structure: string, token: typeof TOKENS[number]): string {
  const parts = structure.split('/').filter(Boolean);
  if (parts.includes(token)) return structure;
  const end = parts[parts.length - 1];
  const isTerminal = end === '%postname%' || end === '%post_id%';
  if (token === '%category%') parts.splice(isTerminal ? parts.length - 1 : parts.length, 0, token);
  else if (isTerminal) parts[parts.length - 1] = token;
  else parts.push(token);
  return `/${parts.join('/')}`;
}

export function discardPermalinkDraft(siteId: string, userId: number): void {
  pendingDrafts.delete(`${siteId}:${userId}`);
}

/** Explicit logout discards all RAM-only form drafts. A session-expiry 401 does not. */
export function clearPermalinkDrafts(): void {
  pendingDrafts.clear();
}

function safeServerPath(path: string | null): string | null {
  return path && /^\/(?!\/)[^\u0000-\u001f\\]*$/.test(path) ? path : null;
}

function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : _x("Unable to connect to the server.", "permalink.network_error");
}

export function PermalinkSettingsPage({ user, siteInstanceId, csrfToken, onApiError, onDirtyChange }: Props) {
  const { __, _x } = useI18n();
  const actorId = user?.id ?? null;
  const key = actorId === null ? null : `${siteInstanceId}:${actorId}`;
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const onDirtyRef = useRef(onDirtyChange);
  onDirtyRef.current = onDirtyChange;
  const mounted = useRef(true);
  const [saved, setSaved] = useState<PermalinkSettings | null>(null);
  const [draft, setDraft] = useState<PermalinkSettingsDraft | null>(null);
  const [types, setTypes] = useState<OwnedContentTypeWithSettings[]>([]);
  const [taxonomies, setTaxonomies] = useState<TaxonomyDefinition[]>([]);
  const [examples, setExamples] = useState<Array<{ label: string; path: string }>>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflicts, setConflicts] = useState<PermalinkConflictItem[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const [customMode, setCustomMode] = useState(false);
  const [activeTab, setActiveTab] = useState<PermalinkTabId>('posts');
  const draftRef = useRef<PermalinkSettingsDraft | null>(null);
  draftRef.current = draft;
  const loadSerialRef = useRef(0);
  const loadInFlightRef = useRef(false);

  const dirty = Boolean(saved && draft && !sameDraft(draft, draftFor(saved)));
  useEffect(() => { onDirtyRef.current?.(dirty); }, [dirty]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; onDirtyRef.current?.(false); };
  }, []);

  const load = useCallback(async (signal?: AbortSignal) => {
    if (key === null) return;
    const serial = ++loadSerialRef.current;
    loadInFlightRef.current = true;
    setLoading(true);
    setError(null);
    setConflicts([]);
    try {
      const [config, typeCatalog, taxonomyCatalog] = await Promise.all([
        getPermalinkSettings(signal), listContentTypes(), listTaxonomyDefinitions(signal),
      ]);
      if (signal?.aborted || !mounted.current || serial !== loadSerialRef.current) return;
      setSaved(config);
      setTypes(typeCatalog);
      setTaxonomies(taxonomyCatalog);
      setExamples([]);
      const pending = pendingDrafts.get(key);
      if (pending) {
        setDraft(pending.draft);
        setCustomMode(!PRESETS.some(item => item.value === pending.draft.post_structure));
        setStale(pending.baselineVersion !== config.version);
      } else {
        setDraft(draftFor(config));
        setCustomMode(!PRESETS.some(item => item.value === config.post_structure));
        setStale(false);
      }

      // Every preview path is returned by the canonical server entry-table API,
      // using CURRENT saved settings. Unsaved form values never synthesize URLs.
      const publicTypes = typeCatalog.filter(type =>
        !['section', 'pattern'].includes(type.descriptor.key)
        && effectiveContentTypeSettings(type.descriptor.key, type.descriptor.settings).public
        && type.status === 'active',
      );
      const candidates = [
        ...publicTypes.filter(type => type.descriptor.key === 'post'),
        ...publicTypes.filter(type => !['post', 'page'].includes(type.descriptor.key)).slice(0, 4),
      ];
      // Optional examples must never hold the authoritative settings form
      // hostage if a provider's entry table is slow or unavailable.
      void Promise.allSettled(candidates.map(async type => {
        const table = await queryContentEntryTable(
          type.descriptor.key, { status: 'published', limit: 1 }, signal,
        );
        const row = table.items[0];
        const path = row ? safeServerPath(row.public_url) : null;
        return path ? { label: type.descriptor.label, path } : null;
      })).then(resolved => {
        if (signal?.aborted || !mounted.current || serial !== loadSerialRef.current) return;
        setExamples(resolved.flatMap(item =>
          item.status === 'fulfilled' && item.value ? [item.value] : [],
        ));
        const unauthorized = resolved.find(item =>
          item.status === 'rejected' && item.reason instanceof ApiClientError && item.reason.status === 401,
        );
        if (unauthorized?.status === 'rejected') onApiErrorRef.current?.(unauthorized.reason);
      });
    } catch (reason) {
      if (!signal?.aborted && mounted.current && serial === loadSerialRef.current) {
        setError(errorText(reason));
        onApiErrorRef.current?.(reason);
      }
    } finally {
      if (serial === loadSerialRef.current) {
        loadInFlightRef.current = false;
        if (mounted.current) setLoading(false);
      }
    }
  }, [key]);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  function updateDraft(next: PermalinkSettingsDraft) {
    if (!saved || !key || loadInFlightRef.current) return;
    setDraft(next);
    setError(null);
    setConflicts([]);
    setNotice(null);
    if (sameDraft(next, draftFor(saved))) pendingDrafts.delete(key);
    else {
      // Once a draft was based on vN, a stale GET/412 must not rebase it by
      // accident. Otherwise editing a field after 412 can authorize PUT vN+1
      // with the whole older draft and clobber a concurrent writer's settings.
      const originalVersion = pendingDrafts.get(key)?.baselineVersion ?? saved.version;
      pendingDrafts.set(key, { baselineVersion: originalVersion, draft: next });
    }
  }

  function updateBaseChoice(
    kind: 'content_type_bases' | 'taxonomy_bases',
    name: string,
    value: string | null | undefined,
  ) {
    if (!draft) return;
    const bases = { ...draft[kind] };
    if (value === undefined) delete bases[name];
    else bases[name] = value;
    updateDraft({ ...draft, [kind]: bases });
  }

  function resetDraft() {
    if (!saved || !key || stale) return;
    pendingDrafts.delete(key);
    setDraft(draftFor(saved));
    setCustomMode(!PRESETS.some(item => item.value === saved.post_structure));
    setStale(false);
    setError(null);
    setConflicts([]);
    setNotice(__("Unsaved changes discarded; using the current server settings."));
  }

  function discardStaleAndReload() {
    if (!key || loadInFlightRef.current || saving) return;
    // Discard synchronously. The form disappears before the async GET begins,
    // so a later result cannot erase a newer edit typed during the request.
    pendingDrafts.delete(key);
    setDraft(null);
    setCustomMode(false);
    setStale(false);
    setConflicts([]);
    setNotice(__("Draft discarded; loading the latest server version."));
    void load();
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!saved || !draft || !key || !csrfToken || !dirty || stale || saving || loadInFlightRef.current) return;
    const payload = draft;
    const baselineVersion = saved.version;
    setSaving(true);
    setError(null);
    setConflicts([]);
    setNotice(null);
    try {
      const result = await savePermalinkSettings(payload, baselineVersion, csrfToken);
      if (!mounted.current) return;
      setSaved(result);
      // Preserve edits typed while the PUT was in flight; do not mark them saved.
      const latest = draftRef.current;
      if (latest && !sameDraft(latest, payload)) {
        pendingDrafts.set(key, { baselineVersion: result.version, draft: latest });
      } else {
        pendingDrafts.delete(key);
        setDraft(draftFor(result));
        setCustomMode(!PRESETS.some(item => item.value === result.post_structure));
      }
      setStale(false);
      setNotice(__("Permalinks saved. Public URLs have been updated together on the server."));
      // Refresh URL examples only after successful persistence.
      void load();
    } catch (reason) {
      if (!mounted.current) return;
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError(__("Settings were changed in another session (412). Your draft is retained; load the latest version and compare before saving again."));
      } else if (reason instanceof ApiClientError && reason.status === 409) {
        setError(__("URL conflict (409). Server response: {{message}}. No changes were saved.", { message: errorText(reason) }));
        setConflicts(reason.conflicts ?? []);
      } else {
        setError(errorText(reason));
      }
      onApiErrorRef.current?.(reason);
    } finally {
      if (mounted.current) setSaving(false);
    }
  }

  const typeOptions = types.filter(type =>
    !['post', 'page', 'section', 'pattern'].includes(type.descriptor.key)
    && type.status === 'active'
    && effectiveContentTypeSettings(type.descriptor.key, type.descriptor.settings).public,
  );
  const taxonomyOptions = taxonomies.filter(type =>
    type.public && type.provider_status === 'active' && type.declaration_state === 'declared',
  );
  // Retain saved keys belonging to deactivated providers; never erase them merely
  // because the current catalog no longer exposes the declaration.
  const typeKeys = Array.from(new Set([
    ...typeOptions.map(type => type.descriptor.key),
    ...Object.keys(saved?.content_type_bases ?? {}).filter(keyName =>
      !['post', 'page', 'section', 'pattern'].includes(keyName)),
    ...Object.keys(draft?.content_type_bases ?? {}).filter(keyName =>
      !['post', 'page', 'section', 'pattern'].includes(keyName)),
  ])).sort();
  const taxonomyKeys = Array.from(new Set([
    ...taxonomyOptions.map(type => type.key),
    ...Object.keys(saved?.taxonomy_bases ?? {}),
    ...Object.keys(draft?.taxonomy_bases ?? {}),
  ])).sort();
  function revertInactiveBase(kind: 'content_type_bases' | 'taxonomy_bases', name: string) {
    if (!draft || !saved) return;
    const bases = { ...draft[kind] };
    if (Object.prototype.hasOwnProperty.call(saved[kind], name)) bases[name] = saved[kind][name];
    else delete bases[name];
    updateDraft({ ...draft, [kind]: bases });
  }
  const preset = customMode ? CUSTOM : (draft?.post_structure ?? CUSTOM);
  const postsDirty = Boolean(saved && draft && draft.post_structure !== saved.post_structure);
  const cptsDirty = Boolean(saved && draft && !sameBases(draft.content_type_bases, saved.content_type_bases));
  const taxonomiesDirty = Boolean(saved && draft && !sameBases(draft.taxonomy_bases, saved.taxonomy_bases));

  return (
    <section className="min-w-0 space-y-5" data-testid="permalink-settings" aria-labelledby="permalink-heading">
      <header>
        <h2 id="permalink-heading" className="text-base font-semibold text-zinc-950">{__("Permalinks")}</h2>
        <p className="mt-1 text-sm text-zinc-600">
          {__("Choose how public URLs are generated for posts, custom post types and taxonomies. Current URLs change only after a successful save. Pages always use the root path (/<slug>); patterns are always private.")}
        </p>
      </header>
      {loading && <p role="status" className="text-sm text-zinc-500">{__("Loading permalink settings…")}</p>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {conflicts.length > 0 && (
        <div role="alert" className="rounded-lg border border-rose-200 bg-rose-50 px-3 py-3 text-sm text-rose-900" data-testid="permalink-conflicts">
          <p className="font-semibold">{__("Conflicting URLs (up to 20 items from the server):")}</p>
          <ul className="mt-2 list-inside list-disc space-y-1 break-all font-mono text-xs">
            {conflicts.map((item, index) => (
              <li key={`${item.path}:${item.new_target}:${index}`}>
                <span className="font-semibold">{item.path}</span> — {__("{{newTarget}} conflicts with {{existingTarget}}", { newTarget: item.new_target, existingTarget: item.existing_target })}
              </li>
            ))}
          </ul>
        </div>
      )}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {stale && (
        <AdminAlert tone="warning" className="space-y-2">
          <p>{__("Server settings are newer than the version you started editing. Your form is retained and Save is temporarily disabled to prevent overwriting.")}</p>
          <div className="flex flex-wrap gap-2">
            <AdminButton variant="secondary" size="sm" disabled={loading || saving} onClick={() => void load()}>
              {__("Check latest version")}
            </AdminButton>
            <AdminButton variant="secondary" size="sm" disabled={loading || saving} onClick={discardStaleAndReload}>
              {__("Discard draft and use latest version")}
            </AdminButton>
          </div>
        </AdminAlert>
      )}
      {!loading && (!saved || !draft) && (
        <AdminButton variant="primary" onClick={() => void load()}>{__("Try reloading")}</AdminButton>
      )}
      {saved && draft && (
        <form onSubmit={event => void save(event)} className="space-y-6">
          <AdminTabs
            tabs={[
              { id: 'posts', label: __("Post URLs"), dirty: postsDirty },
              { id: 'cpts', label: __("Custom post types"), badge: typeKeys.length, dirty: cptsDirty },
              { id: 'taxonomies', label: _x("Taxonomies", "permalink.tab_taxonomies"), badge: taxonomyKeys.length, dirty: taxonomiesDirty },
            ]}
            activeId={activeTab}
            onChange={setActiveTab}
            ariaLabel={__("Permalink configuration sections")}
            testId="permalink-tabs"
            idPrefix="permalink"
          />

          <AdminTabPanel
            active={activeTab === 'posts'}
            id="permalink-panel-posts"
            labelledBy="permalink-tab-posts"
            className="space-y-2"
          >
            <label htmlFor="permalink-post-preset" className="block text-sm font-medium text-zinc-800">{__("Post URL structure")}</label>
            <select id="permalink-post-preset" className="h-9 w-full max-w-2xl rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              value={preset} disabled={loading} onChange={event => {
                if (event.target.value === CUSTOM) {
                  // Selecting Custom keeps the current preset as an editable starting point.
                  setCustomMode(true);
                  setError(null);
                  setNotice(null);
                  setConflicts([]);
                } else {
                  setCustomMode(false);
                  updateDraft({ ...draft, post_structure: event.target.value });
                }
              }}>
              {PRESETS.map(item => <option key={item.value} value={item.value}>{item.label()}</option>)}
              <option value={CUSTOM}>{__("Custom")}</option>
            </select>
            {customMode && (
              <div className="space-y-2">
                <label htmlFor="permalink-post-custom" className="block text-xs font-medium text-zinc-700">{__("Custom format")}</label>
                <input id="permalink-post-custom" className="h-9 w-full max-w-2xl rounded-md border border-zinc-300 bg-white px-3 font-mono text-sm text-zinc-900"
                  value={draft.post_structure} maxLength={512} disabled={loading}
                  onChange={event => updateDraft({ ...draft, post_structure: event.target.value })} />
              </div>
            )}
            <div className="flex flex-wrap gap-1.5" aria-label={__("Add a token to the permalink structure")}>
              {TOKENS.map(token => (
                <AdminButton key={token} variant="secondary" size="sm" disabled={loading}
                  onClick={() => {
                    setCustomMode(true);
                    updateDraft({ ...draft, post_structure: withPostToken(draft.post_structure, token) });
                  }}>
                  {token}
                </AdminButton>
              ))}
            </div>
            <p className="text-xs text-zinc-500">{__("Supports kebab-case literals and the tokens %category%, %postname%, %post_id% (no day, month or year).")}</p>
          </AdminTabPanel>

          <AdminTabPanel
            active={activeTab === 'cpts'}
            id="permalink-panel-cpts"
            labelledBy="permalink-tab-cpts"
          >
            <h3 className="text-sm font-semibold text-zinc-900">{__("Custom post type bases")}</h3>
            <p className="mb-3 mt-0.5 text-xs text-zinc-500">{__("Choose the inherited provider default (if available) or the system default (/{key}/{slug}). A custom base has 1–3 kebab-case segments with no leading or trailing /.")}</p>
            {typeKeys.length === 0 && <p className="text-sm text-zinc-500">{__("No configurable public custom post types.")}</p>}
            <div className="grid gap-3.5 sm:grid-cols-2">
              {typeKeys.map(keyName => {
                const available = typeOptions.find(type => type.descriptor.key === keyName);
                const hasPendingChange = !available &&
                  draft.content_type_bases[keyName] !== saved.content_type_bases[keyName];
                const choice = baseChoice(draft.content_type_bases, keyName);
                return <div key={keyName} className="rounded-xl border border-zinc-200 bg-zinc-50/60 p-3.5 text-sm text-zinc-800">
                  <label htmlFor={`permalink-cpt-${keyName}`} className="font-medium text-zinc-900">
                    {available?.descriptor.label ?? keyName} <span className="font-normal text-zinc-500">({keyName})</span>
                    {!available && <span className="ml-2 text-xs text-amber-700">{__("Inactive, retaining saved settings")}</span>}
                  </label>
                  <input id={`permalink-cpt-${keyName}`} className="mt-1.5 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 font-mono text-sm text-zinc-900"
                    aria-label={__("CPT base {{key}}", { key: keyName })} disabled={!available || loading}
                    value={draft.content_type_bases[keyName] ?? ''}
                    onChange={event => updateBaseChoice('content_type_bases', keyName, event.target.value || null)}
                    placeholder={__("e.g. projects")} />
                  <p className="mt-1 text-xs text-zinc-500" data-testid={`cpt-base-mode-${keyName}`}>
                    {choice === 'inherit' ? __("Inheriting the provider default; otherwise using the system default (/{key}/{slug}).")
                      : choice === 'system' ? __("System default (/{key}/{slug}).")
                        : __("Using a custom site base.")}
                  </p>
                  {available && <div className="mt-2.5 flex flex-wrap gap-1.5">
                    <AdminButton variant="secondary" size="sm" disabled={loading || choice === 'system'}
                      onClick={() => updateBaseChoice('content_type_bases', keyName, null)}
                      aria-label={__("System default (/{key}/{slug}) for CPT {{name}}", { name: keyName })}>{__("System default (/{key}/{slug})")}</AdminButton>
                    <AdminButton variant="secondary" size="sm" disabled={loading || choice === 'inherit'}
                      onClick={() => updateBaseChoice('content_type_bases', keyName, undefined)}
                      aria-label={__("Inherit default for CPT {{key}}", { key: keyName })}>{__("Inherit default")}</AdminButton>
                  </div>}
                  {hasPendingChange && <AdminButton variant="ghost" size="sm" disabled={loading} onClick={() => revertInactiveBase('content_type_bases', keyName)}
                    className="mt-1.5 text-od-accent">
                    {__("Discard unsaved changes for this CPT")}
                  </AdminButton>}
                </div>;
              })}
            </div>
          </AdminTabPanel>

          <AdminTabPanel
            active={activeTab === 'taxonomies'}
            id="permalink-panel-taxonomies"
            labelledBy="permalink-tab-taxonomies"
          >
            <h3 className="text-sm font-semibold text-zinc-900">{__("Taxonomy bases")}</h3>
            <p className="mb-3 mt-0.5 text-xs text-zinc-500">{__("Choose the inherited provider default (if available) or the system default (/{kind}/{slug}). Hierarchical term paths cannot be configured.")}</p>
            {taxonomyKeys.length === 0 && <p className="text-sm text-zinc-500">{__("No configurable public taxonomies.")}</p>}
            <div className="grid gap-3.5 sm:grid-cols-2">
              {taxonomyKeys.map(keyName => {
                const available = taxonomyOptions.find(type => type.key === keyName);
                const hasPendingChange = !available &&
                  draft.taxonomy_bases[keyName] !== saved.taxonomy_bases[keyName];
                const choice = baseChoice(draft.taxonomy_bases, keyName);
                return <div key={keyName} className="rounded-xl border border-zinc-200 bg-zinc-50/60 p-3.5 text-sm text-zinc-800">
                  <label htmlFor={`permalink-taxonomy-${keyName}`} className="font-medium text-zinc-900">
                    {available?.label ?? keyName} <span className="font-normal text-zinc-500">({keyName})</span>
                    {!available && <span className="ml-2 text-xs text-amber-700">{__("Inactive, retaining saved settings")}</span>}
                  </label>
                  <input id={`permalink-taxonomy-${keyName}`} className="mt-1.5 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 font-mono text-sm text-zinc-900"
                    aria-label={__("Taxonomy base {{key}}", { key: keyName })} disabled={!available || loading}
                    value={draft.taxonomy_bases[keyName] ?? ''}
                    onChange={event => updateBaseChoice('taxonomy_bases', keyName, event.target.value || null)}
                    placeholder={__("e.g. genres")} />
                  <p className="mt-1 text-xs text-zinc-500" data-testid={`taxonomy-base-mode-${keyName}`}>
                    {choice === 'inherit' ? __("Inheriting the provider default; otherwise using the system default (/{kind}/{slug}).")
                      : choice === 'system' ? __("System default (/{kind}/{slug}).")
                        : __("Using a custom site base.")}
                  </p>
                  {available && <div className="mt-2.5 flex flex-wrap gap-1.5">
                    <AdminButton variant="secondary" size="sm" disabled={loading || choice === 'system'}
                      onClick={() => updateBaseChoice('taxonomy_bases', keyName, null)}
                      aria-label={__("System default (/{kind}/{slug}) for taxonomy {{name}}", { name: keyName })}>{__("System default (/{kind}/{slug})")}</AdminButton>
                    <AdminButton variant="secondary" size="sm" disabled={loading || choice === 'inherit'}
                      onClick={() => updateBaseChoice('taxonomy_bases', keyName, undefined)}
                      aria-label={__("Inherit default for taxonomy {{key}}", { key: keyName })}>{__("Inherit default")}</AdminButton>
                  </div>}
                  {hasPendingChange && <AdminButton variant="ghost" size="sm" disabled={loading} onClick={() => revertInactiveBase('taxonomy_bases', keyName)}
                    className="mt-1.5 text-od-accent">
                    {__("Discard unsaved changes for this taxonomy")}
                  </AdminButton>}
                </div>;
              })}
            </div>
          </AdminTabPanel>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-4" data-testid="permalink-server-preview">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">{__("Current server URLs")}</h3>
            <p className="mt-1 text-xs text-zinc-600">
              {__("These examples come from the public_url field in the API. While the form is unsaved, these remain the active URLs.")}
            </p>
            {examples.length === 0 && <p className="mt-2 text-sm text-zinc-500">{__("No published entries to show example URLs.")}</p>}
            {examples.map(item => <p key={item.label + item.path} className="mt-1.5 break-all text-sm">
              <span className="mr-2 font-medium text-zinc-800">{item.label}:</span>
              <a href={item.path} target="_blank" rel="noopener noreferrer" className="text-od-accent underline">{item.path}</a>
            </p>)}
          </div>
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <AdminButton type="submit" variant="primary" disabled={!csrfToken || !dirty || stale || saving || loading}>
              {saving ? __("Saving…") : __("Save permalinks")}
            </AdminButton>
            <AdminButton variant="secondary" disabled={!dirty || stale || saving} onClick={resetDraft}>
              {__("Discard changes")}
            </AdminButton>
            <span className="text-xs text-zinc-500">{dirty ? __("Version {{version}} · Unsaved", { version: saved.version }) : __("Version {{version}} · Synced", { version: saved.version })}</span>
          </div>
        </form>
      )}
    </section>
  );
}
