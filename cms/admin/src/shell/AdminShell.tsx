import React from 'react';
import { AuthUser } from '../types.ts';
import { AuthPanel } from './AuthPanel.tsx';

interface AdminShellProps {
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
  sidebarContent: React.ReactNode;
  previewContent: React.ReactNode;
}

export const AdminShell: React.FC<AdminShellProps> = ({
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
  sidebarContent,
  previewContent,
}) => {
  return (
    <div>
      <h1>OpenDash — Quản trị &amp; Builder</h1>

      <AuthPanel
        user={user}
        authUsername={authUsername}
        authPassword={authPassword}
        statusMessage={statusMessage}
        isError={isError}
        isProcessing={isProcessing}
        onUsernameChange={onUsernameChange}
        onPasswordChange={onPasswordChange}
        onSetup={onSetup}
        onLogin={onLogin}
        onLogout={onLogout}
      />

      <main id="builder-main" className={user ? '' : 'hidden'}>
        <section className="editor-sidebar">{sidebarContent}</section>
        {previewContent}
      </main>
    </div>
  );
};
