import type { FieldAcl, FieldRole, FieldSpec, FieldType, PublicExposure } from '../../fields/types';

export type { FieldAcl, FieldRole, FieldSpec, FieldType, PublicExposure };

export const CONTENT_CREATOR_FIELD_TYPES: FieldType[] = [
  'text', 'textarea', 'richtext', 'number', 'range', 'email', 'url',
  'boolean', 'color', 'icon', 'date', 'datetime', 'time',
  'select', 'checkbox', 'radio_buttons',
  'media', 'gallery', 'link', 'entry_ref', 'term_ref',
];

export const OPTION_BEARING_CREATOR_TYPES: ReadonlyArray<FieldType> = [
  'select', 'checkbox', 'radio_buttons',
];

export function parseCreatorFieldOptions(
  optionsText: string,
  formatErr: {
    count: () => string;
    format: (line: string) => string;
    bounds: (line: string) => string;
    duplicate: () => string;
  },
  fieldType?: FieldType,
): SelectOption[] {
  const maxOptions = fieldType === 'radio_buttons' ? 12 : 100;
  const lines = optionsText.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  if (!lines.length || lines.length > maxOptions) throw new Error(formatErr.count());
  const options = lines.map(line => {
    const separator = line.indexOf('|');
    if (separator < 1) throw new Error(formatErr.format(line));
    const value = line.slice(0, separator).trim();
    const label = line.slice(separator + 1).trim();
    if (!value || value.length > 64 || !label || label.length > 100) {
      throw new Error(formatErr.bounds(line));
    }
    return { value, label };
  });
  if (new Set(options.map(option => option.value)).size !== options.length) {
    throw new Error(formatErr.duplicate());
  }
  return options;
}

export function withCreatorFieldDefaults(spec: FieldSpec): FieldSpec {
  return {
    ...spec,
    ...(spec.type === 'range' ? { min: spec.min ?? 0, max: spec.max ?? 100, step: spec.step ?? 1 } : {}),
    ...(spec.type === 'entry_ref' ? { target_types: spec.target_types?.length ? spec.target_types : ['post'] } : {}),
    ...(spec.type === 'term_ref' ? { taxonomy: spec.taxonomy ?? 'category' } : {}),
  };
}
export type DeclarationState = 'active' | 'withdrawn';
export type ProviderStatus = 'active' | 'provider_inactive';
export type EditorRegion = 'header_bar' | 'main_top' | 'main_primary' | 'main_bottom' | 'sidebar';

export interface SelectOption {
  value: string;
  label: string;
}

export interface ContentTypeDefinition {
  key: string;
  label: string;
  fields: FieldSpec[];
}

export interface ResourceOwner {
  kind: 'core' | 'user' | 'theme' | 'plugin';
  id?: string;
}

export interface OwnedContentType {
  owner: ResourceOwner;
  status: ProviderStatus;
  declaration_state?: DeclarationState;
  descriptor: ContentTypeDefinition;
  version: number;
  schema_token: string;
}

export type ContentMode = 'editor' | 'builder';

export type ContentFieldValue =
  | string
  | number
  | boolean
  | null
  | Record<string, unknown>
  | unknown[];

export interface TranslationLinkSummary {
  id: number;
  locale: string;
  slug: string;
  title: string;
  publication_status: 'draft' | 'published';
}

export interface ContentEntryInput {
  title: string;
  slug: string;
  values: Record<string, ContentFieldValue>;
  /** Omitted on legacy updates to preserve existing links; [] removes every link. */
  term_ids?: number[];
  locale?: string;
}

export interface ContentEntryRecord extends ContentEntryInput {
  term_ids: number[];
  primary_category_term_id?: number | null;
  id: number;
  content_type_key: string;
  publication_status: 'draft' | 'published';
  content_mode: ContentMode;
  version: number;
  created_at: string;
  updated_at: string;
  locale?: string;
  translation_group_id?: string;
  translations?: TranslationLinkSummary[];
}

export type ProjectionOutcome =
  | { status: 'not_needed' }
  | { status: 'current' }
  | { status: 'failed'; reason: string };

export interface SecondaryOutcome {
  projection: ProjectionOutcome;
  hook_enqueued: number;
  hook_dropped: number;
}

export interface LifecycleSummary {
  id: number;
  type_key: string;
  locale: string;
  version: number;
  publication_status: 'draft' | 'published';
  trashed_at: string | null;
  published_at: string | null;
  secondary?: SecondaryOutcome;
}

export interface PublishedAtPatch {
  local_datetime: string;
  timezone: string;
}

export type TaxonomyDeltaOp = 'add' | 'remove' | 'replace';

export interface MetadataTaxonomyPatch {
  key: string;
  operation: TaxonomyDeltaOp;
  term_ids: number[];
}

export interface MetadataPatch {
  expected_schema_token: string;
  title?: string;
  slug?: string;
  publication_status?: 'draft' | 'published';
  published_at?: PublishedAtPatch;
  taxonomy?: MetadataTaxonomyPatch;
}

export interface MetadataPatchResponse extends EntryTableRow {
  schema_token: string;
  secondary?: SecondaryOutcome;
}

export interface EntryRowCapabilities {
  edit: boolean;
  quick_edit: boolean;
  publish: boolean;
  unpublish: boolean;
  trash: boolean;
  restore: boolean;
  immediate_public_restore: boolean;
  purge: boolean;
}

