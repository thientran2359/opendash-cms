import React from 'react';
import { useI18n } from '../../i18n/index.tsx';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { getManualRedirect, type ManualRedirectItem } from '../../api/redirects.ts';
import type { RedirectModalMode } from './redirectsDraftStore.ts';

export interface RedirectFormModalProps {
  mode: Extract<RedirectModalMode, 'create' | 'edit' | 'from_404'>;
  editingId: number | null;
  sourcePath: string;
  targetUrl: string;
  statusCode: 301 | 302;
  enabled: boolean;
  note: string;
  resolveNotFound: boolean;
  staleConflict: boolean;
  busy: boolean;
  formError: string | null;
  onChange: (patch: {
    sourcePath?: string;
    targetUrl?: string;
    statusCode?: 301 | 302;
    enabled?: boolean;
    note?: string;
    resolveNotFound?: boolean;
  }) => void;
  onRebaseLatest: (latest: ManualRedirectItem) => void;
  onKeepDraft: () => void;
  onError: (err: unknown) => void;
  onSubmit: (e: React.FormEvent) => void;
  onClose: () => void;
}

export function RedirectFormModal({
  mode,
  editingId,
  sourcePath,
  targetUrl,
  statusCode,
  enabled,
  note,
  resolveNotFound,
  staleConflict,
  busy,
  formError,
  onChange,
  onRebaseLatest,
  onKeepDraft,
  onError,
  onSubmit,
  onClose,
}: RedirectFormModalProps) {
  const { __, _x } = useI18n();
  const [rebasing, setRebasing] = React.useState(false);

  const title =
    mode === 'edit'
      ? __('Edit Manual Redirect')
      : mode === 'from_404'
        ? __('Create Redirect from 404')
        : __('Add Manual Redirect');

  const handleRebase = async () => {
    if (!editingId) return;
    setRebasing(true);
    try {
      const latest = await getManualRedirect(editingId);
      onRebaseLatest(latest);
    } catch (err) {
      onError(err);
    } finally {
      setRebasing(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      data-testid="redirect-form-modal"
    >
      <div className="w-full max-w-lg rounded-xl border border-zinc-200 bg-white p-5 shadow-xl">
        <div className="mb-4 flex items-center justify-between gap-2">
          <h3 className="text-base font-semibold text-zinc-900">{title}</h3>
          <AdminButton variant="ghost" size="sm" onClick={onClose} disabled={busy || rebasing}>
            {_x('Cancel', 'redirects')}
          </AdminButton>
        </div>

        {staleConflict && (
          <AdminAlert tone="warning" className="mb-4 space-y-2">
            <p>{__('This rule was modified concurrently in another session. Reload the latest version to continue saving or keep editing your draft.')}</p>
            <div className="flex flex-wrap gap-2">
              <AdminButton
                variant="primary"
                size="sm"
                onClick={() => {
                  void handleRebase();
                }}
                disabled={rebasing || !editingId}
                data-testid="redirect-rebase-btn"
              >
                {__('Load latest (rebase)')}
              </AdminButton>
              <AdminButton variant="secondary" size="sm" onClick={onKeepDraft}>
                {__('Keep draft')}
              </AdminButton>
            </div>
          </AdminAlert>
        )}

        {formError && (
          <AdminAlert tone="error" className="mb-4">
            {formError}
          </AdminAlert>
        )}

        <form onSubmit={onSubmit} className="space-y-3.5">
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-700">
              {__('Source Path')}
            </label>
            <input
              type="text"
              value={sourcePath}
              onChange={(e) => onChange({ sourcePath: e.target.value })}
              placeholder={__('/old-page.html')}
              required
              data-testid="redirect-source-input"
              className="w-full rounded-md border border-zinc-300 px-3 py-2 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
            />
            <p className="mt-1 text-xs text-zinc-500">{__('Exact case-sensitive match')}</p>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-700">
              {__('Target URL')}
            </label>
            <input
              type="text"
              value={targetUrl}
              onChange={(e) => onChange({ targetUrl: e.target.value })}
              placeholder={__('/new-page or https://example.com/path')}
              required
              data-testid="redirect-target-input"
              className="w-full rounded-md border border-zinc-300 px-3 py-2 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
            />
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div>
              <label className="mb-1 block text-xs font-medium text-zinc-700">
                {__('HTTP Status Code')}
              </label>
              <select
                value={statusCode}
                onChange={(e) => onChange({ statusCode: e.target.value === '302' ? 302 : 301 })}
                data-testid="redirect-status-select"
                className="w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
              >
                <option value={301}>{__('301 — Permanent Redirect')}</option>
                <option value={302}>{__('302 — Temporary Redirect')}</option>
              </select>
            </div>

            <div className="flex items-end pb-2">
              <label className="inline-flex items-center gap-2 text-sm text-zinc-800">
                <input
                  type="checkbox"
                  checked={enabled}
                  onChange={(e) => onChange({ enabled: e.target.checked })}
                  data-testid="redirect-enabled-checkbox"
                  className="h-4 w-4 rounded border-zinc-300"
                />
                <span>{__('Rule enabled')}</span>
              </label>
            </div>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-700">
              {__('Note (optional)')}
            </label>
            <input
              type="text"
              value={note}
              maxLength={256}
              onChange={(e) => onChange({ note: e.target.value })}
              data-testid="redirect-note-input"
              className="w-full rounded-md border border-zinc-300 px-3 py-2 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
            />
          </div>

          {mode !== 'edit' && (
            <label className="inline-flex items-center gap-2 text-xs text-zinc-700">
              <input
                type="checkbox"
                checked={resolveNotFound}
                onChange={(e) => onChange({ resolveNotFound: e.target.checked })}
                data-testid="redirect-resolve-404-checkbox"
                className="h-4 w-4 rounded border-zinc-300"
              />
              <span>{__('Remove matching 404 log entry after saving')}</span>
            </label>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <AdminButton variant="secondary" onClick={onClose} disabled={busy || rebasing}>
              {_x('Cancel', 'redirects')}
            </AdminButton>
            <AdminButton
              variant="primary"
              type="submit"
              disabled={busy || rebasing || staleConflict}
              data-testid="redirect-save-btn"
            >
              {busy ? __('Saving…') : __('Save Redirect')}
            </AdminButton>
          </div>
        </form>
      </div>
    </div>
  );
}
