import { tStatic } from '../i18n/index.tsx';
import type { BlockFieldSpec } from '../types.ts';
import { requestJson } from './client.ts';

export interface ThemeRevisionSummary {
  revision_id: string;
  theme_id: string;
  version: string;
  revision_digest: string;
  source_format: 'v3';
}

export interface ThemeSelection {
  revision_id: string | null;
  theme_id: string | null;
  version: number;
  source_format: 'v3' | null;
}

export interface ThemesSummary {
  version: number;
  revisions: ThemeRevisionSummary[];
  selection: ThemeSelection;
}

export interface ThemeV3InstallResponse {
  revision_id: string;
  theme_id: string;
  version: string;
  revision_digest: string;
  source_format: 'v3';
  activated: false;
}

export type ThemeInstallResponse = ThemeV3InstallResponse;

export function isV3ThemeInstall(response: ThemeInstallResponse): response is ThemeV3InstallResponse {
  return 'revision_id' in response;
}

export type ThemeOptionType =
  | 'string'
  | 'url'
  | 'number'
  | 'boolean'
  | 'select'
  | 'image_ref'
  | 'text'
  | 'textarea'
  | 'richtext'
  | 'range'
  | 'radio_buttons'
  | 'color'
  | 'spacing'
  | 'media'
  | 'link'
  | 'repeater';

export interface ThemeOptionChoice {
  value: string;
  label: string;
}

export interface ThemeOptionDef {
  key: string;
  label: string;
  type: ThemeOptionType;
  default: unknown;
  public: boolean;
  translatable?: boolean;
  min?: number;
  max?: number;
  choices?: ThemeOptionChoice[];
}

export interface ThemeOptionsSnapshot {
  theme_id: string;
  version: number;
  schema: ThemeOptionDef[];
  fields?: BlockFieldSpec[];
  values: Record<string, unknown>;
}

export function getThemes(): Promise<ThemesSummary> {
  return requestJson('/api/admin/themes', {
    defaultErrorMessage: tStatic('api.themes.load_themes'),
  });
}

export function installTheme(file: File, csrfToken: string): Promise<ThemeInstallResponse> {
  return requestJson('/api/admin/themes', {
    method: 'POST',
    body: file,
    contentType: 'application/zip',
    csrfToken,
    defaultErrorMessage: tStatic('api.themes.install_the_theme_zip'),
  });
}

export function activateTheme(revisionId: string, version: number, csrfToken: string): Promise<ThemeSelection> {
  return activateThemeRevision(revisionId, version, csrfToken);
}

export interface ThemeLayoutCatalog {
  active_theme_id: string | null;
  page_layouts: Array<{ key: string; label: string }>;
  custom_templates: Array<{ custom_key: string; label: string; allowed_types: string[] }>;
}

export function getActiveThemeLayouts(): Promise<ThemeLayoutCatalog> {
  return requestJson('/api/admin/themes/active/layouts', {
    defaultErrorMessage: tStatic('api.themes.load_page_layouts_and_custom_single_templates'),
  });
}

export function activateThemeRevision(revisionId: string, selectionVersion: number, csrfToken: string): Promise<ThemeSelection> {
  return requestJson('/api/admin/themes/active', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: selectionVersion,
    contentType: 'application/json',
    body: JSON.stringify({ revision_id: revisionId }),
    defaultErrorMessage: tStatic('api.themes.activate_the_theme_revision'),
  });
}

export function getThemeOptions(
  themeId: string,
  locale?: string | null,
): Promise<ThemeOptionsSnapshot> {
  const query = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/options${query}`, {
    defaultErrorMessage: tStatic('api.themes.load_theme_options'),
  });
}

export function saveThemeOptions(
  themeId: string,
  values: Record<string, unknown>,
  version: number,
  csrfToken: string,
  locale?: string | null,
): Promise<ThemeOptionsSnapshot> {
  const query = locale ? `?locale=${encodeURIComponent(locale)}` : '';
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/options${query}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ values }),
    defaultErrorMessage: tStatic('api.themes.save_theme_options'),
  });
}

export type SampleGroup = 'pages' | 'menus' | 'media' | 'settings';

export interface ImportPlanItem {
  resource_kind: SampleGroup;
  object_key: string;
  logical_target: string;
  action: 'create' | 'skip' | 'missing_dependency';
  reason: string | null;
  publication_implication: string;
  dependencies: string[];
}

export interface UnselectedDependency {
  required_group: SampleGroup;
  object_key: string;
  referenced_by_group: SampleGroup;
  referenced_by_object: string;
}

export interface ImportPlan {
  theme_id: string;
  revision_id: string;
  revision_digest: string;
  sample_key: string;
  selected_groups: SampleGroup[];
  items: ImportPlanItem[];
  unselected_dependencies: UnselectedDependency[];
  expected_versions: {
    theme_selection_version: number;
    theme_options_version: number;
    menu_versions: Record<string, number>;
  };
  requires_public_ack: boolean;
  publication_implications: string[];
  plan_digest: string;
}

export interface ImportExecutionOutcome {
  run_id: string;
  theme_id: string;
  revision_id: string;
  sample_key: string;
  plan_digest: string;
  created: Array<{ resource_kind: SampleGroup; object_key: string; target_id: string; status: string; reason: string | null }>;
  skipped: Array<{ resource_kind: SampleGroup; object_key: string; target_id: string; status: string; reason: string | null }>;
}

export function dryRunSampleImport(
  themeId: string,
  groups: SampleGroup[],
  csrfToken: string,
): Promise<ImportPlan> {
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/import/dry-run`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ groups }),
    defaultErrorMessage: tStatic('api.themes.preview_the_sample_data_import_plan'),
  });
}

export function executeSampleImport(
  themeId: string,
  plan: ImportPlan,
  acknowledgePublicChanges: boolean,
  csrfToken: string,
): Promise<ImportExecutionOutcome> {
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/import/execute`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({
      groups: plan.selected_groups,
      revision_id: plan.revision_id,
      plan_digest: plan.plan_digest,
      expected_versions: plan.expected_versions,
      acknowledge_public_changes: acknowledgePublicChanges,
    }),
    defaultErrorMessage: tStatic('api.themes.import_sample_data'),
  });
}