/** M7-S16-T05 + S02: authenticated, ACL-projected server-side table result. */
export interface EntryTableRow {
  id: number;
  type_key: string;
  title: string;
  slug: string;
  publication_status: 'draft' | 'published';
  lifecycle?: 'active' | 'trash';
  version: number;
  created_at: string;
  updated_at: string;
  published_at?: string | null;
  trashed_at?: string | null;
  term_ids: number[];
  locale?: string;
  translation_group_id?: string;
  /** Null for draft, Trash, or any entry without a publicly routable URL. */
  public_url: string | null;
  /** Only custom fields the current actor is authorized to read (empty for Trash). */
  fields: Record<string, ContentFieldValue>;
  capabilities?: EntryRowCapabilities;
  blocked_reasons?: Record<string, string>;
}

export interface EntryTableResult {
  schema_version: 1;
  items: EntryTableRow[];
  total: number;
  status_counts: { all: number; draft: number; published: number; trash?: number };
  limit: number;
  offset: number;
  schema_token?: string | null;
}

export interface EntryTableQuery {
  q?: string;
  status?: 'all' | 'draft' | 'published' | 'trash';
  locale?: string;
  category_id?: number;
  tag_id?: number;
  include_descendants?: boolean;
  updated_month?: string;
  sort?: 'title' | 'created_at' | 'updated_at' | 'trashed_at' | 'id';
  order?: 'asc' | 'desc';
  limit?: number;
  offset?: number;
  retained_trash?: boolean;
}

export interface UserFieldGroupInput {
  local_key: string;
  title: string;
  target_types: string[];
  fields: FieldSpec[];
  default_region?: EditorRegion;
  default_order?: number;
}

export interface FieldGroupTargetBinding {
  type_key: string;
  association_state: DeclarationState;
}

export interface FieldGroupRecord {
  group_id: string;
  owner: ResourceOwner;
  title: string;
  provider_status: ProviderStatus;
  status: DeclarationState;
  default_region: EditorRegion;
  default_order: number;
  version: number;
  targets: FieldGroupTargetBinding[];
  fields: FieldSpec[];
}

export interface LayoutFieldSubgroup {
  subgroup_id: string;
  title: string;
  field_keys: string[];
}

export interface BaseFieldSlicePanelConfig {
  panel_id: string;
  title: string;
  collapsible: boolean;
  field_keys: string[];
  subgroups: LayoutFieldSubgroup[];
}

export interface PanelPlacementConfig {
  panel_id: string;
  enabled: boolean;
  order: number;
  collapsed_by_default: boolean;
  subgroups: LayoutFieldSubgroup[];
}

export interface EditorRegionsConfig {
  main_top: PanelPlacementConfig[];
  main_primary: PanelPlacementConfig[];
  main_bottom: PanelPlacementConfig[];
  sidebar: PanelPlacementConfig[];
}

export interface EditorLayoutConfigV1 {
  schema_version: number;
  type_key: string;
  version: number;
  base_field_panels: BaseFieldSlicePanelConfig[];
  regions: EditorRegionsConfig;
}

export type PanelKind =
  | { kind: 'core_built_in'; core_key: string }
  | { kind: 'base_fields_slice'; slice_key: string; field_keys: string[] }
  | { kind: 'content_fields'; field_group_id: string }
  | {
      kind: 'action';
      action_id: string;
      button_label: string;
      confirm_message?: string | null;
      public_effect: boolean;
    };

export interface PanelDefinition {
  panel_id: string;
  owner: ResourceOwner;
  title: string;
  kind: PanelKind;
  applies_to: string[];
  default_region: EditorRegion;
  allowed_regions: EditorRegion[];
  default_order: number;
  min_view_role: FieldRole;
  min_edit_role: FieldRole;
  collapsible: boolean;
  required_core: boolean;
  effective_status: 'active' | 'provider_inactive' | 'withdrawn';
}

export interface EditorLayoutSnapshot {
  layout: EditorLayoutConfigV1;
  panels: PanelDefinition[];
  version: number;
}

export interface EffectiveFieldProjection {
  definition: FieldSpec;
  owner: ResourceOwner;
  source_id: string;
  effective_status: 'active' | 'provider_inactive' | 'withdrawn';
  effective_required: boolean;
  editable: boolean;
}

export interface EffectivePanelProjection {
  panel: PanelDefinition;
  placement: PanelPlacementConfig;
  field_keys: string[];
  subgroups: LayoutFieldSubgroup[];
  panel_fields?: FieldSpec[];
}

export interface EffectiveEditorRegionsProjection {
  main_top: EffectivePanelProjection[];
  main_primary: EffectivePanelProjection[];
  main_bottom: EffectivePanelProjection[];
  sidebar: EffectivePanelProjection[];
}

export interface EffectiveEditorConfigProjection {
  type_key: string;
  label: string;
  owner: ResourceOwner;
  provider_status: ProviderStatus;
  declaration_state: DeclarationState;
  base_version: number;
  layout_token: string;
  schema_token: string;
  taxonomies: string[];
  regions: EffectiveEditorRegionsProjection;
  effective_fields: Record<string, EffectiveFieldProjection>;
}

export interface CompositeEntrySaveInput {
  expected_version?: number;
  expected_schema_token: string;
  title: string;
  slug: string;
  publication_status?: 'draft' | 'published';
  content_mode?: ContentMode;
  values: Record<string, ContentFieldValue>;
  term_ids?: number[];
  primary_category_term_id?: number | null;
  auto_unique_slug?: boolean;
  custom_path?: string | null;
  seo?: {
    title?: string | null;
    description?: string | null;
    og_image_id?: string | null;
    canonical_url?: string | null;
    noindex?: boolean;
  } | null;
  created_at?: string;
  updated_at?: string;
}

export interface CompositeEntrySaveResult {
  entry: ContentEntryRecord;
  custom_path: string | null;
  seo: {
    entry_version: number;
    title: string | null;
    description: string | null;
    og_image_id: string | null;
    canonical_url: string | null;
    noindex: boolean;
  } | null;
  schema_token: string;
}
