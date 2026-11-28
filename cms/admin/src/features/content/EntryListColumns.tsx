import React from 'react';
import { ApiClientError } from '../../api/client.ts';
import type { ContentListColumn } from '../../api/content.ts';
import type { TaxonomyDefinition } from '../../api/taxonomy.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import type { EntryTableQuery, EntryTableRow } from './types.ts';

export function readableEntryTableError(cause: unknown): string {
  if (cause instanceof ApiClientError && cause.status === 412) return __("412 — The item changed. Reload the list to get the latest version.");
  if (cause instanceof ApiClientError && cause.status === 403) return __("403 — You do not have permission to perform this action.");
  if (cause instanceof ApiClientError && cause.status === 401) return __("401 — Your session is no longer valid.");
  return cause instanceof Error ? cause.message : __("The request did not complete.");
}

export type TaxonomyTableQuery = EntryTableQuery & {
  taxonomy_key?: string;
  term_id?: number;
};

export interface HierarchicalDisplayRow {
  row: EntryTableRow;
  depth: number;
}

function boundedInt(raw: string | null, fallback: number, min: number, max: number): number {
  if (!raw || !/^\d+$/.test(raw)) return fallback;
  const value = Number(raw);
  return Number.isSafeInteger(value) && value >= min && value <= max ? value : fallback;
}

export function parseEntryTableHash(hash: string): TaxonomyTableQuery {
  const p = new URLSearchParams(hash.split('?')[1] ?? '');
  const query: TaxonomyTableQuery = {};
  const q = p.get('q')?.trim();
  if (q && [...q].length <= 200 && !/[\u0000-\u001f\u007f]/.test(q)) query.q = q;
  const status = p.get('status');
  if (status === 'draft' || status === 'published' || status === 'trash') query.status = status;
  const locale = p.get('locale')?.trim();
  if (locale && locale !== 'all') query.locale = locale;
  const category = boundedInt(p.get('category_id'), 0, 1, Number.MAX_SAFE_INTEGER);
  const tag = boundedInt(p.get('tag_id'), 0, 1, Number.MAX_SAFE_INTEGER);
  if (category) query.category_id = category;
  if (tag) query.tag_id = tag;
  if (p.has('taxonomy_key')) query.taxonomy_key = p.get('taxonomy_key') ?? '';
  if (p.has('term_id')) query.term_id = Number(p.get('term_id'));
  if (p.get('include_descendants') === 'true') query.include_descendants = true;
  const month = p.get('updated_month');
  if (month && /^\d{4}-(0[1-9]|1[0-2])$/.test(month) && month.slice(0, 4) !== '0000') {
    query.updated_month = month;
  }
  const sort = p.get('sort');
  if (
    sort === 'title' ||
    sort === 'id' ||
    sort === 'created_at' ||
    sort === 'updated_at' ||
    sort === 'trashed_at' ||
    sort === 'menu_order'
  ) {
    query.sort = sort;
  }
  if (p.get('order') === 'asc') query.order = 'asc';
  const limit = boundedInt(p.get('limit'), 20, 1, 100);
  if (limit !== 20) query.limit = limit;
  const offset = boundedInt(p.get('offset'), 0, 0, 1000000);
  if (offset) query.offset = offset;
  return query;
}

export function entryTableHash(base: string, filter: TaxonomyTableQuery): string {
  const p = new URLSearchParams();
  if (filter.q) p.set('q', filter.q);
  if (filter.status && filter.status !== 'all') p.set('status', filter.status);
  if (filter.locale && filter.locale !== 'all') p.set('locale', filter.locale);
  if (filter.status !== 'trash') {
    if (filter.category_id) p.set('category_id', String(filter.category_id));
    if (filter.tag_id) p.set('tag_id', String(filter.tag_id));
    if (filter.taxonomy_key !== undefined) p.set('taxonomy_key', filter.taxonomy_key);
    if (filter.term_id !== undefined) p.set('term_id', String(filter.term_id));
    if (filter.include_descendants) p.set('include_descendants', 'true');
    if (filter.updated_month) p.set('updated_month', filter.updated_month);
  }
  const defaultSort = filter.status === 'trash' ? 'trashed_at' : 'updated_at';
  if (filter.sort && filter.sort !== defaultSort) p.set('sort', filter.sort);
  if (filter.order && filter.order !== 'desc') p.set('order', filter.order);
  if (filter.limit && filter.limit !== 20) p.set('limit', String(filter.limit));
  if (filter.offset) p.set('offset', String(filter.offset));
  return `${base}${p.size ? `?${p.toString()}` : ''}`;
}

