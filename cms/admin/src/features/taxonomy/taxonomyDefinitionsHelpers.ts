import type { AuthUser } from '../../types.ts';
import type { TaxonomyDefinition } from '../../api/taxonomy.ts';
import { ApiClientError, requestJson } from '../../api/client.ts';
import { tStatic } from '../../i18n/index.tsx';

export type TaxonomyDefsTabId = 'definitions' | 'eligibility';

export interface Props {
  user: AuthUser | null;
  /** Stable server site instance ID, provided by App. Without it drafts cannot be safely cached. */
  siteInstanceId?: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

export interface FormValues { key: string; label: string; hierarchical: boolean; public: boolean }
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

// Session-only memory. A 401 unmounts AdminShell's taxonomy screen; retaining
// drafts here avoids browser storage and prevents another account/site reading
// an old form. App must pass its authenticated siteInstanceId to enable it.
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
  /** CPT version from the same SQLite read transaction as taxonomies. */
  type_version: number;
}
export const getConfiguredTaxonomies = async (typeKey: string, signal?: AbortSignal): Promise<ConfiguredTaxonomies> => {
  const configured: ConfiguredTaxonomies = await requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}/configured`, {
    signal, defaultErrorMessage: tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_load_saved_taxonomy_configuration"),
  });
  // Fail closed if the server has not deployed the atomic snapshot contract.
  if (!Number.isSafeInteger(configured.type_version) || configured.type_version < 1) {
    throw new Error(tStatic("feature.taxonomy.taxonomydefinitionspage.the_server_did_not_return_type_version_from_the"));
  }
  return configured;
};
// Both the site override and the provider defaults are persisted selections.
// `effective` omits inactive providers, which would otherwise be silently
// lost when a user saves a new explicit override.
export const selectedKeys = (config: ConfiguredTaxonomies): string[] =>
  config.taxonomies;

export const blank = (): FormValues => ({ key: '', label: '', hierarchical: false, public: true });
export const fromDefinition = (definition: TaxonomyDefinition): FormValues => ({
  key: definition.key, label: definition.label,
  hierarchical: definition.hierarchical, public: definition.public,
});
export const fieldClass = 'mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 disabled:opacity-50';
export const buttonClass = 'inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50';

export function failure(cause: unknown, fallback: string): string {
  if (cause instanceof ApiClientError) {
    if (cause.status === 401) return tStatic("feature.taxonomy.taxonomydefinitionspage.your_session_expired_401_input_is_retained");
    if (cause.status === 412) return tStatic("feature.taxonomy.taxonomydefinitionspage.configuration_changed_412_retain_input_and_reload_before_saving");
    if (cause.status === 409) return tStatic("feature.taxonomy.taxonomydefinitionspage.could_not_change_configuration_409_value", { value1: cause.message });
  }
  return cause instanceof Error ? cause.message : fallback;
}

export function termsHash(key: string): string {
  if (key === 'category') return '#/posts/categories';
  if (key === 'tag') return '#/posts/tags';
  return `#/taxonomy/${encodeURIComponent(key)}/terms`;
}

/** Definitions and membership configuration live alongside content schema, away from the term editor. */
