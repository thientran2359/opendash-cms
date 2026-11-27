import React from 'react';
import { __ } from '../../i18n/index.tsx';
import { formatRevisionTimestamp, type AutosaveState } from './types.ts';

export interface PublishBoxRevisionsControlProps {
  revisionCount: number | null;
  hasAutosave: boolean;
  autosaveState: AutosaveState;
  disabled?: boolean;
  onOpenHistory: () => void;
}

export const PublishBoxRevisionsControl: React.FC<PublishBoxRevisionsControlProps> = ({
  revisionCount,
  hasAutosave,
  autosaveState,
  disabled = false,
  onOpenHistory,
}) => {
  const statusText =
    autosaveState.status === 'saving'
      ? __("Autosaving…")
      : autosaveState.status === 'saved' && autosaveState.savedAt
        ? __("Autosaved at {{value1}}", {
            value1: formatRevisionTimestamp(autosaveState.savedAt),
          }) +
          (autosaveState.isStaleBase
            ? ` · ${__("Stale base version")}`
            : '')
        : '';

  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center justify-between gap-1.5">
        <span>
          {__("Revisions:")}{' '}
          <strong
            className="font-semibold text-zinc-900"
            data-testid="publish-box-revision-count"
          >
            {revisionCount ?? 0}
          </strong>
          {hasAutosave && (
            <span className="ml-1.5 inline-flex items-center rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-800">
              {__("Autosave")}
            </span>
          )}
        </span>
        <button
          type="button"
          disabled={disabled}
          onClick={onOpenHistory}
          data-testid="publish-box-open-revisions"
          className="inline-flex min-h-6 min-w-6 items-center justify-center rounded px-1.5 text-od-accent hover:underline disabled:opacity-50"
        >
          {__("Browse")}
        </button>
      </div>
      <div
        role="status"
        aria-live="polite"
        data-testid="editor-autosave-status"
        className={
          statusText
            ? autosaveState.status === 'saved'
              ? 'text-[11px] font-medium text-emerald-700'
              : 'text-[11px] text-zinc-600'
            : 'sr-only'
        }
      >
        {statusText}
      </div>
      {autosaveState.status === 'error' && (
        <div
          role="alert"
          data-testid="editor-autosave-error"
          className="text-[11px] font-medium text-rose-700"
        >
          {autosaveState.errorMessage || __("Autosave failed.")}
        </div>
      )}
    </div>
  );
};
