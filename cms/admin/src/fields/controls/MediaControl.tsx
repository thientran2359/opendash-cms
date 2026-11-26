import React, { useId, useState } from 'react';
import { MediaCardControl } from '../../builder/StudioFieldControls.tsx';
import { MediaPicker } from '../../features/media/MediaPicker.tsx';
import type { MediaKind } from '../../api/media.ts';
import type { FieldSpec } from '../types.ts';

function mapMediaAcceptKind(kind: FieldSpec['media_kind']): MediaKind | 'any' {
  if (kind === 'video') return 'video';
  if (kind === 'file' || kind === 'any') return 'any';
  return 'image';
}

export const MediaControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  csrfToken: string | null;
  disabled?: boolean;
  onChange: (nextId: string | null) => void;
  onApiError: (error: unknown, fallback: string) => void;
}> = ({ field, value, csrfToken, disabled = false, onChange, onApiError }) => {
  const id = useId();
  const [pickerOpen, setPickerOpen] = useState(false);
  const currentId = typeof value === 'string' ? value : '';
  const acceptKind = mapMediaAcceptKind(field.media_kind);
  const cardKind = acceptKind === 'video' ? 'video' : acceptKind === 'any' ? 'any' : 'image';

  return (
    <>
      <MediaCardControl
        id={id}
        label={field.label}
        value={currentId}
        acceptKind={cardKind}
        disabled={disabled}
        testId={`field-media-${field.key}`}
        onPick={() => setPickerOpen(true)}
        onClear={() => onChange(null)}
      />
      {pickerOpen && (
        <MediaPicker
          mode="image"
          acceptKind={acceptKind}
          selectedIds={currentId ? [currentId] : []}
          csrfToken={csrfToken}
          onApply={ids => {
            onChange(ids[0] ?? null);
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
          onError={onApiError}
        />
      )}
    </>
  );
};
