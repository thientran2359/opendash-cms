import React from 'react';
import { __ } from '../../i18n/index.tsx';
import type { RevisionSummaryDto } from '../../api/revisions.ts';
import { formatRevisionSource, formatRevisionTimestamp } from './types.ts';

export interface RevisionListSidebarProps {
  revisions: RevisionSummaryDto[];
  total: number;
  hasAutosave: boolean;
  loadingList: boolean;
  loadingMore: boolean;
  listError: string | null;
  selectedRevId: number | null;
  onSelectRevision: (revId: number) => void;
  onLoadMore: () => void;
}

export const RevisionListSidebar: React.FC<RevisionListSidebarProps> = ({
  revisions,
  total,
  hasAutosave,
  loadingList,
  loadingMore,
  listError,
  selectedRevId,
  onSelectRevision,
  onLoadMore,
}) => (
  <aside
    aria-label={__("Saved revisions")}
    className="max-h-60 overflow-y-auto overscroll-contain border-b border-zinc-200 bg-zinc-50/80 p-3 md:max-h-none md:border-r md:border-b-0"
  >
    <div className="mb-2 flex items-center justify-between text-xs font-semibold text-zinc-700">
      <span>{__("Snapshots ({{value1}})", { value1: total })}</span>
      {hasAutosave && (
        <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-800">
          {__("Autosave")}
        </span>
      )}
    </div>
    {loadingList && (
      <p className="py-2 text-xs text-zinc-500">{__("Loading revisions…")}</p>
    )}
    {listError && (
      <p role="alert" className="py-2 text-xs font-medium text-rose-700">
        {listError}
      </p>
    )}
    <ul className="space-y-1.5">
      {revisions.map(rev => {
        const selected = rev.id === selectedRevId;
        return (
          <li key={rev.id}>
            <button
              type="button"
              data-testid={`revision-item-${rev.revision_number}`}
              onClick={() => onSelectRevision(rev.id)}
              className={`w-full rounded-lg border px-2.5 py-2 text-left text-xs transition-colors ${
                selected
                  ? 'border-od-accent bg-blue-50/70 text-zinc-950'
                  : 'border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50'
              }`}
            >
              <div className="flex items-center justify-between gap-1 font-semibold">
                <span>
                  #{rev.revision_number} · {formatRevisionSource(rev.source)}
                </span>
                <span className="font-mono text-[10px] text-zinc-500">
                  v{rev.entry_version}
                </span>
              </div>
              {rev.restored_from_revision_number !== null && (
                <div className="mt-0.5 text-[11px] text-zinc-600">
                  {__("Restored from #{{value1}}", {
                    value1: rev.restored_from_revision_number,
                  })}
                </div>
              )}
              <div className="mt-0.5 truncate text-[11px] text-zinc-500">
                {rev.author_username} · {formatRevisionTimestamp(rev.created_at)}
              </div>
            </button>
          </li>
        );
      })}
    </ul>
    {revisions.length < total && (
      <button
        type="button"
        disabled={loadingMore}
        onClick={onLoadMore}
        data-testid="revision-load-more"
        className="mt-2 inline-flex min-h-8 w-full items-center justify-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
      >
        {loadingMore ? __("Loading…") : __("Load older revisions")}
      </button>
    )}
  </aside>
);
