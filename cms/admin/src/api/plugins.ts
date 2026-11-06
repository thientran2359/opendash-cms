import { PluginRunResult, PluginSummary } from '../types.ts';
import { ApiClientError, handleResponseText } from './client.ts';

export async function getPlugins(): Promise<PluginSummary[]> {
  const res = await fetch('/api/admin/plugins', {
    method: 'GET',
    credentials: 'same-origin',
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiClientError(res.status, text || 'Không thể tải danh sách plugin.');
  }
  return res.json();
}

export async function installPlugin(
  file: File,
  csrfToken: string,
): Promise<PluginSummary> {
  const res = await fetch('/api/admin/plugins', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/zip',
      'X-CSRF-Token': csrfToken,
    },
    body: file,
  });
  if (!res.ok) {
    const text = await res.text();
    let msg = text;
    try {
      const errObj = JSON.parse(text);
      if (errObj && errObj.error) {
        msg = errObj.error;
      }
    } catch {
      // not JSON
    }
    if (res.status === 413) {
      msg = 'Tập tin ZIP quá lớn (vượt quá giới hạn 5 MiB của plugin).';
    }
    throw new ApiClientError(res.status, msg || `Lỗi HTTP ${res.status} khi cài đặt plugin.`);
  }
  const text = await res.text();
  try {
    return JSON.parse(text);
  } catch {
    throw new ApiClientError(res.status, 'Phản hồi từ máy chủ không phải là JSON hợp lệ.');
  }
}

export async function runPlugin(
  id: string,
  inputJson: string,
  csrfToken: string,
): Promise<PluginRunResult> {
  const res = await fetch(`/api/admin/plugins/${encodeURIComponent(id)}/run`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrfToken,
    },
    body: inputJson || '{}',
  });
  if (!res.ok) {
    const text = await res.text();
    let msg = text;
    try {
      const errObj = JSON.parse(text);
      if (errObj && errObj.error) {
        msg = errObj.error;
      }
    } catch {
      // not JSON
    }
    if (res.status === 413) {
      msg = 'Payload quá lớn (vượt quá giới hạn 1 MiB cho JSON input).';
    }
    throw new ApiClientError(res.status, msg || `Lỗi HTTP ${res.status} khi chạy plugin.`);
  }
  const text = await res.text();
  try {
    return JSON.parse(text);
  } catch {
    throw new ApiClientError(res.status, 'Phản hồi từ máy chủ không phải là JSON hợp lệ.');
  }
}

export async function disablePlugin(id: string, csrfToken: string): Promise<void> {
  const res = await fetch(`/api/admin/plugins/${encodeURIComponent(id)}/disable`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'X-CSRF-Token': csrfToken,
    },
  });
  await handleResponseText(res, 'Không thể vô hiệu hóa plugin.');
}

export async function enablePlugin(id: string, csrfToken: string): Promise<void> {
  const res = await fetch(`/api/admin/plugins/${encodeURIComponent(id)}/enable`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'X-CSRF-Token': csrfToken,
    },
  });
  await handleResponseText(res, 'Không thể kích hoạt plugin.');
}
