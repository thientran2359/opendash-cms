import { useI18n } from '../../i18n/index.tsx';
import React, { useMemo } from 'react';
import type { ThemeOptionDef } from '../../api/themes.ts';
import { BlockSchemaForm } from '../../builder/BlockSchemaForm.tsx';
import type { BuilderViewport } from '../../builder/document.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import type { BlockFieldSpec, BlockSchemaV1, PluginSettingField } from '../../types.ts';

export type OwnerKind = 'theme' | 'plugin';
export type OwnerOptionDef = ThemeOptionDef | PluginSettingField;

export function sameOwnerOptionValues(
  a: Record<string, unknown>,
  b: Record<string, unknown>,
): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function isSecretConfiguredValue(value: unknown): boolean {
  return Boolean(
    value &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      (value as { configured?: unknown }).configured === true,
  );
}

export function isClearSecretMarker(value: unknown): value is { clear: true } {
  return Boolean(
    value &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      (value as { clear?: unknown }).clear === true,
  );
}

export function upcastOwnerDefToBlockField(def: OwnerOptionDef): BlockFieldSpec | null {
  const required = 'required' in def ? Boolean(def.required) : false;
  switch (def.type) {
    case 'secret':
      return null;
    case 'boolean':
      return {
        type: 'boolean',
        key: def.key,
        label: def.label,
        default: typeof def.default === 'boolean' ? def.default : false,
      };
    case 'number':
      return {
        type: 'number',
        key: def.key,
        label: def.label,
        min: def.min ?? null,
        max: def.max ?? null,
        required,
        default: typeof def.default === 'number' ? def.default : 0,
      };
    case 'select':
      return {
        type: 'select',
        key: def.key,
        label: def.label,
        options: (def.choices ?? []).map(c => ({ value: c.value, label: c.label })),
        required,
        default: typeof def.default === 'string' ? def.default : undefined,
      };
    case 'image_ref':
    case 'media':
      return {
        type: 'media',
        key: def.key,
        label: def.label,
        media_kind: 'image',
        required,
        default: typeof def.default === 'string' ? def.default : undefined,
      };
    case 'url':
      return {
        type: 'url',
        key: def.key,
        label: def.label,
        required,
        default: typeof def.default === 'string' ? def.default : '',
      };
    case 'textarea':
      return {
        type: 'textarea',
        key: def.key,
        label: def.label,
        max_chars: 4096,
        required,
        default: typeof def.default === 'string' ? def.default : '',
      };
    default:
      return {
        type: 'text',
        key: def.key,
        label: def.label,
        max_chars: 4096,
        required,
        default: typeof def.default === 'string' ? def.default : '',
      };
  }
}

export function normalizeOwnerValuesForSchema(
  next: Record<string, unknown>,
  schema: OwnerOptionDef[],
  baseline: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...next };
  for (const def of schema) {
    const val = out[def.key];
    if (def.type === 'secret') {
      if (val === undefined && def.key in baseline) {
        out[def.key] = baseline[def.key];
      }
      continue;
    }
    if (def.type === 'image_ref' || def.type === 'media') {
      if (val === '' || val === undefined) out[def.key] = null;
    } else if (def.type === 'number' && (val === '' || val === undefined)) {
      out[def.key] = typeof def.default === 'number' ? def.default : 0;
    } else if (def.type === 'spacing' && val === undefined) {
      out[def.key] = {};
    } else if (val === undefined && def.key in baseline) {
      out[def.key] = baseline[def.key];
    }
  }
  return out;
}

export function buildOwnerOptionsSubmitPayload(
  schema: OwnerOptionDef[],
  draft: Record<string, unknown>,
  keysFilter?: Set<string>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const def of schema) {
    if (keysFilter && !keysFilter.has(def.key)) continue;
    const raw = draft[def.key];
    if (def.type === 'secret') {
      if (typeof raw === 'string' && raw.length > 0) {
        payload[def.key] = raw;
      } else if (isClearSecretMarker(raw)) {
        payload[def.key] = { clear: true };
      } else {
        payload[def.key] = null;
      }
    } else if (def.key in draft) {
      payload[def.key] = raw;
    }
  }
  return payload;
}

