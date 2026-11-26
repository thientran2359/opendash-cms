import { tStatic } from '../i18n/index.tsx';
import type { FieldSpec } from '../fields/types.ts';
import { requestJson } from './client.ts';

/** SITE-01. All values originate from the server's typed singleton. */
export interface GeneralSettings {
  site_title: string;
  tagline: string;
  timezone: string;
  cms_locale: string;
  version: number;
  has_override?: boolean;
  locale?: string;
}

export type GeneralSettingsDraft = Pick<GeneralSettings, 'site_title' | 'tagline' | 'timezone'> & { cms_locale?: string };

export interface TimezoneCatalog {
  version: string;
  items: string[];
}

function localeQuery(locale?: string | null): string {
  return locale ? `?locale=${encodeURIComponent(locale)}` : '';
}

export function getGeneralSettings(
  signal?: AbortSignal,
  locale?: string | null,
): Promise<GeneralSettings> {
  return requestJson<GeneralSettings>(`/api/admin/settings/general${localeQuery(locale)}`, {
    signal,
    defaultErrorMessage: tStatic("api.settings.load_general"),
  });
}

export function getTimezoneCatalog(signal?: AbortSignal): Promise<TimezoneCatalog> {
  return requestJson<TimezoneCatalog>('/api/admin/settings/timezones', {
    signal,
    defaultErrorMessage: tStatic("api.settings.load_timezones"),
  });
}

export function saveGeneralSettings(
  draft: GeneralSettingsDraft,
  version: number,
  csrfToken: string,
  locale?: string | null,
): Promise<GeneralSettings> {
  return requestJson<GeneralSettings>(`/api/admin/settings/general${localeQuery(locale)}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(draft),
    defaultErrorMessage: tStatic("api.settings.save_general"),
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
    defaultErrorMessage: tStatic("api.settings.load_public"),
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
    defaultErrorMessage: tStatic("api.settings.save_public"),
  });
}

/** SITE-02/C3: the server owns the version and every canonical route claim. */
export interface PermalinkSettings {
  version: number;
  post_structure: string;
  content_type_bases: Record<string, string | null>;
  taxonomy_bases: Record<string, string | null>;
}

export type PermalinkSettingsDraft = Omit<PermalinkSettings, 'version'>;

export function getPermalinkSettings(signal?: AbortSignal): Promise<PermalinkSettings> {
  return requestJson<PermalinkSettings>('/api/admin/settings/permalinks', {
    signal,
    defaultErrorMessage: tStatic("api.settings.load_permalinks"),
  });
}

export function savePermalinkSettings(
  draft: PermalinkSettingsDraft,
  expectedVersion: number,
  csrfToken: string,
): Promise<PermalinkSettings> {
  return requestJson<PermalinkSettings>('/api/admin/settings/permalinks', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: expectedVersion,
    contentType: 'application/json',
    body: JSON.stringify(draft),
    defaultErrorMessage: tStatic("api.settings.save_permalinks"),
  });
}

/** EGM S03. Both assignments share one server-owned CAS version and transaction. */
export type ReadingPageStatus = 'unconfigured' | 'published' | 'draft' | 'deleted';

export interface ReadingSettings {
  homepage_page_id: number | null;
  posts_page_id: number | null;
  homepage_status: ReadingPageStatus;
  posts_page_status: ReadingPageStatus;
  version: number;
  locale?: string;
}

export type ReadingSettingsDraft = Pick<ReadingSettings, 'homepage_page_id' | 'posts_page_id'>;

export function getReadingSettings(
  signal?: AbortSignal,
  locale?: string | null,
): Promise<ReadingSettings> {
  return requestJson<ReadingSettings>(`/api/admin/settings/reading${localeQuery(locale)}`, {
    signal,
    defaultErrorMessage: tStatic("api.settings.load_reading"),
  });
}

export function saveReadingSettings(
  draft: ReadingSettingsDraft,
  version: number,
  csrfToken: string,
  locale?: string | null,
): Promise<ReadingSettings> {
  return requestJson<ReadingSettings>(`/api/admin/settings/reading${localeQuery(locale)}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(draft),
    defaultErrorMessage: tStatic("api.settings.save_reading"),
  });
}

export interface GlobalSiteOptionsSnapshot {
  version: number;
  fields: FieldSpec[];
  values: Record<string, unknown>;
}

export function getGlobalSiteOptions(signal?: AbortSignal): Promise<GlobalSiteOptionsSnapshot> {
  return requestJson<GlobalSiteOptionsSnapshot>('/api/admin/settings/site-options', {
    signal,
    defaultErrorMessage: tStatic("api.settings.load_site_options"),
  });
}

export function saveGlobalSiteOptions(
  values: Record<string, unknown>,
  version: number,
  csrfToken: string,
): Promise<GlobalSiteOptionsSnapshot> {
  return requestJson<GlobalSiteOptionsSnapshot>('/api/admin/settings/site-options', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ values }),
    defaultErrorMessage: tStatic("api.settings.save_site_options"),
  });
}
