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

/** M7-S01-T01. Site-public settings singleton (independent CAS version). */
export interface PublicSiteSettings {
  public_origin: string | null;
  content_language: string;
  indexing_enabled: boolean;
  version: number;
}

export type PublicSiteSettingsDraft = Pick<
  PublicSiteSettings,
  'public_origin' | 'content_language' | 'indexing_enabled'
>;

export function getPublicSiteSettings(signal?: AbortSignal): Promise<PublicSiteSettings> {
  return requestJson<PublicSiteSettings>('/api/admin/settings/public', {
    signal,
    defaultErrorMessage: 'Không tải được cài đặt công khai.',
  });
}

export function savePublicSiteSettings(
  draft: PublicSiteSettingsDraft,
  version: number,
  csrfToken: string,
): Promise<PublicSiteSettings> {
  return requestJson<PublicSiteSettings>('/api/admin/settings/public', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(draft),
    defaultErrorMessage: 'Không lưu được cài đặt công khai.',
  });
}
