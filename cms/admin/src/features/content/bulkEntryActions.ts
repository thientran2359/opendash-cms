import { ApiClientError } from '../../api/client.ts';
import {
  changeContentEntryPublication,
  patchEntryMetadata,
  purgeContentEntry,
  restoreContentEntry,
  trashContentEntry,
} from '../../api/content.ts';
import { formatNumber } from '../../i18n/format.ts';
import { __ } from '../../i18n/index.tsx';
import type { EntryTableRow, TaxonomyDeltaOp } from './types.ts';

export const MAX_BULK_ITEMS = 100;

export type BulkItemStatus =
  | 'succeeded'
  | 'stale'
  | 'failed'
  | 'uncertain'
  | 'unattempted';

export type BulkActionSpec =
  | { kind: 'published' }
  | { kind: 'draft' }
  | { kind: 'trash' }
  | { kind: 'restore'; confirmPublicRestore: boolean }
  | { kind: 'purge' }
  | {
      kind: 'taxonomy';
      taxonomyKey: string;
      taxonomyLabel: string;
      operation: TaxonomyDeltaOp;
      termIds: number[];
      schemaToken: string;
    };

export interface BulkCapturedRow {
  id: number;
  version: number;
  title: string;
  locale: string;
  publication_status: 'draft' | 'published';
  term_ids: number[];
  immediate_public_restore: boolean;
}

export interface BulkResultItem {
  id: number;
  status: BulkItemStatus;
  message: string;
  beforeVersion: number;
  unresolvedUncertain?: boolean;
}

export interface BulkRunOutcome {
  correlationId: string;
  reports: BulkResultItem[];
  capturedById: Map<number, BulkCapturedRow>;
  stoppedEarly: boolean;
}

export function captureBulkRows(rows: EntryTableRow[]): BulkCapturedRow[] {
  return rows.slice(0, MAX_BULK_ITEMS).map(row => ({
    id: row.id,
    version: row.version,
    title: row.title,
    locale: row.locale ?? '',
    publication_status: row.publication_status,
    term_ids: [...row.term_ids],
    immediate_public_restore: Boolean(row.capabilities?.immediate_public_restore),
  }));
}

/** Validates current-page selection bounds (1..=100) and single-locale scope. */
export function validateBulkSelection(
  rows: EntryTableRow[],
): { ok: true; locale: string } | { ok: false; message: string } {
  if (!rows.length) {
    return { ok: false, message: __("Select at least one item on the current page first.") };
  }
  const bounded = rows.slice(0, MAX_BULK_ITEMS);
  const locales = new Set(bounded.map(row => row.locale ?? ''));
  if (locales.size > 1) {
    return { ok: false, message: __("Selected entries span multiple languages. Filter by a single language or select rows from one language before running a bulk action.") };
  }
  return { ok: true, locale: bounded[0]?.locale ?? '' };
}

export function describeBulkAction(spec: BulkActionSpec): string {
  switch (spec.kind) {
    case 'published':
      return __("publish");
    case 'draft':
      return __("move to draft");
    case 'trash':
      return __("Move to Trash");
    case 'restore':
      return __("Restore");
    case 'purge':
      return __("Delete permanently");
    case 'taxonomy':
      return spec.operation === 'add'
        ? __("add {{value1}} terms", { value1: spec.taxonomyLabel })
        : spec.operation === 'remove'
          ? __("remove {{value1}} terms", { value1: spec.taxonomyLabel })
          : __("replace {{value1}} terms", { value1: spec.taxonomyLabel });
  }
}

