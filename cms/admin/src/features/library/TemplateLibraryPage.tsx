import { __, _x, useI18n } from '../../i18n/index.tsx';
import { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import { ApiClientError } from '../../api/client.ts';
import {
  getTemplateOverrides, resetTemplateOverride, type TypedTemplateOverride,
} from '../../api/builder.ts';
import { getLocalTemplateLibrary, type LocalTemplateLibrary } from '../../api/sections.ts';
import { listPatterns, type PatternRecord } from '../../api/patterns.ts';
import { AdminTabPanel, AdminTabs } from '../../components/AdminPrimitives.tsx';

type LibraryTabId = 'patterns' | 'theme' | 'overrides';

interface Props {
  user: AuthUser | null;
  csrfToken: string | null;
  themeRevision: number;
  onNavigateHash: (hash: string) => void;
  onApiError: (cause: unknown, fallback: string) => void;
  onApplied: () => void;
}

export function TemplateLibraryPage({ user, csrfToken, themeRevision, onNavigateHash, onApiError, onApplied }: Props) {
  useI18n();
  const [catalog, setCatalog] = useState<LocalTemplateLibrary | null>(null);
  const [patterns, setPatterns] = useState<PatternRecord[]>([]);
  const [overrides, setOverrides] = useState<TypedTemplateOverride[]>([]);
  const [loading, setLoading] = useState(false);
  const [applying, setApplying] = useState<string | null>(null);
  const [notice, setNotice] = useState('');
  const [activeTab, setActiveTab] = useState<LibraryTabId>('patterns');
  const requestSeq = useRef(0);
  const currentAccount = user?.id ?? null;
  const currentRole = user?.role ?? null;

  const refresh = () => {
    if (currentRole !== 'admin' || !csrfToken) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    Promise.all([getLocalTemplateLibrary(), getTemplateOverrides(), listPatterns()]).then(([data, rows, patternList]) => {
      if (seq === requestSeq.current) {
        setCatalog(data);
        setOverrides(rows.items);
        setPatterns(patternList.items);
      }
    }).catch(error => {
      if (seq === requestSeq.current) onApiError(error, __("Could not load the template library."));
    }).finally(() => { if (seq === requestSeq.current) setLoading(false); });
  };

  useEffect(() => {
    requestSeq.current += 1;
    setCatalog(null);
    setPatterns([]);
    setOverrides([]);
    setApplying(null);
    if (currentRole !== 'admin' || !csrfToken) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    Promise.all([getLocalTemplateLibrary(), getTemplateOverrides(), listPatterns()]).then(([data, rows, patternList]) => {
      if (seq === requestSeq.current) {
        setCatalog(data);
        setOverrides(rows.items);
        setPatterns(patternList.items);
      }
    }).catch(error => {
      if (seq === requestSeq.current) onApiError(error, __("Could not load the template library."));
    }).finally(() => { if (seq === requestSeq.current) setLoading(false); });
    return () => { requestSeq.current += 1; };
  }, [currentAccount, currentRole, csrfToken, themeRevision]);

  // Only reset the confirmed site-owned row. Never translate a theme role into
  // a global write: the typed row ID and If-Match make the operation specific
  // and protect another editor's concurrent modifications.
  const resetSiteOverride = async (item: TypedTemplateOverride) => {
    if (!user || user.role !== 'admin' || !csrfToken || applying) return;
    const seq = requestSeq.current;
    const key = item.canonical_target_key;
    if (!window.confirm(__("Reset site template “{{value1}}” to the next theme/core source? The current AST will be deleted and cannot be recovered.", { value1: key }))) return;
    setApplying(key);
    try {
      const result = await resetTemplateOverride(item.id, item.version, csrfToken);
      if (seq !== requestSeq.current) return;
      setNotice(__("Reset {{value1}}. Effective source: {{value2}}.", { value1: result.canonical_target_key, value2: result.next_effective_source.label }));
      setOverrides(rows => rows.filter(row => row.id !== item.id));
      onApplied();
    } catch (error) {
      if (seq !== requestSeq.current) return;
      if (error instanceof ApiClientError && error.status === 412) {
        setNotice(__("Conflict 412: the server version changed; reset canceled. Reopen and compare."));
      } else onApiError(error, __("Could not reset the site template override."));
    } finally { if (seq === requestSeq.current) setApplying(null); }
  };

  if (!user || user.role !== 'admin' || !csrfToken) return null;
  return <main className="od-admin-card min-w-0 space-y-5" data-testid="local-template-library">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{__("Local template library")}</h2>
        <p className="mt-1 text-sm text-slate-600">{__("Shared site patterns and template roles of the active theme.")}</p></div>
      <button type="button" className="btn-secondary" disabled={loading || !!applying} onClick={refresh}>{__("Reload library")}</button>
    </header>
    {loading && <p role="status">{__("Loading library…")}</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {catalog && <>
      <AdminTabs
        tabs={[
          { id: 'patterns', label: __("Synced patterns"), badge: patterns.length },
          { id: 'theme', label: __("Theme templates"), badge: catalog.theme_templates.length },
          { id: 'overrides', label: __("Site overrides"), badge: overrides.length },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={__("Template library sections")}
        testId="template-library-tabs"
        idPrefix="template-library"
      />
      <AdminTabPanel
        active={activeTab === 'patterns'}
        id="template-library-panel-patterns"
        labelledBy="template-library-tab-patterns"
      >
        <section className="space-y-3" aria-label={__("Local patterns")}>
          <h3 className="font-semibold">{__("Synced patterns (")}{patterns.length})</h3>
          <p className="text-sm text-slate-600">{__("Add a Reusable Pattern block from Visual Builder and choose a pattern. Editing the original updates every reference.")}</p>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {patterns.map(item => <article key={item.id} className="min-w-0 rounded-lg border border-slate-200 p-3">
              <h4 className="break-words font-medium">{item.title}</h4>
              <p className="mt-1 break-words text-xs text-slate-500">#{item.id} · {item.slug} · {item.referencing_count} {__("content references")}</p>
              <button type="button" className="btn-secondary mt-3" onClick={() => onNavigateHash(`#/patterns/${item.id}/edit`)}>{__("Open pattern")}</button>
            </article>)}
          </div>
          {!patterns.length && <p className="text-sm">{__("No synced patterns.")}</p>}
          <button type="button" onClick={() => onNavigateHash('#/patterns/new')}>{_x("Create pattern", "feature.library")}</button>
        </section>
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'theme'}
        id="template-library-panel-theme"
        labelledBy="template-library-tab-theme"
      >
        <section className="space-y-3" aria-label={__("Local theme templates")}>
          <h3 className="font-semibold">{_x("Active theme templates", "feature.library")}</h3>
          <p className="text-sm text-slate-600">{__("Theme:")} {catalog.active_theme_id ?? _x("None", "feature.library")} · {catalog.theme_state}</p>
          <div className="space-y-2.5">
            {catalog.theme_templates.map(item => <article key={item.role} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
              <div className="min-w-0"><strong>{item.role}</strong>
                <p className="break-all text-xs text-slate-500">{__("Revision")}: {item.revision_id} · {item.revision_digest.slice(0, 12)}</p>
                {item.reason && <p className="text-xs text-amber-800">{item.reason}</p>}
              </div>
              {item.can_apply && <span className="text-xs text-slate-600">{_x("Active theme templates", "feature.library")}</span>}
            </article>)}
          </div>
          {!catalog.theme_templates.length && <p className="text-sm text-slate-600">{__("The theme does not provide valid local role templates. The Pattern CPT is stored separately and preserved.")}</p>}
        </section>
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'overrides'}
        id="template-library-panel-overrides"
        labelledBy="template-library-tab-overrides"
      >
        <section className="space-y-3" aria-label={__("Site template overrides")}>
          <h3 className="font-semibold">{__("Site template overrides")} ({overrides.length})</h3>
          <p className="text-sm text-slate-600">{__("Site-owned AST takes precedence over a theme template for the same target. Resetting that target reveals the next theme/core source without affecting other templates.")}</p>
          <div className="space-y-2.5">
            {overrides.map(item => <article key={item.id} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
              <div className="min-w-0">
                <strong className="break-all">{item.canonical_target_key}</strong>
                <p className="text-xs text-slate-600">{__("Version v")}{item.version} · {item.effective_source?.label ?? __("Site override")}</p>
                <p className="break-all text-xs text-slate-500">{__("Original theme:")}{' '}{item.base_theme_id ?? 'Core'} · {item.base_revision_digest?.slice(0, 12) ?? __("No revision")}</p>
              </div>
              <button type="button" className="btn-secondary" disabled={!!applying || loading} onClick={() => void resetSiteOverride(item)}>
                {applying === item.canonical_target_key ? __("Resetting…") : __("Reset override")}
              </button>
            </article>)}
          </div>
          {!overrides.length && <p className="text-sm text-slate-600">{__("No site-owned override; the resolver uses active theme/core in candidate order.")}</p>}
        </section>
      </AdminTabPanel>
    </>}
  </main>;
}
