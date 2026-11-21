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

/** EGM S02: canonical typed templates replace the old role/scope endpoints.
 * The only safe persisted writer is PUT /api/admin/template-overrides with
 * optimistic If-Match, and reset is scoped to the immutable row ID. */
export type TemplateCandidateKind =
  | 'home' | 'page' | 'page_layout' | 'single' | 'single_custom'
  | 'archive' | 'taxonomy' | 'singular' | 'index' | 'not_found' | 'part';

export interface TemplateTargetInput {
  kind: TemplateCandidateKind;
  content_type?: string;
  taxonomy?: string;
  selector_slug?: string;
  entry_id?: number;
  term_id?: number;
  custom_key?: string;
  layout_key?: string;
  part_key?: string;
}

export interface TemplateEffectiveSource {
  kind: 'db_override' | 'active_theme' | 'core_fallback';
  label: string;
  target_key: string | null;
}

export interface TypedTemplateOverride {
  id: number;
  target_kind: TemplateCandidateKind;
  canonical_target_key: string;
  target: TemplateTargetInput;
  ast_json: BuilderDocument;
  version: number;
  base_theme_id: string | null;
  base_revision_digest: string | null;
  effective_source?: TemplateEffectiveSource;
  updated_at: string;
}

export function getTemplateOverrides(): Promise<{ items: TypedTemplateOverride[] }> {
  return requestJson('/api/admin/template-overrides', {
    defaultErrorMessage: 'Không thể tải site template overrides.',
  });
}

export function putTemplateOverride(
  target: TemplateTargetInput,
  document: BuilderDocument,
  csrfToken: string,
  expectedVersion?: number,
): Promise<TypedTemplateOverride> {
  return requestJson('/api/admin/template-overrides', {
    method: 'PUT', csrfToken, contentType: 'application/json',
    ifMatchVersion: expectedVersion,
    body: JSON.stringify({ target, document }),
    defaultErrorMessage: 'Không thể lưu site template override.',
  });
}

export interface TypedTemplateResetResult {
  reset_id: number;
  canonical_target_key: string;
  next_effective_source: TemplateEffectiveSource;
}

export function resetTemplateOverride(id: number, version: number, csrfToken: string): Promise<TypedTemplateResetResult> {
  return requestJson(`/api/admin/template-overrides/${encodeURIComponent(id)}/reset`, {
    method: 'POST', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: 'Không thể reset site template override.',
  });
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
