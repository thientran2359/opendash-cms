import React from 'react';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { __, useI18n } from '../../i18n/index.tsx';
import type { EntryTableResult, EntryTableRow } from './types.ts';

interface ContentTypeRetainedTrashPanelProps {
  activeKey: string;
  csrfToken: string | null;
  retainedTrash: { key: string; data: EntryTableResult } | null;
  retainedTrashOffset: number;
  retainedTrashLoading: boolean;
  retainedTrashError: string;
  retainedTrashProviderActive: boolean;
  retainedTrashBusyId: number | null;
  onReload: () => void;
  onChangeOffset: (offset: number) => void;
  onPurgeRow: (row: EntryTableRow) => void;
}

export const ContentTypeRetainedTrashPanel: React.FC<ContentTypeRetainedTrashPanelProps> = ({
  activeKey,
  csrfToken,
  retainedTrash,
  retainedTrashOffset,
  retainedTrashLoading,
  retainedTrashError,
  retainedTrashProviderActive,
  retainedTrashBusyId,
  onReload,
  onChangeOffset,
  onPurgeRow,
}) => {
  useI18n();

  return (
    <section
      className="mt-4 rounded-xl border border-amber-200 bg-white p-5 space-y-3"
      data-testid="retained-trash-maintenance"
      aria-label={__("Retained Trash Maintenance")}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h4 className="text-sm font-semibold text-zinc-950">
            {__("Retained Trash Maintenance")}
            {retainedTrash?.key === activeKey ? ` (${retainedTrash.data.total})` : ''}
          </h4>
          <p className="mt-1 text-xs text-zinc-600">
            {__("Metadata-only view for trashed entries retained under this inactive or withdrawn content type. Administrators may permanently delete retained Trash rows here.")}
          </p>
          <p className="mt-1 text-xs text-amber-800">
            {__("To restore entries instead of purging them, re-enable the content type provider first.")}
          </p>
        </div>
        <AdminButton
          variant="secondary"
          size="sm"
          disabled={retainedTrashLoading || retainedTrashBusyId !== null}
          onClick={onReload}
        >
          {__("Reload")}
        </AdminButton>
      </div>

      {retainedTrashProviderActive && (
        <AdminAlert tone="info" className="flex flex-wrap items-center justify-between gap-2">
          <span>{__("This content type is now active. Manage its entries in the ordinary Trash view.")}</span>
          <a
            href={activeKey === 'post' ? '#/posts?status=trash' : activeKey === 'page' ? '#/pages?status=trash' : `#/cpt/${encodeURIComponent(activeKey)}?status=trash`}
            className="font-medium text-od-accent underline"
          >
            {__("Open Trash")}
          </a>
        </AdminAlert>
      )}

      {retainedTrashError && <AdminAlert tone="error">{retainedTrashError}</AdminAlert>}

      {retainedTrashLoading && (
        <p role="status" className="text-xs text-zinc-500">
          {__("Loading retained Trash entries...")}
        </p>
      )}

      {!retainedTrashLoading && retainedTrash?.key === activeKey && retainedTrash.data.items.length === 0 && !retainedTrashError && (
        <p className="rounded-lg border border-dashed border-zinc-200 p-4 text-xs text-zinc-500">
          {__("No retained Trash entries for this content type.")}
        </p>
      )}

      {retainedTrash?.key === activeKey && retainedTrash.data.items.length > 0 && (
        <div className="space-y-3">
          <div className="overflow-x-auto rounded-lg border border-zinc-200">
            <table className="w-full table-auto text-left text-xs">
              <thead className="border-b border-zinc-200 bg-zinc-50 text-[11px] font-semibold uppercase tracking-wider text-zinc-600">
                <tr>
                  <th className="px-3 py-2">{__("Title")}</th>
                  <th className="px-3 py-2">{__("Language")}</th>
                  <th className="px-3 py-2">{__("Status")}</th>
                  <th className="px-3 py-2">{__("Trashed date")}</th>
                  <th className="px-3 py-2 text-right">{__("Delete")}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-100">
                {retainedTrash.data.items.map(row => (
                  <tr key={row.id} data-retained-trash-id={row.id}>
                    <td className="px-3 py-2">
                      <span className="font-medium text-zinc-900">{row.title}</span>{' '}
                      <span className="font-mono text-zinc-500">#{row.id} · /{row.slug}</span>
                    </td>
                    <td className="px-3 py-2 font-mono text-zinc-700">{row.locale ?? '—'}</td>
                    <td className="px-3 py-2">
                      <AdminBadge tone="neutral">
                        {row.publication_status === 'published' ? __("Published") : __("Draft")}
                      </AdminBadge>
                    </td>
                    <td className="px-3 py-2 font-mono text-zinc-600">
                      {row.trashed_at ? `${row.trashed_at.slice(0, 16)} UTC` : '—'}
                    </td>
                    <td className="px-3 py-2 text-right">
                      <AdminButton
                        variant="danger"
                        size="sm"
                        disabled={!csrfToken || retainedTrashBusyId !== null}
                        data-testid={`retained-trash-purge-${row.id}`}
                        onClick={() => onPurgeRow(row)}
                      >
                        {__("Delete permanently")}
                      </AdminButton>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {retainedTrash.data.total > 20 && (
            <div className="flex items-center justify-between text-xs text-zinc-600">
              <span>
                {retainedTrashOffset + 1}–{Math.min(retainedTrashOffset + retainedTrash.data.items.length, retainedTrash.data.total)} / {retainedTrash.data.total}
              </span>
              <div className="flex gap-2">
                <AdminButton
                  variant="secondary"
                  size="sm"
                  disabled={retainedTrashLoading || retainedTrashOffset === 0}
                  onClick={() => onChangeOffset(Math.max(0, retainedTrashOffset - 20))}
                >
                  {__("Previous page")}
                </AdminButton>
                <AdminButton
                  variant="secondary"
                  size="sm"
                  disabled={retainedTrashLoading || retainedTrashOffset + 20 >= retainedTrash.data.total}
                  onClick={() => onChangeOffset(retainedTrashOffset + 20)}
                >
                  {__("Next page")}
                </AdminButton>
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
};
