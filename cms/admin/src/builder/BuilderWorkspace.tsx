import React from 'react';
import { BlockData, BlockRegistry, Role } from '../types.ts';

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

  return (
    <>
      {role === 'admin' && (
        <div className="form-group">
          <label id="theme-label" htmlFor="theme">
            Gói theme ZIP:
          </label>
          <input
            id="theme"
            type="file"
            accept=".zip"
            onChange={onThemeUpload}
            disabled={isProcessing}
          />
        </div>
      )}

      <div className="form-group">
        <label htmlFor="type">Loại Block:</label>
        <select
          id="type"
          aria-label="Block"
          value={blockData.type}
          onChange={onTypeChange}
          disabled={isProcessing}
        >
          {Object.keys(registry).map(id => (
            <option key={id} value={id}>
              {id}
            </option>
          ))}
        </select>
      </div>

      <form id="fields" onSubmit={e => e.preventDefault()}>
        {Object.entries(currentDescriptor).map(([fieldName, fieldLabel]) => (
          <div key={fieldName} className="form-group">
            <label htmlFor={`field-${fieldName}`}>{fieldLabel}</label>
            <input
              id={`field-${fieldName}`}
              name={fieldName}
              value={blockData.values[fieldName] ?? ''}
              onChange={e => onFieldChange(fieldName, e.target.value)}
              disabled={isProcessing}
            />
          </div>
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
