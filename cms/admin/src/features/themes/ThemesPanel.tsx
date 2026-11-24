import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  activateThemeRevision,
  dryRunSampleImport,
  executeSampleImport,
  getThemes,
  installTheme,
  isV3ThemeInstall,
  type ImportExecutionOutcome,
  type ImportPlan,
  type SampleGroup,
  type ThemesSummary,
} from '../../api/themes.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { ThemeOptionsForm } from './ThemeOptionsForm.tsx';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface Props {
  mode?: 'themes' | 'options';
  csrfToken: string | null;
  localeState?: AdminLocaleState;
  onApiError: (error: unknown, fallback: string) => void;
  onInstalled: () => void;
  onActivated: () => void;
}

export const ThemesPanel: React.FC<Props> = ({
  mode = 'themes',
  csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onApiError,
  onInstalled,
  onActivated,
}) => {
  const { t: translate } = useI18n();
  const [data, setData] = useState<ThemesSummary | null>(null);
  const [busy, setBusy] = useState(false);
  // The chosen revision is an explicit Admin draft, not the active selection.
  // Keep it untouched on stale 412 and permission failures.
  const [selectedRevisionId, setSelectedRevisionId] = useState<string | null>(null);
  const [sampleGroups, setSampleGroups] = useState<SampleGroup[]>(['pages', 'menus', 'media', 'settings']);
  const [importPlan, setImportPlan] = useState<ImportPlan | null>(null);
  const [ackPublic, setAckPublic] = useState(false);
  const [importOutcome, setImportOutcome] = useState<ImportExecutionOutcome | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const mounted = useRef(true);

  const refresh = async (): Promise<boolean> => {
    setLoading(true);
    setError('');
    try {
      const result = await getThemes();
      if (mounted.current) setData(result);
      return mounted.current;
    } catch (cause) {
      if (mounted.current) {
        setError(cause instanceof Error ? cause.message : tStatic('feature.themes.themespanel.could_not_load_themes'));
        onErrorRef.current(cause, tStatic('feature.themes.themespanel.could_not_load_themes'));
      }
      return false;
    } finally {
      if (mounted.current) setLoading(false);
    }
  };

  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => { mounted.current = false; };
  }, []);

  const upload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file || !csrfToken || busy) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const item = await installTheme(file, csrfToken);
      if (isV3ThemeInstall(item)) {
        setSelectedRevisionId(item.revision_id);
        setNotice(tStatic('feature.themes.themespanel.installed_value1_v_value2_revision_value3_not_activated', { value1: item.theme_id, value2: item.version, value3: item.revision_id.slice(0, 16) }));
      }
      onInstalled();
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : tStatic('feature.themes.themespanel.could_not_install_the_theme'));
      onErrorRef.current(cause, tStatic('feature.themes.themespanel.could_not_install_the_theme'));
    } finally {
      setBusy(false);
    }
  };

  const activateRevision = async () => {
    if (!csrfToken || !data || busy || !selectedRevisionId) return;
    const revisionId = selectedRevisionId;
    if (!data.revisions.some(rev => rev.source_format === 'v3' && rev.revision_id === revisionId)) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      // Revision activation returns ThemeSelection, not the legacy ThemeList.
      // Always read the complete projection after the mutation.
      await activateThemeRevision(revisionId, data.selection.version, csrfToken);
      const reloaded = await refresh();
      if (reloaded) setSelectedRevisionId(null);
      setImportPlan(null);
      setImportOutcome(null);
      setAckPublic(false);
      onActivated();
      if (reloaded) setNotice(tStatic('feature.themes.themespanel.activated_the_selected_revision_content_menus_and_saved_options_were_retained'));
      else setError(tStatic('feature.themes.themespanel.activation_was_requested_but_server_status_could_not_be_refreshed_the_selected_r'));
    } catch (cause) {
      const stale = cause instanceof ApiClientError && cause.status === 412;
      setError(stale
        ? tStatic('feature.themes.themespanel.the_theme_changed_in_another_tab_412_the_selected_revision_is_retained_reload_to')
        : cause instanceof Error ? cause.message : tStatic('feature.themes.themespanel.could_not_activate_the_revision'));
      onErrorRef.current(cause, tStatic('feature.themes.themespanel.could_not_activate_the_revision'));
    } finally {
      setBusy(false);
    }
  };

  const v3Revisions = data?.revisions.filter(revision => revision.source_format === 'v3') ?? [];
  const activeThemeKey = data?.selection.source_format === 'v3'
    ? data.selection.theme_id
    : data?.active_id;

  if (mode === 'options') {
    return (
      <section className="min-w-0 space-y-5" aria-label={translate('shell.studio')} data-testid="theme-options-screen">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{translate('feature.themes.themespanel.appearance_customize')}</p>
          <h2 className="mt-1.5 text-xl font-bold tracking-tight text-zinc-950">{translate('feature.themes.themespanel.appearance_options')}</h2>
          <p className="mt-1 text-sm text-zinc-600">{translate('feature.themes.themespanel.configure_the_active_theme_install_and_activate_themes_on_the_separate_appearanc')}</p>
        </div>
        {loading && <p role="status" className="text-sm text-zinc-600">{translate('feature.themes.themespanel.loading_theme_options')}</p>}
        {error && <AdminAlert tone="error">{error}</AdminAlert>}
        {!loading && data && !activeThemeKey && (
          <AdminAlert tone="warning">{translate('feature.themes.themespanel.no_active_theme_to_customize_open_appearance_to_install_and_activate_a_theme_fir')}</AdminAlert>
        )}
        {!loading && data && activeThemeKey && (
          <ThemeOptionsForm key={`${activeThemeKey}@${data.selection.revision_id ?? data.selection.version}`}
            themeId={activeThemeKey} csrfToken={csrfToken} localeState={localeState} onApiError={onApiError} />
        )}
      </section>
    );
  }

  return (
    <section className="min-w-0 space-y-5" aria-label={translate('feature.themes.themespanel.manage_themes')} data-testid="themes-panel">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{translate('feature.themes.themespanel.appearance_themes')}</p>
          <h2 className="mt-1.5 text-xl font-bold tracking-tight text-zinc-950">{translate('feature.themes.themespanel.manage_themes')}</h2>
          <p className="mt-1 text-sm text-zinc-600">{translate('feature.themes.themespanel.activating_a_theme_does_not_change_blocks_fields_or_data_saved_in_the_builder')}</p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy || loading}>{translate('feature.plugins.pluginspanel.reload_list')}</AdminButton>
      </div>
      <label className="block rounded-xl border border-dashed border-zinc-300 bg-zinc-50/70 p-4 transition-colors hover:border-od-accent/60 hover:bg-od-accent-soft/25">
        <span className="block text-sm font-semibold text-zinc-900">{translate('feature.themes.themespanel.install_theme_from_zip')}</span>
        <span className="mt-0.5 block text-xs text-zinc-600">{translate('feature.themes.themespanel.theme_v3_manifest_version_3_theme_json_templates_parts_and_declared_assets')}</span>
        <input data-testid="themes-upload" type="file" accept=".zip,application/zip" onChange={upload} disabled={!csrfToken || busy} className="mt-3 block max-w-full text-sm text-zinc-700 file:mr-3 file:rounded-md file:border file:border-zinc-300 file:bg-white file:px-3 file:py-1.5 file:text-xs file:font-medium file:text-zinc-800 hover:file:bg-zinc-50" />
      </label>
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-zinc-200 pb-2.5">
        <h3 className="text-base font-semibold text-zinc-900">{translate('feature.themes.themespanel.installed')}{v3Revisions.length}{' '}{translate('feature.themes.themespanel.v3_revisions')}</h3>
        {!loading && data && (
          <p className="text-xs text-zinc-600" data-testid="active-theme">{translate('feature.themes.themespanel.active_theme')}{' '}<strong className="font-semibold text-zinc-900">{data.selection.source_format === 'v3' ? `${data.selection.theme_id} (v3)` : translate('feature.themes.themespanel.no_theme')}</strong>{' '}{translate('feature.themes.themespanel.selected_version')}{' '}{data.selection.version}
          </p>
        )}
      </div>
      {loading && <p role="status" className="text-sm text-zinc-600">{translate('feature.themes.themespanel.loading_themes')}</p>}
      {!loading && data && (
        <>
          {data.homepage_compatible === false && (
            <AdminAlert tone="warning" role="alert" data-testid="theme-compat-warning">{translate('feature.themes.themespanel.the_current_theme_does_not_support_homepage_blocks_public_pages_hide_the_blocks_')}</AdminAlert>
          )}
          {data.homepage_compatible === true && (
            <AdminAlert tone="success" data-testid="theme-compat-ok">{translate('feature.themes.themespanel.the_theme_supports_saved_homepage_blocks')}</AdminAlert>
          )}
          {v3Revisions.length === 0 && <p className="text-sm text-zinc-500">{translate('feature.themes.themespanel.no_themes_yet_install_a_zip_to_get_started')}</p>}
          {v3Revisions.length > 0 && (
            <section className="space-y-3" aria-label={translate('feature.themes.themespanel.theme_v3_revisions')} data-testid="theme-v3-revisions">
              <h4 className="text-sm font-semibold text-zinc-700">{translate('feature.themes.themespanel.installed_v3_revisions')}{v3Revisions.length})</h4>
              <ul className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
                {v3Revisions.map(revision => {
                  const isActive = data.selection.revision_id === revision.revision_id;
                  const isSelected = selectedRevisionId === revision.revision_id;
                  return (
                    <li key={revision.revision_id} data-theme-revision-id={revision.revision_id}
                      className={`min-w-0 space-y-2 rounded-xl border p-4 transition-colors ${
                        isActive
                          ? 'border-emerald-200 bg-emerald-50/30'
                          : isSelected
                            ? 'border-od-accent bg-od-accent-soft/20'
                            : 'border-zinc-200 bg-white'
                      }`}>
                      <div className="flex items-start justify-between gap-2">
                        <div className="break-all font-semibold text-zinc-950">{revision.theme_id}</div>
                        {isActive && <AdminBadge tone="success">{translate('feature.themes.themespanel.active')}</AdminBadge>}
                      </div>
                      <p className="break-all font-mono text-xs text-zinc-500">{translate('feature.users.userspanel.version')}{' '}{revision.version} · SHA {revision.revision_digest.slice(0, 12)}…</p>
                      {!isActive && (
                        <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-zinc-700">
                          <input type="radio" name="selected-theme-revision" value={revision.revision_id}
                            checked={isSelected}
                            onChange={() => setSelectedRevisionId(revision.revision_id)} disabled={busy || loading} />{translate('feature.themes.themespanel.select_a_revision_to_activate')}</label>
                      )}
                    </li>
                  );
                })}
              </ul>
              <div className="flex flex-wrap items-center gap-3">
                <AdminButton variant="primary" size="md" onClick={() => void activateRevision()}
                  disabled={!csrfToken || busy || loading || !selectedRevisionId || !v3Revisions.some(rev => rev.revision_id === selectedRevisionId && data.selection.revision_id !== rev.revision_id)}>{translate('feature.themes.themespanel.activate_selected_revision')}</AdminButton>
                <p className="text-xs text-zinc-500">{translate('feature.themes.themespanel.installing_a_zip_does_not_activate_it_automatically_the_active_revision_comes_fr')}</p>
              </div>
            </section>
          )}
          {data.selection.source_format === 'v3' && (
            <nav aria-label={translate('feature.themes.themespanel.edit_theme_templates')} className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-zinc-200 bg-white p-4 text-sm">
              <p className="text-zinc-700">{translate('feature.themes.themespanel.to_edit_header_footer_single_or_archive_open_the_visual_builder_and_select_the_a')}</p>
              <a href="#/builder" className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:border-zinc-400 hover:bg-zinc-50">{translate('feature.themes.themespanel.open_visual_builder')}</a>
            </nav>
          )}
          {activeThemeKey && (
            <div className="space-y-3 rounded-xl border border-zinc-200 bg-zinc-50/70 p-4" data-testid="sample-import-box">
              <h3 className="text-sm font-semibold text-zinc-900">{translate('feature.themes.themespanel.selective_sample_data_import')}</h3>
              <div className="flex flex-wrap gap-3 text-sm text-zinc-700">
                {(['pages', 'menus', 'media', 'settings'] as SampleGroup[]).map(g => (
                  <label key={g} className="inline-flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={sampleGroups.includes(g)}
                      onChange={e => {
                        setImportPlan(null);
                        setImportOutcome(null);
                        setSampleGroups(prev => e.target.checked ? [...prev, g] : prev.filter(x => x !== g));
                      }}
                    />
                    <span>{g}</span>
                  </label>
                ))}
              </div>
              <div className="flex flex-wrap gap-2">
                <AdminButton
                  variant="secondary"
                  size="sm"
                  disabled={!csrfToken || busy || sampleGroups.length === 0}
                  onClick={() => {
                    if (!activeThemeKey || !csrfToken) return;
                    setBusy(true);
                    setError('');
                    setImportOutcome(null);
                    void dryRunSampleImport(activeThemeKey, sampleGroups, csrfToken)
                      .then(plan => { setImportPlan(plan); setAckPublic(false); })
                      .catch(cause => setError(cause instanceof Error ? cause.message : translate('feature.themes.themespanel.dry_run_failed')))
                      .finally(() => setBusy(false));
                  }}
                >{translate('feature.themes.themespanel.preview_dry_run')}</AdminButton>
                {importPlan && (
                  <AdminButton variant="ghost" size="sm" disabled={busy} onClick={() => { setImportPlan(null); setAckPublic(false); }}>{translate('feature.themes.themespanel.cancel_preview')}</AdminButton>
                )}
                {importPlan && (
                  <AdminButton
                    variant="primary"
                    size="sm"
                    disabled={!csrfToken || busy || (importPlan.requires_public_ack && !ackPublic)}
                    onClick={() => {
                      if (!activeThemeKey || !csrfToken) return;
                      setBusy(true);
                      setError('');
                      void executeSampleImport(activeThemeKey, importPlan, ackPublic, csrfToken)
                        .then(out => { setImportOutcome(out); setImportPlan(null); })
                        .catch(cause => setError(cause instanceof Error ? cause.message : translate('feature.themes.themespanel.import_failed')))
                        .finally(() => setBusy(false));
                    }}
                  >{translate('feature.themes.themespanel.import_sample_data')}</AdminButton>
                )}
              </div>
              {importPlan && (
                <div className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3 text-xs text-zinc-700">
                  <p>{translate('feature.themes.themespanel.plan_digest')}{' '}<code className="rounded bg-zinc-100 px-1 py-0.5 font-mono">{importPlan.plan_digest.slice(0, 12)}</code> · {importPlan.items.length}{' '}{translate('feature.themes.themespanel.items')}</p>
                  {importPlan.unselected_dependencies.length > 0 && (
                    <p className="text-amber-800">{translate('feature.themes.themespanel.missing_dependency_groups')}{' '}{importPlan.unselected_dependencies.map(d => `${d.required_group}:${d.object_key}`).join(', ')}</p>
                  )}
                  {importPlan.requires_public_ack && (
                    <label className="flex items-center gap-2 text-amber-900">
                      <input type="checkbox" checked={ackPublic} onChange={e => setAckPublic(e.target.checked)} />
                      <span>{translate('feature.themes.themespanel.i_confirm_that_changes_to_menus_options_affect_the_public_site')}</span>
                    </label>
                  )}
                </div>
              )}
              {importOutcome && (
                <AdminAlert tone="success">{translate('feature.themes.themespanel.imported')}{' '}{importOutcome.created.length}{' '}{translate('feature.themes.themespanel.skipped')}{' '}{importOutcome.skipped.length}
                </AdminAlert>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
};
