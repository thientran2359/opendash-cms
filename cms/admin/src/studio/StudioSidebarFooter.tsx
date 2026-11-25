import { useI18n } from '../i18n/index.tsx';
import React from 'react';
import { AdminButton } from '../components/AdminPrimitives.tsx';
import { type BuilderViewport } from '../builder/document.ts';

export type StudioSavePolicy =
  | { mode: 'customizer' }
  | {
      mode: 'entry';
      isPrivatePattern?: boolean;
      publicationPolicy: 'immediate' | 'draft_publish';
      publicationStatus: 'draft' | 'published' | string;
    };

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
  showPatternButton?: boolean;
  onOpenPatternLibrary?: () => void;
  patternDisabled?: boolean;
}

export const StudioSidebarFooter: React.FC<Props> = ({
  saving, busy = false, viewport, onViewportChange, canShowUndo,
  canUndo, canRedo, onUndo, onRedo, savePolicy, canSave, onSaveCustomizer, onSaveEntry,
  showPatternButton = false, onOpenPatternLibrary, patternDisabled = false,
}) => {
  const { t } = useI18n();
  const disabled = saving || busy;
  return (
    <footer className="shrink-0 space-y-2 border-t border-zinc-200 bg-white p-3" data-testid="studio-sidebar-footer">
      {showPatternButton && onOpenPatternLibrary && (
        <button
          type="button"
          data-testid="studio-open-pattern-library"
          disabled={disabled || patternDisabled}
          onClick={onOpenPatternLibrary}
          className="flex w-full items-center justify-center gap-1.5 rounded-md border border-zinc-200 bg-zinc-50/80 px-2.5 py-1.5 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-100 hover:text-zinc-950 disabled:opacity-50"
        >
          <span aria-hidden="true">▦</span>
          <span>{t("studio.pattern_library")}</span>
        </button>
      )}
      <div className="flex items-center gap-1 rounded-lg border border-zinc-200 bg-zinc-50 p-1" role="group" aria-label={t("studio.select_viewport")}>
        {(['desktop', 'tablet', 'mobile'] as const).map(value => (
          <button key={value} type="button" aria-pressed={viewport === value}
            data-testid={`studio-viewport-${value}`} onClick={() => onViewportChange(value)}
            className={'min-w-0 flex-1 rounded-md px-1.5 py-1.5 text-xs font-medium transition-colors ' +
              (viewport === value ? 'bg-white text-zinc-950 shadow-xs ring-1 ring-zinc-200' : 'text-zinc-600 hover:text-zinc-950')}>
            {{ desktop: t("studio.desktop"), tablet: t("studio.tablet"), mobile: t("studio.mobile") }[value]}
          </button>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        {canShowUndo && (
          <div className="mr-auto flex items-center gap-1">
            <AdminButton variant="ghost" size="sm" disabled={disabled || !canUndo}
              onClick={onUndo} title={t("studio.undo_ctrl_cmd_z")} aria-label={t("studio.undo_ctrl_z")} data-testid="studio-undo-btn">↶</AdminButton>
            <AdminButton variant="ghost" size="sm" disabled={disabled || !canRedo}
              onClick={onRedo} title={t("studio.redo_ctrl_cmd_shift_z")} aria-label={t("studio.redo_ctrl_shift_z")} data-testid="studio-redo-btn">↷</AdminButton>
          </div>
        )}
        {savePolicy.mode === 'customizer' ? (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={onSaveCustomizer} data-testid="studio-save-btn">{saving ? t("studio.saving") : t("studio.save_changes")}</AdminButton>
        ) : savePolicy.isPrivatePattern ? (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={() => onSaveEntry?.()} data-testid="studio-save-btn">{saving ? t("studio.saving") : t("studio.save_pattern")}</AdminButton>
        ) : savePolicy.publicationPolicy === 'immediate' ? (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={() => onSaveEntry?.()} data-testid="studio-save-btn">{saving ? t("studio.saving") : t("studio.save_and_update")}</AdminButton>
        ) : savePolicy.publicationStatus === 'draft' ? (
          <>
            <AdminButton variant="secondary" size="sm" disabled={disabled || !canSave}
              onClick={() => onSaveEntry?.('draft')} data-testid="studio-save-draft-btn">{saving ? t("studio.saving") : t("studio.save_draft")}</AdminButton>
            <AdminButton variant="primary" size="sm" disabled={disabled}
              onClick={() => onSaveEntry?.('published')} data-testid="studio-publish-btn">{t("studio.publish")}</AdminButton>
          </>
        ) : (
          <AdminButton variant="primary" size="sm" disabled={disabled || !canSave}
            onClick={() => onSaveEntry?.('published')} data-testid="studio-save-btn">{saving ? t("studio.saving") : t("studio.update_published_content")}</AdminButton>
        )}
      </div>
    </footer>
  );
};