export function buildBulkConfirmationPrompt(
  spec: BulkActionSpec,
  count: number,
  locale: string,
): string {
  const countText = formatNumber(count);
  const localeLabel = locale || 'default';
  switch (spec.kind) {
    case 'trash':
      return __("Move {{value1}} selected item(s) to Trash? You can restore them later from Trash.", { value1: countText });
    case 'restore':
      return spec.confirmPublicRestore
        ? __("Restore {{value1}} selected item(s) in language {{value2}} from Trash? Immediate-publication entries will become public immediately.", { value1: countText, value2: localeLabel })
        : __("Restore {{value1}} selected item(s) in language {{value2}} from Trash?", { value1: countText, value2: localeLabel });
    case 'purge':
      return __("Permanently delete {{value1}} selected item(s) in language {{value2}}? This action cannot be undone.", { value1: countText, value2: localeLabel });
    case 'taxonomy':
      if (spec.operation === 'replace' && spec.termIds.length === 0) {
        return __("Clear all {{value1}} terms on {{value2}} selected item(s) in language {{value3}}? Other taxonomies remain untouched.", {
          value1: spec.taxonomyLabel,
          value2: countText,
          value3: localeLabel,
        });
      }
      if (spec.operation === 'replace') {
        return __("Replace all {{value1}} terms with {{value2}} selected term(s) on {{value3}} item(s) in language {{value4}}? Other taxonomies remain untouched.", {
          value1: spec.taxonomyLabel,
          value2: formatNumber(spec.termIds.length),
          value3: countText,
          value4: localeLabel,
        });
      }
      return __("Apply \"{{value1}}\" to {{value2}} selected item(s) in language {{value3}}?", {
        value1: describeBulkAction(spec),
        value2: countText,
        value3: localeLabel,
      });
    case 'published':
    case 'draft':
      return __("Are you sure you want to {{value1}} {{value2}} selected items? Each item is processed separately and version conflicts may occur.", {
        value1: describeBulkAction(spec),
        value2: countText,
      });
  }
}

export interface RunSequentialBulkParams {
  typeKey: string;
  spec: BulkActionSpec;
  capturedRows: BulkCapturedRow[];
  csrfToken: string;
  startingScope: string;
  getCurrentScope: () => string;
  isAlive: () => boolean;
  isCancelled: () => boolean;
  setActiveAbort: (controller: AbortController | null) => void;
  formatError: (cause: unknown) => string;
  onUnauthorized: (cause: unknown) => void;
}

/**
 * Executes a bounded sequential 1-in-flight per-row mutation loop with strict CAS,
 * scope/auth/maintenance/cancel stop fences, and 5-way outcome classification
 * (`succeeded | stale | failed | uncertain | unattempted`).
 */
export async function runSequentialBulk(
  params: RunSequentialBulkParams,
): Promise<BulkRunOutcome> {
  const {
    typeKey,
    spec,
    capturedRows,
    csrfToken,
    startingScope,
    getCurrentScope,
    isAlive,
    isCancelled,
    setActiveAbort,
    formatError,
    onUnauthorized,
  } = params;

  const correlationId = `bulk-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  const noun = describeBulkAction(spec);
  const reports: BulkResultItem[] = [];
  const capturedById = new Map<number, BulkCapturedRow>();
  for (const row of capturedRows) capturedById.set(row.id, row);
  let stoppedEarly = false;

  const pushUnattemptedFrom = (startIndex: number) => {
    stoppedEarly = true;
    for (let rem = startIndex; rem < capturedRows.length; rem += 1) {
      const target = capturedRows[rem];
      reports.push({
        id: target.id,
        status: 'unattempted',
        beforeVersion: target.version,
        message: `#${target.id}: ${__("Not attempted because the batch stopped early.")}`,
      });
    }
  };

  for (let idx = 0; idx < capturedRows.length; idx += 1) {
    const row = capturedRows[idx];
    if (!isAlive() || getCurrentScope() !== startingScope || isCancelled()) {
      pushUnattemptedFrom(idx);
      break;
    }

    const controller = new AbortController();
    setActiveAbort(controller);
    try {
      switch (spec.kind) {
        case 'published':
        case 'draft':
          await changeContentEntryPublication(
            typeKey,
            row.id,
            row.version,
            spec.kind,
            csrfToken,
            controller.signal,
          );
          break;
        case 'trash':
          await trashContentEntry(typeKey, row.id, row.version, csrfToken, controller.signal);
          break;
        case 'restore':
          await restoreContentEntry(typeKey, row.id, row.version, csrfToken, {
            confirmPublicRestore: spec.confirmPublicRestore && row.immediate_public_restore,
            signal: controller.signal,
          });
          break;
        case 'purge':
          await purgeContentEntry(typeKey, row.id, row.version, csrfToken, {
            signal: controller.signal,
          });
          break;
        case 'taxonomy':
          await patchEntryMetadata(
            typeKey,
            row.id,
            row.version,
            {
              expected_schema_token: spec.schemaToken,
              taxonomy: {
                key: spec.taxonomyKey,
                operation: spec.operation,
                term_ids: spec.termIds,
              },
            },
            csrfToken,
            controller.signal,
          );
          break;
      }

      reports.push({
        id: row.id,
        status: 'succeeded',
        beforeVersion: row.version,
        message: __("#{{value1}}: {{value2}} succeeded.", {
          value1: row.id,
          value2: noun,
        }),
      });
    } catch (cause) {
      if (cause instanceof ApiClientError) {
        if (cause.status === 412) {
          reports.push({
            id: row.id,
            status: 'stale',
            beforeVersion: row.version,
            message: `#${row.id}: ${formatError(cause)}`,
          });
        } else if (cause.status === 401) {
          reports.push({
            id: row.id,
            status: 'failed',
            beforeVersion: row.version,
            message: `#${row.id}: ${formatError(cause)}`,
          });
          onUnauthorized(cause);
          pushUnattemptedFrom(idx + 1);
          break;
        } else if (cause.status === 503) {
          reports.push({
            id: row.id,
            status: 'failed',
            beforeVersion: row.version,
            message: `#${row.id}: ${__("System entered maintenance (503). Remaining items were not attempted.")}`,
          });
          pushUnattemptedFrom(idx + 1);
          break;
        } else {
          reports.push({
            id: row.id,
            status: 'failed',
            beforeVersion: row.version,
            message: `#${row.id}: ${formatError(cause)}`,
          });
        }
      } else {
        // Network error or in-flight cancellation: commit outcome is strictly uncertain.
        reports.push({
          id: row.id,
          status: 'uncertain',
          beforeVersion: row.version,
          unresolvedUncertain: true,
          message: `#${row.id}: ${__("Commit outcome is uncertain (request interrupted or network error). Refresh and verify state before retrying.")}`,
        });
        if (isCancelled() || !isAlive() || getCurrentScope() !== startingScope) {
          pushUnattemptedFrom(idx + 1);
          break;
        }
      }
    } finally {
      setActiveAbort(null);
    }
  }

  return {
    correlationId,
    reports,
    capturedById,
    stoppedEarly,
  };
}

