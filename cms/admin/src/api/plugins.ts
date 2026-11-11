import { PluginRunResult, PluginSummary } from '../types.ts';
import { requestJson, requestText } from './client.ts';

export async function getPlugins(signal?: AbortSignal): Promise<PluginSummary[]> {
  return requestJson<PluginSummary[]>('/api/admin/plugins', {
    method: 'GET',
    signal,
    defaultErrorMessage: 'Không thể tải danh sách plugin.',
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
    defaultErrorMessage: 'Lỗi khi cài đặt plugin.',
    oversizeErrorMessage: 'Tập tin ZIP quá lớn (vượt quá giới hạn 5 MiB của plugin).',
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
    defaultErrorMessage: 'Lỗi khi chạy plugin.',
    oversizeErrorMessage: 'Payload quá lớn (vượt quá giới hạn 1 MiB cho JSON input).',
  });
}

export async function disablePlugin(id: string, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/plugins/${encodeURIComponent(id)}/disable`, {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: 'Không thể vô hiệu hóa plugin.',
  });
}

export async function enablePlugin(id: string, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/plugins/${encodeURIComponent(id)}/enable`, {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: 'Không thể kích hoạt plugin.',
  });
}
