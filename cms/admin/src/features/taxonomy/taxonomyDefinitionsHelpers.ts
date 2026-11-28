import type { AuthUser } from '../../types.ts';
import type {
  TaxonomyDefinition,
  TaxonomySelectionMode,
  TaxonomySettings,
} from '../../api/taxonomy.ts';
import { ApiClientError, requestJson } from '../../api/client.ts';
import type { TermPickerStyle } from '../../fields/types.ts';
import { __ } from '../../i18n/index.tsx';

export type TaxonomyDefsTabId = 'definitions' | 'eligibility';

export interface Props {
  user: AuthUser | null;
  /** Stable server site instance ID, provided by App. Without it drafts cannot be safely cached. */
  siteInstanceId?: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
  onDefinitionsChanged?: () => void;
}

export interface FormValues {
  key: string;
  label: string;
  hierarchical: boolean;
  public: boolean;
  rewrite_base: string;
  singular_label: string;
  description: string;
  selection_mode: TaxonomySelectionMode;
  picker_style: TermPickerStyle;
  default_term_id: number | null;
  required_on_publish: boolean;
  editor_can_create_terms: boolean;
  show_in_menu: boolean;
  show_admin_column: boolean;
  content_types: string[];
}

export interface DefinitionsDraft {
  activeKey: string;
  form: FormValues;
  baseline: FormValues;
  definitionVersion: number | null;
  conflict: boolean;
  typeKey: string;
  eligible: string[];
  eligibilityBaseline: string[];
  typeVersion: number | null;
  eligibilityConflict: boolean;
}

export const definitionDrafts = new Map<string, DefinitionsDraft>();
export const draftScope = (siteInstanceId: string | undefined, user: AuthUser | null): string | null =>
  siteInstanceId && siteInstanceId !== 'default-site' && user
    ? JSON.stringify([siteInstanceId, user.id, user.role]) : null;
export const keysEqual = (a: string[], b: string[]): boolean =>
  [...a].sort().join('\u0000') === [...b].sort().join('\u0000');

export interface ConfiguredTaxonomies {
  taxonomies: string[];
  has_override: boolean;
  effective: string[];
  type_version: number;
}

export const getConfiguredTaxonomies = async (typeKey: string, signal?: AbortSignal): Promise<ConfiguredTaxonomies> => {
  const configured: ConfiguredTaxonomies = await requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}/configured`, {
    signal, defaultErrorMessage: __("Could not load saved taxonomy configuration."),
  });
  if (!Number.isSafeInteger(configured.type_version) || configured.type_version < 1) {
    throw new Error(__("The server did not return type_version from the same taxonomy snapshot; saving is unsafe."));
  }
  return configured;
};

export const selectedKeys = (config: ConfiguredTaxonomies): string[] => config.taxonomies;

export const DEFAULT_TAXONOMY_SETTINGS: Required<TaxonomySettings> = {
  singular_label: null,
  description: '',
  selection_mode: 'multiple',
  picker_style: 'auto',
  default_term_id: null,
  required_on_publish: false,
  editor_can_create_terms: false,
  show_in_menu: true,
  show_admin_column: false,
};

export const blank = (): FormValues => ({
  key: '',
  label: '',
  hierarchical: false,
  public: true,
  rewrite_base: '',
  singular_label: '',
  description: '',
  selection_mode: 'multiple',
  picker_style: 'auto',
  default_term_id: null,
  required_on_publish: false,
  editor_can_create_terms: false,
  show_in_menu: true,
  show_admin_column: false,
  content_types: [],
});

export const fromDefinition = (definition: TaxonomyDefinition): FormValues => {
  const s = definition.settings ?? {};
  return {
    key: definition.key,
    label: definition.label,
    hierarchical: definition.hierarchical,
    public: definition.public,
    rewrite_base: definition.rewrite_base ?? '',
    singular_label: s.singular_label ?? '',
    description: s.description ?? '',
    selection_mode: s.selection_mode ?? 'multiple',
    picker_style: s.picker_style ?? 'auto',
    default_term_id: s.default_term_id ?? null,
    required_on_publish: Boolean(s.required_on_publish),
    editor_can_create_terms: Boolean(s.editor_can_create_terms),
    show_in_menu: s.show_in_menu ?? true,
    show_admin_column: Boolean(s.show_admin_column),
    content_types: [...(definition.content_types ?? [])].sort(),
  };
};

export function settingsFromForm(form: FormValues): TaxonomySettings {
  return {
    singular_label: form.singular_label.trim() || null,
    description: form.description,
    selection_mode: form.selection_mode,
    picker_style: form.picker_style,
    default_term_id: form.default_term_id,
    required_on_publish: form.required_on_publish,
    editor_can_create_terms: form.editor_can_create_terms,
    show_in_menu: form.show_in_menu,
    show_admin_column: form.show_admin_column,
  };
}

export const fieldClass = 'mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 disabled:opacity-50';
export const buttonClass = 'inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50';

export function failure(cause: unknown, fallback: string): string {
  if (cause instanceof ApiClientError) {
    if (cause.status === 401) return __("Your session expired (401); input is retained.");
    if (cause.status === 412) return __("Configuration changed (412); retain input and reload before saving.");
    if (cause.status === 409) return __("Could not change configuration (409): {{value1}}", { value1: cause.message });
  }
  return cause instanceof Error ? cause.message : fallback;
}

export function termsHash(key: string): string {
  if (key === 'category') return '#/posts/categories';
  if (key === 'tag') return '#/posts/tags';
  return `#/taxonomy/${encodeURIComponent(key)}/terms`;
}
