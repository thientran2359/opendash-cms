import { isUiOnlyFieldType } from '../fields/helpers.ts';
import type { FieldSpec } from '../fields/types.ts';
import type { StudioOptionsScope, StudioPanelId } from '../shell/studioRouter.ts';

export interface StudioOptionSection {
  key: string;
  label: string;
  description?: string;
  fields: FieldSpec[];
}

export interface StudioBackTarget {
  kind: 'root' | 'options' | 'theme' | 'custom_page' | 'templates';
  hash: string;
}

/**
 * Splits a field list by top-level UI-only `accordion` / `tab` dividers into
 * flat drill-down sections so Studio sidebar sub-routes render without nested accordions.
 */
export function partitionFieldSections(
  fields: FieldSpec[],
  defaultLabel: string,
): StudioOptionSection[] {
  const hasDivider = fields.some(spec => spec.type === 'accordion' || spec.type === 'tab');
  if (!hasDivider) {
    return fields.length > 0
      ? [{ key: 'default', label: defaultLabel, fields }]
      : [];
  }

  const sections: StudioOptionSection[] = [];
  let current: StudioOptionSection | null = null;
  for (const spec of fields) {
    if (spec.type === 'accordion' || spec.type === 'tab') {
      current = {
        key: spec.key,
        label: spec.label || spec.key,
        description: spec.instructions,
        fields: [],
      };
      sections.push(current);
      continue;
    }
    if (!current) {
      current = {
        key: 'general',
        label: defaultLabel,
        fields: [],
      };
      sections.push(current);
    }
    current.fields.push(spec);
  }
  return sections.filter(sec => sec.fields.length > 0);
}

/**
 * Matches backend `HydrationVisibility::OptionsPublicOnly` + `child_visibility()`:
 * root option fields require `public: true`, `min_view_role !== 'admin'`, and `public_exposure !== 'private'`;
 * nested `group`/`repeater` children inherit Editor-readable public visibility unless explicitly restricted.
 */
export function isFieldPublicPreviewable(spec: FieldSpec, isRoot: boolean): boolean {
  if (isUiOnlyFieldType(spec.type) || spec.type === 'secret') return false;
  const minView = spec.acl?.min_view_role ?? 'editor';
  if (minView === 'admin') return false;
  if (isRoot) {
    const exposure = spec.acl?.public_exposure ?? (spec.public ? 'public' : 'private');
    return Boolean(spec.public) && exposure === 'public';
  }
  const exposure = spec.acl?.public_exposure ?? 'public';
  return exposure === 'public';
}

function sanitizeFieldValueForPreview(
  spec: FieldSpec,
  value: unknown,
  isRoot: boolean,
): unknown {
  if (!isFieldPublicPreviewable(spec, isRoot)) return undefined;
  if (spec.type === 'group' && value && typeof value === 'object' && !Array.isArray(value)) {
    return stripNonPublicPreviewValues(spec.fields ?? [], value as Record<string, unknown>, false);
  }
  if (spec.type === 'repeater' && Array.isArray(value)) {
    const subSpecs = spec.fields ?? [];
    return value.map(item =>
      item && typeof item === 'object' && !Array.isArray(item)
        ? stripNonPublicPreviewValues(subSpecs, item as Record<string, unknown>, false)
        : item,
    );
  }
  return value;
}

/**
 * Recursively removes non-public, admin-only, and secret fields (including nested
 * Group/Repeater children) before sending unsaved values to `POST /api/admin/studio/preview`.
 */
export function stripNonPublicPreviewValues(
  specs: FieldSpec[],
  values: Record<string, unknown>,
  isRoot = true,
): Record<string, unknown> {
  const byKey = new Map<string, FieldSpec>();
  for (const spec of specs) {
    if (!isUiOnlyFieldType(spec.type)) byKey.set(spec.key, spec);
  }
  const out: Record<string, unknown> = {};
  for (const [key, raw] of Object.entries(values)) {
    const spec = byKey.get(key);
    if (!spec || !isFieldPublicPreviewable(spec, isRoot)) continue;
    const cleaned = sanitizeFieldValueForPreview(spec, raw, isRoot);
    if (cleaned !== undefined) {
      out[key] = cleaned;
    }
  }
  return out;
}

export function buildStudioHash(
  panel: StudioPanelId,
  optionsRoute?: StudioOptionsScope,
  templateKey?: string | null,
): string {
  if (panel === 'root') return '#/studio';
  if (panel === 'identity') return '#/studio/identity';
  if (panel === 'menus') return '#/studio/menus';
  if (panel === 'reading') return '#/studio/reading';
  if (panel === 'templates') {
    return templateKey
      ? `#/studio/templates/${encodeURIComponent(templateKey)}`
      : '#/studio/templates';
  }
  if (!optionsRoute || optionsRoute.kind === 'index') return '#/studio/options';
  if (optionsRoute.kind === 'global') {
    return `#/studio/options/global/${encodeURIComponent(optionsRoute.section)}`;
  }
  if (optionsRoute.kind === 'theme') {
    return optionsRoute.section
      ? `#/studio/options/theme/${encodeURIComponent(optionsRoute.section)}`
      : '#/studio/options/theme';
  }
  return optionsRoute.groupId
    ? `#/studio/options/custom/${encodeURIComponent(optionsRoute.pageKey)}/${encodeURIComponent(optionsRoute.groupId)}`
    : `#/studio/options/custom/${encodeURIComponent(optionsRoute.pageKey)}`;
}

export function resolveStudioBackTarget(
  panel: StudioPanelId,
  optionsRoute: StudioOptionsScope | undefined,
  templateKey: string | null | undefined,
  counts: { themeSectionCount: number; customGroupCount: number } = {
    themeSectionCount: 1,
    customGroupCount: 1,
  },
): StudioBackTarget | null {
  if (panel === 'root') return null;
  if (panel === 'identity' || panel === 'menus' || panel === 'reading') {
    return { kind: 'root', hash: '#/studio' };
  }
  if (panel === 'templates') {
    return templateKey
      ? { kind: 'templates', hash: '#/studio/templates' }
      : { kind: 'root', hash: '#/studio' };
  }
  if (!optionsRoute || optionsRoute.kind === 'index') {
    return { kind: 'root', hash: '#/studio' };
  }
  if (optionsRoute.kind === 'global') {
    return { kind: 'options', hash: '#/studio/options' };
  }
  if (optionsRoute.kind === 'theme') {
    if (optionsRoute.section && counts.themeSectionCount > 1) {
      return { kind: 'theme', hash: '#/studio/options/theme' };
    }
    return { kind: 'options', hash: '#/studio/options' };
  }
  if (optionsRoute.groupId && counts.customGroupCount > 1) {
    return {
      kind: 'custom_page',
      hash: `#/studio/options/custom/${encodeURIComponent(optionsRoute.pageKey)}`,
    };
  }
  return { kind: 'options', hash: '#/studio/options' };
}
