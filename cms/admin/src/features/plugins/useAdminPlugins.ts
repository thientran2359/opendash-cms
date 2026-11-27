import type { PluginSummary } from '../../types.ts';
import * as api from '../../api/index.ts';
import { __, _x } from '../../i18n/index.tsx';

export interface UseAdminPluginsArgs {
  csrfToken: string | null;
  selectedPluginId: string;
  pluginInputJson: string;
  setSelectedPluginId: (id: string) => void;
  setPluginRunOutput: (out: string) => void;
  setPlugins: (items: PluginSummary[]) => void;
  setIsProcessing: (busy: boolean) => void;
  setStatus: (msg: () => string, isError?: boolean) => void;
  handleApiError: (err: unknown, fallback: string) => void;
  loadPlugins: () => Promise<void>;
}

export function useAdminPlugins(args: UseAdminPluginsArgs) {
  const {
    csrfToken,
    selectedPluginId,
    pluginInputJson,
    setSelectedPluginId,
    setPluginRunOutput,
    setIsProcessing,
    setStatus,
    handleApiError,
    loadPlugins,
  } = args;

  const handlePluginUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !csrfToken) return;
    setIsProcessing(true);
    setStatus(() => __('Installing plugin…'), false);
    try {
      const summary = await api.installPlugin(file, csrfToken);
      setStatus(() => __('Installed plugin {{name}} ({{id}}).', { name: summary.name, id: summary.id }), false);
      setSelectedPluginId(summary.id);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, _x('Could not install the plugin.', 'app.install_error'));
    } finally {
      setIsProcessing(false);
      e.target.value = '';
    }
  };

  const handleDisablePlugin = async (id: string) => {
    if (!csrfToken) return;
    setIsProcessing(true);
    try {
      await api.disablePlugin(id, csrfToken);
      setStatus(() => __('Disabled plugin {{id}}.', { id }), false);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, __('Could not disable the plugin.'));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleEnablePlugin = async (id: string) => {
    if (!csrfToken) return;
    setIsProcessing(true);
    try {
      await api.enablePlugin(id, csrfToken);
      setStatus(() => __('Enabled plugin {{id}}.', { id }), false);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, __('Could not enable the plugin.'));
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRunPlugin = async () => {
    if (!selectedPluginId || !csrfToken) return;
    setIsProcessing(true);
    setStatus(() => __('Running plugin {{id}}…', { id: selectedPluginId }), false);
    setPluginRunOutput('');
    try {
      const res = await api.runPlugin(selectedPluginId, pluginInputJson, csrfToken);
      setPluginRunOutput(JSON.stringify(res, null, 2));
      if (res.success) {
        setStatus(() => __('Execution succeeded [correlation: {{id}}]', { id: res.correlation_id }), false);
      } else {
        setStatus(() => __('Plugin error: {{error}} [correlation: {{id}}]', { error: res.error || res.code || '', id: res.correlation_id }), true);
      }
      await loadPlugins();
    } catch (err) {
      const msg = err instanceof Error ? err.message : _x('Could not run the plugin.', 'app.run_error');
      setPluginRunOutput(
        JSON.stringify(
          {
            success: false,
            error: msg,
            status: err instanceof api.ApiClientError ? err.status : undefined,
          },
          null,
          2,
        ),
      );
      handleApiError(err, _x('Could not run the plugin.', 'app.run_error'));
    } finally {
      setIsProcessing(false);
    }
  };

  return {
    handlePluginUpload,
    handleDisablePlugin,
    handleEnablePlugin,
    handleRunPlugin,
  };
}
