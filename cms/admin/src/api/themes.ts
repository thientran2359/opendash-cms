import { requestJson } from './client.ts';

export interface ThemeItem {
  id: string;
  compatible_block_id: string | null;
}

export interface ThemesSummary {
  items: ThemeItem[];
  active_id: string | null;
  version: number;
  homepage_compatible: boolean | null;
}

export type ThemeOptionType = 'string' | 'number' | 'boolean' | 'select' | 'image_ref';

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
  min?: number;
  max?: number;
  choices?: ThemeOptionChoice[];
}

export interface ThemeOptionsSnapshot {
  theme_id: string;
  version: number;
  schema: ThemeOptionDef[];
  values: Record<string, unknown>;
}

export function getThemes(): Promise<ThemesSummary> {
  return requestJson('/api/admin/themes', {
    defaultErrorMessage: 'Không thể tải danh sách theme.',
  });
}

export function installTheme(file: File, csrfToken: string): Promise<ThemeItem> {
  return requestJson('/api/admin/themes', {
    method: 'POST',
    body: file,
    contentType: 'application/zip',
    csrfToken,
    defaultErrorMessage: 'Không thể cài theme ZIP.',
  });
}

export function activateTheme(id: string, version: number, csrfToken: string): Promise<ThemesSummary> {
  return requestJson('/api/admin/themes/active', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ id }),
    defaultErrorMessage: 'Không thể kích hoạt theme.',
  });
}

export function getThemeOptions(themeId: string): Promise<ThemeOptionsSnapshot> {
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/options`, {
    defaultErrorMessage: 'Không thể tải tùy chọn theme.',
  });
}

export function saveThemeOptions(
  themeId: string,
  values: Record<string, unknown>,
  version: number,
  csrfToken: string,
): Promise<ThemeOptionsSnapshot> {
  return requestJson(`/api/themes/${encodeURIComponent(themeId)}/options`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify({ values }),
    defaultErrorMessage: 'Không thể lưu tùy chọn theme.',
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
    defaultErrorMessage: 'Không thể xem trước kế hoạch nhập dữ liệu mẫu.',
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
    defaultErrorMessage: 'Không thể thực thi nhập dữ liệu mẫu.',
  });
}
