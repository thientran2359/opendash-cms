import { __ } from '../i18n/index.tsx';
import type { BlockCatalogItemDto } from '../types.ts';
import type { ContentMode } from '../features/content/types.ts';
import type { BuilderDocument } from '../builder/document.ts';
import { requestBlob, requestJson } from './client.ts';

export interface BuilderDiagnostic { node_id: string; code: string }
export interface BlockDiagnostic {
  node_id: string;
  code: string;
  message: string;
}
export interface HomepagePreview {
  html: string;
  diagnostics: BuilderDiagnostic[];
}

export interface EntryBuilderOpen {
  entry_id: number;
  type_key: string;
  content_mode: ContentMode;
  version: number;
  updated_at: string;
  document: BuilderDocument | null;
  publication_status: 'draft' | 'published' | string;
  publication_policy: 'immediate' | 'draft_publish';
  diagnostics: BlockDiagnostic[];
  had_freeform_wrap: boolean;
  had_recovery: boolean;
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
    defaultErrorMessage: __("Could not load site template overrides."),
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
    defaultErrorMessage: __("Could not save the site template override."),
  });
}

export interface TemplatePreviewEntry {
  type_key: string;
  entry_id: number;
}

export interface TypedTemplateResetResult {
  reset_id: number;
  canonical_target_key: string;
  next_effective_source: TemplateEffectiveSource;
}

export function resetTemplateOverride(id: number, version: number, csrfToken: string): Promise<TypedTemplateResetResult> {
  return requestJson(`/api/admin/template-overrides/${encodeURIComponent(id)}/reset`, {
    method: 'POST', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: __("Could not reset the site template override."),
  });
}

const entryBuilderPath = (typeKey: string, id: number): string =>
  `/api/admin/entries/${encodeURIComponent(typeKey)}/${encodeURIComponent(id)}/builder`;

export function getEntryBuilder(typeKey: string, id: number): Promise<EntryBuilderOpen> {
  return requestJson(entryBuilderPath(typeKey, id), {
    defaultErrorMessage: __("Could not open the entry builder."),
  });
}

export function previewEntryBuilder(
  typeKey: string, id: number, document: BuilderDocument, csrfToken: string, signal?: AbortSignal,
): Promise<HomepagePreview> {
  return requestJson(entryBuilderPath(typeKey, id) + '/preview', {
    method: 'POST', csrfToken, contentType: 'application/json',
    body: JSON.stringify(document), signal,
    defaultErrorMessage: __("Could not preview the entry builder."),
  });
}

export function saveEntryBuilder(
  typeKey: string, id: number, document: BuilderDocument, version: number, csrfToken: string,
): Promise<EntryBuilderOpen> {
  return requestJson(entryBuilderPath(typeKey, id), {
    method: 'PUT', csrfToken, contentType: 'application/json', ifMatchVersion: version,
    body: JSON.stringify(document),
    defaultErrorMessage: __("Could not save the entry builder."),
  });
}

/** Unified Core/Theme/Plugin catalog. Templates are never sent to Admin. */
export async function getBlockSchemas(): Promise<BlockCatalogItemDto[]> {
  const response = await requestJson<{ items: BlockCatalogItemDto[] }>('/api/admin/builder/blocks', {
    method: 'GET',
    defaultErrorMessage: __("Could not load blocks for the page builder."),
  });
  return response.items;
}

export function getPrivatePreviewMedia(path: string, signal: AbortSignal): Promise<Blob> {
  return requestBlob(path, {
    signal,
    defaultErrorMessage: __("Could not load the private preview image."),
  });
}
