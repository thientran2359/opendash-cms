import React from 'react';
import { AdminButton } from '../components/AdminPrimitives.tsx';
import { type BuilderViewport, VIEWPORT_LABELS } from '../builder/document.ts';

export type StudioSavePolicy =
  | { mode: 'customizer' }
  | { mode: 'entry'; publicationPolicy: 'immediate' | 'draft_publish'; publicationStatus: 'draft' | 'published' | string };

interface Props {
  saving: boolean;
  busy?: boolean;
  viewport: BuilderViewport;
  onViewportChange: (viewport: BuilderViewport) => void;
  canShowUndo: boolean;
  canUndo: boolean;
  canRedo: boolean;
  onUndo?: () => void;
  onRedo?: () => void;
  savePolicy: StudioSavePolicy;
  canSave: boolean;
  onSaveCustomizer?: () => void;
  onSaveEntry?: (intent?: 'draft' | 'published') => void;
}

export const StudioSidebarFooter: React.FC<Props> = ({
  saving, busy = false, viewport, onViewportChange, canShowUndo,
  canUndo, canRedo, onUndo, onRedo, savePolicy, canSave, onSaveCustomizer, onSaveEntry,
}) => {
  const disabled = saving || busy;
  return (
    <footer className="shrink-0 space-y-2 border-t border-zinc-200 bg-white p-3" data-testid="studio-sidebar-footer">
      <div className="flex items-center gap-1 rounded-lg border border-zinc-200 bg-zinc-50 p-1" role="group" aria-label="Chọn viewport">
        {(['desktop', 'tablet', 'mobile'] as const).map(value => (
          <button key={value} type="button" aria-pressed={viewport === value}
            data-testid={`studio-viewport-${value}`} onClick={() => onViewportChange(value)}
            className={'min-w-0 flex-1 rounded-md px-1.5 py-1.5 text-xs font-medium transition-colors ' +
              (viewport === value ? 'bg-white text-zinc-950 shadow-xs ring-1 ring-zinc-200' : 'text-zinc-600 hover:text-zinc-950')}>
            {VIEWPORT_LABELS[value]}
          </button>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        {canShowUndo && (
          <div className="mr-auto flex items-center gap-1">
            <AdminButton variant="ghost" size="sm" disabled={disabled || !canUndo}
              onClick={onUndo} title="Hoàn tác (Ctrl/Cmd+Z)" aria-label="Hoàn tác (Ctrl+Z)" data-testid="studio-undo-btn">↶</AdminButton>
            <AdminButton variant="ghost" size="sm" disabled={disabled || !canRedo}
              onClick={onRedo} title="Làm lại (Ctrl/Cmd+Shift+Z)" aria-label="Làm lại (Ctrl+Shift+Z)" data-testid="studio-redo-btn">↷</AdminButton>
          </div>
        )}
        {savePolicy.mode === 'customizer' ? (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={onSaveCustomizer} data-testid="studio-save-btn">{saving ? 'Đang lưu…' : 'Lưu thay đổi'}</AdminButton>
        ) : savePolicy.publicationPolicy === 'immediate' ? (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={() => onSaveEntry?.()} data-testid="studio-save-btn">{saving ? 'Đang lưu…' : 'Lưu & Cập nhật'}</AdminButton>
        ) : savePolicy.publicationStatus === 'draft' ? (
          <>
            <AdminButton variant="secondary" size="sm" disabled={disabled || !canSave}
              onClick={() => onSaveEntry?.('draft')} data-testid="studio-save-draft-btn">{saving ? 'Đang lưu…' : 'Lưu bản nháp'}</AdminButton>
            <AdminButton variant="primary" size="sm" disabled={disabled}
              onClick={() => onSaveEntry?.('published')} data-testid="studio-publish-btn">Xuất bản</AdminButton>
          </>
        ) : (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={() => onSaveEntry?.('published')} data-testid="studio-save-btn">{saving ? 'Đang lưu…' : 'Cập nhật đã xuất bản'}</AdminButton>
        )}
      </div>
    </footer>
  );
};
