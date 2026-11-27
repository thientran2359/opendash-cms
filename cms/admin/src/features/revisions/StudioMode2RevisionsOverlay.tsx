import React from 'react';
import { __ } from '../../i18n/index.tsx';
import type {
  AutosaveSnapshotDto,
  RestoreRevisionResponse,
} from '../../api/revisions.ts';
import { AutosaveRecoveryBanner } from './AutosaveRecoveryBanner.tsx';
import { RevisionHistoryModal } from './RevisionHistoryModal.tsx';
import {
  formatRevisionTimestamp,
  type RevisionModalInitialCompare,
} from './types.ts';

export interface StudioMode2RevisionsOverlayProps {
  entryTypeKey: string;
  entryId: number | null;
  isPatternEntry: boolean;
  csrfToken: string | null;
  currentVersion: number;
  serverAutosave: AutosaveSnapshotDto | null;
  hasNewerLocalDraft: boolean;
  discardingAutosave: boolean;
  historyOpen: boolean;
  initialCompare: RevisionModalInitialCompare;
  restoredEditorModeNotice: boolean;
  singleEditHash: string;
  onDismissEditorModeNotice: () => void;
  onNavigateHash: (hash: string) => void;
  onApplyAutosave: () => void;
  onCompareAutosave: () => void;
  onDiscardAutosave: () => void;
  onCloseHistory: () => void;
  onBeforeRestore: () => void;
  onRestored: (
    res: RestoreRevisionResponse,
    restoredFromRevisionNumber: number,
  ) => void | Promise<void>;
  onApiError: (error: unknown, fallback: string) => void;
}

export const StudioMode2RevisionsOverlay: React.FC<
  StudioMode2RevisionsOverlayProps
> = ({
  entryTypeKey,
  entryId,
  isPatternEntry,
  csrfToken,
  currentVersion,
  serverAutosave,
  hasNewerLocalDraft,
  discardingAutosave,
  historyOpen,
  initialCompare,
  restoredEditorModeNotice,
  singleEditHash,
  onDismissEditorModeNotice,
  onNavigateHash,
  onApplyAutosave,
  onCompareAutosave,
  onDiscardAutosave,
  onCloseHistory,
  onBeforeRestore,
  onRestored,
  onApiError,
}) => (
  <>
    {restoredEditorModeNotice && !isPatternEntry && (
      <div
        role="status"
        aria-live="polite"
        data-testid="studio-restored-editor-mode-notice"
        className="space-y-1.5 border-b border-sky-200 bg-sky-50 px-3 py-2 text-xs text-sky-950"
      >
        <p className="font-medium">
          {__("This restored revision uses the standard Editor mode (editor).")}
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            data-testid="studio-open-editor-after-restore"
            onClick={() => onNavigateHash(singleEditHash)}
            className="inline-flex min-h-7 items-center rounded border border-sky-300 bg-white px-2 text-xs font-medium text-sky-900 hover:bg-sky-100"
          >
            {__("Open in Editor")}
          </button>
          <button
            type="button"
            onClick={onDismissEditorModeNotice}
            className="text-xs font-medium text-sky-700 hover:underline"
          >
            {__("Dismiss")}
          </button>
        </div>
      </div>
    )}

    {serverAutosave && (
      <div className="space-y-1.5 border-b border-amber-200 bg-amber-50/40 p-2">
        {serverAutosave.builder_document === null ? (
          <section
            aria-label={__("Autosave recovery")}
            role="status"
            aria-live="polite"
            data-testid="autosave-recovery-banner"
            className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-950"
          >
            <p className="font-medium">
              {__(
                "An autosaved draft from Content Editor ({{value1}}) has no Visual Builder layout data.",
                { value1: formatRevisionTimestamp(serverAutosave.updated_at) },
              )}
            </p>
            {serverAutosave.is_stale_base && (
              <p className="text-[11px] text-amber-900">
                {__("This autosave was based on version v{{value1}}.", {
                  value1: serverAutosave.base_version,
                })}
              </p>
            )}
            <div className="flex flex-wrap gap-1.5">
              {!isPatternEntry && (
                <button
                  type="button"
                  disabled={discardingAutosave}
                  onClick={() => onNavigateHash(singleEditHash)}
                  data-testid="studio-autosave-open-editor"
                  className="inline-flex min-h-7 items-center rounded bg-od-accent px-2.5 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
                >
                  {__("Open in Editor")}
                </button>
              )}
              <button
                type="button"
                disabled={discardingAutosave}
                onClick={onCompareAutosave}
                data-testid="autosave-banner-compare"
                className="inline-flex min-h-7 items-center rounded border border-amber-400 bg-white px-2 text-xs font-medium text-amber-950 hover:bg-amber-100 disabled:opacity-50"
              >
                {__("Compare with current")}
              </button>
              <button
                type="button"
                disabled={discardingAutosave}
                onClick={onDiscardAutosave}
                data-testid="autosave-banner-discard"
                className="inline-flex min-h-7 items-center rounded border border-zinc-300 bg-white px-2 text-xs font-medium text-zinc-700 hover:bg-zinc-50 disabled:opacity-50"
              >
                {__("Discard autosave")}
              </button>
            </div>
          </section>
        ) : (
          <>
            <AutosaveRecoveryBanner
              autosave={serverAutosave}
              hasNewerLocalDraft={hasNewerLocalDraft}
              busy={discardingAutosave}
              onRestore={onApplyAutosave}
              onCompare={onCompareAutosave}
              onDiscard={onDiscardAutosave}
            />
            {serverAutosave.source_surface === 'editor' && !isPatternEntry && (
              <div className="flex flex-wrap items-center justify-between gap-2 px-1 text-[11px] text-amber-900">
                <span>
                  {__(
                    "Saved from Content Editor; applying here loads only Visual Builder blocks.",
                  )}
                </span>
                <button
                  type="button"
                  onClick={() => onNavigateHash(singleEditHash)}
                  data-testid="studio-autosave-open-editor"
                  className="font-medium text-od-accent hover:underline"
                >
                  {__("Open in Editor")}
                </button>
              </div>
            )}
          </>
        )}
      </div>
    )}

    {entryId !== null && (
      <RevisionHistoryModal
        open={historyOpen}
        typeKey={entryTypeKey}
        entryId={entryId}
        csrfToken={csrfToken}
        currentVersion={currentVersion}
        initialCompare={initialCompare}
        onClose={onCloseHistory}
        onBeforeRestore={onBeforeRestore}
        onRestored={(res, revNum) => void onRestored(res, revNum)}
        onApiError={onApiError}
      />
    )}
  </>
);
