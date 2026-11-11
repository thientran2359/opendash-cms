/** Wire types for the M3.1–M3.3 content API. */
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
}

export interface ContentTypeDefinition {
  key: string;
  label: string;
  fields: ContentFieldDefinition[];
}

export interface ContentTypeOwnership {
  kind: 'user' | 'core' | 'theme' | 'plugin';
  id?: string;
}

export interface OwnedContentType {
  owner: ContentTypeOwnership;
  status: 'active' | 'provider_inactive';
  descriptor: ContentTypeDefinition;
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
  version: number;
  created_at: string;
  updated_at: string;
}
