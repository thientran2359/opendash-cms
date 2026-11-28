import React, { useCallback, useEffect, useRef, useState } from 'react';
import { __, _x } from '../../i18n/index.tsx';
import { ApiClientError } from '../../api/client.ts';
import {
  compareEntryRevisions,
  getEntryRevision,
  listEntryRevisions,
  restoreEntryRevision,
  type MissingRefsReport,
  type RestoreRevisionResponse,
  type RevisionCompareResponse,
  type RevisionDetailDto,
  type RevisionSummaryDto,
} from '../../api/revisions.ts';
import { RevisionDiffView } from './RevisionDiffView.tsx';
import { RevisionListSidebar } from './RevisionListSidebar.tsx';
import { RevisionMissingRefsNotice } from './RevisionMissingRefsNotice.tsx';
import { RevisionSnapshotView } from './RevisionSnapshotView.tsx';
import {
  formatRevisionSource,
  type RevisionModalInitialCompare,
} from './types.ts';

export interface RevisionHistoryModalProps {
  open: boolean;
  typeKey: string;
  entryId: number;
  csrfToken: string | null;
  currentVersion: number;
  initialCompare?: RevisionModalInitialCompare;
  onClose: () => void;
  onBeforeRestore?: () => void;
  onRestored: (res: RestoreRevisionResponse, restoredFromRevisionNumber: number) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

const PAGE_LIMIT = 25;

export const RevisionHistoryModal: React.FC<RevisionHistoryModalProps> = ({
  open,
  typeKey,
  entryId,
  csrfToken,
  currentVersion,
  initialCompare = null,
  onClose,
  onBeforeRestore,
  onRestored,
  onApiError,
}) => {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const diffTabRef = useRef<HTMLButtonElement | null>(null);
  const snapshotTabRef = useRef<HTMLButtonElement | null>(null);
  const backdropPressedRef = useRef(false);

  const [revisions, setRevisions] = useState<RevisionSummaryDto[]>([]);
  const [total, setTotal] = useState(0);
  const [hasAutosave, setHasAutosave] = useState(false);
  const [loadingList, setLoadingList] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [listError, setListError] = useState<string | null>(null);

  const [activeTab, setActiveTab] = useState<'diff' | 'snapshot'>('diff');
  const [selectedRevId, setSelectedRevId] = useState<number | null>(null);
  const [fromTarget, setFromTarget] = useState<string>('');
  const [toTarget, setToTarget] = useState<string>('current');

  const [compareData, setCompareData] = useState<RevisionCompareResponse | null>(null);
  const [loadingCompare, setLoadingCompare] = useState(false);
  const [compareError, setCompareError] = useState<string | null>(null);

  const [detailData, setDetailData] = useState<RevisionDetailDto | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);