/** Never render a user-supplied URL with a protocol or scheme-relative origin. */
export function safePublicPath(path: string | null | undefined): string | null {
  return path && path.startsWith('/') && !path.startsWith('//') && !/[\\\u0000-\u001f\u007f]/.test(path)
    ? path : null;
}

/**
 * Builds deterministic indented tree rows for visible entries whose parents
 * exist in the loaded slice, preserving orphan/filtered entries safely at root.
 */
export function buildHierarchicalDisplayRows(
  rows: EntryTableRow[],
  enabled: boolean,
): HierarchicalDisplayRow[] {
  if (!enabled || rows.length === 0) {
    return rows.map(row => ({ row, depth: 0 }));
  }

  const presentIds = new Set(rows.map(r => r.id));
  const childrenByParent = new Map<number, EntryTableRow[]>();
  const roots: EntryTableRow[] = [];

  for (const row of rows) {
    const pid = row.parent_id ?? null;
    if (pid !== null && pid !== row.id && presentIds.has(pid)) {
      const bucket = childrenByParent.get(pid);
      if (bucket) bucket.push(row);
      else childrenByParent.set(pid, [row]);
    } else {
      roots.push(row);
    }
  }

  const out: HierarchicalDisplayRow[] = [];
  const visited = new Set<number>();

  const walk = (node: EntryTableRow, depth: number) => {
    if (visited.has(node.id)) return;
    visited.add(node.id);
    out.push({ row: node, depth: Math.min(depth, 7) });
    const children = childrenByParent.get(node.id);
    if (children) {
      for (const child of children) {
        walk(child, depth + 1);
      }
    }
  };

  for (const root of roots) {
    walk(root, 0);
  }
  for (const row of rows) {
    if (!visited.has(row.id)) {
      walk(row, 0);
    }
  }
  return out;
}

export function resolveListColumnLabel(
  col: ContentListColumn,
  taxonomies: readonly TaxonomyDefinition[],
): string {
  const custom = col.label?.trim();
  if (custom) return custom;
  if (col.kind === 'menu_order') return __("Order");
  if (col.kind === 'parent') return __("Parent");
  if (col.kind === 'taxonomy') {
    return taxonomies.find(t => t.key === col.key)?.label ?? col.key;
  }
  return col.key;
}

function formatFieldCellValue(value: unknown): string {
  if (value === null || value === undefined || value === '') return '—';
  if (typeof value === 'boolean') return value ? __("Yes") : __("No");
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  if (typeof value === 'string') return value.slice(0, 140);
  if (Array.isArray(value)) {
    return value.length === 0 ? '—' : value.map(item => String(item)).join(', ').slice(0, 140);
  }
  if (typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    if (typeof obj.label === 'string' && obj.label.trim()) return obj.label.slice(0, 140);
    if (typeof obj.url === 'string' && obj.url.trim()) return obj.url.slice(0, 140);
    return JSON.stringify(obj).slice(0, 120);
  }
  return '—';
}

export const EntryListColumnCell: React.FC<{
  column: ContentListColumn;
  row: EntryTableRow;
  rowTitleById: ReadonlyMap<number, string>;
  renderTermsCell: (row: EntryTableRow, taxKey: string) => React.ReactNode;
}> = ({ column, row, rowTitleById, renderTermsCell }) => {
  useI18n();
  if (column.kind === 'menu_order') {
    return <span className="font-mono text-xs tabular-nums text-zinc-600">{row.menu_order ?? 0}</span>;
  }
  if (column.kind === 'parent') {
    if (!row.parent_id) return <span className="text-zinc-400">—</span>;
    const label = rowTitleById.get(row.parent_id);
    return (
      <span className="text-xs text-zinc-700">
        {label ? `${label} (#${row.parent_id})` : `#${row.parent_id}`}
      </span>
    );
  }
  if (column.kind === 'taxonomy') {
    return <>{renderTermsCell(row, column.key)}</>;
  }
  const hasField = Object.prototype.hasOwnProperty.call(row.fields ?? {}, column.key);
  if (!hasField) {
    return <span className="text-zinc-400">—</span>;
  }
  return (
    <span className="text-xs text-zinc-700">
      {formatFieldCellValue(row.fields[column.key])}
    </span>
  );
};
