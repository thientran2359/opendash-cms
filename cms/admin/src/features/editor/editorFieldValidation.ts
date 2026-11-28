import { evaluateConditions } from '../../fields/conditions.ts';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import type { ContentTypeSettings, EffectiveEditorConfigWithSettings } from '../../api/content.ts';
import type {
  ContentFieldValue,
  EffectiveEditorConfigProjection,
  EffectivePanelProjection,
} from '../content/types.ts';
import {
  evaluateLocationRules,
  type LocationEvalContext,
} from '../field-groups/locationRulesEvaluator.ts';

export function supportAllowsField(
  typeKey: string,
  key: string,
  settings: ContentTypeSettings,
): boolean {
  if (key === 'featured_image' && typeKey !== 'section' && typeKey !== 'pattern') {
    return settings.supports.includes('featured_image');
  }
  if (key === 'excerpt' && typeKey !== 'section' && typeKey !== 'pattern') {
    return settings.supports.includes('excerpt');
  }
  if (typeKey !== 'post' && typeKey !== 'page') return true;
  if (key === 'body') return settings.supports.includes('editor') || settings.supports.includes('builder');
  return true;
}

export function displayEditorConfigWithActiveKeys(
  config: EffectiveEditorConfigWithSettings,
  settings: ContentTypeSettings,
  locationCtx: LocationEvalContext,
): {
  projected: EffectiveEditorConfigProjection;
  activeFieldKeys: Set<string>;
} {
  const typeKey = config.type_key;
  const activeFieldKeys = new Set<string>();

  const projectPanels = (panels: EffectivePanelProjection[]): EffectivePanelProjection[] =>
    panels
      .filter(({ panel, location }) => {
        if (panel.kind.kind === 'core_built_in') {
          if (panel.kind.core_key === 'body_editor') {
            return settings.supports.includes('editor') || settings.supports.includes('builder');
          }
          if (panel.kind.core_key === 'featured_image') {
            return settings.supports.includes('featured_image');
          }
          if (panel.kind.core_key === 'excerpt') {
            return settings.supports.includes('excerpt');
          }
          if (panel.kind.core_key === 'page_attributes') {
            return Boolean(
              typeKey === 'page' ||
                settings.hierarchical ||
                settings.supports.includes('menu_order') ||
                settings.supports.includes('custom_template'),
            );
          }
          return true;
        }
        if (location && location.length > 0) {
          return evaluateLocationRules(location, locationCtx);
        }
        return true;
      })
      .map(panel => ({
        ...panel,
        field_keys: panel.field_keys.filter(key => supportAllowsField(typeKey, key, settings)),
        panel_fields: panel.panel_fields?.filter(
          spec => isUiOnlyFieldType(spec.type) || supportAllowsField(typeKey, spec.key, settings),
        ),
        subgroups: panel.subgroups
          .map(group => ({
            ...group,
            field_keys: group.field_keys.filter(key => supportAllowsField(typeKey, key, settings)),
          }))
          .filter(group => group.field_keys.length > 0),
      }))
      .filter(panel => {
        if (panel.panel.kind.kind === 'content_fields') {
          return panel.field_keys.length > 0;
        }
        return true;
      });

  const regions = {
    main_top: projectPanels(config.regions.main_top),
    main_primary: projectPanels(config.regions.main_primary),
    main_bottom: projectPanels(config.regions.main_bottom),
    sidebar: projectPanels(config.regions.sidebar),
  };

  for (const panel of [
    ...regions.main_top,
    ...regions.main_primary,
    ...regions.main_bottom,
    ...regions.sidebar,
  ]) {
    for (const fk of panel.field_keys) {
      if (supportAllowsField(typeKey, fk, settings)) {
        activeFieldKeys.add(fk);
      }
    }
  }
  // Core managed fields active via CPT supports remain active if not rendered inside content_fields.
  for (const fk of ['body', 'featured_image', 'excerpt']) {
    if (fk in config.effective_fields && supportAllowsField(typeKey, fk, settings)) {
      activeFieldKeys.add(fk);
    }
  }

  return {
    activeFieldKeys,
    projected: {
      ...config,
      effective_fields: Object.fromEntries(
        Object.entries(config.effective_fields).filter(([key]) =>
          activeFieldKeys.has(key) && supportAllowsField(typeKey, key, settings),
        ),
      ),
      regions,
    },
  };
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
  activeFieldKeys?: ReadonlySet<string>,
): Record<string, string> {
  const rootSpecs = Object.values(config.effective_fields).map((e) => e.definition);
  const rootValues = snapshot.values as Record<string, unknown>;
  const errors: Record<string, string> = {};

  for (const [fk, eff] of Object.entries(config.effective_fields)) {
    if (
      (activeFieldKeys && !activeFieldKeys.has(fk)) ||
      !eff.effective_required ||
      !eff.editable ||
      !supportAllowsField(typeKey, fk, settings)
    ) {
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
