import { __, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useState } from 'react';
import { getProfile, updateProfile, type UserProfile } from '../../api/users.ts';
import {
  AdminAlert,
  AdminBadge,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
} from '../../components/AdminPrimitives.tsx';

type ProfileTabId = 'account' | 'password';

interface UserProfilePanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onProfileUpdated?: (profile: UserProfile) => void;
}

const INPUT_CLASS =
  'h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent';

export const UserProfilePanel: React.FC<UserProfilePanelProps> = ({
  csrfToken,
  onApiError,
  onProfileUpdated,
}) => {
  const { __ } = useI18n();
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [username, setUsername] = useState('');
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<ProfileTabId>('account');

  useEffect(() => {
    let active = true;
    getProfile()
      .then(data => {
        if (!active) return;
        setProfile(data);
        setUsername(data.username);
      })
      .catch(err => onApiError(err, __("Could not load your profile.")));
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
        setErrorMsg(__("There are no changes to save."));
        setSaving(false);
        return;
      }
      const updated = await updateProfile(profile.version, payload, csrfToken);
      setProfile(updated);
      setUsername(updated.username);
      setCurrentPassword('');
      setNewPassword('');
      setMessage(__("Your profile has been updated."));
      onProfileUpdated?.(updated);
    } catch (err) {
      const msg = err instanceof Error ? err.message : __("Could not update your profile.");
      setErrorMsg(msg);
      onApiError(err, msg);
    } finally {
      setSaving(false);
    }
  };

  if (!profile) {
    return <p className="text-sm text-zinc-500">{__("Loading profile...")}</p>;
  }

  return (
    <section className="max-w-xl space-y-5" data-testid="user-profile-panel">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{__("My profile")}</h2>
          <p className="mt-0.5 text-xs text-zinc-500">{__("Version v")}{profile.version}</p>
        </div>
        <AdminBadge tone={profile.role === 'admin' ? 'success' : 'neutral'} className="uppercase">
          {profile.role}
        </AdminBadge>
      </div>
      {message && <AdminAlert tone="success">{message}</AdminAlert>}
      {errorMsg && <AdminAlert tone="error">{errorMsg}</AdminAlert>}
      <form onSubmit={handleSubmit} className="space-y-4">
        <AdminTabs
          tabs={[
            {
              id: 'account',
              label: __("Account info"),
              dirty: Boolean(username.trim() && username.trim() !== profile.username),
            },
            {
              id: 'password',
              label: __("Password"),
              dirty: Boolean(currentPassword || newPassword),
            },
          ]}
          activeId={activeTab}
          onChange={setActiveTab}
          ariaLabel={__("Profile sections")}
          testId="profile-tabs"
          idPrefix="profile"
        />
        <AdminTabPanel
          active={activeTab === 'account'}
          id="profile-panel-account"
          labelledBy="profile-tab-account"
          className="space-y-3"
        >
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
            <span>{__("Username / Display name")}</span>
            <input
              type="text"
              className={INPUT_CLASS}
              value={username}
              onChange={e => setUsername(e.target.value)}
              required
            />
          </label>
        </AdminTabPanel>
        <AdminTabPanel
          active={activeTab === 'password'}
          id="profile-panel-password"
          labelledBy="profile-tab-password"
          className="space-y-3"
        >
          <div>
            <h3 className="text-sm font-semibold text-zinc-900">{__("Change password (optional)")}</h3>
            <p className="mt-0.5 text-xs text-zinc-500">{__("Your current password is required when changing your password.")}</p>
          </div>
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
            <span>{__("Current password")}</span>
            <input
              type="password"
              className={INPUT_CLASS}
              value={currentPassword}
              onChange={e => setCurrentPassword(e.target.value)}
              autoComplete="current-password"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
            <span>{__("New password")}</span>
            <input
              type="password"
              className={INPUT_CLASS}
              value={newPassword}
              onChange={e => setNewPassword(e.target.value)}
              autoComplete="new-password"
            />
          </label>
        </AdminTabPanel>
        <AdminButton type="submit" variant="primary" disabled={saving || !csrfToken}>
          {saving ? __("Saving...") : __("Save profile")}
        </AdminButton>
      </form>
    </section>
  );
};
