import { __ } from '../i18n/index.tsx';
import type { BuilderDocument } from '../builder/document.ts';
import type {
  HomepagePreview,
  TemplateEffectiveSource,
  TemplatePreviewEntry,
  TemplatePreviewTerm,
  TemplateTargetInput,
} from './builder.ts';
import { requestJson } from './client.ts';
import type { GeneralSettingsDraft } from './settings.ts';

export type StudioTargetGroup =
  | 'parts'
  | 'core'
  | 'content_types'
  | 'taxonomies'
  | 'page_layouts'
  | 'custom_templates'
  | 'overrides';

export type EntryContentRule = 'required_or_entry_block' | 'optional_max_one' | 'forbidden';

export interface StudioTargetItem {
  canonical_target_key: string;
  label: string;
  group: StudioTargetGroup;
  target: TemplateTargetInput;
  override_id: number | null;
  version: number | null;
  effective_source: TemplateEffectiveSource;
  entry_content_rule: EntryContentRule;
  has_theme_source: boolean;
}

export interface StudioTargetsResponse {
  active_theme_id: string | null;
  active_theme_name: string | null;
  items: StudioTargetItem[];
}

export interface TransientStudioOverrides {
  general?: GeneralSettingsDraft;
  global_options?: Record<string, unknown>;
  theme_options?: Record<string, unknown>;
  menu_slots?: Record<string, number | null>;
}

export interface StudioPreviewInput {
  target: TemplateTargetInput;
  document?: BuilderDocument | null;
  preview_entry?: TemplatePreviewEntry | null;
  preview_term?: TemplatePreviewTerm | null;
  preview_annotations?: boolean;
  preview_locale?: string | null;
  transient?: TransientStudioOverrides | null;
}

export function getStudioTargets(signal?: AbortSignal): Promise<StudioTargetsResponse> {
  return requestJson<StudioTargetsResponse>('/api/admin/studio/targets', {
    signal,
    defaultErrorMessage: __("Could not load OpenDash Studio targets."),
  });
}

export function postStudioPreview(
  input: StudioPreviewInput,
  csrfToken: string,
  signal?: AbortSignal,
): Promise<HomepagePreview> {
  const payload: Record<string, unknown> = { target: input.target };
  if (input.document !== undefined) payload.document = input.document;
  if (input.preview_entry !== undefined) payload.preview_entry = input.preview_entry;
  if (input.preview_term !== undefined) payload.preview_term = input.preview_term;
  if (input.preview_annotations !== undefined) payload.preview_annotations = input.preview_annotations;
  if (input.preview_locale) payload.preview_locale = input.preview_locale;
  if (input.transient !== undefined && input.transient !== null) {
    const transient: TransientStudioOverrides = {};
    if (input.transient.general) {
      transient.general = {
        site_title: input.transient.general.site_title,
        tagline: input.transient.general.tagline,
        timezone: input.transient.general.timezone,
      };
    }
    if (input.transient.global_options) {
      transient.global_options = input.transient.global_options;
    }
    if (input.transient.theme_options) {
      transient.theme_options = input.transient.theme_options;
    }
    if (input.transient.menu_slots) {
      transient.menu_slots = input.transient.menu_slots;
    }
    if (Object.keys(transient).length > 0) {
      payload.transient = transient;
    }
  }
  return requestJson<HomepagePreview>('/api/admin/studio/preview', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    signal,
    body: JSON.stringify(payload),
    defaultErrorMessage: __("Could not generate the OpenDash Studio preview."),
  });
}
