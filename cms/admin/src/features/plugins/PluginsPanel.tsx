import { __, _x, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import type { PluginSettingsSnapshot, PluginSummary } from '../../types.ts';
import { ApiClientError } from '../../api/client.ts';
import { getPluginSettings, getPlugins, savePluginSettings } from '../../api/plugins.ts';
import { ZipUploadField } from '../../components/FormPrimitives.tsx';
import {
  AdminAlert,
  AdminBadge,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
} from '../../components/AdminPrimitives.tsx';

type PluginsTabId = 'installed' | 'settings' | 'run';
import {
  buildOwnerOptionsSubmitPayload,
  OwnerOptionsForm,
} from '../shared/OwnerOptionsForm.tsx';
import { DeclarativePluginUi } from './DeclarativePluginUi.tsx';

interface PluginsPanelProps {
  mode?: 'list' | 'install';
  plugins: PluginSummary[];
  devPlugins?: string[];
  selectedPluginId: string;
  pluginInputJson: string;
  pluginRunOutput: string;
  isProcessing: boolean;
  onPluginUpload: (event: React.ChangeEvent<HTMLInputElement>) => void;
  onSelectPlugin: (id: string) => void;
  onDisablePlugin: (id: string) => void;
  onEnablePlugin: (id: string) => void;
  onPluginInputChange: (json: string) => void;
  onRunPlugin: () => void;
  csrfToken?: string | null;
  onUnauthorized?: () => void;
}

function permissionDetails(plugin: PluginSummary): string[] {
  const permissions = plugin.permissions;
  return [
    ...(permissions?.sqlite ?? []).map(value => 'SQLite: ' + value),
    ...(permissions?.http ?? []).map(value => 'HTTP: ' + value),
    ...(permissions?.content_read ?? []).map(value => __("Read content: {{value1}}", { value1: value })),
    ...(permissions?.content_write ?? []).map(value => __("Write content: {{value1}}", { value1: value })),
    ...(permissions?.content_delete ?? []).map(value => __("Move content to Trash (recoverable): {{value1}}", { value1: value })),
    ...(permissions?.content_trash_read ?? []).map(value => __("Read Trash metadata: {{value1}}", { value1: value })),
    ...(permissions?.content_restore ?? []).map(value => __("Restore from Trash: {{value1}}", { value1: value })),
    ...(permissions?.content_purge ?? []).map(value => __("Permanently delete from Trash (Admin, irreversible): {{value1}}", { value1: value })),
  ];
}

function readableError(error: unknown): string {
  return error instanceof Error ? error.message : _x("Could not load plugins.", "api.plugins");
}

/**
 * Scope M3-S28 + Plugin S02: keep App-owned lifecycle/run mutations intact while
 * hosting owner-isolated plugin settings (`GET/PUT /api/admin/plugins/{id}/settings`)
 * through shared `<OwnerOptionsForm>`.
 */
export const PluginsPanel: React.FC<PluginsPanelProps> = ({
  mode = 'list',
  plugins,
  devPlugins = [],
  selectedPluginId,
  pluginInputJson,
  pluginRunOutput,
  isProcessing,
  onPluginUpload,
  onSelectPlugin,
  onDisablePlugin,
  onEnablePlugin,
  onPluginInputChange,
  onRunPlugin,
  csrfToken,
  onUnauthorized,
}) => {
  const { __, _x } = useI18n();
  const [visiblePlugins, setVisiblePlugins] = useState<PluginSummary[]>(plugins);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [pendingAction, setPendingAction] = useState('');
  const [notice, setNotice] = useState('');
  const [settingsPluginId, setSettingsPluginId] = useState('');
  const [settingsSnapshot, setSettingsSnapshot] = useState<PluginSettingsSnapshot | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<Record<string, unknown>>({});
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsNotice, setSettingsNotice] = useState('');
  const [settingsError, setSettingsError] = useState('');
  const [settingsReloadTick, setSettingsReloadTick] = useState(0);
  const [activeTab, setActiveTab] = useState<PluginsTabId>('installed');
  const latestRequest = useRef(0);
  const pendingRef = useRef(false);
  const mounted = useRef(true);
  const onUnauthorizedRef = useRef(onUnauthorized);
  onUnauthorizedRef.current = onUnauthorized;

  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    const current = ++latestRequest.current;
    getPlugins(controller.signal)
      .then(list => {
        if (mounted.current && latestRequest.current === current) {
          setVisiblePlugins(list);
          setLoadError('');
          setLoading(false);
        }
      })
      .catch(error => {
        if (controller.signal.aborted || !mounted.current || latestRequest.current !== current) return;
        setLoadError(readableError(error));
        setLoading(false);
      });
    return () => {
      mounted.current = false;
      controller.abort();
      latestRequest.current += 1;
    };
  }, []);

  // App's existing callbacks own mutation requests, global status and 401 logout.
  useEffect(() => {
    setVisiblePlugins(plugins);
  }, [plugins]);

  const settingsTargetPlugin = visiblePlugins.find(
    plugin => plugin.id === (settingsPluginId || selectedPluginId),
  );
  const effectiveSettingsPluginId = settingsTargetPlugin?.id ?? '';

  useEffect(() => {
    if (mode !== 'list' || !effectiveSettingsPluginId) {
      setSettingsSnapshot(null);
      setSettingsDraft({});
      setSettingsError('');
      setSettingsNotice('');
      setSettingsLoading(false);
      return;
    }
    const controller = new AbortController();
    let active = true;
    setSettingsLoading(true);
    setSettingsError('');
    setSettingsNotice('');
    void getPluginSettings(effectiveSettingsPluginId, controller.signal)
      .then(snapshot => {
        if (!active) return;
        setSettingsSnapshot(snapshot);
        setSettingsDraft(snapshot.values);
      })
      .catch(cause => {
        if (!active || controller.signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 401) {
          onUnauthorizedRef.current?.();
        }
        setSettingsSnapshot(null);
        setSettingsError(
          cause instanceof Error ? cause.message : __("Could not load plugin settings."),
        );
      })
      .finally(() => {
        if (active) setSettingsLoading(false);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [mode, effectiveSettingsPluginId, settingsReloadTick]);

  const refresh = async () => {
    const current = ++latestRequest.current;
    setLoading(true);
    try {
      const list = await getPlugins();
      if (mounted.current && latestRequest.current === current) {
        setVisiblePlugins(list);
        setLoadError('');
      }
    } catch (error) {
      if (mounted.current && latestRequest.current === current) {
        setLoadError(readableError(error));
      }
    } finally {
      if (mounted.current && latestRequest.current === current) setLoading(false);
    }
  };

  const invoke = async (label: string, operation: () => void) => {
    if (pendingRef.current || isProcessing) return;
    pendingRef.current = true;
    setPendingAction(label);
    setNotice('');
    try {
      // The current App callbacks are async functions typed as void in this UI
      // contract. Await their actual returned promise before re-reading server state.
      await operation();
      await refresh();
    } finally {
      pendingRef.current = false;
      if (mounted.current) setPendingAction('');
    }
  };

  const upload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    if (!file || pendingRef.current || isProcessing) return;
    if (!file.name.toLowerCase().endsWith('.zip')) {
      setNotice(__("Only plugin .zip files are accepted."));
      event.currentTarget.value = '';
      return;
    }
    if (file.size > 5 * 1024 * 1024) {
      setNotice(__("The plugin package exceeds the 5 MiB upload limit."));
      event.currentTarget.value = '';
      return;
    }
    // The install endpoint is replace-and-enable, not a dry-run. Warn before
    // submitting whenever an existing plugin might be replaced.
    if (visiblePlugins.length > 0 &&
        !window.confirm(__("Installing a ZIP automatically enables the plugin. If its ID matches an installed plugin, the current version will be REPLACED and RE-ENABLED. Continue?"))) {
      event.currentTarget.value = '';
      return;
    }
    void invoke(__("Installing the plugin ZIP..."), () => onPluginUpload(event));
  };

  const enable = (plugin: PluginSummary) => {
    const grants = permissionDetails(plugin);
    const permissionText = grants.length > 0 ? grants.join('\n') : __("No SQLite or HTTP permissions declared.");
    if (!window.confirm(__("Enable plugin {{name}} ({{id}})?\n\nManifest permissions:\n{{permissions}}", { name: plugin.name, id: plugin.id, permissions: permissionText }))) return;
    void invoke(__("Enabling {{name}}...", { name: plugin.name }), () => onEnablePlugin(plugin.id));
  };

  const disable = (plugin: PluginSummary) => {
    void invoke(__("Disabling {{name}}...", { name: plugin.name }), () => onDisablePlugin(plugin.id));
  };

  const handleSaveSettings = async () => {
    if (!settingsSnapshot || !csrfToken || settingsSaving) return;
    setSettingsSaving(true);
    setSettingsError('');
    setSettingsNotice('');
    try {
      const payload = buildOwnerOptionsSubmitPayload(settingsSnapshot.fields, settingsDraft);
      const saved = await savePluginSettings(
        settingsSnapshot.plugin_id,
        payload,
        settingsSnapshot.version,
        csrfToken,
      );
      setSettingsSnapshot(saved);
      setSettingsDraft(saved.values);
      setSettingsNotice(__("Plugin settings saved."));
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 401) {
        onUnauthorized?.();
        setSettingsError(
          __("Your session has expired (401). Unsaved changes have been retained."),
        );
      } else if (cause instanceof ApiClientError && cause.status === 412) {
        setSettingsError(
          __("Plugin settings changed in another session (412). Your changes have been retained."),
        );
      } else {
        setSettingsError(
          cause instanceof Error ? cause.message : __("Could not save plugin settings."),
        );
      }
    } finally {
      setSettingsSaving(false);
    }
  };

  const handleSettingsApiError = (cause: unknown, fallback: string) => {
    if (cause instanceof ApiClientError && cause.status === 401) {
      onUnauthorized?.();
    }
    setSettingsError(cause instanceof Error ? cause.message : fallback);
  };

  const busy = isProcessing || Boolean(pendingAction);
  const selected = visiblePlugins.find(plugin => plugin.id === selectedPluginId);
  const canRun = Boolean(selected?.enabled) && !busy;

  return (
    <div id="plugin-management-section" className="min-w-0 space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{mode === 'install' ? _x("Install plugin", "feature.plugins") : __("Manage plugins")}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            {mode === 'install'
              ? __("Install a plugin ZIP package. The installed package is automatically enabled after validation.")
              : __("Review permissions, configure settings and enable or disable plugins running in the Wasmi sandbox.")}
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy || loading}>
          {loading ? __("Loading...") : __("Reload list")}
        </AdminButton>
      </div>

      {mode === 'install' && <section className="max-w-2xl rounded-xl border border-zinc-200 bg-zinc-50 p-4 sm:p-5" aria-labelledby="plugin-upload-heading">
        <h3 id="plugin-upload-heading" className="mb-2 text-sm font-semibold text-zinc-950">{_x("Install plugin", "feature.plugins")}</h3>
        <ZipUploadField id="plugin-zip" label={__("Select plugin ZIP (up to 5 MiB)")} onChange={upload}
          disabled={busy || loading || Boolean(loadError)} />
        <p className="mt-2.5 text-xs leading-5 text-amber-800">{__("Uploading a ZIP automatically enables the plugin. If the ID already exists, the package is replaced and the plugin re-enabled. Check its permissions before enabling a disabled plugin.")}</p>
      </section>}

      {pendingAction && <AdminAlert tone="info" aria-live="polite">{pendingAction}</AdminAlert>}
      {notice && <AdminAlert tone="error">{notice}</AdminAlert>}

      {mode === 'list' && <>
        <AdminTabs
          tabs={[
            { id: 'installed', label: _x("Installed plugins", "plugins.tab_installed"), badge: visiblePlugins.length },
            { id: 'settings', label: __("Plugin settings") },
            { id: 'run', label: __("Test execution") },
          ]}
          activeId={activeTab}
          onChange={setActiveTab}
          ariaLabel={__("Plugin management sections")}
          testId="plugins-tabs"
          idPrefix="plugins"
        />

        <AdminTabPanel
          active={activeTab === 'installed'}
          id="plugins-panel-installed"
          labelledBy="plugins-tab-installed"
        >
      <section className="space-y-3" aria-labelledby="plugin-list-heading">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 id="plugin-list-heading" className="text-sm font-semibold text-zinc-900">{__("Installed plugins (")}{visiblePlugins.length})
          </h3>
          {loading && <span role="status" className="text-xs text-zinc-500">{__("Synchronizing with the server...")}</span>}
        </div>
        {loadError && (
          <AdminAlert tone="error" className="space-y-2">
            <p>{__("Could not refresh the list:")}{' '}{loadError}</p>
            <AdminButton variant="secondary" size="sm" disabled={busy || loading} onClick={() => void refresh()}>{_x("Retry", "general.retry")}</AdminButton>
          </AdminAlert>
        )}
        {!loading && visiblePlugins.length === 0 && !loadError && (
          <p className="rounded-xl border border-dashed border-zinc-300 p-5 text-sm text-zinc-500">{__("No plugins yet. Select a ZIP package to install.")}</p>
        )}
        <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-2">
          {visiblePlugins.map(plugin => {
            const grants = permissionDetails(plugin);
            const isSettingsOpen = effectiveSettingsPluginId === plugin.id;
            return (
              <article key={plugin.id} className="min-w-0 rounded-xl border border-zinc-200 bg-white p-4" data-plugin-id={plugin.id}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <h4 className="break-words text-sm font-semibold text-zinc-950">{plugin.name}</h4>
                    <p className="mt-0.5 break-all text-xs text-zinc-500">{plugin.id} · v{plugin.version}</p>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    {devPlugins.includes(plugin.id) && (
                      <AdminBadge tone="warning">{__("DEV MODE")}</AdminBadge>
                    )}
                    <AdminBadge tone={plugin.enabled ? 'success' : 'neutral'}>
                      {plugin.enabled ? __("Enabled") : _x("Disabled", "multilingual.inactive")}
                    </AdminBadge>
                  </div>
                </div>
                {plugin.description && <p className="mt-2.5 break-words text-sm text-zinc-600">{plugin.description}</p>}
                <div className="mt-3 rounded-lg border border-zinc-100 bg-zinc-50 p-3">
                  <h5 className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500">{__("Manifest permissions")}</h5>
                  {grants.length === 0
                    ? <p className="mt-1 text-xs text-zinc-500">{__("No SQLite or HTTP permissions required.")}</p>
                    : <ul className="mt-1.5 space-y-1 text-xs text-zinc-700">
                        {grants.map((grant, index) => <li key={index} className="break-all">{grant}</li>)}
                      </ul>}
                </div>
                {plugin.last_error && (
                  <AdminAlert tone="error" role="status" className="mt-3 break-words text-xs">{__("Last error:")}{' '}{plugin.last_error}
                  </AdminAlert>
                )}
                <div className="mt-3.5 flex flex-wrap gap-2">
                  {plugin.enabled
                    ? <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => disable(plugin)}
                        aria-label={__("Disable plugin {{name}}", { name: plugin.name })}>{__("Disable")}</AdminButton>
                    : <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => enable(plugin)}
                        aria-label={__("Enable plugin {{name}}", { name: plugin.name })}>{__("Enable / Retry")}</AdminButton>}
                  <AdminButton
                    variant={isSettingsOpen ? 'primary' : 'secondary'}
                    size="sm"
                    disabled={busy}
                    onClick={() => {
                      setSettingsPluginId(plugin.id);
                      setActiveTab('settings');
                    }}
                    aria-pressed={isSettingsOpen}
                    aria-label={__("Plugin settings: {{name}}", { name: plugin.name })}
                  >
                    {isSettingsOpen ? __("Configuring") : __("Settings")}
                  </AdminButton>
                  <AdminButton variant={selectedPluginId === plugin.id ? 'primary' : 'secondary'} size="sm" disabled={busy || !plugin.enabled}
                    onClick={() => {
                      setSettingsPluginId(plugin.id);
                      onSelectPlugin(plugin.id);
                    }}
                    aria-pressed={selectedPluginId === plugin.id}>
                    {selectedPluginId === plugin.id ? __("Selected to run") : __("Select to run")}
                  </AdminButton>
                </div>
              </article>
            );
          })}
        </div>
      </section>
        </AdminTabPanel>

        <AdminTabPanel
          active={activeTab === 'settings'}
          id="plugins-panel-settings"
          labelledBy="plugins-tab-settings"
        >
      {settingsTargetPlugin ? (
        <section className="min-w-0 max-w-2xl space-y-3" aria-labelledby="plugin-settings-heading">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 id="plugin-settings-heading" className="text-sm font-semibold text-zinc-950">{__("Plugin settings:")}{' '}{settingsTargetPlugin.name} ({settingsTargetPlugin.id})
            </h3>
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={settingsLoading || settingsSaving}
              onClick={() => setSettingsReloadTick(n => n + 1)}
            >{__("Reload from server")}</AdminButton>
          </div>
          {settingsLoading && (
            <p role="status" className="text-sm text-zinc-600">{__("Loading plugin settings…")}</p>
          )}
          {!settingsLoading && !settingsSnapshot && settingsError && (
            <AdminAlert tone="error">{settingsError}</AdminAlert>
          )}
          {!settingsLoading && settingsSnapshot && settingsSnapshot.fields.length === 0 && (
            <p className="text-xs text-zinc-500">{__("This plugin does not declare a settings schema (settings_schema).")}</p>
          )}
          {!settingsLoading && settingsSnapshot && settingsSnapshot.fields.length > 0 && (
            <OwnerOptionsForm
              ownerKind="plugin"
              ownerId={settingsSnapshot.plugin_id}
              title={__("Plugin settings ({{value1}})", { value1: settingsTargetPlugin.name })}
              fields={settingsSnapshot.fields}
              values={settingsDraft}
              savedValues={settingsSnapshot.values}
              version={settingsSnapshot.version}
              csrfToken={csrfToken ?? null}
              saving={settingsSaving}
              notice={settingsNotice}
              error={settingsError}
              submitLabel={settingsSaving ? __("Saving…") : __("Save settings")}
              testId="plugin-settings-form"
              onChange={(next) => {
                setSettingsDraft(next);
                setSettingsNotice('');
                setSettingsError('');
              }}
              onSubmit={() => void handleSaveSettings()}
              onApiError={handleSettingsApiError}
            />
          )}
        </section>
      ) : (
        <AdminAlert tone="info" className="max-w-2xl">
          {__("Select \"Settings\" on an installed plugin in the Installed plugins tab to configure its options.")}
        </AdminAlert>
      )}
        </AdminTabPanel>

        <AdminTabPanel
          active={activeTab === 'run'}
          id="plugins-panel-run"
          labelledBy="plugins-tab-run"
          className="space-y-4"
        >
      <section className="min-w-0 max-w-2xl space-y-3" aria-labelledby="plugin-run-heading">
        <h3 id="plugin-run-heading" className="text-sm font-semibold text-zinc-950">{__("Test plugin execution")}</h3>
        {selected
          ? <p className="break-all text-xs text-zinc-600">{__("Selected plugin:")}{' '}<strong className="text-zinc-900">{selected.name}</strong> ({selected.id})</p>
          : <p className="text-xs text-zinc-500">{__("No plugin selected to run.")}</p>}
        {selected && !selected.enabled && (
          <p role="status" className="text-xs text-amber-800">{__("The plugin is disabled. You can re-enable it after reviewing its permissions.")}</p>
        )}
        <label htmlFor="plugin-input-json" className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{__("JSON payload")}</span>
          <textarea id="plugin-input-json" rows={4} className="w-full min-w-0 rounded-md border border-zinc-300 bg-white p-2.5 font-mono text-xs text-zinc-900"
            value={pluginInputJson} onChange={event => onPluginInputChange(event.target.value)}
            disabled={busy} placeholder={__("{\"text\": \"Sample data\"}")} />
        </label>
        <AdminButton id="btn-run-plugin" variant="primary" onClick={() => void invoke(__("Running plugin..."), onRunPlugin)}
          disabled={!canRun}>{__("Run plugin")}</AdminButton>
        {pluginRunOutput && (
          <div>
            <p className="mb-1.5 text-xs font-semibold text-zinc-700">{__("Result")}</p>
            <pre id="plugin-output" className="max-h-48 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-lg border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-800">{pluginRunOutput}</pre>
          </div>
        )}
      </section>
      {selected && selected.enabled && (
        <DeclarativePluginUi
          pluginId={selected.id}
          csrfToken={csrfToken ?? null}
          onUnauthorized={onUnauthorized ?? (() => {})}
        />
      )}
        </AdminTabPanel>
      </>}
    </div>
  );
};
