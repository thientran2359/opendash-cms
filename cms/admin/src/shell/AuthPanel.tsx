import React from 'react';
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
  return (
    <div id="auth-bar">
      {!user ? (
        <form id="guest-panel" className="panel-row" onSubmit={event => { event.preventDefault(); onLogin(); }}>
          <TextInput
            id="auth-username"
            placeholder="Tên đăng nhập"
            autoComplete="username"
            value={authUsername}
            onValueChange={onUsernameChange}
            disabled={isProcessing}
          />
          <TextInput
            id="auth-password"
            type="password"
            placeholder="Mật khẩu"
            autoComplete="current-password"
            value={authPassword}
            onValueChange={onPasswordChange}
            disabled={isProcessing}
          />
          <button type="button" id="auth-setup" onClick={onSetup} disabled={isProcessing}>
            Thiết lập Admin
          </button>
          <button type="submit" id="auth-login" disabled={isProcessing}>
            Đăng nhập
          </button>
        </form>
      ) : (
        <div id="user-panel" className="panel-row">
          <span id="user-info">
            Đang đăng nhập: {user.username} ({user.role})
          </span>
          <button
            id="auth-logout"
            className="btn-secondary"
            onClick={onLogout}
            disabled={isProcessing}
          >
            Đăng xuất
          </button>
        </div>
      )}
      <p id="status" role="status" className={isError ? 'error' : 'success'}>
        {statusMessage}
      </p>
    </div>
  );
};
