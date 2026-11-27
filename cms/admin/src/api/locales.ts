import { __ } from '../i18n/index.tsx';
import { requestJson } from './client.ts';

export type LocaleDirection = 'ltr' | 'rtl';

export interface SiteLocale {
  code: string;
  bcp47_tag: string;
  label: string;
  direction: LocaleDirection;
  is_default: boolean;
  enabled: boolean;
  position: number;
}

export interface SiteLocalesSnapshot {
  version: number;
  multilingual_enabled: boolean;
  max_locales: number;
  default_code: string;
  items: SiteLocale[];
}

export interface CreateLocaleInput {
  code: string;
  label: string;
  direction?: LocaleDirection;
}

export interface UpdateLocaleInput {
  label: string;
  direction: LocaleDirection;
  enabled: boolean;
  position: number;
}

export interface AdminLocaleState {
  allLocales: SiteLocale[];
  enabledLocales: SiteLocale[];
  defaultLocale: SiteLocale | null;
  version: number;
  maxLocales: number;
  multilingualEnabled: boolean;
  hasStoredSecondaryLocales: boolean;
  hasMultipleLocales: boolean;
  hasMultiplePublicLocales: boolean;
}

export function deriveAdminLocaleState(snapshot: SiteLocalesSnapshot): AdminLocaleState {
  const allLocales = snapshot.items;
  const enabledLocales = allLocales.filter((item) => item.enabled);
  const defaultLocale =
    allLocales.find((item) => item.is_default) ??
    allLocales.find((item) => item.code === snapshot.default_code) ??
    allLocales[0] ??
    null;
  return {
    allLocales,
    enabledLocales,
    defaultLocale,
    version: snapshot.version,
    maxLocales: snapshot.max_locales,
    multilingualEnabled: snapshot.multilingual_enabled,
    hasStoredSecondaryLocales: allLocales.length > 1,
    hasMultipleLocales: snapshot.multilingual_enabled && allLocales.length > 1,
    hasMultiplePublicLocales: snapshot.multilingual_enabled && enabledLocales.length > 1,
  };
}

export const EMPTY_ADMIN_LOCALE_STATE: AdminLocaleState = {
  allLocales: [],
  enabledLocales: [],
  defaultLocale: null,
  version: 1,
  maxLocales: 20,
  multilingualEnabled: false,
  hasStoredSecondaryLocales: false,
  hasMultipleLocales: false,
  hasMultiplePublicLocales: false,
};

export function listLocales(signal?: AbortSignal): Promise<SiteLocalesSnapshot> {
  return requestJson<SiteLocalesSnapshot>('/api/admin/locales', {
    signal,
    defaultErrorMessage: __("Could not load content languages."),
  });
}

export function createLocale(
  input: CreateLocaleInput,
  expectedVersion: number,
  csrfToken: string,
): Promise<SiteLocalesSnapshot> {
  return requestJson<SiteLocalesSnapshot>('/api/admin/locales', {
    method: 'POST',
    csrfToken,
    ifMatchVersion: expectedVersion,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not add the language."),
  });
}

