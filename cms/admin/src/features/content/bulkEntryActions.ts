import { ApiClientError } from '../../api/client.ts';
import {
  changeContentEntryPublication,
  patchEntryMetadata,
  purgeContentEntry,
  restoreContentEntry,
  trashContentEntry,
} from '../../api/content.ts';
import { formatNumber } from '../../i18n/format.ts';
import { tStatic } from '../../i18n/index.tsx';
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
    return { ok: false, message: tStatic('bulk.no_rows_selected') };
  }
  const bounded = rows.slice(0, MAX_BULK_ITEMS);
  const locales = new Set(bounded.map(row => row.locale ?? ''));
  if (locales.size > 1) {
    return { ok: false, message: tStatic('entry_lifecycle.bulk_mixed_locale_blocked') };
  }
  return { ok: true, locale: bounded[0]?.locale ?? '' };
}

export function describeBulkAction(spec: BulkActionSpec): string {
  switch (spec.kind) {
    case 'published':
      return tStatic('feature.content.entrylistpage.publish');
    case 'draft':
      return tStatic('feature.content.entrylistpage.move_to_draft');
    case 'trash':
      return tStatic('entry_lifecycle.move_to_trash');
    case 'restore':
      return tStatic('entry_lifecycle.restore');
    case 'purge':
      return tStatic('entry_lifecycle.purge_permanently');
    case 'taxonomy':
      return spec.operation === 'add'
        ? tStatic('bulk.taxonomy_action_add', { value1: spec.taxonomyLabel })
        : spec.operation === 'remove'
          ? tStatic('bulk.taxonomy_action_remove', { value1: spec.taxonomyLabel })
          : tStatic('bulk.taxonomy_action_replace', { value1: spec.taxonomyLabel });
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
      return tStatic('entry_lifecycle.bulk_confirm_move_to_trash', { value1: countText });
    case 'restore':
      return spec.confirmPublicRestore
        ? tStatic('bulk.confirm_restore_public', { value1: countText, value2: localeLabel })
        : tStatic('bulk.confirm_restore', { value1: countText, value2: localeLabel });
    case 'purge':
      return tStatic('bulk.confirm_purge', { value1: countText, value2: localeLabel });
    case 'taxonomy':
      if (spec.operation === 'replace' && spec.termIds.length === 0) {
        return tStatic('bulk.confirm_taxonomy_clear', {
          value1: spec.taxonomyLabel,
          value2: countText,
          value3: localeLabel,
        });
      }
      if (spec.operation === 'replace') {
        return tStatic('bulk.confirm_taxonomy_replace', {
          value1: spec.taxonomyLabel,
          value2: formatNumber(spec.termIds.length),
          value3: countText,
          value4: localeLabel,
        });
      }
      return tStatic('bulk.confirm_taxonomy_delta', {
        value1: describeBulkAction(spec),
        value2: countText,
        value3: localeLabel,
      });
    case 'published':
    case 'draft':
      return tStatic('feature.content.entrylistpage.are_you_sure_you_want_to_value_value_selected', {
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
        message: `#${target.id}: ${tStatic('entry_lifecycle.bulk_unattempted_message')}`,
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
        message: tStatic('feature.content.entrylistpage.value_value_succeeded', {
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
            message: `#${row.id}: ${tStatic('entry_lifecycle.bulk_maintenance_stopped')}`,
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
          message: `#${row.id}: ${tStatic('entry_lifecycle.bulk_uncertain_message')}`,
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
            ? tStatic('bulk.uncertain_reconciled_absent', { value1: item.id })
            : tStatic('bulk.uncertain_reconciled_changed', {
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
