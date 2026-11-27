import { __, _x } from '../i18n/index.tsx';
import { PluginRunResult, PluginSettingsSnapshot, PluginSummary } from '../types.ts';
import { requestJson, requestText } from './client.ts';

export async function getPlugins(signal?: AbortSignal): Promise<PluginSummary[]> {
  return requestJson<PluginSummary[]>('/api/admin/plugins', {
    method: 'GET',
    signal,
    defaultErrorMessage: _x("Could not load plugins.", "api.plugins"),
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
    defaultErrorMessage: _x("Could not install the plugin.", "api.plugins"),
    oversizeErrorMessage: __("The ZIP file exceeds the 5 MiB plugin limit."),
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
    defaultErrorMessage: _x("Could not run the plugin.", "api.plugins"),
    oversizeErrorMessage: __("The payload exceeds the 1 MiB JSON input limit."),
  });
}

export async function disablePlugin(id: string, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/plugins/${encodeURIComponent(id)}/disable`, {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: __("Could not disable the plugin."),
  });
}

export async function enablePlugin(id: string, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/plugins/${encodeURIComponent(id)}/enable`, {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: __("Could not enable the plugin."),
  });
}

export async function getPluginUiSchema(
  id: string,
  signal?: AbortSignal,
): Promise<unknown> {
  return requestJson<unknown>(`/api/admin/plugins/${encodeURIComponent(id)}/ui-schema`, {
    method: 'GET',
    signal,
    defaultErrorMessage: __("Could not load the declarative UI."),
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
      expectLifecycleConfirmation: true,
      defaultErrorMessage: __("Could not execute the action."),
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
      expectLifecycleConfirmation: true,
      defaultErrorMessage: __("Could not execute the plugin action."),
    },
  );
}

export async function confirmPluginLifecycleAction(
  id: string,
  actionId: string,
  confirmationId: string,
  decision: 'confirm' | 'cancel',
  csrfToken: string,
  signal?: AbortSignal,
): Promise<PluginRunResult> {
  return requestJson<PluginRunResult>(
    `/api/admin/plugins/${encodeURIComponent(id)}/actions/${encodeURIComponent(actionId)}/confirm-lifecycle`,
    {
      method: 'POST',
      csrfToken,
      contentType: 'application/json',
      body: JSON.stringify({ confirmation_id: confirmationId, decision }),
      signal,
      defaultErrorMessage: __("Could not resolve plugin lifecycle confirmation."),
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
      defaultErrorMessage: __("Could not load plugin settings."),
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
      defaultErrorMessage: __("Could not save plugin settings."),
    },
  );
}
