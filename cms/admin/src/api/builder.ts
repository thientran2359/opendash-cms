import type { BlockRegistry } from '../types.ts';
import type { ContentMode } from '../features/content/types.ts';
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

export type OverrideRole = 'page' | 'header' | 'footer' | 'archive' | 'single' | 'taxonomy_archive';

export interface EntryBuilderOpen {
  entry_id: number;
  type_key: string;
  content_mode: ContentMode;
  version: number;
  document: BuilderDocument | null;
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

export interface TemplatePreviewEntry {
  type_key: string;
  entry_id: number;
}

export function previewTemplateOverride(
  target: TemplateTargetInput,
  document: BuilderDocument,
  csrfToken: string,
  previewEntry?: TemplatePreviewEntry,
  expectedVersion?: number,
  signal?: AbortSignal,
): Promise<HomepagePreview> {
  return requestJson('/api/admin/template-overrides/preview', {
    method: 'POST', csrfToken, contentType: 'application/json',
    ifMatchVersion: expectedVersion, signal,
    body: JSON.stringify({ target, document, preview_entry: previewEntry }),
    defaultErrorMessage: 'Không thể xem trước template dùng chung.',
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

const entryBuilderPath = (typeKey: string, id: number): string =>
  `/api/admin/entries/${encodeURIComponent(typeKey)}/${encodeURIComponent(id)}/builder`;

export function getEntryBuilder(typeKey: string, id: number): Promise<EntryBuilderOpen> {
  return requestJson(entryBuilderPath(typeKey, id), {
    defaultErrorMessage: 'Không thể mở Builder của nội dung.',
  });
}

export function previewEntryBuilder(
  typeKey: string, id: number, document: BuilderDocument, csrfToken: string, signal?: AbortSignal,
): Promise<HomepagePreview> {
  return requestJson(entryBuilderPath(typeKey, id) + '/preview', {
    method: 'POST', csrfToken, contentType: 'application/json',
    body: JSON.stringify(document), signal,
    defaultErrorMessage: 'Không thể xem trước Builder của nội dung.',
  });
}

export function saveEntryBuilder(
  typeKey: string, id: number, document: BuilderDocument, version: number, csrfToken: string,
): Promise<EntryBuilderOpen> {
  return requestJson(entryBuilderPath(typeKey, id), {
    method: 'PUT', csrfToken, contentType: 'application/json', ifMatchVersion: version,
    body: JSON.stringify(document),
    defaultErrorMessage: 'Không thể lưu Builder của nội dung.',
  });
}

export function resetEntryBuilder(
  typeKey: string, id: number, version: number, csrfToken: string,
): Promise<EntryBuilderOpen> {
  return requestJson(entryBuilderPath(typeKey, id) + '/reset', {
    method: 'POST', csrfToken, contentType: 'application/json', ifMatchVersion: version,
    body: JSON.stringify({ confirm: true }),
    defaultErrorMessage: 'Không thể reset Builder của nội dung.',
  });
}

export async function getRegistry(): Promise<BlockRegistry> {
  return requestJson<BlockRegistry>('/registry', {
    method: 'GET',
    defaultErrorMessage: 'Không thể tải danh sách block.',
  });
}

export function getPrivatePreviewMedia(path: string, signal: AbortSignal): Promise<Blob> {
  return requestBlob(path, {
    signal,
    defaultErrorMessage: 'Không tải được ảnh riêng tư cho Preview.',
  });
}
