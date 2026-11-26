import React, { useId } from 'react';
import { LightboxRichTextTrigger } from '../../builder/StudioFieldControls.tsx';
import { useI18n } from '../../i18n/index.tsx';
import type { FieldSpec } from '../types.ts';

export const RichTextControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  onChange: (nextHtml: string) => void;
}> = ({ field, value, disabled = false, onChange }) => {
  const { t } = useI18n();
  const id = useId();
  const html = typeof value === 'string' ? value : '';
  const toolbarNote = field.toolbar === 'minimal'
    ? t('fields.richtext.toolbar_minimal')
    : t('fields.richtext.toolbar_standard');

  return (
    <div className="space-y-1">
      <LightboxRichTextTrigger
        id={id}
        label={field.label}
        value={html}
        placeholder={field.placeholder}
        maxBytes={field.max_bytes ?? 32_768}
        disabled={disabled}
        onChange={onChange}
      />
      <p className="text-[11px] text-zinc-500">{toolbarNote}</p>
    </div>
  );
};
