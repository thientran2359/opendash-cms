import React, { useState } from 'react';
import { useI18n } from '../../i18n/index.tsx';
import { AdminTabPanel, AdminTabs } from '../../components/AdminPrimitives.tsx';
import { BackupsPanel } from './BackupsPanel.tsx';
import { RestorePanel } from './RestorePanel.tsx';

type BackupTabId = 'backup' | 'restore';

interface BackupToolsPanelProps {
  csrfToken: string | null;
  onBackupError: (error: unknown) => void;
  onRestoreError: (error: unknown) => void;
}

/** Coordinates Backup creation and Restore/Safe Mode panels under shared tabs. */
export const BackupToolsPanel: React.FC<BackupToolsPanelProps> = ({
  csrfToken,
  onBackupError,
  onRestoreError,
}) => {
  const { __, _x } = useI18n();
  const [activeTab, setActiveTab] = useState<BackupTabId>('backup');

  return (
    <div className="min-w-0 space-y-5" data-testid="backup-tools-panel">
      <AdminTabs
        tabs={[
          { id: 'backup', label: __("Backup") },
          { id: 'restore', label: _x("Restore & safe mode", "backup.tab_restore") },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={__("Backup and restore sections")}
        testId="backup-tools-tabs"
        idPrefix="backup-tools"
      />
      <AdminTabPanel
        active={activeTab === 'backup'}
        id="backup-tools-panel-backup"
        labelledBy="backup-tools-tab-backup"
      >
        <BackupsPanel csrfToken={csrfToken} onApiError={onBackupError} />
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'restore'}
        id="backup-tools-panel-restore"
        labelledBy="backup-tools-tab-restore"
      >
        <RestorePanel csrfToken={csrfToken} onApiError={onRestoreError} />
      </AdminTabPanel>
    </div>
  );
};
