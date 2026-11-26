export type FieldContext = 'content' | 'term' | 'options' | 'owner_settings' | 'studio_block';

export type FieldType =
  | 'text'
  | 'textarea'
  | 'number'
  | 'range'
  | 'email'
  | 'url'
  | 'secret'
  | 'richtext'
  | 'media'
  | 'gallery'
  | 'link'
  | 'select'
  | 'checkbox'
  | 'radio_buttons'
  | 'boolean'
  | 'color'
  | 'icon'
  | 'date'
  | 'datetime'
  | 'time'
  | 'spacing'
  | 'entry_ref'
  | 'term_ref'
  | 'group'
  | 'repeater'
  | 'tab'
  | 'accordion'
  | 'message';

export type SpacingProperty = 'padding' | 'margin';

export type SelectOptionsSource =
  | 'menus'
  | 'menu_slots'
  | 'contact_forms'
  | 'content_types'
  | 'taxonomies'
  | 'context_fields'
  | 'template_parts'
  | 'patterns';

export type MediaFieldKind = 'image' | 'video' | 'file' | 'any';
export type SelectDisplay = 'dropdown' | 'buttons';
export type TermPickerStyle = 'auto' | 'checklist' | 'tags' | 'select' | 'radio';
export type FieldRole = 'editor' | 'admin';
export type PublicExposure = 'public' | 'private';
export type FieldDeclarationState = 'declared' | 'withdrawn';
export type RichTextToolbar = 'minimal' | 'standard';
export type CssUnit = 'px' | 'rem' | 'em' | '%' | 'vw' | 'vh';
export type CheckboxLayout = 'vertical' | 'horizontal';
export type RadioLayout = 'buttons' | 'radio';
export type RepeaterLayout = 'accordion' | 'table' | 'block';
export type FieldLayoutPlacement = 'top' | 'left';
export type MessageTone = 'info' | 'warning' | 'neutral';
export type ConditionOp = 'eq' | 'neq' | 'empty' | 'not_empty' | 'contains' | 'gt' | 'lt' | 'in';
export type FieldViewport = 'desktop' | 'tablet' | 'mobile';

export interface ConditionRule {
  field: string;
  op: ConditionOp;
  value?: unknown;
}

export type ConditionGroups = ConditionRule[][];

export interface SelectOption {
  value: string;
  label: string;
  icon?: string;
}

export interface LinkValue {
  url: string;
  label?: string;
  new_tab?: boolean;
  nofollow?: boolean;
}

export interface SpacingValue {
  top?: number | null;
  right?: number | null;
  bottom?: number | null;
  left?: number | null;
}

export interface ResponsiveValue<T = unknown> {
  desktop?: T;
  tablet?: T;
  mobile?: T;
}

export interface FieldAcl {
  min_view_role?: FieldRole;
  min_edit_role?: FieldRole;
  public_exposure?: PublicExposure;
}

export interface FieldSpec {
  type: FieldType;
  key: string;
  label: string;
  instructions?: string;
  placeholder?: string;
  required?: boolean;
  default?: unknown;
  width?: 25 | 33 | 50 | 66 | 75 | 100;
  conditions?: ConditionGroups;

  acl?: FieldAcl;
  declaration_state?: FieldDeclarationState;
  show_in_list?: boolean;
  public?: boolean;
  translatable?: boolean;
  responsive?: boolean;
  css_var?: string;

  min_chars?: number;
  max_chars?: number;
  max_bytes?: number;
  rows?: number;
  prepend?: string;
  append?: string;
  toolbar?: RichTextToolbar;
  min?: number;
  max?: number;
  step?: number;
  unit?: CssUnit;
  property?: SpacingProperty;
  min_date?: string;
  max_date?: string;

  options?: SelectOption[];
  options_source?: SelectOptionsSource;
  display?: SelectDisplay;
  multiple?: boolean;
  allow_null?: boolean;
  on_label?: string;
  off_label?: string;
  allow_alpha?: boolean;
  media_kind?: MediaFieldKind;
  min_items?: number;
  max_items?: number;
  target_types?: string[];
  taxonomy?: string;
  taxonomy_from_field?: string;
  picker_style?: TermPickerStyle;
  sync_entry_terms?: boolean;

  fields?: FieldSpec[];
  layout?: CheckboxLayout | RadioLayout | RepeaterLayout;
  item_label_field?: string;
  button_label?: string;

  placement?: FieldLayoutPlacement;
  open_by_default?: boolean;
  multi_expand?: boolean;
  endpoint?: boolean;
  tone?: MessageTone;
  body?: string;
}

export interface DynamicSelectOption {
  value: string;
  label: string;
  meta?: string;
}

export interface OptionsSourceResponse {
  source: SelectOptionsSource;
  items: DynamicSelectOption[];
}

export interface ResolvedEntrySummary {
  id: number;
  content_type_key: string;
  title: string;
  slug: string;
  publication_status: 'draft' | 'published';
  locale: string;
}
