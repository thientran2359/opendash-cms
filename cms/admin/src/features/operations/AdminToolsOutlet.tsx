import React from 'react';
import { useI18n } from '../../i18n/index.tsx';
import type { AuthUser } from '../../types.ts';
import { SystemStatus } from '../system/SystemStatus.tsx';
import { BackupToolsPanel } from './BackupToolsPanel.tsx';
import { ReconcilePanel } from './ReconcilePanel.tsx';
import { RedirectsToolsPanel } from './RedirectsToolsPanel.tsx';
import { SearchIndexToolsPanel } from './SearchIndexToolsPanel.tsx';

interface AdminToolsOutletProps {
  sub: string;
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  scopedTaxonomyApiError: (error: unknown, fallback: string) => void;
  onToolsDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

export const AdminToolsOutlet: React.FC<AdminToolsOutletProps> = ({
  sub,
  user,
  siteInstanceId,
  csrfToken,
  onApiError,
  scopedTaxonomyApiError,
  onToolsDirtyChange,
  onNavigateHash,
}) => {
  const { __ } = useI18n();

  if (sub === 'backup') {
    return (
      <BackupToolsPanel
        csrfToken={csrfToken}
        onBackupError={(error) => onApiError(error, __("Encrypted backup failed."))}
        onRestoreError={(error) => onApiError(error, __("Restore or safe mode failed."))}
      />
    );
  }
  if (sub === 'media-reconcile') {
    return (
      <ReconcilePanel
        csrfToken={csrfToken}
        onApiError={(error) => onApiError(error, __("Media reconciliation or quarantine cleanup failed."))}
      />
    );
  }
  if (sub === 'redirects' || sub === '404-log') {
    return (
      <RedirectsToolsPanel
        key={`${siteInstanceId}:${user?.id ?? 'guest'}`}
        initialTab={sub === '404-log' ? 'not-found' : 'redirects'}
        user={user}
        siteInstanceId={siteInstanceId}
        csrfToken={csrfToken}
        onApiError={(error) => scopedTaxonomyApiError(error, __("Redirects & 404 operation failed"))}
        onDirtyChange={onToolsDirtyChange}
        onNavigateHash={onNavigateHash}
      />
    );
  }
  if (sub === 'search') {
    return (
      <SearchIndexToolsPanel
        csrfToken={csrfToken}
        onApiError={(error) => onApiError(error, __("Could not rebuild search index."))}
      />
    );
  }
  if (sub === 'health') {
    return <SystemStatus />;
  }
  return null;
};
