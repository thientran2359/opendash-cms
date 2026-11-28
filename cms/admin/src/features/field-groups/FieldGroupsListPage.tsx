import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { deleteFieldGroup, setFieldGroupActive } from '../../api/content.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { ConfirmDialog } from '../../components/ConfirmDialog.tsx';
import { __, useI18n } from '../../i18n/index.tsx';
import type { FieldGroupDomain, FieldGroupRecord } from '../content/types.ts';

interface FieldGroupsListPageProps {
  groups: FieldGroupRecord[];
  loading: boolean;
  csrfToken: string | null;
  onRefresh: () => void;
  onCreateNew: () => void;
  onEdit: (groupId: string) => void;
  onDuplicate: (group: FieldGroupRecord) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export const FieldGroupsListPage: React.FC<FieldGroupsListPageProps> = ({
  groups,
  loading,
  csrfToken,
  onRefresh,
  onCreateNew,
  onEdit,
  onDuplicate,
  onApiError,
}) => {
  useI18n();
  const [domainFilter, setDomainFilter] = useState<'all' | FieldGroupDomain>('all');
  const [query, setQuery] = useState('');
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [purgeTarget, setPurgeTarget] = useState<FieldGroupRecord | null>(null);
  const [withdrawTarget, setWithdrawTarget] = useState<FieldGroupRecord | null>(null);
  const dialogRef = useRef<HTMLDialogElement | null>(null);

  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    if (purgeTarget && !dlg.open) dlg.showModal();
    else if (!purgeTarget && dlg.open) dlg.close();
  }, [purgeTarget]);

  const filtered = groups.filter(g => {
    if (domainFilter !== 'all' && g.domain !== domainFilter) return false;
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      g.title.toLowerCase().includes(q) ||
      g.local_key.toLowerCase().includes(q) ||
      g.group_id.toLowerCase().includes(q)
    );
  });

  const handleToggleActive = async (g: FieldGroupRecord) => {
    if (!csrfToken || busyId || g.owner.kind !== 'user') return;
    setBusyId(g.group_id);
    setError('');
    setNotice('');
    try {
      await setFieldGroupActive(g.group_id, g.version, !g.active, csrfToken);
      setNotice(__("Updated active state for {{value1}}.", { value1: g.title }));
      onRefresh();
    } catch (cause) {
      if (cause instanceof ApiClientError && (cause.status === 412 || cause.status === 409)) {
        setError(cause.message);
      } else {
        onApiError(cause, __("Could not update field group active state."));
      }
    } finally {
      setBusyId(null);
    }
  };

  const handleWithdraw = async (g: FieldGroupRecord, purge: boolean) => {
    if (!csrfToken || busyId || g.owner.kind !== 'user') return;
    setBusyId(g.group_id);
    setError('');
    setNotice('');
    try {
      await deleteFieldGroup(g.group_id, g.version, csrfToken, purge);
      setPurgeTarget(null);
      setNotice(
        purge
          ? __("Permanently deleted field group {{value1}}.", { value1: g.title })
          : __("Withdrew field group {{value1}}.", { value1: g.title }),
      );
      onRefresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : __("Could not remove field group."));
      if (cause instanceof ApiClientError && cause.status === 401) {
        onApiError(cause, __("Your session expired while deleting the field group."));
      }
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section className="space-y-4" data-testid="field-groups-list-page">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{__("Structure / Field groups")}</p>
          <h2 className="mt-1 text-xl font-bold tracking-tight text-zinc-950">{__("Field groups")}</h2>
          <p className="mt-1 text-sm text-zinc-600">
            {__("Create reusable field groups targeting content types, taxonomy terms, or custom options pages.")}
          </p>
        </div>
        <AdminButton type="button" variant="primary" size="md" disabled={!csrfToken} onClick={onCreateNew} data-testid="new-field-group-btn">
          {__("+ Add field group")}
        </AdminButton>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2.5 border-b border-zinc-200 pb-3">
        <div className="flex flex-wrap gap-1.5" role="group" aria-label={__("Filter by domain")}>
          {([
            ['all', __("All")],
            ['content', __("Content")],
            ['term', __("Term")],
            ['options_page', __("Options Page")],
          ] as const).map(([key, label]) => (
            <button
              key={key}
              type="button"
              onClick={() => setDomainFilter(key)}
              className={
                'h-8 rounded-md px-3 text-xs font-medium ' +
                (domainFilter === key
                  ? 'bg-od-accent text-white'
                  : 'border border-zinc-300 bg-white text-zinc-700 hover:bg-zinc-50')
              }
            >
              {label}
            </button>
          ))}
        </div>
        <input
          type="search"
          aria-label={__("Search field groups")}
          placeholder={__("Search title or key…")}
          className="h-8 w-full rounded-md border border-zinc-300 bg-white px-3 text-xs sm:w-64"
          value={query}
          onChange={e => setQuery(e.target.value)}
        />
      </div>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {loading && <p role="status" className="text-xs text-zinc-500">{__("Loading field groups…")}</p>}

      {!loading && filtered.length === 0 ? (
        <p className="rounded-xl border border-dashed border-zinc-300 bg-zinc-50/40 p-6 text-center text-sm text-zinc-500">
          {__("No matching field groups found.")}
        </p>
      ) : (
        <div role="region" aria-label={__("Field groups table")} tabIndex={0} className="overflow-x-auto rounded-xl border border-zinc-200 bg-white">
          <table className="w-full min-w-[700px] table-auto text-left text-xs">
            <thead className="border-b border-zinc-200 bg-zinc-50 text-[11px] font-semibold uppercase tracking-wider text-zinc-600">
              <tr>
                <th scope="col" className="px-3 py-2.5">{__("Title & key")}</th>
                <th scope="col" className="px-3 py-2.5">{__("Domain")}</th>
                <th scope="col" className="px-3 py-2.5">{__("Fields")}</th>
                <th scope="col" className="px-3 py-2.5">{__("Status")}</th>
                <th scope="col" className="px-3 py-2.5 text-right">{__("Actions")}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-100">
              {filtered.map(g => {
                const isUser = g.owner.kind === 'user';
                return (
                  <tr key={g.group_id} data-group-id={g.group_id} className="hover:bg-zinc-50/60">
                    <td className="px-3 py-2.5">
                      <button
                        type="button"
                        onClick={() => onEdit(g.group_id)}
                        className="text-left text-sm font-semibold text-zinc-950 hover:text-od-accent"
                      >
                        {g.title}
                      </button>
                      <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[11px] text-zinc-500">
                        <code className="font-mono">{g.local_key}</code>
                        <span>· {g.owner.kind}{g.owner.id ? `:${g.owner.id}` : ''}</span>
                      </div>
                    </td>
                    <td className="px-3 py-2.5">
                      <AdminBadge tone="neutral">{g.domain}</AdminBadge>
                    </td>
                    <td className="px-3 py-2.5 tabular-nums text-zinc-700">{g.fields.length}</td>
                    <td className="px-3 py-2.5">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <AdminBadge tone={g.active && g.status === 'active' ? 'success' : 'neutral'}>
                          {g.status !== 'active' ? __("Withdrawn") : g.active ? __("Active") : __("Inactive")}
                        </AdminBadge>
                        {isUser && g.status === 'active' && (
                          <button
                            type="button"
                            disabled={!csrfToken || busyId === g.group_id}
                            onClick={() => void handleToggleActive(g)}
                            className="text-[11px] font-medium text-od-accent hover:underline"
                          >
                            {g.active ? __("Disable") : __("Enable")}
                          </button>
                        )}
                      </div>
                    </td>
                    <td className="px-3 py-2.5 text-right">
                      <div className="flex flex-wrap justify-end gap-1.5">
                        <AdminButton type="button" variant="secondary" size="sm" onClick={() => onEdit(g.group_id)}>
                          {isUser ? __("Edit") : __("View")}
                        </AdminButton>
                        <AdminButton type="button" variant="secondary" size="sm" onClick={() => onDuplicate(g)}>
                          {__("Duplicate")}
                        </AdminButton>
                        {isUser && g.status === 'active' && (
                          <AdminButton type="button" variant="danger" size="sm"
                            disabled={!csrfToken || busyId === g.group_id}
                            onClick={() => setWithdrawTarget(g)}>
                            {__("Withdraw")}
                          </AdminButton>
                        )}
                        {isUser && (
                          <AdminButton type="button" variant="danger" size="sm"
                            disabled={!csrfToken || busyId === g.group_id}
                            data-testid={`purge-group-btn-${g.group_id}`}
                            onClick={() => setPurgeTarget(g)}>
                            {__("Purge")}
                          </AdminButton>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <ConfirmDialog
        open={Boolean(withdrawTarget)}
        title={__("Withdraw field group {{value1}}?", { value1: withdrawTarget?.title ?? '' })}
        description={__("Withdrawing detaches this field group from active editors while preserving existing stored values until purged.")}
        confirmLabel={busyId ? __("Withdrawing…") : __("Withdraw")}
        busy={Boolean(busyId)}
        testId="confirm-withdraw-group-dialog"
        onCancel={() => setWithdrawTarget(null)}
        onConfirm={() => {
          if (!withdrawTarget) return;
          const target = withdrawTarget;
          setWithdrawTarget(null);
          void handleWithdraw(target, false);
        }}
      />

      <dialog
        ref={dialogRef}
        aria-labelledby="purge-group-dialog-title"
        onCancel={e => {
          if (busyId) e.preventDefault();
          else setPurgeTarget(null);
        }}
        className="fixed inset-0 m-auto max-w-md rounded-xl border border-zinc-200 bg-white p-5 shadow-xl overscroll-contain backdrop:bg-zinc-950/50 backdrop:overscroll-contain"
      >
        {purgeTarget && (
          <div className="space-y-4">
            <h2 id="purge-group-dialog-title" className="text-base font-semibold text-zinc-950">
              {__("Permanently delete {{value1}}?", { value1: purgeTarget.title })}
            </h2>
            <p className="text-sm text-zinc-600">
              {__("Purging removes the field group definition permanently (?purge=true) once all stored references are safe or empty.")}
            </p>
            <div className="flex justify-end gap-2">
              <AdminButton type="button" variant="secondary" size="md" autoFocus disabled={Boolean(busyId)} onClick={() => setPurgeTarget(null)}>
                {__("Cancel")}
              </AdminButton>
              <AdminButton type="button" variant="danger" size="md" disabled={Boolean(busyId)}
                data-testid="confirm-purge-group-btn"
                onClick={() => void handleWithdraw(purgeTarget, true)}>
                {busyId ? __("Deleting…") : __("Delete permanently")}
              </AdminButton>
            </div>
          </div>
        )}
      </dialog>
    </section>
  );
};
