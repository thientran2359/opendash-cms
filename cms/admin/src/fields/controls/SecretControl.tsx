import React, { useId, useState } from 'react';
import { STUDIO_INPUT_CLASS } from '../../builder/StudioFieldControls.tsx';
import { AdminButton } from '../../components/AdminPrimitives.tsx';
import { useI18n } from '../../i18n/index.tsx';
import type { FieldSpec } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

function isConfiguredSentinel(value: unknown): boolean {
  return Boolean(
    value &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      (value as { configured?: unknown }).configured === true,
  );
}

function isClearMarker(value: unknown): boolean {
  return Boolean(
    value &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      (value as { clear?: unknown }).clear === true,
  );
}

export const SecretControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  savedValue?: unknown;
  disabled?: boolean;
  onChange: (next: unknown) => void;
}> = ({ field, value, savedValue, disabled = false, onChange }) => {
  const { t } = useI18n();
  const id = useId();
  const [replacing, setReplacing] = useState(false);

  const savedConfigured = isConfiguredSentinel(savedValue) || isConfiguredSentinel(value);
  const markedClear = isClearMarker(value);
  const draftPlain = typeof value === 'string' ? value : '';
  const effectiveReplacing = replacing || (!savedConfigured && !markedClear);

  return (
    <div className="space-y-1.5" data-testid={`field-secret-${field.key}`}>
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required && !savedConfigured} />
      {markedClear ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-amber-200 bg-amber-50 px-2.5 py-2 text-xs text-amber-900">
          <span>{t('fields.secret.pending_clear')}</span>
          <AdminButton
            size="sm"
            variant="secondary"
            disabled={disabled}
            onClick={() => {
              setReplacing(false);
              onChange(savedValue ?? null);
            }}
          >
            {t('fields.secret.cancel_clear')}
          </AdminButton>
        </div>
      ) : savedConfigured && !effectiveReplacing ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-2 text-xs">
          <span className="font-mono text-zinc-700">{t('fields.secret.configured_masked')}</span>
          <div className="flex items-center gap-1.5">
            <AdminButton
              size="sm"
              variant="secondary"
              disabled={disabled}
              onClick={() => {
                setReplacing(true);
                onChange('');
              }}
            >
              {t('fields.secret.replace')}
            </AdminButton>
            {!field.required && (
              <AdminButton
                size="sm"
                variant="danger"
                disabled={disabled}
                onClick={() => {
                  setReplacing(false);
                  onChange({ clear: true });
                }}
              >
                {t('fields.secret.clear')}
              </AdminButton>
            )}
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <input
            id={id}
            type="password"
            autoComplete="new-password"
            value={draftPlain}
            placeholder={field.placeholder ?? t('fields.secret.placeholder')}
            disabled={disabled}
            onChange={e => onChange(e.target.value)}
            className={`${STUDIO_INPUT_CLASS} flex-1`}
          />
          {savedConfigured && (
            <AdminButton
              size="sm"
              variant="ghost"
              disabled={disabled}
              onClick={() => {
                setReplacing(false);
                onChange(savedValue ?? null);
              }}
            >
              {t('fields.common.cancel')}
            </AdminButton>
          )}
        </div>
      )}
    </div>
  );
};
