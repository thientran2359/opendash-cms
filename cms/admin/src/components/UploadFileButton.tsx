import React, { useId, useRef } from 'react';
import { AdminButton } from './AdminPrimitives.tsx';
import { useI18n } from '../i18n/index.tsx';

export interface UploadFileButtonProps {
  id?: string;
  accept?: string;
  multiple?: boolean;
  disabled?: boolean;
  required?: boolean;
  buttonLabel: string;
  selectedFileName?: string | null;
  emptyLabel?: string;
  ariaLabel?: string;
  testId?: string;
  showFileName?: boolean;
  onChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
}

export const UploadFileButton: React.FC<UploadFileButtonProps> = ({
  id,
  accept,
  multiple = false,
  disabled = false,
  required = false,
  buttonLabel,
  selectedFileName,
  emptyLabel,
  ariaLabel,
  testId,
  showFileName = true,
  onChange,
}) => {
  const { __ } = useI18n();
  const autoId = useId();
  const inputId = id ?? autoId;
  const inputRef = useRef<HTMLInputElement | null>(null);

  return (
    <div className="flex flex-wrap items-center gap-2">
      <input
        ref={inputRef}
        id={inputId}
        type="file"
        accept={accept}
        multiple={multiple}
        disabled={disabled}
        required={required}
        aria-label={ariaLabel ?? buttonLabel}
        data-testid={testId}
        onChange={onChange}
        className="sr-only"
      />
      <AdminButton
        type="button"
        variant="secondary"
        size="sm"
        disabled={disabled}
        onClick={() => inputRef.current?.click()}
      >
        {buttonLabel}
      </AdminButton>
      {showFileName && (
        <span className="min-w-0 max-w-xs truncate text-xs text-zinc-600" title={selectedFileName ?? undefined}>
          {selectedFileName || emptyLabel || __("No file selected")}
        </span>
      )}
    </div>
  );
};