export function updateLocale(
  code: string,
  input: UpdateLocaleInput,
  expectedVersion: number,
  csrfToken: string,
): Promise<SiteLocalesSnapshot> {
  return requestJson<SiteLocalesSnapshot>(`/api/admin/locales/${encodeURIComponent(code)}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: expectedVersion,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not update the language."),
  });
}

export function setDefaultLocale(
  code: string,
  expectedVersion: number,
  csrfToken: string,
): Promise<SiteLocalesSnapshot> {
  return requestJson<SiteLocalesSnapshot>(
    `/api/admin/locales/${encodeURIComponent(code)}/set-default`,
    {
      method: 'POST',
      csrfToken,
      ifMatchVersion: expectedVersion,
      contentType: 'application/json',
      body: JSON.stringify({}),
      defaultErrorMessage: __("Could not set the default language."),
    },
  );
}

export interface DeleteLocaleResponse {
  snapshot: SiteLocalesSnapshot;
  deleted_code: string;
  soft_deleted: boolean;
  orphan_total: number;
}

export interface OrphanedLocaleCounts {
  entries: number;
  terms: number;
  settings: number;
  menu_slots: number;
  routes: number;
  projections: number;
  media_seo_refs: number;
  total: number;
}

export interface OrphanedLocaleSummary {
  code: string;
  bcp47_tag: string;
  label: string;
  deleted_at: string;
  counts: OrphanedLocaleCounts;
}

export interface OrphanedLocalesSnapshot {
  version: number;
  items: OrphanedLocaleSummary[];
}

export interface PurgeOrphanLocaleResult {
  snapshot: OrphanedLocalesSnapshot;
  purged_code: string;
  deleted: OrphanedLocaleCounts;
}

export function setMultilingualEnabled(
  enabled: boolean, expectedVersion: number, csrfToken: string,
): Promise<SiteLocalesSnapshot> {
  return requestJson<SiteLocalesSnapshot>('/api/admin/locales/multilingual', {
    method: 'PUT', csrfToken, ifMatchVersion: expectedVersion,
    contentType: 'application/json', body: JSON.stringify({ enabled }),
    defaultErrorMessage: __("Could not change multilingual support."),
  });
}

export function deleteLocale(
  code: string, expectedVersion: number, csrfToken: string,
): Promise<DeleteLocaleResponse> {
  return requestJson<DeleteLocaleResponse>(`/api/admin/locales/${encodeURIComponent(code)}`, {
    method: 'DELETE', csrfToken, ifMatchVersion: expectedVersion,
    defaultErrorMessage: __("Could not delete the language."),
  });
}

export function listOrphanedLocales(signal?: AbortSignal): Promise<OrphanedLocalesSnapshot> {
  return requestJson<OrphanedLocalesSnapshot>('/api/admin/locales/orphans', {
    signal, defaultErrorMessage: __("Could not load orphaned language data."),
  });
}

export function purgeOrphanedLocale(
  code: string, confirmCode: string, expectedVersion: number, csrfToken: string,
): Promise<PurgeOrphanLocaleResult> {
  return requestJson<PurgeOrphanLocaleResult>(`/api/admin/locales/orphans/${encodeURIComponent(code)}/purge`, {
    method: 'POST', csrfToken, ifMatchVersion: expectedVersion,
    contentType: 'application/json', body: JSON.stringify({ confirm_code: confirmCode }),
    defaultErrorMessage: __("Could not purge orphaned language data."),
  });
}

export type L10nPackScope = 'core' | 'theme' | 'plugin';

export interface LanguagePackSummary {
  locale: string;
  source: string;
  entries: number;
  coverage_pct: number;
  has_override: boolean;
  etag: string;
}

export interface LanguageDomainSummary {
  scope: L10nPackScope;
  domain: string;
  name: string;
  reference_count: number | null;
  packs: LanguagePackSummary[];
}

export interface LanguagePacksListResponse {
  domains: LanguageDomainSummary[];
}

export interface UploadLanguagePackInput {
  scope: L10nPackScope;
  domain: string;
  locale: string;
  filename: string;
  content_base64: string;
}

export function listLanguagePacks(signal?: AbortSignal): Promise<LanguagePacksListResponse> {
  return requestJson<LanguagePacksListResponse>('/api/admin/l10n/packs', {
    signal,
    defaultErrorMessage: __("Could not load language packs."),
  });
}

export function uploadLanguagePack(
  input: UploadLanguagePackInput,
  csrfToken: string,
): Promise<{ ok: boolean; scope: string; domain: string; locale: string; entries: number }> {
  return requestJson('/api/admin/l10n/packs/upload', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not upload language pack."),
  });
}

export function deleteLanguagePack(
  scope: L10nPackScope,
  domain: string,
  locale: string,
  csrfToken: string,
): Promise<{ ok: boolean }> {
  return requestJson(
    `/api/admin/l10n/packs/${encodeURIComponent(scope)}/${encodeURIComponent(domain)}/${encodeURIComponent(locale)}`,
    {
      method: 'DELETE',
      csrfToken,
      defaultErrorMessage: __("Could not delete language pack."),
    },
  );
}
