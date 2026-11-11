import { requestJson } from './client.ts';

export interface ThemeItem {
  id: string;
  compatible_block_id: string | null;
}

export interface ThemesSummary {
  items: ThemeItem[];
  active_id: string | null;
  version: number;
  homepage_compatible: boolean | null;
}

export function getThemes(): Promise<ThemesSummary> {
  return requestJson('/api/admin/themes', {
    defaultErrorMessage: 'Không thể tải danh sách theme.',
  });
}

export function installTheme(file: File, csrfToken: string): Promise<ThemeItem> {
  return requestJson('/api/admin/themes', {
    method: 'POST',
    body: file,
    contentType: 'application/zip',
    csrfToken,
    defaultErrorMessage: 'Không thể cài theme ZIP.',
  });
}

export function activateTheme(id: string, version: number, csrfToken: string): Promise<ThemesSummary> {
  return requestJson('/api/admin/themes/active', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ id }),
    defaultErrorMessage: 'Không thể kích hoạt theme.',
  });
}
