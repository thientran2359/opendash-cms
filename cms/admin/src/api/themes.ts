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

export type ThemeOptionType = 'string' | 'number' | 'boolean' | 'select' | 'image_ref';

export interface ThemeOptionChoice {
  value: string;
  label: string;
}

export interface ThemeOptionDef {
  key: string;
  label: string;
  type: ThemeOptionType;
  default: unknown;
  public: boolean;
  min?: number;
  max?: number;
  choices?: ThemeOptionChoice[];
}

export interface ThemeOptionsSnapshot {
  theme_id: string;
  version: number;
  schema: ThemeOptionDef[];
  values: Record<string, unknown>;
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

export function getThemeOptions(themeId: string): Promise<ThemeOptionsSnapshot> {
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/options`, {
    defaultErrorMessage: 'Không thể tải tùy chọn theme.',
  });
}

export function saveThemeOptions(
  themeId: string,
  values: Record<string, unknown>,
  version: number,
  csrfToken: string,
): Promise<ThemeOptionsSnapshot> {
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/options`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ values }),
    defaultErrorMessage: 'Không thể lưu tùy chọn theme.',
  });
}
