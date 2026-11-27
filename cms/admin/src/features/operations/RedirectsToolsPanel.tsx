import React from 'react';
import type { AuthUser } from '../../types.ts';
import { useI18n } from '../../i18n/index.tsx';
import { AdminTabPanel, AdminTabs } from '../../components/AdminPrimitives.tsx';
import { ManualRedirectsPanel } from './ManualRedirectsPanel.tsx';
import { NotFoundLogPanel } from './NotFoundLogPanel.tsx';

export interface RedirectsToolsPanelProps {
  initialTab: 'redirects' | 'not-found';
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

export function RedirectsToolsPanel({
  initialTab,
  user,
  siteInstanceId,
  csrfToken,
  onApiError,
  onDirtyChange,
  onNavigateHash,
}: RedirectsToolsPanelProps) {
  const { __ } = useI18n();
  const [redirectsRefreshRevision, setRedirectsRefreshRevision] = React.useState(0);
  const [notFoundRefreshRevision, setNotFoundRefreshRevision] = React.useState(0);
  const [manualDirty, setManualDirty] = React.useState(false);
  const [notFoundDirty, setNotFoundDirty] = React.useState(false);

  React.useEffect(() => {
    onDirtyChange(manualDirty || notFoundDirty);
  }, [manualDirty, notFoundDirty, onDirtyChange]);

  const handleTabChange = (nextTab: 'redirects' | 'not-found') => {
    onNavigateHash(nextTab === 'not-found' ? '#/tools/404-log' : '#/tools/redirects');
  };

  return (
    <section className="space-y-4" data-testid="redirects-tools-panel">
      <div>
        <h2 className="text-lg font-semibold text-zinc-900">{__('Redirects & 404')}</h2>
        <p className="text-sm text-zinc-600">{__('Manage manual 301/302 redirect rules, bulk CSV migration, and grouped 404 URLs.')}</p>
      </div>

      <AdminTabs<'redirects' | 'not-found'>
        tabs={[
          { id: 'redirects', label: __('Redirects (301/302)'), dirty: manualDirty },
          { id: 'not-found', label: __('404 Log'), dirty: notFoundDirty },
        ]}
        activeId={initialTab}
        onChange={handleTabChange}
        ariaLabel={__('Redirects and 404 views')}
        testId="redirects-tools-tabs"
        idPrefix="redirects-tools"
      />

      <AdminTabPanel
        active={initialTab === 'redirects'}
        id="redirects-tools-panel-redirects"
        labelledBy="redirects-tools-tab-redirects"
      >
        <ManualRedirectsPanel
          user={user}
          siteInstanceId={siteInstanceId}
          csrfToken={csrfToken}
          refreshRevision={redirectsRefreshRevision}
          initialTab={initialTab}
          onResolved404={() => setNotFoundRefreshRevision((v) => v + 1)}
          onDirtyChange={setManualDirty}
          onApiError={onApiError}
        />
      </AdminTabPanel>

      <AdminTabPanel
        active={initialTab === 'not-found'}
        id="redirects-tools-panel-not-found"
        labelledBy="redirects-tools-tab-not-found"
      >
        <NotFoundLogPanel
          user={user}
          siteInstanceId={siteInstanceId}
          csrfToken={csrfToken}
          refreshRevision={notFoundRefreshRevision}
          initialTab={initialTab}
          onCreatedRedirect={() => setRedirectsRefreshRevision((v) => v + 1)}
          onNavigateHash={onNavigateHash}
          onDirtyChange={setNotFoundDirty}
          onApiError={onApiError}
        />
      </AdminTabPanel>
    </section>
  );
}
