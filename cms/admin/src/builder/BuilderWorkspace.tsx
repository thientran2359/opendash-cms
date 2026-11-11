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
  blockVisibility?: 'rendered' | 'unsupported' | null;
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
  blockVisibility,
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
      {blockVisibility === 'unsupported' && (
        <p role="alert" data-testid="builder-visibility" className="mt-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950">
          Theme đang kích hoạt không hỗ trợ block này. Nội dung trong Builder vẫn được giữ nguyên; khi quay về theme tương thích, block sẽ xuất hiện lại.
        </p>
      )}
      {blockVisibility === 'rendered' && (
        <p role="status" data-testid="builder-visibility" className="mt-3 text-sm text-emerald-800">Block tương thích với theme hiện tại.</p>
      )}
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
