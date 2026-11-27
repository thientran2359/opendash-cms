import React, { useMemo } from 'react';
import { useI18n } from '../../i18n/index.tsx';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import { UnifiedFieldForm } from '../../fields/UnifiedFieldForm.tsx';
import type {
  ContentFieldValue,
  EffectiveEditorConfigProjection,
  EffectivePanelProjection,
} from '../content/types.ts';

interface Props {
  config: EffectiveEditorConfigProjection;
  proj: EffectivePanelProjection;
  fieldKeys: readonly string[];
  usePanelFields?: boolean;
  labelPlacement?: 'top' | 'left';
  values: Record<string, ContentFieldValue>;
  clearedFields: readonly string[];
  fieldErrors: Record<string, string>;
  csrfToken: string | null;
  locale?: string | null;
  siteTimezone?: string;
  onFieldChange: (key: string, value: ContentFieldValue) => void;
  onFieldClear: (key: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const PanelFieldSliceForm: React.FC<Props> = ({
  config,
  proj,
  fieldKeys,
  usePanelFields = false,
  labelPlacement = 'top',
  values,
  clearedFields,
  fieldErrors,
  csrfToken,
  locale,
  siteTimezone = 'UTC',
  onFieldChange,
  onFieldClear,
  onApiError,
}) => {
  const { __ } = useI18n();

  const rootSpecs = useMemo(
    () => Object.values(config.effective_fields).map((e) => e.definition),
    [config.effective_fields],
  );

  const sliceSpecs = useMemo<FieldSpec[]>(() => {
    const allowedSet = new Set(fieldKeys);
    if (usePanelFields && proj.panel_fields && proj.panel_fields.length > 0) {
      return proj.panel_fields.filter(
        (spec) => isUiOnlyFieldType(spec.type) || (allowedSet.has(spec.key) && Boolean(config.effective_fields[spec.key])),
      );
    }
    return fieldKeys
      .map((fk) => config.effective_fields[fk]?.definition)
      .filter((spec): spec is FieldSpec => Boolean(spec));
  }, [usePanelFields, proj.panel_fields, fieldKeys, config.effective_fields]);

  const readOnlyKeys = useMemo(() => {
    const ro = new Set<string>();
    for (const fk of fieldKeys) {
      const eff = config.effective_fields[fk];
      if (!eff) continue;
      if (
        !eff.editable ||
        ((eff.definition.type === 'media' || eff.definition.type === 'gallery') &&
          eff.effective_status !== 'active')
      ) {
        ro.add(fk);
      }
    }
    return ro;
  }, [fieldKeys, config.effective_fields]);

  const statusItems = fieldKeys
    .map((fk) => ({ fk, eff: config.effective_fields[fk] }))
    .filter(({ fk, eff }) => {
      if (!eff) return false;
      const withdrawn = eff.effective_status !== 'active';
      const readOnly = !eff.editable;
      const canClear = eff.editable && !eff.effective_required && fk in values && !clearedFields.includes(fk);
      return withdrawn || readOnly || canClear;
    });

  if (sliceSpecs.length === 0) return null;

  return (
    <div
      className={labelPlacement === 'left' ? 'space-y-2.5 [&_.space-y-1]:md:flex [&_.space-y-1]:md:items-start [&_.space-y-1]:md:gap-3' : 'space-y-2.5'}
      data-label-placement={labelPlacement}
    >
      {statusItems.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-xs text-slate-600">
          {statusItems.map(({ fk, eff }) => {
            if (!eff) return null;
            const isWithdrawn = eff.effective_status !== 'active';
            const canClear = eff.editable && !eff.effective_required && fk in values && !clearedFields.includes(fk);
            return (
              <span key={fk} className="inline-flex items-center gap-1.5">
                {isWithdrawn && (
                  <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[11px] font-semibold text-amber-800">
                    {eff.definition.label}: {__("Fallback")} ({eff.effective_status})
                  </span>
                )}
                {!eff.editable && (
                  <span className="rounded bg-slate-100 px-1.5 py-0.5 text-[11px] font-semibold text-slate-600">
                    {eff.definition.label}: {__("Read-only")}
                  </span>
                )}
                {canClear && (
                  <button
                    type="button"
                    onClick={() => onFieldClear(fk)}
                    className="text-xs text-slate-500 underline hover:text-rose-700"
                  >
                    {__("Clear value")} ({eff.definition.label})
                  </button>
                )}
              </span>
            );
          })}
        </div>
      )}
      <UnifiedFieldForm
        context="content"
        layoutMode="grid"
        fields={sliceSpecs}
        rootSpecs={rootSpecs}
        rootValues={values}
        values={values}
        onChange={(next) => {
          for (const spec of sliceSpecs) {
            if (isUiOnlyFieldType(spec.type)) continue;
            if (spec.key in next && next[spec.key] !== values[spec.key]) {
              onFieldChange(spec.key, next[spec.key] as ContentFieldValue);
            }
          }
        }}
        csrfToken={csrfToken}
        contextTypeKey={config.type_key}
        locale={locale ?? null}
        siteTimezone={siteTimezone}
        readOnlyKeys={readOnlyKeys}
        fieldErrors={fieldErrors}
        onApiError={onApiError}
      />
    </div>
  );
};
