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
  activeView: 'builder' | 'content';
  onViewChange: (view: 'builder' | 'content') => void;
  sidebarContent: React.ReactNode;
  previewContent: React.ReactNode;
  contentContent: React.ReactNode;
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
  activeView,
  onViewChange,
  sidebarContent,
  previewContent,
  contentContent,
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

      <nav className={user ? 'admin-view-tabs' : 'hidden'} role="tablist" aria-label="Khu vực quản trị">
        <button
          type="button"
          id="tab-builder"
          role="tab"
          aria-controls="builder-main"
          aria-selected={activeView === 'builder'}
          className={activeView === 'builder' ? 'selected' : 'btn-secondary'}
          onClick={() => onViewChange('builder')}
        >Builder</button>
        <button
          type="button"
          id="tab-content"
          role="tab"
          aria-controls="content-main"
          aria-selected={activeView === 'content'}
          className={activeView === 'content' ? 'selected' : 'btn-secondary'}
          onClick={() => onViewChange('content')}
        >Nội dung</button>
      </nav>

      <main
        id="builder-main"
        role="tabpanel"
        aria-labelledby="tab-builder"
        className={user && activeView === 'builder' ? '' : 'hidden'}
      >
        <section className="editor-sidebar">{sidebarContent}</section>
        {previewContent}
      </main>
      <main
        id="content-main"
        role="tabpanel"
        aria-labelledby="tab-content"
        className={user && activeView === 'content' ? '' : 'hidden'}
      >
        {contentContent}
      </main>
    </div>
  );
};
