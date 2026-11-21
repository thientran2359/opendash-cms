import React from 'react';
import { AdminBadge, AdminButton } from '../components/AdminPrimitives.tsx';
import { type BuilderViewport, VIEWPORT_LABELS, VIEWPORT_WIDTHS } from '../builder/document.ts';

export type StudioSavePolicy =
  | { mode: 'customizer' }
  | {
      mode: 'entry';
      publicationPolicy: 'immediate' | 'draft_publish';
      publicationStatus: 'draft' | 'published' | string;
    };

interface Props {
  title: string;
  subtitle?: string | null;
  dirty: boolean;
  saving: boolean;
  busy?: boolean;
  viewport: BuilderViewport;
  onViewportChange: (viewport: BuilderViewport) => void;
  onExit: () => void;
  canUndo?: boolean;
  canRedo?: boolean;
  onUndo?: () => void;
  onRedo?: () => void;
  canReset?: boolean;
  resetLabel?: string;
  onReset?: () => void;
  savePolicy: StudioSavePolicy;
  canSave: boolean;
  onSaveCustomizer?: () => void;
  onSaveEntry?: (intent?: 'draft' | 'published') => void;
}

const VIEWPORTS: BuilderViewport[] = ['desktop', 'tablet', 'mobile'];

export const StudioTopBar: React.FC<Props> = ({
  title,
  subtitle,
  dirty,
  saving,
  busy = false,
  viewport,
  onViewportChange,
  onExit,
  canUndo = false,
  canRedo = false,
  onUndo,
  onRedo,
  canReset = false,
  resetLabel = 'Reset mặc định',
  onReset,
  savePolicy,
  canSave,
  onSaveCustomizer,
  onSaveEntry,
}) => {
  const actionsDisabled = saving || busy;

  return (
    <header
      className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-zinc-200 bg-white px-3 sm:px-4"
      data-testid="studio-topbar"
    >
      <div className="flex min-w-0 items-center gap-2.5">
        <AdminButton
          variant="ghost"
          size="sm"
          onClick={onExit}
          disabled={saving}
          data-testid="studio-exit-btn"
        >
          ← Thoát
        </AdminButton>
        <div className="h-5 w-px bg-zinc-200" aria-hidden="true" />
        <div className="flex min-w-0 items-center gap-2">
          <h1 className="truncate text-sm font-semibold text-zinc-950">{title}</h1>
          {subtitle && (
            <span className="hidden truncate text-xs text-zinc-500 md:inline">{subtitle}</span>
          )}
          {dirty && (
            <AdminBadge tone="warning" data-testid="studio-dirty-badge">
              ● Chưa lưu
            </AdminBadge>
          )}
        </div>
      </div>

      <div className="hidden items-center gap-1 rounded-lg border border-zinc-200 bg-zinc-50 p-0.5 sm:flex" role="group" aria-label="Chọn viewport">
        {VIEWPORTS.map(vp => (
          <button
            key={vp}
            type="button"
            aria-pressed={viewport === vp}
            data-testid={`studio-viewport-${vp}`}
            onClick={() => onViewportChange(vp)}
            className={
              'rounded-md px-2.5 py-1 text-xs font-medium transition-colors ' +
              (viewport === vp
                ? 'bg-white text-zinc-950 shadow-2xs ring-1 ring-zinc-200'
                : 'text-zinc-600 hover:text-zinc-950')
            }
          >
            {VIEWPORT_LABELS[vp]} · {VIEWPORT_WIDTHS[vp]}px
          </button>
        ))}
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {onUndo && onRedo && (
          <div className="flex items-center gap-1 mr-1">
            <AdminButton
              variant="ghost"
              size="sm"
              disabled={actionsDisabled || !canUndo}
              onClick={onUndo}
              aria-label="Hoàn tác (Ctrl+Z)"
              title="Hoàn tác (Ctrl/Cmd+Z)"
              data-testid="studio-undo-btn"
            >
              ↶
            </AdminButton>
            <AdminButton
              variant="ghost"
              size="sm"
              disabled={actionsDisabled || !canRedo}
              onClick={onRedo}
              aria-label="Làm lại (Ctrl+Shift+Z)"
              title="Làm lại (Ctrl/Cmd+Shift+Z)"
              data-testid="studio-redo-btn"
            >
              ↷
            </AdminButton>
          </div>
        )}

        {canReset && onReset && (
          <AdminButton
            variant="secondary"
            size="sm"
            disabled={actionsDisabled}
            onClick={onReset}
            data-testid="studio-reset-btn"
          >
            {resetLabel}
          </AdminButton>
        )}

        {savePolicy.mode === 'customizer' ? (
          <AdminButton
            variant="primary"
            size="sm"
            disabled={actionsDisabled || !canSave}
            onClick={onSaveCustomizer}
            data-testid="studio-save-btn"
          >
            {saving ? 'Đang lưu…' : 'Lưu thay đổi'}
          </AdminButton>
        ) : savePolicy.publicationPolicy === 'immediate' ? (
          <AdminButton
            variant="primary"
            size="sm"
            disabled={actionsDisabled || !canSave}
            onClick={() => onSaveEntry?.()}
            data-testid="studio-save-btn"
          >
            {saving ? 'Đang lưu…' : 'Lưu & Cập nhật'}
          </AdminButton>
        ) : savePolicy.publicationStatus === 'draft' ? (
          <>
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={actionsDisabled || !canSave}
              onClick={() => onSaveEntry?.('draft')}
              data-testid="studio-save-draft-btn"
            >
              {saving ? 'Đang lưu…' : 'Lưu bản nháp'}
            </AdminButton>
            <AdminButton
              variant="primary"
              size="sm"
              disabled={actionsDisabled}
              onClick={() => onSaveEntry?.('published')}
              data-testid="studio-publish-btn"
            >
              Xuất bản
            </AdminButton>
          </>
        ) : (
          <AdminButton
            variant="primary"
            size="sm"
            disabled={actionsDisabled || !canSave}
            onClick={() => onSaveEntry?.('published')}
            data-testid="studio-save-btn"
          >
            {saving ? 'Đang lưu…' : 'Cập nhật đã xuất bản'}
          </AdminButton>
        )}
      </div>
    </header>
  );
};
