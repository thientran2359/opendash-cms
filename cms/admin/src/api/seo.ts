import { __ } from '../i18n/index.tsx';
import { requestJson } from './client.ts';

export interface EntryRouteSnapshot {
  entry_version: number;
  custom_path: string | null;
  /** Server-resolved canonical public path; absent until backend DTO supports it. */
  public_url?: string | null;
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
    { defaultErrorMessage: __("Could not load the custom permalink.") },
  );
}

export function getEntrySeo(typeKey: string, id: number): Promise<EntrySeoSnapshot> {
  return requestJson<EntrySeoSnapshot>(
    `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${id}/seo`,
    { defaultErrorMessage: __("Could not load entry SEO.") },
  );
}

export function getHomeSeo(locale?: string | null): Promise<HomeSeoSnapshot> {
  const query = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson<HomeSeoSnapshot>(`/api/admin/seo/home${query}`, {
    defaultErrorMessage: __("Could not load homepage SEO."),
  });
}

export function putHomeSeo(
  expectedVersion: number,
  input: SeoMetadataInput,
  csrfToken: string,
  locale?: string | null,
): Promise<HomeSeoSnapshot> {
  const query = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson<HomeSeoSnapshot>(`/api/admin/seo/home${query}`, {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: expectedVersion,
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not save homepage SEO."),
  });
}
