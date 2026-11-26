import React from 'react';
import {
  SpacingBoxControl,
  type ResponsiveDotInfo,
} from '../../builder/StudioFieldControls.tsx';
import type { BoxSpacing } from '../../builder/document.ts';
import type { BlockSpacingValue } from '../../types.ts';
import type { FieldSpec, SpacingValue } from '../types.ts';

function toBoxSpacing(raw: unknown): BoxSpacing | undefined {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return undefined;
  const obj = raw as Record<string, unknown>;
  const out: BoxSpacing = {};
  if (typeof obj.top === 'number' && Number.isFinite(obj.top)) out.top = obj.top;
  if (typeof obj.right === 'number' && Number.isFinite(obj.right)) out.right = obj.right;
  if (typeof obj.bottom === 'number' && Number.isFinite(obj.bottom)) out.bottom = obj.bottom;
  if (typeof obj.left === 'number' && Number.isFinite(obj.left)) out.left = obj.left;
  return out;
}

function fromBoxSpacing(box: BlockSpacingValue | BoxSpacing): SpacingValue {
  const out: SpacingValue = {};
  if (typeof box.top === 'number' && Number.isFinite(box.top)) out.top = box.top === 0 ? 0 : box.top;
  if (typeof box.right === 'number' && Number.isFinite(box.right)) out.right = box.right === 0 ? 0 : box.right;
  if (typeof box.bottom === 'number' && Number.isFinite(box.bottom)) out.bottom = box.bottom === 0 ? 0 : box.bottom;
  if (typeof box.left === 'number' && Number.isFinite(box.left)) out.left = box.left === 0 ? 0 : box.left;
  return out;
}

export const SpacingControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  disabled?: boolean;
  responsiveInfo?: ResponsiveDotInfo;
  onChange: (next: SpacingValue) => void;
}> = ({ field, value, disabled = false, responsiveInfo, onChange }) => {
  const prop = field.property === 'margin' ? 'margin' : 'padding';
  const propPrefix = prop === 'margin' ? 'm' : 'p';
  const minVal = field.min ?? (prop === 'margin' ? -128 : 0);
  const maxVal = field.max ?? 256;
  const unit = field.unit ?? 'px';

  return (
    <div data-testid={`field-spacing-${field.key}`}>
      <SpacingBoxControl
        label={field.label}
        propPrefix={propPrefix}
        value={toBoxSpacing(value)}
        min={minVal}
        max={maxVal}
        unit={unit}
        allowTailwind={unit === 'px'}
        disabled={disabled}
        responsiveInfo={responsiveInfo}
        onChange={nextBox => onChange(fromBoxSpacing(nextBox))}
        testIdPrefix={`field-spacing-${field.key}`}
      />
    </div>
  );
};