export interface OwnerOptionsFormProps {
  ownerKind: OwnerKind;
  ownerId: string;
  title: string;
  schema: OwnerOptionDef[];
  fields?: BlockFieldSpec[];
  values: Record<string, unknown>;
  savedValues?: Record<string, unknown>;
  version: number;
  csrfToken: string | null;
  viewport?: BuilderViewport;
  saving?: boolean;
  notice?: string;
  error?: string;
  hideSubmitButton?: boolean;
  submitLabel?: string;
  testId?: string;
  headerExtra?: React.ReactNode;
  beforeFields?: React.ReactNode;
  afterFields?: React.ReactNode;
  onChange: (nextValues: Record<string, unknown>) => void;
  onSubmit?: () => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const OwnerOptionsForm: React.FC<OwnerOptionsFormProps> = ({
  ownerKind,
  ownerId,
  title,
  schema,
  fields,
  values,
  savedValues,
  version,
  csrfToken,
  viewport = 'desktop',
  saving = false,
  notice = '',
  error = '',
  hideSubmitButton = false,
  submitLabel,
  testId,
  headerExtra,
  beforeFields,
  afterFields,
  onChange,
  onSubmit,
  onApiError,
}) => {
  const { t: translate } = useI18n();
  const blockSchema: BlockSchemaV1 = useMemo(() => {
    const resolvedFields =
      fields ??
      schema
        .map(upcastOwnerDefToBlockField)
        .filter((f): f is BlockFieldSpec => f !== null);
    return {
      schema_version: 1,
      id: `${ownerKind}/${ownerId}`,
      title: ownerId,
      category: 'content',
      render: 'render.html',
      fields: resolvedFields,
    };
  }, [ownerKind, ownerId, schema, fields]);

  const secretFields = useMemo(
    () => schema.filter((d) => d.type === 'secret'),
    [schema],
  );

  const handleStandardChange = (nextStandard: Record<string, unknown>) => {
    const merged = { ...values, ...nextStandard };
    onChange(normalizeOwnerValuesForSchema(merged, schema, savedValues ?? values));
  };

  const renderFields = () => (
    <>
      {blockSchema.fields.length > 0 && (
        <BlockSchemaForm
          schema={blockSchema}
          values={values}
          onChange={handleStandardChange}
          viewport={viewport}
          csrfToken={csrfToken}
          onError={onApiError}
          disabled={saving}
        />
      )}
      {secretFields.length > 0 && (
        <div className="space-y-3 border-t border-zinc-100 pt-3" data-testid="owner-secret-fields">
          {secretFields.map((field) => {
            const draftVal = values[field.key];
            const baselineVal = savedValues?.[field.key];
            const configured = isSecretConfiguredValue(baselineVal ?? draftVal);
            const clearing = isClearSecretMarker(draftVal);
            const pendingPlaintext = typeof draftVal === 'string' ? draftVal : '';
            const hasPendingChange = clearing || pendingPlaintext.length > 0;
            const inputId = `${ownerKind}-${ownerId}-secret-${field.key}`;
            const isRequired = 'required' in field && Boolean(field.required);
            return (
              <div
                key={field.key}
                className="space-y-1.5 rounded-lg border border-zinc-200 bg-zinc-50/70 p-3"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <label htmlFor={inputId} className="text-xs font-medium text-zinc-800">
                    {field.label}
                    {isRequired && (
                      <span className="ml-1 text-rose-600" aria-hidden="true">*</span>
                    )}
                  </label>
                  {clearing ? (
                    <AdminBadge tone="warning">{translate('feature.shared.owneroptionsform.will_be_cleared_on_save')}</AdminBadge>
                  ) : pendingPlaintext.length > 0 ? (
                    <AdminBadge tone="warning">{translate('feature.shared.owneroptionsform.will_be_updated_on_save')}</AdminBadge>
                  ) : configured ? (
                    <AdminBadge tone="success">{translate('feature.shared.owneroptionsform.configured')}</AdminBadge>
                  ) : (
                    <AdminBadge tone="neutral">{translate('feature.shared.owneroptionsform.not_configured')}</AdminBadge>
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <input
                    id={inputId}
                    type="password"
                    autoComplete="new-password"
                    disabled={saving || clearing}
                    value={pendingPlaintext}
                    placeholder={
                      clearing
                        ? translate('feature.shared.owneroptionsform.the_secret_will_be_cleared_when_you_save')
                        : configured
                          ? translate('feature.shared.owneroptionsform.leave_blank_to_keep_unchanged')
                          : translate('feature.shared.owneroptionsform.enter_a_new_secret')
                    }
                    onChange={(event) => {
                      const nextText = event.currentTarget.value;
                      onChange({
                        ...values,
                        [field.key]:
                          nextText.length > 0
                            ? nextText
                            : (baselineVal ?? { configured: false }),
                      });
                    }}
                    className="min-w-0 flex-1 rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900 placeholder:text-zinc-400 focus:border-od-accent focus:outline-none disabled:bg-zinc-100"
                  />
                  {configured && !isRequired && !clearing && (
                    <AdminButton
                      variant="danger"
                      size="sm"
                      disabled={saving}
                      onClick={() => onChange({ ...values, [field.key]: { clear: true } })}
                    >{translate('feature.shared.owneroptionsform.clear_secret')}</AdminButton>
                  )}
                  {hasPendingChange && (
                    <AdminButton
                      variant="ghost"
                      size="sm"
                      disabled={saving}
                      onClick={() =>
                        onChange({
                          ...values,
                          [field.key]: baselineVal ?? { configured: false },
                        })
                      }
                    >{translate('feature.shared.owneroptionsform.undo')}</AdminButton>
                  )}
                </div>
                <p className="text-[11px] text-zinc-500">{translate('feature.shared.owneroptionsform.the_secret_is_write_only_leave_blank_to_keep_it_unchanged')}</p>
              </div>
            );
          })}
        </div>
      )}
    </>
  );

  if (hideSubmitButton) {
    return (
      <div data-testid={testId} className="space-y-3">
        <div className="flex items-center justify-between gap-2 text-xs text-zinc-500">
          <span>
            {ownerKind === 'theme' ? 'Theme: ' : 'Plugin: '}
            <strong className="font-medium text-zinc-700">{ownerId}</strong>
          </span>
          <AdminBadge tone="neutral">v{version}</AdminBadge>
        </div>
        {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
        {error && <AdminAlert tone="error">{error}</AdminAlert>}
        {renderFields()}
      </div>
    );
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit?.();
      }}
      data-testid={testId}
      className="space-y-4 rounded-xl border border-zinc-200 bg-white p-5"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-100 pb-3">
        <h3 className="text-base font-semibold text-zinc-950">{title}</h3>
        <div className="flex items-center gap-2">
          {headerExtra}
          <AdminBadge tone="neutral">v{version}</AdminBadge>
        </div>
      </div>
      {beforeFields}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {renderFields()}
      {afterFields}
      <div className="pt-1">
        <AdminButton type="submit" variant="primary" size="md" disabled={!csrfToken || saving}>
          {submitLabel ?? (saving ? translate('general.saving') : translate('general.save'))}
        </AdminButton>
      </div>
    </form>
  );
};
