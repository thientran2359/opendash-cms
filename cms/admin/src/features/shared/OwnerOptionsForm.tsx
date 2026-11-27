import { __ } from '../../i18n/index.tsx';
import React from 'react';
import type { BuilderViewport } from '../../builder/document.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import { UnifiedFieldForm } from '../../fields/UnifiedFieldForm.tsx';

export type OwnerKind = 'theme' | 'plugin' | 'theme_functions';

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

export function normalizeOwnerValuesForFields(
  next: Record<string, unknown>,
  fields: readonly FieldSpec[],
  baseline: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...next };
  for (const spec of fields) {
    if (isUiOnlyFieldType(spec.type)) continue;
    const val = out[spec.key];
    if (spec.type === 'secret') {
      if (val === undefined && spec.key in baseline) {
        out[spec.key] = baseline[spec.key];
      }
      continue;
    }
    if (spec.type === 'media') {
      if (val === '' || val === undefined) out[spec.key] = null;
    } else if (spec.type === 'spacing' && val === undefined) {
      out[spec.key] = {};
    } else if (val === undefined && spec.key in baseline) {
      out[spec.key] = baseline[spec.key];
    }
  }
  return out;
}

export function buildOwnerOptionsSubmitPayload(
  fields: readonly FieldSpec[],
  draft: Record<string, unknown>,
  keysFilter?: Set<string>,
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const spec of fields) {
    if (isUiOnlyFieldType(spec.type)) continue;
    if (keysFilter && !keysFilter.has(spec.key)) continue;
    const raw = draft[spec.key];
    if (spec.type === 'secret') {
      if (typeof raw === 'string' && raw.length > 0) {
        payload[spec.key] = raw;
      } else if (isClearSecretMarker(raw)) {
        payload[spec.key] = { clear: true };
      } else {
        payload[spec.key] = null;
      }
    } else if (spec.key in draft) {
      payload[spec.key] = raw;
    }
  }
  return payload;
}

export interface OwnerOptionsFormProps {
  ownerKind: OwnerKind;
  ownerId: string;
  title: string;
  fields: readonly FieldSpec[];
  values: Record<string, unknown>;
  savedValues?: Record<string, unknown>;
  version: number;
  csrfToken: string | null;
  viewport?: BuilderViewport;
  locale?: string | null;
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
  fields,
  values,
  savedValues,
  version,
  csrfToken,
  viewport = 'desktop',
  locale,
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

  const renderFields = () => (
    <UnifiedFieldForm
      fields={fields}
      context={ownerKind === 'theme' ? 'options' : 'owner_settings'}
      layoutMode={hideSubmitButton ? 'sidebar' : 'grid'}
      values={values}
      savedValues={savedValues}
      onChange={(next) =>
        onChange(normalizeOwnerValuesForFields(next, fields, savedValues ?? values))
      }
      viewport={viewport}
      csrfToken={csrfToken}
      locale={locale}
      disabled={saving}
      onApiError={onApiError}
    />
  );

  if (hideSubmitButton) {
    return (
      <div data-testid={testId} className="space-y-3">
        <div className="flex items-center justify-between gap-2 text-xs text-zinc-500">
          <span>
            {ownerKind === 'theme' || ownerKind === 'theme_functions' ? __("Theme: ") : __("Plugin: ")}
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
          {submitLabel ?? (saving ? __("Saving…") : __("Save settings"))}
        </AdminButton>
      </div>
    </form>
  );
};
