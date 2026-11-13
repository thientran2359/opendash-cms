import type { BlockRegistry } from '../types.ts';
import type { BuilderDocument } from '../builder/document.ts';
import { requestBlob, requestJson } from './client.ts';

export interface BuilderDiagnostic { node_id: string; code: string }
export interface HomepageOpen {
  document: BuilderDocument;
  source: 'empty' | 'legacy' | 'layout';
  version: number;
}
export interface HomepagePreview {
  html: string;
  diagnostics: BuilderDiagnostic[];
}
export interface HomepageSaved extends HomepageOpen, HomepagePreview {}

export type OverrideRole = 'page' | 'header' | 'footer' | 'archive' | 'single' | 'taxonomy_archive';

export interface OverrideOpen {
  exists: boolean;
  role: OverrideRole;
  scope_type_key: string | null;
  scope_entry_id: number | null;
  document: BuilderDocument;
  version: number;
  base_theme_id: string | null;
  base_revision_id: string | null;
}

export interface OverrideSaved extends HomepagePreview {
  role: OverrideRole;
  scope_type_key: string | null;
  scope_entry_id: number | null;
  document: BuilderDocument;
  version: number;
  base_theme_id: string | null;
  base_revision_id: string | null;
}

function overrideQuery(typeKey?: string | null, entryId?: number | null): string {
  const params = new URLSearchParams();
  if (typeKey) params.set('type_key', typeKey);
  if (entryId) params.set('entry_id', String(entryId));
  const qs = params.toString();
  return qs ? '?' + qs : '';
}

export function getOverride(role: OverrideRole, typeKey?: string | null, entryId?: number | null): Promise<OverrideOpen> {
  return requestJson('/api/admin/builder/overrides/' + role + overrideQuery(typeKey, entryId), {
    method: 'GET',
    defaultErrorMessage: 'Không thể mở template override.',
  });
}

export function previewOverride(
  role: OverrideRole,
  document: BuilderDocument,
  csrfToken: string,
  typeKey?: string | null,
  entryId?: number | null,
  signal?: AbortSignal,
): Promise<HomepagePreview> {
  return requestJson('/api/admin/builder/overrides/' + role + '/preview' + overrideQuery(typeKey, entryId), {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(document),
    signal,
    defaultErrorMessage: 'Không thể xem trước template override.',
  });
}

export function saveOverride(
  role: OverrideRole,
  document: BuilderDocument,
  version: number,
  csrfToken: string,
  typeKey?: string | null,
  entryId?: number | null,
): Promise<OverrideSaved> {
  return requestJson('/api/admin/builder/overrides/' + role + overrideQuery(typeKey, entryId), {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: version,
    body: JSON.stringify(document),
    defaultErrorMessage: 'Không thể lưu template override.',
  });
}

export function resetOverride(
  role: OverrideRole,
  version: number,
  csrfToken: string,
  typeKey?: string | null,
  entryId?: number | null,
): Promise<{ reset: boolean; role: string }> {
  return requestJson('/api/admin/builder/overrides/' + role + '/reset' + overrideQuery(typeKey, entryId), {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: version,
    body: JSON.stringify({ confirm: true }),
    defaultErrorMessage: 'Không thể reset template override.',
  });
}

export async function getRegistry(): Promise<BlockRegistry> {
  return requestJson<BlockRegistry>('/registry', {
    method: 'GET',
    defaultErrorMessage: 'Không thể tải danh sách block.',
  });
}

export function getHomepage(): Promise<HomepageOpen> {
  return requestJson('/api/admin/builder/homepage', {
    method: 'GET',
    defaultErrorMessage: 'Không thể mở homepage.',
  });
}

export function previewHomepage(document: BuilderDocument, csrfToken: string, signal?: AbortSignal): Promise<HomepagePreview> {
  return requestJson('/api/admin/builder/homepage/preview', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(document),
    signal,
    defaultErrorMessage: 'Không thể xem trước homepage.',
  });
}

export function saveHomepage(document: BuilderDocument, version: number, csrfToken: string): Promise<HomepageSaved> {
  return requestJson('/api/admin/builder/homepage', {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: version,
    body: JSON.stringify(document),
    defaultErrorMessage: 'Không thể lưu homepage.',
  });
}

export function getPrivatePreviewMedia(path: string, signal: AbortSignal): Promise<Blob> {
  return requestBlob(path, {
    signal,
    defaultErrorMessage: 'Không tải được ảnh riêng tư cho Preview.',
  });
}
