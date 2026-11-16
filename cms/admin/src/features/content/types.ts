/** Wire types for M3–M7 content, field groups, panel registry, and composite save APIs. */
export type ContentFieldType =
  | 'short_text'
  | 'long_text'
  | 'rich_text'
  | 'number'
  | 'boolean'
  | 'date'
  | 'select'
  | 'image'
  | 'gallery';

export const CONTENT_FIELD_TYPES: Array<{ value: ContentFieldType; label: string }> = [
  { value: 'short_text', label: 'Chữ ngắn' },
  { value: 'long_text', label: 'Chữ dài' },
  { value: 'rich_text', label: 'Nội dung rich text' },
  { value: 'number', label: 'Số' },
  { value: 'boolean', label: 'Bật / tắt' },
  { value: 'date', label: 'Ngày' },
  { value: 'select', label: 'Lựa chọn' },
  { value: 'image', label: 'Ảnh (mã media)' },
  { value: 'gallery', label: 'Thư viện ảnh (danh sách mã)' },
];

export type FieldRole = 'editor' | 'admin';
export type PublicExposure = 'public' | 'private';
export type DeclarationState = 'active' | 'withdrawn';
export type ProviderStatus = 'active' | 'provider_inactive';
export type EditorRegion = 'header_bar' | 'main_top' | 'main_primary' | 'main_bottom' | 'sidebar';

export interface SelectOption {
  value: string;
  label: string;
}

export interface ContentFieldDefinition {
  key: string;
  label: string;
  type: ContentFieldType;
  required: boolean;
  options: SelectOption[];
  default_value?: ContentFieldValue;
  min_view_role?: FieldRole;
  min_edit_role?: FieldRole;
  public_exposure?: PublicExposure;
  declaration_state?: DeclarationState;
}

export interface ContentTypeDefinition {
  key: string;
  label: string;
  fields: ContentFieldDefinition[];
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

export type ContentFieldValue = string | number | boolean | string[] | null;

export interface ContentEntryInput {
  title: string;
  slug: string;
  values: Record<string, ContentFieldValue>;
  /** Omitted on legacy updates to preserve existing links; [] removes every link. */
  term_ids?: number[];
}

export interface ContentEntryRecord extends ContentEntryInput {
  term_ids: number[];
  id: number;
  content_type_key: string;
  publication_status: 'draft' | 'published';
  version: number;
  created_at: string;
  updated_at: string;
}

export interface UserFieldGroupInput {
  local_key: string;
  title: string;
  target_types: string[];
  fields: ContentFieldDefinition[];
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
  fields: ContentFieldDefinition[];
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
  definition: ContentFieldDefinition;
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
  values: Record<string, ContentFieldValue>;
  term_ids?: number[];
  custom_path?: string | null;
  seo?: {
    title?: string | null;
    description?: string | null;
    og_image_id?: string | null;
    canonical_url?: string | null;
    noindex?: boolean;
  } | null;
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
