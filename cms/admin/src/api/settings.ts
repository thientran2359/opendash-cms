import { requestJson } from './client.ts';

/** SITE-01. All values originate from the server's typed singleton. */
export interface GeneralSettings {
  site_title: string;
  tagline: string;
  timezone: string;
  version: number;
}

export type GeneralSettingsDraft = Pick<GeneralSettings, 'site_title' | 'tagline' | 'timezone'>;

export interface TimezoneCatalog {
  version: string;
  items: string[];
}

export function getGeneralSettings(signal?: AbortSignal): Promise<GeneralSettings> {
  return requestJson<GeneralSettings>('/api/admin/settings/general', {
    signal,
    defaultErrorMessage: 'Không tải được cài đặt website.',
  });
}

export function getTimezoneCatalog(signal?: AbortSignal): Promise<TimezoneCatalog> {
  return requestJson<TimezoneCatalog>('/api/admin/settings/timezones', {
    signal,
    defaultErrorMessage: 'Không tải được danh sách múi giờ.',
  });
}

export function saveGeneralSettings(
  draft: GeneralSettingsDraft,
  version: number,
  csrfToken: string,
): Promise<GeneralSettings> {
  return requestJson<GeneralSettings>('/api/admin/settings/general', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(draft),
    defaultErrorMessage: 'Không lưu được cài đặt website.',
  });
}
