import { __, _x, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  activateThemeRevision,
  dryRunSampleImport,
  executeSampleImport,
  getActiveThemeFunctionsLogs,
  getActiveThemeFunctionsSettings,
  getThemes,
  installTheme,
  isV3ThemeInstall,
  runActiveThemeAction,
  saveActiveThemeFunctionsSettings,
  type ImportExecutionOutcome,
  type ImportPlan,
  type SampleGroup,
  type ThemeFunctionLogItem,
  type ThemeFunctionsSettingsSnapshot,
  type ThemesSummary,
} from '../../api/themes.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  buildOwnerOptionsSubmitPayload,
  OwnerOptionsForm,
} from '../shared/OwnerOptionsForm.tsx';

interface Props {
  mode?: 'list' | 'install';
  csrfToken: string | null;
  devTheme?: string | null;
  themeRevision?: number;
  onApiError: (error: unknown, fallback: string) => void;
  onInstalled: () => void;
  onActivated: () => void;
}

export const ThemesPanel: React.FC<Props> = ({
  mode = 'list',
  csrfToken,
  devTheme = null,
  themeRevision = 0,
  onApiError,
  onInstalled,
  onActivated,
}) => {
  const { __, _x } = useI18n();
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
  const [fnSettings, setFnSettings] = useState<ThemeFunctionsSettingsSnapshot | null>(null);
  const [fnDraft, setFnDraft] = useState<Record<string, unknown>>({});
  const [fnSaving, setFnSaving] = useState(false);
  const [fnNotice, setFnNotice] = useState('');
  const [fnError, setFnError] = useState('');
  const [fnLogs, setFnLogs] = useState<ThemeFunctionLogItem[]>([]);
  const [runningActionId, setRunningActionId] = useState<string | null>(null);
  const [actionOutput, setActionOutput] = useState<string>('');
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const mounted = useRef(true);

  const loadThemeFunctions = async () => {
    try {
      const [settings, logsResp] = await Promise.all([
        getActiveThemeFunctionsSettings(),
        getActiveThemeFunctionsLogs(50),
      ]);
      if (!mounted.current) return;
      setFnSettings(settings);
      setFnDraft(settings.values);
      setFnLogs(logsResp.items);
    } catch {
      if (mounted.current) {
        setFnSettings(null);
        setFnLogs([]);
      }
    }
  };

  const refresh = async (): Promise<boolean> => {
    setLoading(true);
    setError('');
    try {
      const result = await getThemes();
      if (mounted.current) setData(result);
      await loadThemeFunctions();
      return mounted.current;
    } catch (cause) {
      if (mounted.current) {
        setError(cause instanceof Error ? cause.message : _x("Could not load themes.", "feature.themes"));
        onErrorRef.current(cause, _x("Could not load themes.", "feature.themes"));
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
  }, [themeRevision]);

  useEffect(() => {
    setNotice('');
    setError('');
  }, [mode]);

  const handleSaveThemeFunctions = async () => {
    if (!fnSettings || !fnSettings.has_functions || !csrfToken || fnSaving) return;
    setFnSaving(true);
    setFnError('');
    setFnNotice('');
    try {
      const payload = buildOwnerOptionsSubmitPayload(fnSettings.fields, fnDraft);
      const saved = await saveActiveThemeFunctionsSettings(payload, fnSettings.version, csrfToken);
      if (mounted.current) {
        setFnSettings(saved);
        setFnDraft(saved.values);
        setFnNotice(__("Theme Functions settings saved."));
      }
    } catch (cause) {
      if (mounted.current) {
        setFnError(cause instanceof Error ? cause.message : __("Could not save Theme Functions settings."));
      }
      onErrorRef.current(cause, __("Could not save Theme Functions settings."));
    } finally {
      if (mounted.current) setFnSaving(false);
    }
  };

  const handleRunThemeAction = async (actionId: string) => {
    if (!csrfToken || runningActionId) return;
    setRunningActionId(actionId);
    setFnError('');
    try {
      const result = await runActiveThemeAction(actionId, {}, csrfToken);
      if (mounted.current) {
        setActionOutput(JSON.stringify(result, null, 2));
      }
      const logsResp = await getActiveThemeFunctionsLogs(50);
      if (mounted.current) setFnLogs(logsResp.items);
    } catch (cause) {
      if (mounted.current) {
        setFnError(cause instanceof Error ? cause.message : __("Could not execute theme action."));
      }
      onErrorRef.current(cause, __("Could not execute theme action."));
    } finally {
      if (mounted.current) setRunningActionId(null);
    }
  };

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
        setNotice(__("Installed {{value1}} v{{value2}} (revision {{value3}}…). Not activated.", { value1: item.theme_id, value2: item.version, value3: item.revision_id.slice(0, 16) }));
      }
      onInstalled();
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : __("Could not install the theme."));
      onErrorRef.current(cause, __("Could not install the theme."));
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
      if (reloaded) setNotice(__("Activated the selected revision. Content, menus and saved options were retained."));
      else setError(__("Activation was requested but server status could not be refreshed. The selected revision is retained."));
    } catch (cause) {
      const stale = cause instanceof ApiClientError && cause.status === 412;
      setError(stale
        ? __("The theme changed in another tab (412). The selected revision is retained. Reload to see the latest version before retrying.")
        : cause instanceof Error ? cause.message : __("Could not activate the revision."));
      onErrorRef.current(cause, __("Could not activate the revision."));
    } finally {
      setBusy(false);
    }
  };

  const v3Revisions = data?.revisions.filter(revision => revision.source_format === 'v3') ?? [];
  const activeThemeKey = data?.selection.source_format === 'v3'
    ? data.selection.theme_id
    : null;

  return (
    <section className="min-w-0 space-y-5" aria-label={mode === 'install' ? __("Install theme") : __("Installed themes")} data-testid="themes-panel">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{__("Appearance / Themes")}</p>
          <h2 className="mt-1.5 text-xl font-bold tracking-tight text-zinc-950">
            {mode === 'install' ? __("Install theme") : __("Installed themes")}
          </h2>
          <p className="mt-1 text-sm text-zinc-600">
            {mode === 'install'
              ? __("Upload a Theme v3 ZIP package. Activating a theme does not change blocks, fields or data saved in the builder.")
              : __("Activating a theme does not change blocks, fields or data saved in the builder.")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {mode === 'install' ? (
            <a
              href="#/appearance/themes"
              data-testid="themes-installed-link"
              className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 no-underline transition-colors hover:bg-zinc-50"
            >
              {__("Installed themes")}
            </a>
          ) : (
            <>
              <a
                href="#/appearance/themes/install"
                data-testid="themes-install-link"
                className="inline-flex h-8 items-center rounded-md bg-od-accent px-3 text-xs font-medium text-white no-underline transition-colors hover:bg-od-accent-hover"
              >
                {__("Install theme")}
              </a>
              <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy || loading}>{__("Reload list")}</AdminButton>
            </>
          )}
        </div>
      </div>
      {mode === 'install' && (
        <label className="block max-w-2xl rounded-xl border border-dashed border-zinc-300 bg-zinc-50/70 p-4 transition-colors hover:border-od-accent/60 hover:bg-od-accent-soft/25">
          <span className="block text-sm font-semibold text-zinc-900">{__("Install theme from ZIP")}</span>
          <span className="mt-0.5 block text-xs text-zinc-600">{__("Theme v3 (manifest_version=3, theme.json, templates, parts and declared assets).")}</span>
          <input data-testid="themes-upload" type="file" accept=".zip,application/zip" onChange={upload} disabled={!csrfToken || busy} className="mt-3 block max-w-full text-sm text-zinc-700 file:mr-3 file:rounded-md file:border file:border-zinc-300 file:bg-white file:px-3 file:py-1.5 file:text-xs file:font-medium file:text-zinc-800 hover:file:bg-zinc-50" />
        </label>
      )}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {mode === 'list' && <>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-zinc-200 pb-2.5">
        <h3 className="text-base font-semibold text-zinc-900">{__("Installed (")}{v3Revisions.length}{' '}{__("v3 revisions)")}</h3>
        {!loading && data && (
          <p className="text-xs text-zinc-600" data-testid="active-theme">{__("Active theme:")}{' '}<strong className="font-semibold text-zinc-900">{data.selection.source_format === 'v3' ? `${data.selection.theme_id} (v3)` : __("No theme")}</strong>{' '}{__("· Selected version")}{' '}{data.selection.version}
          </p>
        )}
      </div>
      {loading && <p role="status" className="text-sm text-zinc-600">{__("Loading themes…")}</p>}
      {!loading && data && (
        <>
          {v3Revisions.length === 0 && <p className="text-sm text-zinc-500">{__("No themes yet. Install a ZIP to get started.")}</p>}
          {v3Revisions.length > 0 && (
            <section className="space-y-3" aria-label={__("Theme v3 revisions")} data-testid="theme-v3-revisions">
              <h4 className="text-sm font-semibold text-zinc-700">{__("Installed v3 revisions (")}{v3Revisions.length})</h4>
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
                        {isActive && <AdminBadge tone="success">{_x("Active", "feature.themes")}</AdminBadge>}
                      </div>
                      <p className="break-all font-mono text-xs text-zinc-500">{__("Version")}{' '}{revision.version} · SHA {revision.revision_digest.slice(0, 12)}…</p>
                      {!isActive && (
                        <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-zinc-700">
                          <input type="radio" name="selected-theme-revision" value={revision.revision_id}
                            checked={isSelected}
                            onChange={() => setSelectedRevisionId(revision.revision_id)} disabled={busy || loading} />{__("Select a revision to activate")}</label>
                      )}
                    </li>
                  );
                })}
              </ul>
              <div className="flex flex-wrap items-center gap-3">
                <AdminButton variant="primary" size="md" onClick={() => void activateRevision()}
                  disabled={!csrfToken || busy || loading || !selectedRevisionId || !v3Revisions.some(rev => rev.revision_id === selectedRevisionId && data.selection.revision_id !== rev.revision_id)}>{__("Activate selected revision")}</AdminButton>
                <p className="text-xs text-zinc-500">{__("Installing a ZIP does not activate it automatically. The active revision comes from the version selected on the server.")}</p>
              </div>
            </section>
          )}
          {data.selection.source_format === 'v3' && (
            <nav aria-label={__("Edit theme templates")} className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-zinc-200 bg-white p-4 text-sm">
              <p className="text-zinc-700">{__("To edit header, footer, single or archive, open the visual builder and select the appropriate role.")}</p>
              <a href="#/studio" className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:border-zinc-400 hover:bg-zinc-50">{__("Open visual builder")}</a>
            </nav>
          )}
          {activeThemeKey && (
            <div className="space-y-3 rounded-xl border border-zinc-200 bg-zinc-50/70 p-4" data-testid="sample-import-box">
              <h3 className="text-sm font-semibold text-zinc-900">{__("Selective sample data import")}</h3>
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
                      .catch(cause => setError(cause instanceof Error ? cause.message : __("Dry run failed.")))
                      .finally(() => setBusy(false));
                  }}
                >{__("Preview (dry run)")}</AdminButton>
                {importPlan && (
                  <AdminButton variant="ghost" size="sm" disabled={busy} onClick={() => { setImportPlan(null); setAckPublic(false); }}>{__("Cancel preview")}</AdminButton>
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
                        .catch(cause => setError(cause instanceof Error ? cause.message : __("Import failed.")))
                        .finally(() => setBusy(false));
                    }}
                  >{__("Import sample data")}</AdminButton>
                )}
              </div>
              {importPlan && (
                <div className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3 text-xs text-zinc-700">
                  <p>{__("Plan digest:")}{' '}<code className="rounded bg-zinc-100 px-1 py-0.5 font-mono">{importPlan.plan_digest.slice(0, 12)}</code> · {importPlan.items.length}{' '}{__("items")}</p>
                  {importPlan.unselected_dependencies.length > 0 && (
                    <p className="text-amber-800">{__("Missing dependency groups:")}{' '}{importPlan.unselected_dependencies.map(d => `${d.required_group}:${d.object_key}`).join(', ')}</p>
                  )}
                  {importPlan.requires_public_ack && (
                    <label className="flex items-center gap-2 text-amber-900">
                      <input type="checkbox" checked={ackPublic} onChange={e => setAckPublic(e.target.checked)} />
                      <span>{__("I confirm that changes to menus/options affect the public site.")}</span>
                    </label>
                  )}
                </div>
              )}
              {importOutcome && (
                <AdminAlert tone="success">{__("Imported:")}{' '}{importOutcome.created.length}{' '}{__("· Skipped:")}{' '}{importOutcome.skipped.length}
                </AdminAlert>
              )}
            </div>
          )}
          {fnSettings?.has_functions && (
            <section
              className="space-y-4 rounded-xl border border-zinc-200 bg-white p-4"
              aria-label={__("Theme Functions")}
              data-testid="theme-functions-section"
            >
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div>
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="text-base font-semibold text-zinc-950">
                      {__("Theme Functions")}
                    </h3>
                    {fnSettings.theme_id && (
                      <AdminBadge tone="neutral">{fnSettings.theme_id}</AdminBadge>
                    )}
                    {devTheme && devTheme === fnSettings.theme_id && (
                      <AdminBadge tone="warning">{__("DEV MODE")}</AdminBadge>
                    )}
                  </div>
                  <p className="mt-0.5 text-xs text-zinc-600">
                    {__("Configure settings, secrets, actions, and recent execution logs for the active theme's functions.js.")}
                  </p>
                </div>
                <AdminButton
                  variant="secondary"
                  size="sm"
                  disabled={fnSaving || Boolean(runningActionId)}
                  onClick={() => void loadThemeFunctions()}
                >
                  {__("Reload logs")}
                </AdminButton>
              </div>
              {fnSettings.fields.length > 0 && (
                <OwnerOptionsForm
                  ownerKind="theme_functions"
                  ownerId={fnSettings.theme_id ?? 'active'}
                  title={__("Theme Functions settings")}
                  fields={fnSettings.fields}
                  values={fnDraft}
                  savedValues={fnSettings.values}
                  version={fnSettings.version}
                  csrfToken={csrfToken}
                  saving={fnSaving}
                  notice={fnNotice}
                  error={fnError}
                  testId="theme-functions-settings-form"
                  onChange={setFnDraft}
                  onSubmit={() => { void handleSaveThemeFunctions(); }}
                  onApiError={onApiError}
                />
              )}
              {fnSettings.fields.length === 0 && fnError && (
                <AdminAlert tone="error">{fnError}</AdminAlert>
              )}
              {fnSettings.actions.length > 0 && (
                <div className="space-y-2 border-t border-zinc-100 pt-3" data-testid="theme-functions-actions">
                  <h4 className="text-sm font-semibold text-zinc-900">
                    {__("Theme actions")}
                  </h4>
                  <div className="flex flex-wrap gap-2">
                    {fnSettings.actions.map(action => (
                      <AdminButton
                        key={action.id}
                        variant="secondary"
                        size="sm"
                        disabled={!csrfToken || Boolean(runningActionId)}
                        onClick={() => void handleRunThemeAction(action.id)}
                      >
                        {runningActionId === action.id
                          ? __("Running…")
                          : `${action.title || action.id} (${action.required_role})`}
                      </AdminButton>
                    ))}
                  </div>
                  {actionOutput && (
                    <div className="space-y-1">
                      <p className="text-xs font-medium text-zinc-700">
                        {__("Last action output")}
                      </p>
                      <pre className="max-h-48 overflow-auto rounded-lg border border-zinc-200 bg-zinc-50 p-2.5 font-mono text-xs text-zinc-800">
                        {actionOutput}
                      </pre>
                    </div>
                  )}
                </div>
              )}
              <div className="space-y-2 border-t border-zinc-100 pt-3" data-testid="theme-functions-logs">
                <h4 className="text-sm font-semibold text-zinc-900">
                  {__("Recent Theme Functions logs")}
                </h4>
                {fnLogs.length === 0 ? (
                  <p className="text-xs text-zinc-500">
                    {__("No Theme Functions logs recorded yet.")}
                  </p>
                ) : (
                  <ul className="space-y-1.5 text-xs">
                    {fnLogs.map(log => (
                      <li
                        key={log.id}
                        className="flex flex-wrap items-baseline gap-2 rounded-md border border-zinc-100 bg-zinc-50/70 px-2.5 py-1.5"
                      >
                        <AdminBadge tone={log.level === 'error' ? 'danger' : log.level === 'warn' ? 'warning' : 'neutral'}>
                          {log.level}
                        </AdminBadge>
                        <span className="font-mono text-[11px] text-zinc-500">{log.created_at}</span>
                        <span className="break-words text-zinc-800">{log.message}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </section>
          )}
        </>
      )}
      </>}
    </section>
  );
};
