import { evaluateConditions } from '../../fields/conditions.ts';
import type { FieldSpec } from '../../fields/types.ts';
import type { ContentTypeSettings } from '../../api/content.ts';
import type { ContentFieldValue, EffectiveEditorConfigProjection } from '../content/types.ts';

function supportAllowsField(typeKey: string, key: string, settings: ContentTypeSettings): boolean {
  if (typeKey !== 'post' && typeKey !== 'page') return true;
  if (key === 'body') return settings.supports.includes('editor') || settings.supports.includes('builder');
  if (key === 'featured_image') return settings.supports.includes('featured_image');
  return true;
}

export function matchesMediaPickerKind(
  fieldType: FieldSpec['type'] | undefined,
  kind: 'image' | 'gallery',
): boolean {
  return kind === 'image' ? fieldType === 'media' : fieldType === 'gallery';
}

function isEmptyRequiredValue(spec: FieldSpec, val: unknown): boolean {
  if (val === undefined || val === null) return true;
  if (typeof val === 'string') return !val.trim();
  if (Array.isArray(val)) return val.length === 0;
  if (spec.type === 'link' && typeof val === 'object') {
    const url = (val as { url?: unknown }).url;
    return typeof url !== 'string' || !url.trim();
  }
  return false;
}

export function validateRequiredEditorFields(
  config: EffectiveEditorConfigProjection,
  typeKey: string,
  settings: ContentTypeSettings,
  snapshot: { values: Record<string, ContentFieldValue>; cleared_fields: string[] },
  missingMsg: (label: string) => string,
): Record<string, string> {
  const rootSpecs = Object.values(config.effective_fields).map((e) => e.definition);
  const rootValues = snapshot.values as Record<string, unknown>;
  const errors: Record<string, string> = {};

  for (const [fk, eff] of Object.entries(config.effective_fields)) {
    if (!eff.effective_required || !eff.editable || !supportAllowsField(typeKey, fk, settings)) {
      continue;
    }
    if (!evaluateConditions(eff.definition.conditions, rootSpecs, rootValues, rootSpecs, rootValues)) {
      continue;
    }
    const empty = snapshot.cleared_fields.includes(fk) || isEmptyRequiredValue(eff.definition, snapshot.values[fk]);
    if (empty) {
      errors[fk] = missingMsg(eff.definition.label);
    }
  }
  return errors;
}