  const [restoring, setRestoring] = useState(false);
  const [pendingMissingRefs, setPendingMissingRefs] = useState<MissingRefsReport | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [restoreFieldErrors, setRestoreFieldErrors] = useState<Record<string, string> | null>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  // Mandatory light-dismiss fallback when closedby="any" is unsupported.
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || 'closedBy' in HTMLDialogElement.prototype) return;

    const isOutsideBounds = (event: PointerEvent) => {
      const rect = dialog.getBoundingClientRect();
      return (
        event.clientX < rect.left ||
        event.clientX > rect.right ||
        event.clientY < rect.top ||
        event.clientY > rect.bottom
      );
    };

    const onPointerDown = (event: PointerEvent) => {
      backdropPressedRef.current = event.target === dialog && isOutsideBounds(event);
    };
    const onPointerUp = (event: PointerEvent) => {
      const clickedOutside =
        backdropPressedRef.current && event.target === dialog && isOutsideBounds(event);
      backdropPressedRef.current = false;
      if (clickedOutside && dialog.open) {
        dialog.close();
      }
    };
    const onPointerCancel = () => {
      backdropPressedRef.current = false;
    };

    dialog.addEventListener('pointerdown', onPointerDown);
    dialog.addEventListener('pointerup', onPointerUp);
    dialog.addEventListener('pointercancel', onPointerCancel);
    return () => {
      dialog.removeEventListener('pointerdown', onPointerDown);
      dialog.removeEventListener('pointerup', onPointerUp);
      dialog.removeEventListener('pointercancel', onPointerCancel);
    };
  }, []);

  useEffect(() => {
    if (!open) {
      setPendingMissingRefs(null);
      setRestoreError(null);
      setRestoreFieldErrors(null);
      return;
    }

    const controller = new AbortController();
    setLoadingList(true);
    setListError(null);
    setCompareData(null);
    setDetailData(null);
    setActiveTab('diff');

    void listEntryRevisions(typeKey, entryId, {
      limit: PAGE_LIMIT,
      offset: 0,
      signal: controller.signal,
    })
      .then(res => {
        if (controller.signal.aborted) return;
        setRevisions(res.items);
        setTotal(res.total);
        setHasAutosave(res.has_autosave);
        if (initialCompare) {
          const parsedFrom = Number(initialCompare.from);
          setSelectedRevId(
            Number.isSafeInteger(parsedFrom) && parsedFrom > 0
              ? parsedFrom
              : (res.items[0]?.id ?? null),
          );
          setFromTarget(initialCompare.from);
          setToTarget(initialCompare.to);
        } else if (res.items.length >= 2) {
          setSelectedRevId(res.items[0].id);
          setFromTarget(String(res.items[1].id));
          setToTarget(String(res.items[0].id));
        } else if (res.items.length === 1) {
          setSelectedRevId(res.items[0].id);
          setFromTarget(String(res.items[0].id));
          setToTarget('current');
        } else {
          setSelectedRevId(null);
          setFromTarget('current');
          setToTarget(res.has_autosave ? 'autosave' : 'current');
        }
      })
      .catch(err => {
        if (controller.signal.aborted) return;
        setListError(err instanceof Error ? err.message : __("Could not load revision history."));
        if (err instanceof ApiClientError && err.status === 401) {
          onApiError(err, __("Your session expired while loading revision history."));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoadingList(false);
      });

    return () => controller.abort();
  }, [open, typeKey, entryId, initialCompare, onApiError]);

  useEffect(() => {
    if (!open || activeTab !== 'diff' || !fromTarget || !toTarget) return;
    const controller = new AbortController();
    setLoadingCompare(true);
    setCompareError(null);

    void compareEntryRevisions(typeKey, entryId, fromTarget, toTarget, controller.signal)
      .then(res => {
        if (!controller.signal.aborted) setCompareData(res);
      })
      .catch(err => {
        if (controller.signal.aborted) return;
        setCompareError(err instanceof Error ? err.message : __("Could not compare revisions."));
        if (err instanceof ApiClientError && err.status === 401) {
          onApiError(err, __("Your session expired while comparing revisions."));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoadingCompare(false);
      });

    return () => controller.abort();
  }, [open, activeTab, typeKey, entryId, fromTarget, toTarget, onApiError]);

  useEffect(() => {
    if (!open || activeTab !== 'snapshot' || selectedRevId === null) return;
    const controller = new AbortController();
    setLoadingDetail(true);
    setDetailError(null);

    void getEntryRevision(typeKey, entryId, selectedRevId, controller.signal)
      .then(res => {
        if (!controller.signal.aborted) setDetailData(res);
      })
      .catch(err => {
        if (controller.signal.aborted) return;
        setDetailError(err instanceof Error ? err.message : __("Could not load revision details."));
        if (err instanceof ApiClientError && err.status === 401) {
          onApiError(err, __("Your session expired while loading revision details."));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoadingDetail(false);
      });

    return () => controller.abort();
  }, [open, activeTab, typeKey, entryId, selectedRevId, onApiError]);

  const handleLoadMore = async () => {
    if (loadingMore || revisions.length >= total) return;
    setLoadingMore(true);
    try {
      const next = await listEntryRevisions(typeKey, entryId, {
        limit: PAGE_LIMIT,
        offset: revisions.length,
      });
      setRevisions(prev => {
        const seen = new Set(prev.map(item => item.id));
        return [...prev, ...next.items.filter(item => !seen.has(item.id))];
      });
      setTotal(next.total);
      setHasAutosave(next.has_autosave);
    } catch (err) {
      setListError(err instanceof Error ? err.message : __("Could not load more revisions."));
      onApiError(err, __("Could not load more revisions."));
    } finally {
      setLoadingMore(false);
    }
  };

  const selectedRevision = revisions.find(item => item.id === selectedRevId) ?? null;

  const executeRestore = useCallback(
    async (acknowledgeMissingRefs: boolean) => {
      if (!selectedRevision || !csrfToken || restoring) return;
      onBeforeRestore?.();
      setRestoring(true);
      setRestoreError(null);
      setRestoreFieldErrors(null);
      try {
        const res = await restoreEntryRevision(
          typeKey,
          entryId,
          selectedRevision.id,
          currentVersion,
          csrfToken,
          { acknowledgeMissingRefs },
        );
        setPendingMissingRefs(null);
        onRestored(res, selectedRevision.revision_number);
      } catch (err) {
        if (
          err instanceof ApiClientError &&
          err.status === 409 &&
          (err.bodyCode === 'revision_missing_refs' || err.errorCode === 'revision.missing_refs') &&
          err.missingRefs
        ) {
          setPendingMissingRefs(err.missingRefs);
          return;
        }
        if (err instanceof ApiClientError && err.status === 422) {
          setRestoreError(err.message);
          setRestoreFieldErrors(err.fieldErrors ?? null);
          return;
        }
        const fallback = __("Could not restore this revision.");
        setRestoreError(err instanceof Error ? err.message : fallback);
        onApiError(err, fallback);
      } finally {
        setRestoring(false);
      }
    },
    [
      selectedRevision,
      csrfToken,
      restoring,
      onBeforeRestore,
      typeKey,
      entryId,
      currentVersion,
      onRestored,
      onApiError,
    ],
  );

  const handleTabKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    const nextTab =
      event.key === 'ArrowRight' || event.key === 'ArrowLeft'
        ? activeTab === 'diff'
          ? 'snapshot'
          : 'diff'
        : event.key === 'Home'
          ? 'diff'
          : event.key === 'End'
            ? 'snapshot'
            : null;
    if (!nextTab) return;
    event.preventDefault();
    setActiveTab(nextTab);
    if (nextTab === 'diff') diffTabRef.current?.focus();
    else snapshotTabRef.current?.focus();
  };

  const targetOptions = (
    <>
      <option value="current">{__("Current saved state")}</option>
      {hasAutosave && <option value="autosave">{__("Latest autosave")}</option>}
      {revisions.map(rev => (
        <option key={rev.id} value={String(rev.id)}>
          {`#${rev.revision_number} · ${formatRevisionSource(rev.source)} (${rev.author_username})`}
        </option>
      ))}
    </>
  );

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="revision-history-modal-title"
      {...({ closedby: 'any' } as Record<string, string>)}
      onClose={onClose}
      data-testid="revision-history-modal"
      className="od-modal-dialog m-auto max-h-[90vh] w-[min(1120px,calc(100vw-2rem))] overflow-hidden rounded-xl border border-zinc-200 bg-white p-0 text-zinc-900 shadow-2xl"
    >
      <div className="flex items-center justify-between gap-3 border-b border-zinc-200 px-5 py-3.5">
        <div className="min-w-0">
          <h1
            id="revision-history-modal-title"
            className="text-base font-semibold text-zinc-950"
          >
            {__("Revision History")}
          </h1>
          <p className="text-xs text-zinc-500">
            {__("Current saved version: v{{value1}}", { value1: currentVersion })}
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          data-testid="revision-modal-close"
          className="inline-flex min-h-8 min-w-8 items-center justify-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-700 hover:bg-zinc-50"
        >
          {__("Close")}
        </button>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-hidden md:grid-cols-[290px_minmax(0,1fr)]">
        <RevisionListSidebar
          revisions={revisions}
          total={total}
          hasAutosave={hasAutosave}
          loadingList={loadingList}
          loadingMore={loadingMore}
          listError={listError}
          selectedRevId={selectedRevId}
          onSelectRevision={revId => {
            setSelectedRevId(revId);
            setFromTarget(String(revId));
            setToTarget('current');
            setPendingMissingRefs(null);
            setRestoreError(null);
            setRestoreFieldErrors(null);
          }}
          onLoadMore={() => void handleLoadMore()}
        />

        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-200 bg-white px-4 py-2.5">
            <div role="tablist" aria-label={__("Revision inspection mode")} className="flex gap-1">
              <button
                ref={diffTabRef}
                id="rev-tab-diff"
                type="button"
                role="tab"
                aria-selected={activeTab === 'diff'}
                aria-controls="rev-panel-diff"
                tabIndex={activeTab === 'diff' ? 0 : -1}
                onKeyDown={handleTabKeyDown}
                onClick={() => setActiveTab('diff')}
                data-testid="revision-tab-diff"
                className={`inline-flex min-h-8 items-center rounded-md px-3 text-xs font-medium ${
                  activeTab === 'diff'
                    ? 'bg-zinc-900 text-white'
                    : 'border border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50'
                }`}
              >
                {__("Compare changes")}
              </button>
              <button
                ref={snapshotTabRef}
                id="rev-tab-snapshot"
                type="button"
                role="tab"
                aria-selected={activeTab === 'snapshot'}
                aria-controls="rev-panel-snapshot"
                tabIndex={activeTab === 'snapshot' ? 0 : -1}
                onKeyDown={handleTabKeyDown}
                onClick={() => setActiveTab('snapshot')}
                data-testid="revision-tab-snapshot"
                className={`inline-flex min-h-8 items-center rounded-md px-3 text-xs font-medium ${
                  activeTab === 'snapshot'
                    ? 'bg-zinc-900 text-white'
                    : 'border border-zinc-200 bg-white text-zinc-700 hover:bg-zinc-50'
                }`}
              >
                {__("Snapshot details")}
              </button>
            </div>
            <button
              type="button"
              disabled={!selectedRevision || !csrfToken || restoring}
              onClick={() => void executeRestore(false)}
              data-testid="revision-restore-btn"
              className="inline-flex min-h-8 items-center rounded-md bg-od-accent px-3.5 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
            >
              {restoring
                ? __("Restoring…")
                : selectedRevision
                  ? __("Restore #{{value1}}", {
                      value1: selectedRevision.revision_number,
                    })
                  : __("Restore")}
            </button>
          </div>

          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto overscroll-contain p-4">
            {restoreError && (
              <div
                role="alert"
                data-testid="revision-restore-error"
                className="space-y-1 rounded-lg border border-rose-300 bg-rose-50 px-3 py-2 text-xs text-rose-900"
              >
                <p className="font-semibold">{restoreError}</p>
                {restoreFieldErrors && (
                  <ul className="list-disc pl-4">
                    {Object.entries(restoreFieldErrors).map(([key, msg]) => (
                      <li key={key}>
                        <code>{key}</code>: {msg}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}

            {pendingMissingRefs && (
              <div
                role="alert"
                data-testid="revision-missing-refs-confirm"
                className="space-y-3 rounded-lg border border-amber-400 bg-amber-50 p-3.5 text-xs text-amber-950"
              >
                <p className="font-semibold">
                  {__(
                    "This revision references items that are missing or inactive. Restoring it will drop or sanitize those references.",
                  )}
                </p>
                <RevisionMissingRefsNotice
                  report={pendingMissingRefs}
                  variant="warning"
                />
                <div className="flex flex-wrap justify-end gap-2">
                  <button
                    type="button"
                    autoFocus
                    disabled={restoring}
                    onClick={() => setPendingMissingRefs(null)}
                    data-testid="revision-missing-refs-cancel"
                    className="inline-flex min-h-8 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
                  >
                    {_x("Cancel", "multilingual.cancel")}
                  </button>
                  <button
                    type="button"
                    disabled={restoring || !csrfToken}
                    onClick={() => void executeRestore(true)}
                    data-testid="revision-missing-refs-proceed"
                    className="inline-flex min-h-8 items-center rounded-md bg-amber-700 px-3 text-xs font-medium text-white hover:bg-amber-800 disabled:opacity-50"
                  >
                    {restoring ? __("Restoring…") : __("Acknowledge and restore")}
                  </button>
                </div>
              </div>
            )}

            <div
              id="rev-panel-diff"
              role="tabpanel"
              aria-labelledby="rev-tab-diff"
              hidden={activeTab !== 'diff'}
              className="space-y-3"
            >
              <div className="grid grid-cols-1 gap-3 rounded-lg border border-zinc-200 bg-zinc-50 p-3 sm:grid-cols-2">
                <div className="flex flex-col gap-1 text-xs">
                  <label htmlFor="rev-compare-from" className="font-medium text-zinc-700">
                    {__("Compare from")}
                  </label>
                  <select
                    id="rev-compare-from"
                    data-testid="revision-compare-from"
                    value={fromTarget}
                    onChange={e => setFromTarget(e.target.value)}
                    className="h-9 rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-900"
                  >
                    {targetOptions}
                  </select>
                </div>
                <div className="flex flex-col gap-1 text-xs">
                  <label htmlFor="rev-compare-to" className="font-medium text-zinc-700">
                    {__("Compare to")}
                  </label>
                  <select
                    id="rev-compare-to"
                    data-testid="revision-compare-to"
                    value={toTarget}
                    onChange={e => setToTarget(e.target.value)}
                    className="h-9 rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-900"
                  >
                    {targetOptions}
                  </select>
                </div>
              </div>
              {loadingCompare && (
                <p className="text-xs text-zinc-500">{__("Computing comparison…")}</p>
              )}
              {compareError && (
                <p role="alert" className="text-xs font-medium text-rose-700">
                  {compareError}
                </p>
              )}
              {compareData && <RevisionDiffView compare={compareData} />}
            </div>

            <div
              id="rev-panel-snapshot"
              role="tabpanel"
              aria-labelledby="rev-tab-snapshot"
              hidden={activeTab !== 'snapshot'}
              className="space-y-3"
            >
              <RevisionSnapshotView
                loadingDetail={loadingDetail}
                detailError={detailError}
                detailData={detailData}
              />
            </div>
          </div>
        </div>
      </div>
    </dialog>
  );
};
