import React from 'react';
import { __ } from '../../i18n/index.tsx';
import {
  hasBlockingMissingRefs,
  type AutosaveSnapshotDto,
} from '../../api/revisions.ts';
import { RevisionMissingRefsNotice } from './RevisionMissingRefsNotice.tsx';
import { formatRevisionTimestamp } from './types.ts';

export interface AutosaveRecoveryBannerProps {
  autosave: AutosaveSnapshotDto;
  hasNewerLocalDraft: boolean;
  busy?: boolean;
  onRestore: () => void;
  onCompare: () => void;
  onDiscard: () => void;
}

export const AutosaveRecoveryBanner: React.FC<AutosaveRecoveryBannerProps> = ({
  autosave,
  hasNewerLocalDraft,
  busy = false,
  onRestore,
  onCompare,
  onDiscard,
}) => {
  const savedAt = formatRevisionTimestamp(autosave.updated_at);
  const surfaceLabel =
    autosave.source_surface === 'studio' ? __("Studio") : __("Editor");

  return (
    <section
      aria-label={__("Autosave recovery")}
      data-testid="autosave-recovery-banner"
      className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 px-3.5 py-2.5 text-xs text-amber-950"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="space-y-1">
          <p role="status" aria-live="polite" className="font-medium">
            {hasNewerLocalDraft
              ? __(
                  "Your unsaved draft from this session is already active. A server autosave from {{value1}} ({{value2}}) is also stored.",
                  { value1: savedAt, value2: surfaceLabel },
                )
              : __(
                  "A newer autosave from {{value1}} ({{value2}}) is available.",
                  { value1: savedAt, value2: surfaceLabel },
                )}
          </p>
          {autosave.is_stale_base && (
            <p className="text-[11px] text-amber-900">
              {__(
                "This autosave was based on version v{{value1}}. Restoring it preserves the current entry version so you can review before saving.",
                { value1: autosave.base_version },
              )}
            </p>
          )}
          {autosave.has_hidden_admin_fields && (
            <p className="text-[11px] text-amber-900">
              {__(
                "Administrator-only fields are omitted from this autosave and will stay unchanged from the saved entry.",
              )}
            </p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {!hasNewerLocalDraft && (
            <button
              type="button"
              disabled={busy}
              onClick={onRestore}
              data-testid="autosave-banner-restore"
              className="inline-flex min-h-8 items-center rounded-md bg-od-accent px-3 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
            >
              {__("Restore autosave")}
            </button>
          )}
          <button
            type="button"
            disabled={busy}
            onClick={onCompare}
            data-testid="autosave-banner-compare"
            className="inline-flex min-h-8 items-center rounded-md border border-amber-400 bg-white px-2.5 text-xs font-medium text-amber-950 hover:bg-amber-100 disabled:opacity-50"
          >
            {__("Compare with current")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={onDiscard}
            data-testid="autosave-banner-discard"
            className="inline-flex min-h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-700 hover:bg-zinc-50 disabled:opacity-50"
          >
            {__("Discard autosave")}
          </button>
        </div>
      </div>
      {hasBlockingMissingRefs(autosave.missing_refs) && (
        <RevisionMissingRefsNotice
          report={autosave.missing_refs}
          variant="info"
          testId="autosave-banner-missing-refs"
        />
      )}
    </section>
  );
};
