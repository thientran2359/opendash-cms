import { __ } from '../i18n/index.tsx';
import type { BuilderDocument } from '../builder/document.ts';
import type {
  ContentEntryRecord,
  ContentFieldValue,
  ContentMode,
} from '../features/content/types.ts';
import type { BlockDiagnostic } from './builder.ts';
import type { ContentPublicationPolicy } from './content.ts';
import { requestJson, requestText } from './client.ts';
import type { EntrySeoSnapshot, SeoMetadataInput } from './seo.ts';

export type RevisionSource =
  | 'create'
  | 'save'
  | 'builder_save'
  | 'builder_reset'
  | 'quick_edit'
  | 'seo_save'
  | 'pattern_save'
  | 'plugin_save'
  | 'restore'
  | 'import'
  | 'baseline';

export interface CanonicalRevisionSeo {
  title: string | null;
  description: string | null;
  og_image_id: string | null;
  canonical_url: string | null;
  noindex: boolean;
}

export type DiffOpKind = 'equal' | 'added' | 'removed' | 'modified';

export interface WordSpan {
  kind: DiffOpKind;
  text: string;
}

export interface DiffLine {
  kind: DiffOpKind;
  line_no_before: number | null;
  line_no_after: number | null;
  text: string;
  word_spans?: WordSpan[];
}

export interface TextDiffHunk {
  header: string;
  lines: DiffLine[];
}

export interface TextDiffResult {
  truncated_diff: boolean;
  hunks: TextDiffHunk[];
  unchanged_collapsed_count: number;
}

export type BlockNodeKind = 'section' | 'row' | 'column' | 'element';
export type BlockDiffKind = 'added' | 'removed' | 'moved' | 'modified';

export interface BlockPropChange {
  prop_path: string;
  before: unknown;
  after: unknown;
}

export interface BlockDiffItem {
  kind: BlockDiffKind;
  node_id: string;
  node_kind: BlockNodeKind;
  block_type: string;
  label: string;
  before_path: string | null;
  after_path: string | null;
  before_order: number | null;
  after_order: number | null;
  prop_changes?: BlockPropChange[];
}

export interface BlockDiffSummary {
  total_nodes_before: number;
  total_nodes_after: number;
  truncated: boolean;
  unchanged_count: number;
  items: BlockDiffItem[];
}

export interface ModeTransition {
  from: ContentMode;
  to: ContentMode;
}

export interface FieldDiffItem {
  key: string;
  label: string;
  field_type: string;
  editable: boolean;
  before: unknown;
  after: unknown;
}

export interface RevisionTermRef {
  id: number;
  name: string;
  taxonomy_key: string | null;
  missing: boolean;
}

export interface TaxonomyDiffSummary {
  added_terms: RevisionTermRef[];
  removed_terms: RevisionTermRef[];
  primary_category_before: RevisionTermRef | null;
  primary_category_after: RevisionTermRef | null;
}

export interface ScalarChange<T> {
  before: T;
  after: T;
}

export interface RevisionDiffPayload {
  title_diff: TextDiffResult;
  mode_transition: ModeTransition | null;
  body_text_diff: TextDiffResult | null;
  body_block_diff: BlockDiffSummary | null;
  field_diffs: FieldDiffItem[];
  taxonomy_diff: TaxonomyDiffSummary;
  layout_diff: ScalarChange<string | null> | null;
  template_diff: ScalarChange<string | null> | null;
  seo_diff: FieldDiffItem[] | null;
  has_hidden_admin_changes: boolean;
}

export interface MissingMediaRef {
  media_id: string;
  location: string;
  expected_kind: string;
  required: boolean;
}

export interface MissingListRef {
  node_id: string;
  reason: string;
}

export interface MissingBlockTypeRef {
  node_id: string;
  block_id: string;
  reason: string;
}

export interface MissingTemplateRef {
  kind: string;
  key: string;
}

export interface MissingDynamicOptionRef {
  source: string;
  value: string;
  field_path: string;
}

export interface DroppedBlockFieldKey {
  node_id: string;
  block_type: string;
  field_path: string;
}

export interface ClearedBindingNode {
  node_id: string;
  field_key: string;
}

export interface MissingRefsReport {
  missing_media: MissingMediaRef[];
  missing_term_ids: number[];
  missing_entry_ids: number[];
  missing_pattern_ids: number[];
  missing_contact_form_ids: number[];
  missing_dynamic_options: MissingDynamicOptionRef[];
  missing_list_refs: MissingListRef[];
  missing_block_types: MissingBlockTypeRef[];
  cleared_binding_nodes: ClearedBindingNode[];
  missing_templates: MissingTemplateRef[];
  dropped_field_keys: string[];
  dropped_block_field_keys: DroppedBlockFieldKey[];
  non_editable_fields: string[];
}

