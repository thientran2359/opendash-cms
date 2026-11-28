import React from 'react';
import type { TemplatePreviewEntry, TemplatePreviewTerm } from '../api/builder.ts';
import type { StudioTargetItem } from '../api/studio.ts';
import { BuilderPreviewFrame } from '../builder/BuilderPreviewFrame.tsx';
import type { BuilderDocument, BuilderViewport } from '../builder/document.ts';
import { __ } from '../i18n/index.tsx';
import type { StudioPanelId } from '../shell/router.ts';
import { StudioSamplePickerBar } from './StudioSamplePickerBar.tsx';

export const StudioMobileViewTabs: React.FC<{
  mobilePane: 'controls' | 'preview';
  onMobilePaneChange: (pane: 'controls' | 'preview') => void;
}> = ({ mobilePane, onMobilePaneChange }) => (
  <div
    className="flex items-center gap-1 border-b border-zinc-200 bg-white px-3 py-1.5 md:hidden"
    role="tablist"
    aria-label={__("Studio mobile view")}
  >
    <button
      type="button"
      role="tab"
      aria-selected={mobilePane === 'controls'}
      onClick={() => onMobilePaneChange('controls')}
      className={`flex-1 rounded-md px-3 py-1.5 text-xs font-semibold transition-colors ${mobilePane === 'controls' ? 'bg-zinc-900 text-white' : 'bg-zinc-100 text-zinc-700'}`}
    >
      {__("Controls")}
    </button>
    <button
      type="button"
      role="tab"
      aria-selected={mobilePane === 'preview'}
      onClick={() => onMobilePaneChange('preview')}
      className={`flex-1 rounded-md px-3 py-1.5 text-xs font-semibold transition-colors ${mobilePane === 'preview' ? 'bg-zinc-900 text-white' : 'bg-zinc-100 text-zinc-700'}`}
    >
      {__("Preview")}
    </button>
  </div>
);

export interface StudioPreviewPaneProps {
  active: boolean;
  mobilePane: 'controls' | 'preview';
  viewport: BuilderViewport;
  loading: boolean;
  previewLoading: boolean;
  previewHtml: string;
  isMode2: boolean;
  panel: StudioPanelId;
  selectedTarget: StudioTargetItem | null;
  templateEditingEnabled: boolean;
  activeSelectedId: string | null;
  activeDoc: BuilderDocument;
  effectivePreviewLocale: string;
  previewEntry: TemplatePreviewEntry | null;
  previewTerm: TemplatePreviewTerm | null;
  onChangePreviewEntry: (next: TemplatePreviewEntry | null) => void;
  onChangePreviewTerm: (next: TemplatePreviewTerm | null) => void;
  disableSamplePicker: boolean;
  onSelectNode: (id: string) => void;
  onEditNode: (id: string) => void;
  onOpenNodeMenu: (id: string, mode: 'open' | 'toggle', anchor?: { x: number; y: number } | null) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const StudioPreviewPane: React.FC<StudioPreviewPaneProps> = ({
  active,
  mobilePane,
  viewport,
  loading,
  previewLoading,
  previewHtml,
  isMode2,
  panel,
  selectedTarget,
  templateEditingEnabled,
  activeSelectedId,
  activeDoc,
  effectivePreviewLocale,
  previewEntry,
  previewTerm,
  onChangePreviewEntry,
  onChangePreviewTerm,
  disableSamplePicker,
  onSelectNode,
  onEditNode,
  onOpenNodeMenu,
  onApiError,
}) => {
  const PreviewTag: 'main' | 'div' = active ? 'main' : 'div';
  return (
    <PreviewTag
      className={`min-w-0 flex-1 flex-col overflow-hidden md:flex ${mobilePane === 'preview' ? 'flex' : 'hidden'} ${viewport === 'desktop' ? 'bg-white' : 'bg-zinc-200/70'}`}
    >
      {!isMode2 && panel === 'templates' && selectedTarget && (
        <StudioSamplePickerBar
          target={selectedTarget}
          previewLocale={effectivePreviewLocale}
          previewEntry={previewEntry}
          previewTerm={previewTerm}
          onChangePreviewEntry={onChangePreviewEntry}
          onChangePreviewTerm={onChangePreviewTerm}
          disabled={disableSamplePicker}
        />
      )}
      <div className={'min-h-0 flex-1 overflow-auto ' + (viewport === 'desktop' ? 'p-0' : 'p-0 sm:p-4')}>
        {previewHtml ? (
          <BuilderPreviewFrame
            html={previewHtml}
            viewport={viewport}
            interactive={isMode2 || Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled)}
            selectedId={activeSelectedId}
            document={activeDoc}
            onSelectNode={onSelectNode}
            onEditNode={onEditNode}
            onOpenNodeMenu={onOpenNodeMenu}
            onApiError={onApiError}
            title={isMode2 ? __("Content preview") : __("Website preview")}
            className="h-full w-full"
            frameClassName={
              viewport === 'desktop'
                ? '!h-full !w-full !border-0 !rounded-none !shadow-none'
                : '!h-full min-h-[540px] !border border-zinc-300 rounded-lg shadow-sm'
            }
          />
        ) : (
          <div className="flex h-full min-h-[360px] items-center justify-center bg-white p-8 text-center text-sm text-zinc-500">
            {loading || previewLoading ? __("Preparing live preview…") : __("No preview content for this target yet.")}
          </div>
        )}
      </div>
    </PreviewTag>
  );
};