export interface ReconciledBulkPlan {
  retryableRows: EntryTableRow[];
  unresolvedUncertainIds: number[];
  annotatedResults: BulkResultItem[];
}

/**
 * Reconciles bulk results against freshly loaded authoritative server rows before
 * any explicit retry. Never replays `succeeded` rows and never assumes an `uncertain`
 * row succeeded merely because its version or presence changed (another actor may have
 * edited it).
 */
export function reconcileBulkResults(
  spec: BulkActionSpec,
  results: BulkResultItem[],
  refreshedRows: EntryTableRow[],
): ReconciledBulkPlan {
  const rowById = new Map<number, EntryTableRow>();
  for (const r of refreshedRows) rowById.set(r.id, r);

  const retryableRows: EntryTableRow[] = [];
  const unresolvedUncertainIds: number[] = [];
  const annotatedResults: BulkResultItem[] = [];

  for (const item of results) {
    if (item.status === 'succeeded') {
      annotatedResults.push(item);
      continue;
    }

    const fresh = rowById.get(item.id);
    if (item.status === 'uncertain') {
      if (!fresh || fresh.version !== item.beforeVersion) {
        // Server state or version changed after an uncertain outcome; attribution cannot
        // be proven automatically — keep unresolved and require explicit user review.
        unresolvedUncertainIds.push(item.id);
        annotatedResults.push({
          ...item,
          unresolvedUncertain: true,
          message: !fresh
            ? __("#{{value1}}: Uncertain commit — item is no longer in this view after refresh. Verify its state manually; it will not be retried automatically.", { value1: item.id })
            : __("#{{value1}}: Uncertain commit — version changed on the server (v{{value2}} → v{{value3}}). Another actor or the prior request may have modified it; verify manually before taking further action.", {
                value1: item.id,
                value2: item.beforeVersion,
                value3: fresh.version,
              }),
        });
        continue;
      }
      // Version is unchanged -> the previous uncertain request definitely did not commit.
      retryableRows.push(fresh);
      annotatedResults.push({
        ...item,
        unresolvedUncertain: false,
      });
      continue;
    }

    annotatedResults.push(item);
    if (!fresh) continue;
    if (spec.kind === 'published' && fresh.publication_status === 'published') continue;
    if (spec.kind === 'draft' && fresh.publication_status === 'draft') continue;
    retryableRows.push(fresh);
  }

  return {
    retryableRows,
    unresolvedUncertainIds,
    annotatedResults,
  };
}
