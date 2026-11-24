export type Role = 'admin' | 'editor';

export interface AuthUser {
  id: number;
  username: string;
  role: Role;
}

export interface AuthResponse {
  user: AuthUser;
  csrf_token: string;
  site_instance_id?: string;
}

export type BlockRegistry = Record<string, Record<string, string>>;

/** Declarative block.json v1 DTOs, mirrored from opendash-core/block_schema.rs.
 * Unknown persisted values are retained by Builder and sent back unchanged. */
export type BlockPropValue =
  | string | number | boolean | null
  | BlockPropValue[] | { [key: string]: BlockPropValue };
export type BlockCategory = 'layout' | 'media' | 'interactive' | 'content' | 'marketing' | 'plugin';
export type BlockMediaKind = 'image' | 'video' | 'any';
export type BlockCssUnit = 'px' | 'rem' | 'em' | '%' | 'vw' | 'vh';
export interface BlockShowIfRule {
  field: string;
  equals: string | number | boolean | null;
}
export interface BlockSelectOption { value: string; label: string; icon?: string | null }
export interface BlockSpacingValue {
  top?: number | null;
  right?: number | null;
  bottom?: number | null;
  left?: number | null;
}
export interface BlockLinkValue {
  url: string;
  new_tab?: boolean;
  nofollow?: boolean;
}
interface BlockFieldBase {
  key: string;
  label: string;
  show_if?: BlockShowIfRule | null;
}
interface BlockFieldWithDefault<T> extends BlockFieldBase {
  default?: T | null;
}
interface BlockTextField extends BlockFieldWithDefault<string> {
  type: 'text' | 'textarea';
  required?: boolean;
  max_chars?: number | null;
  placeholder?: string | null;
}
interface BlockRichTextField extends BlockFieldWithDefault<string> {
  type: 'richtext';
  required?: boolean;
  max_bytes?: number | null;
}
interface BlockNumericField extends BlockFieldWithDefault<number> {
  type: 'number' | 'range';
  min?: number | null;
  max?: number | null;
  step?: number | null;
  unit?: BlockCssUnit | null;
  css_var?: string | null;
  responsive?: boolean;
  required?: boolean;
}
interface BlockSelectField extends BlockFieldWithDefault<string> {
  type: 'select';
  options: BlockSelectOption[];
  display?: 'dropdown' | 'buttons' | null;
  required?: boolean;
  css_var?: string | null;
  responsive?: boolean;
}
interface BlockRadioButtonsField extends BlockFieldWithDefault<string> {
  type: 'radio_buttons';
  options: BlockSelectOption[];
  required?: boolean;
  css_var?: string | null;
  responsive?: boolean;
}
interface BlockSpacingField extends BlockFieldWithDefault<BlockSpacingValue> {
  type: 'spacing';
  min?: number | null;
  max?: number | null;
  unit?: BlockCssUnit | null;
  css_var?: string | null;
  responsive?: boolean;
}
interface BlockColorField extends BlockFieldWithDefault<string> {
  type: 'color';
  required?: boolean;
  css_var?: string | null;
}
interface BlockMediaField extends BlockFieldWithDefault<string> {
  type: 'media';
  media_kind?: BlockMediaKind;
  required?: boolean;
  css_var?: string | null;
}
interface BlockUrlField extends BlockFieldWithDefault<string> {
  type: 'url';
  required?: boolean;
  placeholder?: string | null;
}
interface BlockLinkField extends BlockFieldWithDefault<BlockLinkValue> {
  type: 'link';
  required?: boolean;
  placeholder?: string | null;
}
interface BlockBooleanField extends BlockFieldWithDefault<boolean> {
  type: 'boolean';
}
interface BlockGroupField extends BlockFieldBase {
  type: 'group';
  fields: BlockFieldSpec[];
}
interface BlockRepeaterField extends BlockFieldBase {
  type: 'repeater';
  fields: BlockFieldSpec[];
  min_items?: number;
  max_items?: number;
  item_label_field?: string | null;
}
export type BlockFieldSpec =
  | BlockTextField | BlockRichTextField | BlockNumericField | BlockSelectField
  | BlockRadioButtonsField | BlockSpacingField | BlockColorField | BlockMediaField
  | BlockUrlField | BlockLinkField | BlockBooleanField | BlockGroupField | BlockRepeaterField;
export interface BlockSchemaV1 {
  schema_version: 1;
  id: string;
  title: string;
  category: BlockCategory;
  icon?: string | null;
  render: string;
  allowed_parents?: string[] | null;
  allowed_children?: string[] | null;
  fields: BlockFieldSpec[];
}
export interface BlockCatalogItemDto {
  block_id: string;
  owner_kind: 'core' | 'theme' | 'plugin';
  owner_id: string;
  status: 'active' | 'provider_inactive';
  declaration_state: 'declared' | 'withdrawn';
  schema: BlockSchemaV1;
}

export interface BlockData {
  type: string;
  values: Record<string, string>;
}

export interface PluginPermissions {
  sqlite: string[];
  http: string[];
}

export interface PluginSummary {
  id: string;
  version: string;
  name: string;
  description: string;
  enabled: boolean;
  permissions: PluginPermissions;
  last_error: string | null;
  installed_at: string;
  updated_at: string;
}

export interface PluginRunResult {
  success: boolean;
  data: unknown;
  error?: string;
  code?: string;
  correlation_id: string;
}

export type PluginSettingFieldType =
  | 'text'
  | 'textarea'
  | 'number'
  | 'boolean'
  | 'select'
  | 'url'
  | 'media'
  | 'secret';

export interface PluginSettingChoice {
  value: string;
  label: string;
}

export interface PluginSettingField {
  key: string;
  label: string;
  type: PluginSettingFieldType;
  required?: boolean;
  default?: unknown;
  min?: number | null;
  max?: number | null;
  choices?: PluginSettingChoice[];
}

export type PluginSecretValue =
  | { configured: boolean }
  | string
  | { clear: true }
  | null;

export interface PluginSettingsSnapshot {
  plugin_id: string;
  version: number;
  schema: PluginSettingField[];
  values: Record<string, unknown>;
  updated_at: string;
}

export interface CmsLocaleOption {
  code: string;
  bcp47: string;
  native_name: string;
  english_name: string;
  direction: 'ltr' | 'rtl';
  installed: boolean;
  translation_coverage: number;
}

export interface AdminI18nBootstrap {
  configured_locale: string;
  effective_locale: 'en' | 'vi';
  effective_bcp47: string;
  effective_direction: 'ltr' | 'rtl';
  configured_installed: boolean;
  catalog_version: string;
  available_locales: CmsLocaleOption[];
}
