import React from 'react';
import { useI18n } from '../i18n/index.tsx';
import { AdminButton } from '../components/AdminPrimitives.tsx';
import { TextInput } from '../components/FormPrimitives.tsx';
import { AuthUser } from '../types.ts';

interface AuthPanelProps {
  user: AuthUser | null;
  authUsername: string;
  authPassword: string;
  statusMessage: string;
  isError: boolean;
  isProcessing: boolean;
  onUsernameChange: (username: string) => void;
  onPasswordChange: (password: string) => void;
  onSetup: () => void;
  onLogin: () => void;
  onLogout: () => void;
}

const AUTH_INPUT_CLASS =
  'h-10 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 ' +
  'placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent ' +
  'disabled:cursor-not-allowed disabled:bg-zinc-50 disabled:opacity-60';

export const AuthPanel: React.FC<AuthPanelProps> = ({
  user,
  authUsername,
  authPassword,
  statusMessage,
  isError,
  isProcessing,
  onUsernameChange,
  onPasswordChange,
  onSetup,
  onLogin,
  onLogout,
}) => {
  const { __ } = useI18n();
  return (
    <div id="auth-bar">
      {!user ? (
        <form
          id="guest-panel"
          className="flex flex-col gap-3"
          onSubmit={event => {
            event.preventDefault();
            onLogin();
          }}
        >
          <TextInput
            id="auth-username"
            className={AUTH_INPUT_CLASS}
            placeholder={__("Username")}
            autoComplete="username"
            value={authUsername}
            onValueChange={onUsernameChange}
            disabled={isProcessing}
          />
          <TextInput
            id="auth-password"
            type="password"
            className={AUTH_INPUT_CLASS}
            placeholder={__("Password")}
            autoComplete="current-password"
            value={authPassword}
            onValueChange={onPasswordChange}
            disabled={isProcessing}
          />
          <div className="mt-1 flex flex-col gap-2">
            <AdminButton
              type="submit"
              id="auth-login"
              variant="primary"
              className="h-10 w-full"
              disabled={isProcessing}
            >
              {__("Sign in")}
            </AdminButton>
            <AdminButton
              type="button"
              id="auth-setup"
              variant="secondary"
              className="h-9 w-full text-zinc-700"
              onClick={onSetup}
              disabled={isProcessing}
            >
              {__("Set up admin (first time)")}
            </AdminButton>
          </div>
        </form>
      ) : (
        <div id="user-panel" className="flex items-center gap-3">
          <span id="user-info" className="text-xs font-medium text-zinc-600">
            {user.username} <span className="text-zinc-400">({user.role === 'admin' ? __("Admin") : __("Editor")})</span>
          </span>
          <AdminButton
            id="auth-logout"
            variant="secondary"
            size="sm"
            onClick={onLogout}
            disabled={isProcessing}
          >
            {__("Sign out")}
          </AdminButton>
        </div>
      )}
      <p
        id="status"
        role="status"
        className={
          isError
            ? 'error mt-3 rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-xs font-medium text-rose-700'
            : 'success mt-3 text-xs text-zinc-500'
        }
      >
        {statusMessage}
      </p>
    </div>
  );
};
