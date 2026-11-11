import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  createUser, listUsers, removeUser, resetUserPassword, updateUser,
  type UserRole, type UserSummary,
} from '../../api/auth.ts';
import { ApiClientError } from '../../api/client.ts';

interface UsersPanelProps {
  csrfToken: string | null;
  currentUserId: number | null;
  onApiError: (error: unknown, fallback: string) => void;
  onOwnAccountChanged: () => void;
}

const PAGE_SIZE = 20;
const fieldClass = 'w-full min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600';
const buttonClass = 'rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50';

export const UsersPanel: React.FC<UsersPanelProps> = ({
  csrfToken, currentUserId, onApiError, onOwnAccountChanged,
}) => {
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
      setError(err instanceof Error ? err.message : 'Không tải được danh sách người dùng.');
      if (err instanceof ApiClientError && err.status === 401) {
        onApiError(err, 'Phiên đăng nhập đã hết hạn.');
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
      setNotice('Đã tạo tài khoản ' + user.username + '.');
      if (page === 0) await refresh();
      else setPage(0);
    }, 'Không thể tạo tài khoản.');
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
      setNotice('Đã lưu tài khoản ' + changed.username + '.');
      await refresh();
    }, 'Không thể cập nhật tài khoản.');
  };

  const handleReset = (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken || !resetting) return;
    const target = resetting;
    if (!window.confirm('Đặt lại mật khẩu và thu hồi mọi phiên của ' + target.username + '?')) return;
    void runMutation(async () => {
      await resetUserPassword(target.id, target.version, resetPassword, csrfToken);
      setResetting(null);
      setResetPassword('');
      if (target.id === currentUserId) {
        onOwnAccountChanged();
        return;
      }
      setNotice('Đã thay mật khẩu và thu hồi phiên của ' + target.username + '.');
      await refresh();
    }, 'Không thể đặt lại mật khẩu.');
  };

  const handleDelete = (user: UserSummary) => {
    if (!csrfToken || !window.confirm('Xóa vĩnh viễn tài khoản ' + user.username + '?')) return;
    void runMutation(async () => {
      await removeUser(user.id, user.version, csrfToken);
      setEditing(null);
      setResetting(null);
      if (user.id === currentUserId) {
        onOwnAccountChanged();
        return;
      }
      setNotice('Đã xóa tài khoản ' + user.username + '.');
      if (users.length === 1 && page > 0) setPage(page - 1);
      else await refresh();
    }, 'Không thể xóa tài khoản.');
  };

  const userActions = (person: UserSummary) => (
    <div className="flex flex-wrap gap-2">
      <button className={buttonClass} type="button" disabled={busy} onClick={() => openEdit(person)}>
        Sửa<span className="sr-only"> tài khoản {person.username}</span>
      </button>
      <button className={buttonClass} type="button" disabled={busy} onClick={() => { setResetting(person); setResetPassword(''); setEditing(null); setError(''); }}>
        Đổi mật khẩu<span className="sr-only"> của {person.username}</span>
      </button>
      <button className={buttonClass} type="button" disabled={busy} onClick={() => handleDelete(person)}>
        Xóa<span className="sr-only"> tài khoản {person.username}</span>
      </button>
    </div>
  );

  return (
    <div className="w-full min-w-0 space-y-8">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Danh sách tài khoản</h2>
        <p className="mt-1 text-sm text-slate-500">Chỉ Admin có thể quản lý tài khoản. Thay đổi username, vai trò hoặc mật khẩu sẽ thu hồi phiên đang hoạt động của tài khoản đó.</p>
      </div>
      {error && <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800" role="alert">{error} <button className="underline" type="button" onClick={() => void refresh()}>Tải lại</button></div>}
      {notice && <p className="rounded-lg bg-emerald-50 p-3 text-sm text-emerald-800" role="status">{notice}</p>}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-slate-600">{loading ? 'Đang tải...' : total + ' tài khoản'}</p>
        <button className={buttonClass} type="button" disabled={loading || busy} onClick={() => void refresh()}>Làm mới</button>
      </div>
      <div className="od-users-table w-full min-w-0 max-w-full overflow-x-auto rounded-xl border border-slate-200">
        <table className="w-full min-w-[520px] text-left text-sm">
          <thead className="bg-slate-50 text-slate-600"><tr><th className="p-3">Tài khoản</th><th className="p-3">Vai trò</th><th className="p-3">Phiên bản</th><th className="p-3">Thao tác</th></tr></thead>
          <tbody className="divide-y divide-slate-200">
            {users.map(person => <tr key={person.id}>
              <th scope="row" className="p-3 font-medium text-slate-900">{person.username}{person.id === currentUserId && <span className="ml-2 text-xs text-slate-500">(Bạn)</span>}</th>
              <td className="p-3">{person.role === 'admin' ? 'Quản trị viên' : 'Biên tập viên'}</td>
              <td className="p-3">v{person.version}</td>
              <td className="p-3">{userActions(person)}</td>
            </tr>)}
            {!loading && users.length === 0 && <tr><td colSpan={4} className="p-5 text-center text-slate-500">Không có tài khoản trong trang này.</td></tr>}
          </tbody>
        </table>
      </div>
      <div className="od-users-cards space-y-3" aria-label="Danh sách tài khoản di động">
        {users.map(person => (
          <article key={person.id} aria-label={'Tài khoản ' + person.username} className="min-w-0 space-y-3 rounded-xl border border-slate-200 p-3">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div className="min-w-0">
                <strong className="block break-all text-sm text-slate-900">{person.username}</strong>
                {person.id === currentUserId && <span className="text-xs text-slate-500">(Bạn)</span>}
              </div>
              <span className="rounded-md bg-slate-100 px-2 py-1 text-xs text-slate-600">v{person.version}</span>
            </div>
            <p className="text-sm text-slate-600">{person.role === 'admin' ? 'Quản trị viên' : 'Biên tập viên'}</p>
            {userActions(person)}
          </article>
        ))}
        {!loading && users.length === 0 && <p className="rounded-xl border border-slate-200 p-4 text-sm text-slate-500">Không có tài khoản trong trang này.</p>}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3 text-sm text-slate-600">
        <span>Trang {page + 1} / {Math.max(1, Math.ceil(total / PAGE_SIZE))}</span>
        <div className="flex gap-2">
          <button className={buttonClass} type="button" disabled={page === 0 || loading || busy} onClick={() => setPage(p => p - 1)}>Trước</button>
          <button className={buttonClass} type="button" disabled={(page + 1) * PAGE_SIZE >= total || loading || busy} onClick={() => setPage(p => p + 1)}>Sau</button>
        </div>
      </div>
      {editing && <form className="space-y-4 rounded-xl border border-blue-200 bg-blue-50/40 p-4" onSubmit={handleSave}>
        <h3 className="font-semibold text-slate-900">Sửa {editing.username} (v{editing.version})</h3>
        <label className="block text-sm font-medium">Tên đăng nhập<input ref={editInputRef} className={'mt-1 ' + fieldClass} autoComplete="off" required minLength={3} maxLength={32} value={editUsername} onChange={e => setEditUsername(e.target.value)} disabled={busy}/></label>
        <label className="block text-sm font-medium">Vai trò<select className={'mt-1 ' + fieldClass} value={editRole} onChange={e => setEditRole(e.target.value as UserRole)} disabled={busy}><option value="admin">Admin</option><option value="editor">Editor</option></select></label>
        <div className="flex gap-2"><button type="submit" className="od-primary-action" disabled={busy}>Lưu thay đổi</button><button type="button" className={buttonClass} onClick={() => setEditing(null)} disabled={busy}>Hủy</button></div>
      </form>}
      {resetting && <form className="space-y-4 rounded-xl border border-amber-200 bg-amber-50/40 p-4" onSubmit={handleReset}>
        <h3 className="font-semibold text-slate-900">Đặt lại mật khẩu: {resetting.username}</h3>
        <label className="block text-sm font-medium">Mật khẩu mới<input ref={resetInputRef} type="password" className={'mt-1 ' + fieldClass} required minLength={8} maxLength={128} autoComplete="new-password" value={resetPassword} onChange={e => setResetPassword(e.target.value)} disabled={busy}/></label>
        <div className="flex gap-2"><button type="submit" className="od-primary-action" disabled={busy}>Đặt lại mật khẩu</button><button type="button" className={buttonClass} onClick={() => setResetting(null)} disabled={busy}>Hủy</button></div>
      </form>}
      <form className="space-y-4 rounded-xl border border-slate-200 p-4" onSubmit={handleCreate}>
        <h3 className="font-semibold text-slate-900">Thêm tài khoản</h3>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <label className="text-sm font-medium">Tên đăng nhập<input className={'mt-1 ' + fieldClass} required minLength={3} maxLength={32} autoComplete="off" value={newUsername} onChange={e => setNewUsername(e.target.value)} disabled={busy}/></label>
          <label className="text-sm font-medium">Mật khẩu<input type="password" className={'mt-1 ' + fieldClass} required minLength={8} maxLength={128} autoComplete="new-password" value={newPassword} onChange={e => setNewPassword(e.target.value)} disabled={busy}/></label>
          <label className="text-sm font-medium">Vai trò<select className={'mt-1 ' + fieldClass} value={newRole} onChange={e => setNewRole(e.target.value as UserRole)} disabled={busy}><option value="editor">Editor</option><option value="admin">Admin</option></select></label>
        </div>
        <button type="submit" disabled={busy || !csrfToken} className="od-primary-action">Tạo tài khoản</button>
      </form>
    </div>
  );
};
