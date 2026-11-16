import React, { useEffect, useState } from 'react';
import { getProfile, updateProfile, type UserProfile } from '../../api/users.ts';

interface UserProfilePanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onProfileUpdated?: (profile: UserProfile) => void;
}

export const UserProfilePanel: React.FC<UserProfilePanelProps> = ({
  csrfToken,
  onApiError,
  onProfileUpdated,
}) => {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [username, setUsername] = useState('');
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    getProfile()
      .then(data => {
        if (!active) return;
        setProfile(data);
        setUsername(data.username);
      })
      .catch(err => onApiError(err, 'Không thể tải hồ sơ cá nhân.'));
    return () => {
      active = false;
    };
  }, [onApiError]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!profile || !csrfToken) return;
    setSaving(true);
    setMessage(null);
    setErrorMsg(null);
    try {
      const payload: {
        username?: string;
        current_password?: string;
        new_password?: string;
      } = {};
      if (username.trim() && username.trim() !== profile.username) {
        payload.username = username.trim();
      }
      if (newPassword) {
        payload.current_password = currentPassword;
        payload.new_password = newPassword;
      }
      if (!payload.username && !payload.new_password) {
        setErrorMsg('Không có thay đổi nào để lưu.');
        setSaving(false);
        return;
      }
      const updated = await updateProfile(profile.version, payload, csrfToken);
      setProfile(updated);
      setUsername(updated.username);
      setCurrentPassword('');
      setNewPassword('');
      setMessage('Đã cập nhật hồ sơ cá nhân.');
      onProfileUpdated?.(updated);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Không thể cập nhật hồ sơ cá nhân.';
      setErrorMsg(msg);
      onApiError(err, msg);
    } finally {
      setSaving(false);
    }
  };

  if (!profile) {
    return <p className="text-sm text-slate-600">Đang tải hồ sơ...</p>;
  }

  return (
    <section className="max-w-xl space-y-5" data-testid="user-profile-panel">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Hồ sơ của tôi</h2>
        <p className="mt-1 text-sm text-slate-600">
          Vai trò: <strong className="uppercase">{profile.role}</strong> · Phiên bản v{profile.version}
        </p>
      </div>
      {message && (
        <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800" role="status">
          {message}
        </div>
      )}
      {errorMsg && (
        <div className="rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800" role="alert">
          {errorMsg}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-4">
        <label className="block text-sm font-medium text-slate-800">
          Tên đăng nhập / Hiển thị
          <input
            type="text"
            className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
            value={username}
            onChange={e => setUsername(e.target.value)}
            required
          />
        </label>
        <div className="border-t border-slate-200 pt-4">
          <h3 className="text-sm font-semibold text-slate-800">Đổi mật khẩu (tùy chọn)</h3>
          <p className="mt-0.5 text-xs text-slate-500">Bắt buộc nhập mật khẩu hiện tại khi đổi mật khẩu.</p>
          <div className="mt-3 space-y-3">
            <label className="block text-sm font-medium text-slate-800">
              Mật khẩu hiện tại
              <input
                type="password"
                className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
                value={currentPassword}
                onChange={e => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
              />
            </label>
            <label className="block text-sm font-medium text-slate-800">
              Mật khẩu mới
              <input
                type="password"
                className="mt-1 w-full rounded-lg border border-slate-300 px-3 py-2 text-sm"
                value={newPassword}
                onChange={e => setNewPassword(e.target.value)}
                autoComplete="new-password"
              />
            </label>
          </div>
        </div>
        <button
          type="submit"
          disabled={saving || !csrfToken}
          className="rounded-lg bg-slate-900 px-4 py-2 text-sm font-semibold text-white hover:bg-slate-800 disabled:opacity-50"
        >
          {saving ? 'Đang lưu...' : 'Lưu hồ sơ'}
        </button>
      </form>
    </section>
  );
};
