import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  createUser, listUsers, removeUser, resetUserPassword, updateUser,
  type UserRole, type UserSummary,
} from '../../api/auth.ts';
import { ApiClientError } from '../../api/client.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface UsersPanelProps {
  mode?: 'list' | 'new';
  csrfToken: string | null;
  currentUserId: number | null;
  onApiError: (error: unknown, fallback: string) => void;
  onOwnAccountChanged: () => void;
}

const PAGE_SIZE = 20;
const fieldClass =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent disabled:opacity-50';

export const UsersPanel: React.FC<UsersPanelProps> = ({
  mode = 'list', csrfToken, currentUserId, onApiError, onOwnAccountChanged,
}) => {
  const { t: translate, tCount } = useI18n();
  const [users, setUsers] = useState<UserSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [newUsername, setNewUsername] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [newRole, setNewRole] = useState<UserRole>('editor');
  const [editing, setEditing] = useState<UserSummary | null>(null);
  const [editUsername, setEditUsername] = useState('');
  const [editRole, setEditRole] = useState<UserRole>('editor');
  const [resetting, setResetting] = useState<UserSummary | null>(null);
  const [resetPassword, setResetPassword] = useState('');
  const refreshSeq = useRef(0);
  const editInputRef = useRef<HTMLInputElement>(null);
  const resetInputRef = useRef<HTMLInputElement>(null);

  const refresh = useCallback(async () => {
    if (!csrfToken || currentUserId === null) return;
    const sequence = ++refreshSeq.current;
    setLoading(true);
    try {
      const result = await listUsers(PAGE_SIZE, page * PAGE_SIZE);
      if (sequence !== refreshSeq.current) return;
      setUsers(result.items);
      setTotal(result.total);
      // A 412 retry must use the latest version without losing form inputs.
      setEditing(previous => {
        if (!previous) return previous;
        return result.items.find(person => person.id === previous.id) ?? previous;
      });
      setResetting(previous => {
        if (!previous) return previous;
        return result.items.find(person => person.id === previous.id) ?? previous;
      });
      setError('');
    } catch (err) {
      if (sequence !== refreshSeq.current) return;
      setError(err instanceof Error ? err.message : tStatic('feature.users.userspanel.could_not_load_users'));
      if (err instanceof ApiClientError && err.status === 401) {
        onApiError(err, tStatic('feature.users.userspanel.your_session_has_expired'));
      }
    } finally {
      if (sequence === refreshSeq.current) setLoading(false);
    }
  }, [csrfToken, currentUserId, page, onApiError]);

  useEffect(() => {
    void refresh();
    return () => { refreshSeq.current += 1; };
  }, [refresh]);

  useEffect(() => {
    if (editing) editInputRef.current?.focus();
  }, [editing?.id]);

  useEffect(() => {
    if (resetting) resetInputRef.current?.focus();
  }, [resetting?.id]);

  const runMutation = async (action: () => Promise<void>, fallback: string) => {
    if (busy || !csrfToken) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      await action();
    } catch (err) {
      // Keep all form fields open and unchanged after 401/409/412/503.
      setError(err instanceof Error ? err.message : fallback);
      if (err instanceof ApiClientError && err.status === 401) onApiError(err, fallback);
    } finally {
      setBusy(false);
    }
  };

  const openEdit = (user: UserSummary) => {
    setEditing(user);
    setEditUsername(user.username);
    setEditRole(user.role);
    setResetting(null);
    setError('');
  };

  const handleCreate = (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken) return;
    void runMutation(async () => {
      const user = await createUser(newUsername, newPassword, newRole, csrfToken);
      setNewUsername('');
      setNewPassword('');
      setNewRole('editor');
      setNotice(tStatic('feature.users.userspanel.created_account', { username: user.username }));
      if (page === 0) await refresh();
      else setPage(0);
    }, tStatic('feature.users.userspanel.could_not_create_the_account'));
  };

  const handleSave = (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken || !editing) return;
    const target = editing;
    void runMutation(async () => {
      const changes = { username: editUsername, role: editRole };
      const changed = await updateUser(target.id, target.version, changes, csrfToken);
      setEditing(null);
      if (target.id === currentUserId && changed.version !== target.version) {
        onOwnAccountChanged();
        return;
      }
      setNotice(tStatic('feature.users.userspanel.saved_account', { username: changed.username }));
      await refresh();
    }, tStatic('feature.users.userspanel.could_not_update_the_account'));
  };

  const handleReset = (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken || !resetting) return;
    const target = resetting;
    if (!window.confirm(tStatic('feature.users.userspanel.reset_the_password_and_revoke_all_sessions_for', { username: target.username }))) return;
    void runMutation(async () => {
      await resetUserPassword(target.id, target.version, resetPassword, csrfToken);
      setResetting(null);
      setResetPassword('');
      if (target.id === currentUserId) {
        onOwnAccountChanged();
        return;
      }
      setNotice(tStatic('feature.users.userspanel.changed_the_password_and_revoked_sessions_for', { username: target.username }));
      await refresh();
    }, tStatic('api.auth.reset_password_failed'));
  };

  const handleDelete = (user: UserSummary) => {
    if (!csrfToken || !window.confirm(tStatic('feature.users.userspanel.permanently_delete_account', { username: user.username }))) return;
    void runMutation(async () => {
      await removeUser(user.id, user.version, csrfToken);
      setEditing(null);
      setResetting(null);
      if (user.id === currentUserId) {
        onOwnAccountChanged();
        return;
      }
      setNotice(tStatic('feature.users.userspanel.deleted_account', { username: user.username }));
      if (users.length === 1 && page > 0) setPage(page - 1);
      else await refresh();
    }, tStatic('api.auth.remove_user_failed'));
  };

  const userActions = (person: UserSummary) => (
    <div className="flex flex-wrap items-center gap-1.5">
      <AdminButton variant="secondary" size="sm" disabled={busy} aria-label={translate('feature.users.userspanel.edit_account', { username: person.username })} onClick={() => openEdit(person)}>{translate('multilingual.edit')}
      </AdminButton>
      <AdminButton variant="secondary" size="sm" disabled={busy} aria-label={translate('feature.users.userspanel.reset_account_password', { username: person.username })} onClick={() => { setResetting(person); setResetPassword(''); setEditing(null); setError(''); }}>{translate('feature.users.userspanel.change_password')}
      </AdminButton>
      <AdminButton variant="danger" size="sm" disabled={busy} aria-label={translate('feature.users.userspanel.delete_account', { username: person.username })} onClick={() => handleDelete(person)}>{translate('multilingual.delete')}
      </AdminButton>
    </div>
  );

  return (
    <div className="w-full min-w-0 space-y-5">
      {error && (
        <AdminAlert tone="error" className="flex flex-wrap items-center gap-2">
          <span>{error}</span>
          <AdminButton variant="ghost" size="sm" onClick={() => void refresh()}>{translate('multilingual.reload')}</AdminButton>
        </AdminAlert>
      )}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {mode === 'list' && <>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{translate('feature.users.userspanel.accounts')}</h2>
          <p className="mt-0.5 text-xs text-zinc-500">{translate('feature.users.userspanel.changing_a_username_role_or_password_will_revoke_active_sessions_for_that_accoun')}</p>
        </div>
        <div className="flex items-center gap-2.5">
          <span className="text-xs font-medium text-zinc-500">{loading ? translate('feature.dashboard.dashboardpage.loading') : tCount('feature.users.userspanel.accounts_count', total)}</span>
          <AdminButton variant="secondary" size="sm" disabled={loading || busy} onClick={() => void refresh()}>{translate('feature.users.userspanel.refresh')}</AdminButton>
        </div>
      </div>
      <div className="od-users-table w-full min-w-0 max-w-full overflow-x-auto rounded-xl border border-zinc-200 bg-white">
        <table className="w-full min-w-[520px] text-left text-sm">
          <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold uppercase tracking-wider text-zinc-600">
            <tr><th className="px-3 py-2.5">{translate('feature.users.userspanel.account_2')}</th><th className="px-3 py-2.5">{translate('feature.users.userspanel.role')}</th><th className="px-3 py-2.5">{translate('feature.users.userspanel.version')}</th><th className="px-3 py-2.5">{translate('multilingual.actions')}</th></tr>
          </thead>
          <tbody className="divide-y divide-zinc-100">
            {users.map(person => <tr key={person.id} className="hover:bg-zinc-50/70">
              <th scope="row" className="px-3 py-2.5 font-medium text-zinc-950">
                {person.username}
                {person.id === currentUserId && <AdminBadge tone="neutral" className="ml-2">{translate('feature.users.userspanel.you')}</AdminBadge>}
              </th>
              <td className="px-3 py-2.5">
                <AdminBadge tone={person.role === 'admin' ? 'success' : 'neutral'}>
                  {person.role === 'admin' ? translate('app.role_admin') : translate('app.role_editor')}
                </AdminBadge>
              </td>
              <td className="px-3 py-2.5 text-xs tabular-nums text-zinc-500">v{person.version}</td>
              <td className="px-3 py-2.5">{userActions(person)}</td>
            </tr>)}
            {!loading && users.length === 0 && <tr><td colSpan={4} className="p-6 text-center text-sm text-zinc-500">{translate('feature.users.userspanel.no_accounts_on_this_page')}</td></tr>}
          </tbody>
        </table>
      </div>
      <div className="od-users-cards space-y-3" aria-label={translate('feature.users.userspanel.mobile_account_list')}>
        {users.map(person => (
          <article key={person.id} aria-label={translate('feature.users.userspanel.account_3', { username: person.username })} className="min-w-0 space-y-2.5 rounded-xl border border-zinc-200 bg-white p-3.5">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div className="min-w-0">
                <strong className="block break-all text-sm text-zinc-950">{person.username}</strong>
                {person.id === currentUserId && <span className="text-xs text-zinc-500">{translate('feature.users.userspanel.you_2')}</span>}
              </div>
              <AdminBadge tone="neutral">v{person.version}</AdminBadge>
            </div>
            <p className="text-xs text-zinc-600">{person.role === 'admin' ? translate('app.role_admin') : translate('app.role_editor')}</p>
            {userActions(person)}
          </article>
        ))}
        {!loading && users.length === 0 && <p className="rounded-xl border border-zinc-200 p-4 text-sm text-zinc-500">{translate('feature.users.userspanel.no_accounts_on_this_page')}</p>}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-zinc-600">
        <span className="tabular-nums">{translate('feature.users.userspanel.trang')}{' '}{page + 1} / {Math.max(1, Math.ceil(total / PAGE_SIZE))}</span>
        <div className="flex gap-2">
          <AdminButton variant="secondary" size="sm" disabled={page === 0 || loading || busy} onClick={() => setPage(p => p - 1)}>{translate('feature.users.userspanel.previous')}</AdminButton>
          <AdminButton variant="secondary" size="sm" disabled={(page + 1) * PAGE_SIZE >= total || loading || busy} onClick={() => setPage(p => p + 1)}>{translate('feature.users.userspanel.sau')}</AdminButton>
        </div>
      </div>
      {editing && <form className="max-w-xl space-y-3.5 rounded-xl border border-blue-200 bg-blue-50/40 p-4" onSubmit={handleSave}>
        <h3 className="text-sm font-semibold text-zinc-950">{translate('multilingual.edit')}{' '}{editing.username} (v{editing.version})</h3>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700"><span>{translate('shell.auth_username')}</span><input ref={editInputRef} className={fieldClass} autoComplete="off" required minLength={3} maxLength={32} value={editUsername} onChange={e => setEditUsername(e.target.value)} disabled={busy}/></label>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700"><span>{translate('feature.users.userspanel.role')}</span><select className={fieldClass} value={editRole} onChange={e => setEditRole(e.target.value as UserRole)} disabled={busy}><option value="admin">{translate('shell.role_admin')}</option><option value="editor">{translate('app.role_editor')}</option></select></label>
        <div className="flex gap-2"><AdminButton type="submit" variant="primary" size="sm" disabled={busy}>{translate('feature.users.userspanel.save_changes')}</AdminButton><AdminButton variant="secondary" size="sm" onClick={() => setEditing(null)} disabled={busy}>{translate('multilingual.cancel')}</AdminButton></div>
      </form>}
      {resetting && <form className="max-w-xl space-y-3.5 rounded-xl border border-amber-200 bg-amber-50/40 p-4" onSubmit={handleReset}>
        <h3 className="text-sm font-semibold text-zinc-950">{translate('feature.users.userspanel.reset_password')}{' '}{resetting.username}</h3>
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700"><span>{translate('feature.users.userspanel.new_password')}</span><input ref={resetInputRef} type="password" className={fieldClass} required minLength={8} maxLength={128} autoComplete="new-password" value={resetPassword} onChange={e => setResetPassword(e.target.value)} disabled={busy}/></label>
        <div className="flex gap-2"><AdminButton type="submit" variant="primary" size="sm" disabled={busy}>{translate('feature.users.userspanel.reset_password_2')}</AdminButton><AdminButton variant="secondary" size="sm" onClick={() => setResetting(null)} disabled={busy}>{translate('multilingual.cancel')}</AdminButton></div>
      </form>}
      </>}
      {mode === 'new' && <form className="max-w-2xl space-y-4 rounded-xl border border-zinc-200 bg-white p-5" onSubmit={handleCreate}>
        <h3 className="text-base font-semibold text-zinc-950">{translate('feature.users.userspanel.add_account')}</h3>
        <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-3">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700"><span>{translate('shell.auth_username')}</span><input className={fieldClass} required minLength={3} maxLength={32} autoComplete="off" value={newUsername} onChange={e => setNewUsername(e.target.value)} disabled={busy}/></label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700"><span>{translate('shell.auth_password')}</span><input type="password" className={fieldClass} required minLength={8} maxLength={128} autoComplete="new-password" value={newPassword} onChange={e => setNewPassword(e.target.value)} disabled={busy}/></label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700"><span>{translate('feature.users.userspanel.role')}</span><select className={fieldClass} value={newRole} onChange={e => setNewRole(e.target.value as UserRole)} disabled={busy}><option value="editor">{translate('app.role_editor')}</option><option value="admin">{translate('shell.role_admin')}</option></select></label>
        </div>
        <AdminButton type="submit" variant="primary" disabled={busy || !csrfToken}>{translate('feature.users.userspanel.create_account')}</AdminButton>
      </form>}
    </div>
  );
};
