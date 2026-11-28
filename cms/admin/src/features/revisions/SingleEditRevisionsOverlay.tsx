import React from 'react';
import {
  type AutosaveSnapshotDto,
  type RestoreRevisionResponse,
} from '../../api/revisions.ts';
import { AutosaveRecoveryBanner } from './AutosaveRecoveryBanner.tsx';
import { RevisionHistoryModal } from './RevisionHistoryModal.tsx';
import type { RevisionModalInitialCompare } from './types.ts';

export interface SingleEditRevisionsOverlayProps {
  typeKey: string;
  entryId: number | null;
  csrfToken: string | null;
  currentVersion: number;
  serverAutosave: AutosaveSnapshotDto | null;
  hasNewerLocalDraft: boolean;
  discardingAutosave: boolean;
  historyOpen: boolean;
  initialCompare: RevisionModalInitialCompare;
  onApplyAutosave: () => void;
  onCompareAutosave: () => void;
  onDiscardAutosave: () => void;
  onCloseHistory: () => void;
  onBeforeRestore: () => void;
  onRestored: (res: RestoreRevisionResponse, restoredFromRevisionNumber: number) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const SingleEditRevisionsOverlay: React.FC<SingleEditRevisionsOverlayProps> = ({
  typeKey,
  entryId,
  csrfToken,
  currentVersion,
  serverAutosave,
  hasNewerLocalDraft,
  discardingAutosave,
  historyOpen,
  initialCompare,
  onApplyAutosave,
  onCompareAutosave,
  onDiscardAutosave,
  onCloseHistory,
  onBeforeRestore,
  onRestored,
  onApiError,
}) => (
  <>
    {serverAutosave && (
      <AutosaveRecoveryBanner
        autosave={serverAutosave}
        hasNewerLocalDraft={hasNewerLocalDraft}
        busy={discardingAutosave}
        onRestore={onApplyAutosave}
        onCompare={onCompareAutosave}
        onDiscard={onDiscardAutosave}
      />
    )}
    {entryId !== null && (
      <RevisionHistoryModal
        open={historyOpen}
        typeKey={typeKey}
        entryId={entryId}
        csrfToken={csrfToken}
        currentVersion={currentVersion}
        initialCompare={initialCompare}
        onClose={onCloseHistory}
        onBeforeRestore={onBeforeRestore}
        onRestored={onRestored}
        onApiError={onApiError}
      />
    )}
  </>
);