export function hasBlockingMissingRefs(report: MissingRefsReport): boolean {
  return (
    report.missing_media.length > 0 ||
    report.missing_term_ids.length > 0 ||
    report.missing_entry_ids.length > 0 ||
    report.missing_pattern_ids.length > 0 ||
    report.missing_contact_form_ids.length > 0 ||
    report.missing_dynamic_options.length > 0 ||
    report.missing_list_refs.length > 0 ||
    report.missing_block_types.length > 0 ||
    report.cleared_binding_nodes.length > 0 ||
    report.missing_templates.length > 0
  );
}

export function hasAnyMissingRefsInfo(report: MissingRefsReport): boolean {
  return (
    hasBlockingMissingRefs(report) ||
    report.dropped_field_keys.length > 0 ||
    report.dropped_block_field_keys.length > 0 ||
    report.non_editable_fields.length > 0
  );
}

export interface RevisionSummaryDto {
  id: number;
  revision_number: number;
  content_hash: string;
  source: RevisionSource;
  restored_from_revision_number: number | null;
  author_user_id: number | null;
  author_username: string;
  title: string;
  content_mode: ContentMode;
  entry_version: number;
  created_at: string;
}

export interface RevisionsListResponse {
  items: RevisionSummaryDto[];
  total: number;
  limit: number;
  offset: number;
  current_version: number;
  has_autosave: boolean;
}

export interface RevisionFieldView {
  key: string;
  label: string;
  field_type: string;
  value: unknown;
  editable: boolean;
}

export interface RevisionDetailDto {
  revision: RevisionSummaryDto;
  current_version: number;
  slug: string;
  title: string;
  content_mode: ContentMode;
  fields: RevisionFieldView[];
  builder_document: BuilderDocument | null;
  term_ids: number[];
  terms: RevisionTermRef[];
  primary_category_term_id: number | null;
  page_layout_key: string | null;
  custom_template_key: string | null;
  seo: CanonicalRevisionSeo | null;
  has_hidden_admin_fields: boolean;
  missing_refs: MissingRefsReport;
}

export type CompareTarget =
  | { kind: 'revision'; rev_id: number }
  | { kind: 'current' }
  | { kind: 'autosave' };

export interface CompareTargetMeta {
  target: CompareTarget;
  revision_number: number | null;
  label: string;
  updated_at: string | null;
  entry_version: number;
}

export interface RevisionCompareResponse {
  entry_id: number;
  content_type_key: string;
  current_version: number;
  from_meta: CompareTargetMeta;
  to_meta: CompareTargetMeta;
  diff: RevisionDiffPayload;
  from_missing_refs: MissingRefsReport;
  to_missing_refs: MissingRefsReport;
}

export type AutosaveSurface = 'editor' | 'studio';

export interface AutosaveSnapshotDto {
  source_surface: AutosaveSurface;
  base_version: number;
  is_stale_base: boolean;
  updated_at: string;
  client_seq: number;
  title: string;
  content_mode: ContentMode;
  values: Record<string, ContentFieldValue>;
  cleared_fields: string[];
  primary_category_touched: boolean;
  primary_category_term_id: number | null;
  page_layout_touched: boolean;
  page_layout_key: string | null;
  custom_template_touched: boolean;
  custom_template_key: string | null;
  seo_touched: boolean;
  seo: CanonicalRevisionSeo | null;
  builder_document: BuilderDocument | null;
  term_ids: number[];
  has_hidden_admin_fields: boolean;
  missing_refs: MissingRefsReport;
}

export interface AutosaveSnapshotResponse {
  current_version: number;
  autosave: AutosaveSnapshotDto | null;
}

export interface AutosaveUpsertInput {
  source_surface: AutosaveSurface;
  base_version: number;
  client_seq: number;
  title?: string;
  content_mode?: ContentMode;
  values?: Record<string, ContentFieldValue>;
  cleared_fields?: string[];
  builder_document?: BuilderDocument;
  term_ids?: number[];
  primary_category_term_id?: number | null;
  page_layout_key?: string | null;
  custom_template_key?: string | null;
  seo?: SeoMetadataInput | null;
}

export interface AutosaveUpsertResponse {
  applied: boolean;
  saved_at: string;
  base_version: number;
  current_version: number;
  is_stale_base: boolean;
  client_seq: number;
}

export interface RestoreEntryRevisionResponse {
  entry: ContentEntryRecord & {
    page_layout_key?: string | null;
    custom_template_key?: string | null;
  };
  custom_path: string | null;
  seo: EntrySeoSnapshot | null;
  schema_token: string;
  revision: RevisionSummaryDto;
}

