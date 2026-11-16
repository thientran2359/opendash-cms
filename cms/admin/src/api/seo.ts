import { requestJson } from './client.ts';

export interface EntryRouteSnapshot {
  entry_version: number;
  custom_path: string | null;
}

export interface SeoMetadataInput {
  title: string | null;
  description: string | null;
  og_image_id: string | null;
  canonical_url: string | null;
  noindex: boolean;
}

export interface EntrySeoSnapshot extends SeoMetadataInput {
  entry_version: number;
}

export interface HomeSeoSnapshot extends SeoMetadataInput {
  version: number;
}

export function getEntryRoute(typeKey: string, id: number): Promise<EntryRouteSnapshot> {
  return requestJson<EntryRouteSnapshot>(
    `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${id}/route`,
    { defaultErrorMessage: 'Không thể tải đường dẫn tùy chỉnh.' },
  );
}

export function putEntryRoute(
  typeKey: string,
  id: number,
  expectedVersion: number,
  customPath: string | null,
  csrfToken: string,
): Promise<EntryRouteSnapshot> {
  return requestJson<EntryRouteSnapshot>(
    `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${id}/route`,
    {
      method: 'PUT',
      csrfToken,
      contentType: 'application/json',
      ifMatchVersion: expectedVersion,
      body: JSON.stringify({ custom_path: customPath }),
      defaultErrorMessage: 'Không thể lưu đường dẫn tùy chỉnh.',
    },
  );
}

export function getEntrySeo(typeKey: string, id: number): Promise<EntrySeoSnapshot> {
  return requestJson<EntrySeoSnapshot>(
    `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${id}/seo`,
    { defaultErrorMessage: 'Không thể tải SEO entry.' },
  );
}

export function putEntrySeo(
  typeKey: string,
  id: number,
  expectedVersion: number,
  input: SeoMetadataInput,
  csrfToken: string,
): Promise<EntrySeoSnapshot> {
  return requestJson<EntrySeoSnapshot>(
    `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${id}/seo`,
    {
      method: 'PUT',
      csrfToken,
      contentType: 'application/json',
      ifMatchVersion: expectedVersion,
      body: JSON.stringify(input),
      defaultErrorMessage: 'Không thể lưu SEO entry.',
    },
  );
}

export function getHomeSeo(): Promise<HomeSeoSnapshot> {
  return requestJson<HomeSeoSnapshot>('/api/admin/seo/home', {
    defaultErrorMessage: 'Không thể tải SEO trang chủ.',
  });
}

export function putHomeSeo(
  expectedVersion: number,
  input: SeoMetadataInput,
  csrfToken: string,
): Promise<HomeSeoSnapshot> {
  return requestJson<HomeSeoSnapshot>('/api/admin/seo/home', {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: expectedVersion,
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể lưu SEO trang chủ.',
  });
}
