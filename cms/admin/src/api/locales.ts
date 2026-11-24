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
    hasMultipleLocales: allLocales.length > 1,
    hasMultiplePublicLocales: enabledLocales.length > 1,
  };
}

export const EMPTY_ADMIN_LOCALE_STATE: AdminLocaleState = {
  allLocales: [],
  enabledLocales: [],
  defaultLocale: null,
  version: 1,
  maxLocales: 20,
  hasMultipleLocales: false,
  hasMultiplePublicLocales: false,
};

export function listLocales(signal?: AbortSignal): Promise<SiteLocalesSnapshot> {
  return requestJson<SiteLocalesSnapshot>('/api/admin/locales', {
    signal,
    defaultErrorMessage: 'Không thể tải danh sách ngôn ngữ nội dung.',
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
    defaultErrorMessage: 'Không thể thêm ngôn ngữ mới.',
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
    defaultErrorMessage: 'Không thể cập nhật ngôn ngữ.',
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
      defaultErrorMessage: 'Không thể đặt ngôn ngữ mặc định.',
    },
  );
}
