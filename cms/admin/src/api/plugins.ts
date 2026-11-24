import { tStatic } from '../i18n/index.tsx';
import { PluginRunResult, PluginSettingsSnapshot, PluginSummary } from '../types.ts';
import { requestJson, requestText } from './client.ts';

export async function getPlugins(signal?: AbortSignal): Promise<PluginSummary[]> {
  return requestJson<PluginSummary[]>('/api/admin/plugins', {
    method: 'GET',
    signal,
    defaultErrorMessage: tStatic('api.plugins.load_plugins'),
  });
}

export async function installPlugin(
  file: File,
  csrfToken: string,
): Promise<PluginSummary> {
  return requestJson<PluginSummary>('/api/admin/plugins', {
    method: 'POST',
    csrfToken,
    contentType: 'application/zip',
    body: file,
    defaultErrorMessage: tStatic('api.plugins.install_the_plugin'),
    oversizeErrorMessage: tStatic('api.plugins.the_zip_file_exceeds_the_5_mib_plugin_limit'),
  });
}

export async function runPlugin(
  id: string,
  inputJson: string,
  csrfToken: string,
): Promise<PluginRunResult> {
  return requestJson<PluginRunResult>(`/api/admin/plugins/${encodeURIComponent(id)}/run`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: inputJson || '{}',
    defaultErrorMessage: tStatic('api.plugins.run_the_plugin'),
    oversizeErrorMessage: tStatic('api.plugins.the_payload_exceeds_the_1_mib_json_input_limit'),
  });
}

export async function disablePlugin(id: string, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/plugins/${encodeURIComponent(id)}/disable`, {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: tStatic('api.plugins.disable_the_plugin'),
  });
}

export async function enablePlugin(id: string, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/plugins/${encodeURIComponent(id)}/enable`, {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: tStatic('api.plugins.enable_the_plugin'),
  });
}

export async function getPluginUiSchema(
  id: string,
  signal?: AbortSignal,
): Promise<unknown> {
  return requestJson<unknown>(`/api/admin/plugins/${encodeURIComponent(id)}/ui-schema`, {
    method: 'GET',
    signal,
    defaultErrorMessage: tStatic('api.plugins.load_the_declarative_ui'),
  });
}

export async function invokePluginAction(
  id: string,
  action: string,
  payload: unknown,
  csrfToken: string | null,
  signal?: AbortSignal,
): Promise<unknown> {
  return requestJson<unknown>(
    `/api/admin/plugins/${encodeURIComponent(id)}/actions/${encodeURIComponent(action)}`,
    {
      method: 'POST',
      csrfToken: csrfToken ?? undefined,
      contentType: 'application/json',
      body: JSON.stringify(payload ?? {}),
      signal,
      defaultErrorMessage: tStatic('api.plugins.execute_the_action'),
    },
  );
}


export async function runPluginAction(
  id: string,
  actionId: string,
  payload: unknown,
  csrfToken: string,
): Promise<PluginRunResult> {
  return requestJson<PluginRunResult>(
    `/api/admin/plugins/${encodeURIComponent(id)}/actions/${encodeURIComponent(actionId)}`,
    {
      method: 'POST',
      csrfToken,
      contentType: 'application/json',
      body: JSON.stringify(payload ?? {}),
      defaultErrorMessage: tStatic('api.plugins.execute_the_plugin_action'),
    },
  );
}

export function getPluginSettings(
  id: string,
  signal?: AbortSignal,
): Promise<PluginSettingsSnapshot> {
  return requestJson<PluginSettingsSnapshot>(
    `/api/admin/plugins/${encodeURIComponent(id)}/settings`,
    {
      signal,
      defaultErrorMessage: tStatic('api.plugins.load_plugin_settings'),
    },
  );
}

export function savePluginSettings(
  id: string,
  values: Record<string, unknown>,
  version: number,
  csrfToken: string,
): Promise<PluginSettingsSnapshot> {
  return requestJson<PluginSettingsSnapshot>(
    `/api/admin/plugins/${encodeURIComponent(id)}/settings`,
    {
      method: 'PUT',
      csrfToken,
      ifMatchVersion: version,
      contentType: 'application/json',
      body: JSON.stringify({ values }),
      defaultErrorMessage: tStatic('api.plugins.save_plugin_settings'),
    },
  );
}
