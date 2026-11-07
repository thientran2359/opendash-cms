import React from 'react';
import { BlockData, BlockRegistry, Role } from '../types.ts';
import { SelectField, TextField, ZipUploadField } from '../components/FormPrimitives.tsx';

export function normalizeValues(
  targetType: string,
  reg: BlockRegistry,
  sourceValues: Record<string, string> = {},
): Record<string, string> {
  const descriptor = reg[targetType] || {};
  const normalized: Record<string, string> = {};
  for (const key of Object.keys(descriptor)) {
    normalized[key] = sourceValues[key] ?? '';
  }
  return normalized;
}

interface BuilderControlsProps {
  role?: Role;
  registry: BlockRegistry;
  blockData: BlockData;
  isProcessing: boolean;
  onThemeUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onTypeChange: (e: React.ChangeEvent<HTMLSelectElement>) => void;
  onFieldChange: (name: string, val: string) => void;
  onPreview: () => void;
  onSave: () => void;
}

export const BuilderControls: React.FC<BuilderControlsProps> = ({
  role,
  registry,
  blockData,
  isProcessing,
  onThemeUpload,
  onTypeChange,
  onFieldChange,
  onPreview,
  onSave,
}) => {
  const currentDescriptor = registry[blockData.type] || {};
  const blockOptions = Object.keys(registry).map(id => ({ value: id, label: id }));

  return (
    <>
      {role === 'admin' && (
        <ZipUploadField
          id="theme"
          labelId="theme-label"
          label="Gói theme ZIP:"
          onChange={onThemeUpload}
          disabled={isProcessing}
        />
      )}

      <SelectField
        id="type"
        label="Loại Block:"
        ariaLabel="Block"
        value={blockData.type}
        options={blockOptions}
        onChange={onTypeChange}
        disabled={isProcessing}
      />

      <form id="fields" onSubmit={e => e.preventDefault()}>
        {Object.entries(currentDescriptor).map(([fieldName, fieldLabel]) => (
          <TextField
            key={fieldName}
            id={`field-${fieldName}`}
            name={fieldName}
            label={fieldLabel}
            value={blockData.values[fieldName] ?? ''}
            onChange={val => onFieldChange(fieldName, val)}
            disabled={isProcessing}
          />
        ))}
      </form>

      <div className="action-buttons">
        <button id="preview" onClick={onPreview} disabled={isProcessing || !blockData.type}>
          Xem trước
        </button>
        <button id="save" onClick={onSave} disabled={isProcessing || !blockData.type}>
          Lưu
        </button>
        <a href="/" target="_blank" rel="noopener noreferrer">
          Trang công khai
        </a>
      </div>
    </>
  );
};

interface BuilderPreviewProps {
  previewHtml: string;
}

export const BuilderPreview: React.FC<BuilderPreviewProps> = ({ previewHtml }) => {
  return (
    <section className="preview-container">
      <div className="preview-header">Khung xem trước (Sandbox)</div>
      <iframe
        sandbox=""
        id="canvas"
        title="Xem trước block"
        srcDoc={previewHtml}
      />
    </section>
  );
};
