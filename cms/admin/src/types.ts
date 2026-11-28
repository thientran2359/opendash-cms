import type { FieldSpec } from './fields/types.ts';

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

/** Declarative block.json v1 DTOs, mirrored from opendash-core/block_schema.rs.
 * Unknown persisted values are retained by Builder and sent back unchanged. */
export type BlockPropValue =
  | string | number | boolean | null
  | BlockPropValue[] | { [key: string]: BlockPropValue };
export type BlockCategory = 'layout' | 'media' | 'interactive' | 'content' | 'marketing' | 'plugin';
export interface DefaultChildSpec {
  block_id: string;
  props?: Record<string, unknown>;
  children?: DefaultChildSpec[];
}
export interface BlockPresetSpec {
  key: string;
  title: string;
  icon?: string | null;
  props?: Record<string, unknown>;
  children?: DefaultChildSpec[];
}
export interface BlockSchema {
  schema_version: 1;
  id: string;
  title: string;
  category: BlockCategory;
  icon?: string | null;
  render: string;
  supports_children?: boolean;
  allowed_parents?: string[] | null;
  allowed_children?: string[] | null;
  allowed_contexts?: string[] | null;
  default_children?: DefaultChildSpec[];
  presets?: BlockPresetSpec[];
  fields: FieldSpec[];
}
export interface BlockCatalogItemDto {
  block_id: string;
  owner_kind: 'core' | 'theme' | 'plugin';
  owner_id: string;
  status: 'active' | 'provider_inactive';
  declaration_state: 'declared' | 'withdrawn';
  schema: BlockSchema;
}

export interface BlockData {
  type: string;
  values: Record<string, string>;
}

export interface PluginPermissions {
  sqlite: string[];
  http: string[];
  content_read?: string[];
  content_write?: string[];
  content_delete?: string[];
  content_trash_read?: string[];
  content_restore?: string[];
  content_purge?: string[];
}

export interface PluginLifecycleConfirmation {
  confirmation_id: string;
  operation: 'purge' | 'restore';
  type_key: string;
  entry_id: number;
  locale: string;
  title: string;
  slug: string;
  version: number;
  expires_at: string;
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
  error?: string | null;
  code?: string | null;
  correlation_id: string;
  confirmation?: PluginLifecycleConfirmation;
}

export type PluginSecretValue =
  | { configured: boolean }
  | string
  | { clear: true }
  | null;

export interface PluginSettingsSnapshot {
  plugin_id: string;
  version: number;
  fields: FieldSpec[];
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
  effective_locale: string;
  effective_bcp47: string;
  effective_direction: 'ltr' | 'rtl';
  configured_installed: boolean;
  catalog_version: string;
  available_locales: CmsLocaleOption[];
  catalog_etag?: string;
  catalog_url?: string;
}