export interface RestorePatternRevisionResponse {
  entry_id: number;
  type_key: string;
  slug: string;
  title: string;
  content_mode: ContentMode;
  version: number;
  updated_at: string;
  document: BuilderDocument | null;
  publication_status: string;
  publication_policy: ContentPublicationPolicy;
  diagnostics: BlockDiagnostic[];
  had_freeform_wrap: boolean;
  had_recovery: boolean;
  revision: RevisionSummaryDto;
}

export type RestoreRevisionResponse =
  | RestoreEntryRevisionResponse
  | RestorePatternRevisionResponse;

export function isRestoreEntryOutcome(
  outcome: RestoreRevisionResponse,
): outcome is RestoreEntryRevisionResponse {
  return 'entry' in outcome;
}

function entryRevisionsBase(typeKey: string, entryId: number): string {
  return `/api/admin/content-types/${encodeURIComponent(typeKey)}/entries/${entryId}`;
}

export function listEntryRevisions(
  typeKey: string,
  entryId: number,
  opts?: { limit?: number; offset?: number; signal?: AbortSignal },
): Promise<RevisionsListResponse> {
  const query = new URLSearchParams();
  if (opts?.limit !== undefined) query.set('limit', String(opts.limit));
  if (opts?.offset !== undefined) query.set('offset', String(opts.offset));
  const suffix = query.toString() ? `?${query}` : '';
  return requestJson<RevisionsListResponse>(
    `${entryRevisionsBase(typeKey, entryId)}/revisions${suffix}`,
    {
      signal: opts?.signal,
      defaultErrorMessage: __("Could not load revision history."),
    },
  );
}

export function getEntryRevision(
  typeKey: string,
  entryId: number,
  revId: number,
  signal?: AbortSignal,
): Promise<RevisionDetailDto> {
  return requestJson<RevisionDetailDto>(
    `${entryRevisionsBase(typeKey, entryId)}/revisions/${revId}`,
    {
      signal,
      defaultErrorMessage: __("Could not load revision details."),
    },
  );
}

export function compareEntryRevisions(
  typeKey: string,
  entryId: number,
  from: string,
  to: string,
  signal?: AbortSignal,
): Promise<RevisionCompareResponse> {
  const query = new URLSearchParams({ from, to });
  return requestJson<RevisionCompareResponse>(
    `${entryRevisionsBase(typeKey, entryId)}/revisions/compare?${query}`,
    {
      signal,
      defaultErrorMessage: __("Could not compare revisions."),
    },
  );
}

export function restoreEntryRevision<T = RestoreRevisionResponse>(
  typeKey: string,
  entryId: number,
  revId: number,
  expectedVersion: number,
  csrfToken: string,
  opts?: { acknowledgeMissingRefs?: boolean; signal?: AbortSignal },
): Promise<T> {
  return requestJson<T>(
    `${entryRevisionsBase(typeKey, entryId)}/revisions/${revId}/restore`,
    {
      method: 'POST',
      csrfToken,
      ifMatchVersion: expectedVersion,
      contentType: 'application/json',
      body: JSON.stringify({
        acknowledge_missing_refs: Boolean(opts?.acknowledgeMissingRefs),
      }),
      signal: opts?.signal,
      defaultErrorMessage: __("Could not restore this revision."),
    },
  );
}

export function getEntryAutosave(
  typeKey: string,
  entryId: number,
  signal?: AbortSignal,
): Promise<AutosaveSnapshotResponse> {
  return requestJson<AutosaveSnapshotResponse>(
    `${entryRevisionsBase(typeKey, entryId)}/autosave`,
    {
      signal,
      defaultErrorMessage: __("Could not check for an autosaved draft."),
    },
  );
}

export function upsertEntryAutosave(
  typeKey: string,
  entryId: number,
  input: AutosaveUpsertInput,
  csrfToken: string,
  opts?: { signal?: AbortSignal; keepalive?: boolean },
): Promise<AutosaveUpsertResponse> {
  return requestJson<AutosaveUpsertResponse>(
    `${entryRevisionsBase(typeKey, entryId)}/autosave`,
    {
      method: 'PUT',
      csrfToken,
      contentType: 'application/json',
      body: JSON.stringify(input),
      signal: opts?.signal,
      keepalive: opts?.keepalive,
      defaultErrorMessage: __("Could not save autosave draft."),
    },
  );
}

export async function deleteEntryAutosave(
  typeKey: string,
  entryId: number,
  csrfToken: string,
  signal?: AbortSignal,
): Promise<void> {
  await requestText(`${entryRevisionsBase(typeKey, entryId)}/autosave`, {
    method: 'DELETE',
    csrfToken,
    signal,
    defaultErrorMessage: __("Could not discard the autosaved draft."),
  });
}
