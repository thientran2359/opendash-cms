import { tStatic, useI18n } from '../../i18n/index.tsx';
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
      if (seq === requestSeq.current) onApiError(error, tStatic("feature.library.templatelibrarypage.could_not_load_the_template_library"));
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
      if (seq === requestSeq.current) onApiError(error, tStatic("feature.library.templatelibrarypage.could_not_load_the_template_library"));
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
    if (!window.confirm(tStatic("feature.library.templatelibrarypage.reset_site_template_value_to_the_next_theme_core", { value1: key }))) return;
    setApplying(key);
    try {
      const result = await resetTemplateOverride(item.id, item.version, csrfToken);
      if (seq !== requestSeq.current) return;
      setNotice(tStatic("feature.library.templatelibrarypage.reset_value_effective_source_value", { value1: result.canonical_target_key, value2: result.next_effective_source.label }));
      setOverrides(rows => rows.filter(row => row.id !== item.id));
      onApplied();
    } catch (error) {
      if (seq !== requestSeq.current) return;
      if (error instanceof ApiClientError && error.status === 412) {
        setNotice(tStatic("feature.library.templatelibrarypage.conflict_412_the_server_version_changed_reset_canceled_reopen"));
      } else onApiError(error, tStatic("api.builder.reset_the_site_template_override"));
    } finally { if (seq === requestSeq.current) setApplying(null); }
  };

  if (!user || user.role !== 'admin' || !csrfToken) return null;
  return <main className="od-admin-card min-w-0 space-y-5" data-testid="local-template-library">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{tStatic("feature.library.templatelibrarypage.local_template_library")}</h2>
        <p className="mt-1 text-sm text-slate-600">{tStatic("feature.library.templatelibrarypage.shared_site_patterns_and_template_roles_of_the_active")}</p></div>
      <button type="button" className="btn-secondary" disabled={loading || !!applying} onClick={refresh}>{tStatic("feature.library.templatelibrarypage.reload_library")}</button>
    </header>
    {loading && <p role="status">{tStatic("feature.library.templatelibrarypage.loading_library")}</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {catalog && <>
      <AdminTabs
        tabs={[
          { id: 'patterns', label: tStatic('library.tab_patterns'), badge: patterns.length },
          { id: 'theme', label: tStatic('library.tab_theme_templates'), badge: catalog.theme_templates.length },
          { id: 'overrides', label: tStatic('library.tab_overrides'), badge: overrides.length },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={tStatic('library.tabs_label')}
        testId="template-library-tabs"
        idPrefix="template-library"
      />
      <AdminTabPanel
        active={activeTab === 'patterns'}
        id="template-library-panel-patterns"
        labelledBy="template-library-tab-patterns"
      >
        <section className="space-y-3" aria-label={tStatic("feature.library.templatelibrarypage.local_patterns")}>
          <h3 className="font-semibold">{tStatic("feature.library.templatelibrarypage.synced_patterns")}{patterns.length})</h3>
          <p className="text-sm text-slate-600">{tStatic("feature.library.templatelibrarypage.add_a_reusable_pattern_block_from_visual_builder_and")}</p>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {patterns.map(item => <article key={item.id} className="min-w-0 rounded-lg border border-slate-200 p-3">
              <h4 className="break-words font-medium">{item.title}</h4>
              <p className="mt-1 break-words text-xs text-slate-500">#{item.id} · {item.slug} · {item.referencing_count} {tStatic("feature.library.templatelibrarypage.content_references")}</p>
              <button type="button" className="btn-secondary mt-3" onClick={() => onNavigateHash(`#/patterns/${item.id}/edit`)}>{tStatic("feature.library.templatelibrarypage.open_pattern")}</button>
            </article>)}
          </div>
          {!patterns.length && <p className="text-sm">{tStatic("feature.library.templatelibrarypage.no_synced_patterns")}</p>}
          <button type="button" onClick={() => onNavigateHash('#/patterns/new')}>{tStatic("feature.library.templatelibrarypage.create_pattern")}</button>
        </section>
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'theme'}
        id="template-library-panel-theme"
        labelledBy="template-library-tab-theme"
      >
        <section className="space-y-3" aria-label={tStatic("feature.library.templatelibrarypage.local_theme_templates")}>
          <h3 className="font-semibold">{tStatic("feature.library.templatelibrarypage.active_theme_templates")}</h3>
          <p className="text-sm text-slate-600">{tStatic("feature.common.ui_theme")} {catalog.active_theme_id ?? tStatic("feature.library.templatelibrarypage.none")} · {catalog.theme_state}</p>
          <div className="space-y-2.5">
            {catalog.theme_templates.map(item => <article key={item.role} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
              <div className="min-w-0"><strong>{item.role}</strong>
                <p className="break-all text-xs text-slate-500">{tStatic("feature.common.revision")}: {item.revision_id} · {item.revision_digest.slice(0, 12)}</p>
                {item.reason && <p className="text-xs text-amber-800">{item.reason}</p>}
              </div>
              {item.can_apply && <span className="text-xs text-slate-600">{tStatic("feature.library.templatelibrarypage.active_theme_templates_2")}</span>}
            </article>)}
          </div>
          {!catalog.theme_templates.length && <p className="text-sm text-slate-600">{tStatic("feature.library.templatelibrarypage.the_theme_does_not_provide_valid_local_role_templates")}</p>}
        </section>
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'overrides'}
        id="template-library-panel-overrides"
        labelledBy="template-library-tab-overrides"
      >
        <section className="space-y-3" aria-label={tStatic("feature.common.site_template_overrides")}>
          <h3 className="font-semibold">{tStatic("feature.common.site_template_overrides")} ({overrides.length})</h3>
          <p className="text-sm text-slate-600">{tStatic("feature.library.templatelibrarypage.site_owned_ast_takes_precedence_over_a_theme_template")}</p>
          <div className="space-y-2.5">
            {overrides.map(item => <article key={item.id} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
              <div className="min-w-0">
                <strong className="break-all">{item.canonical_target_key}</strong>
                <p className="text-xs text-slate-600">{tStatic("feature.users.userprofilepanel.version_v")}{item.version} · {item.effective_source?.label ?? tStatic("feature.library.templatelibrarypage.site_override")}</p>
                <p className="break-all text-xs text-slate-500">{tStatic("feature.library.templatelibrarypage.original_theme")}{' '}{item.base_theme_id ?? 'Core'} · {item.base_revision_digest?.slice(0, 12) ?? tStatic("feature.library.templatelibrarypage.no_revision")}</p>
              </div>
              <button type="button" className="btn-secondary" disabled={!!applying || loading} onClick={() => void resetSiteOverride(item)}>
                {applying === item.canonical_target_key ? tStatic("feature.library.templatelibrarypage.resetting") : tStatic("feature.library.templatelibrarypage.reset_override")}
              </button>
            </article>)}
          </div>
          {!overrides.length && <p className="text-sm text-slate-600">{tStatic("feature.library.templatelibrarypage.no_site_owned_override_the_resolver_uses_active_theme")}</p>}
        </section>
      </AdminTabPanel>
    </>}
  </main>;
}
