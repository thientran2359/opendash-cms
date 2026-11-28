import React from 'react';
import { useI18n } from '../../i18n/index.tsx';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  importRedirectsCsv,
  type ConflictPolicy,
  type CsvImportResponse,
  type CsvImportRowStatus,
} from '../../api/redirects.ts';

const MAX_CSV_BYTES = 2 * 1024 * 1024;

export interface CsvImportModalProps {
  csrfToken: string | null;
  csvText: string;
  conflictPolicy: ConflictPolicy;
  onChange: (patch: { csvText?: string; conflictPolicy?: ConflictPolicy }) => void;
  onCommitted: (res: CsvImportResponse) => void;
  onApiError: (err: unknown) => void;
  onClose: () => void;
}

function rowTone(status: CsvImportRowStatus): 'success' | 'warning' | 'neutral' | 'danger' {
  if (status === 'create') return 'success';
  if (status === 'update') return 'warning';
  if (status === 'error') return 'danger';
  return 'neutral';
}

function csvRowStatusLabel(
  status: CsvImportRowStatus,
  translate: (msgid: string) => string,
): string {
  if (status === 'create') return translate('Create');
  if (status === 'update') return translate('Update');
  if (status === 'skip_existing') return translate('Skip');
  return translate('Error');
}

export function CsvImportModal({
  csrfToken,
  csvText,
  conflictPolicy,
  onChange,
  onCommitted,
  onApiError,
  onClose,
}: CsvImportModalProps) {
  const { __, _x } = useI18n();
  const [preview, setPreview] = React.useState<CsvImportResponse | null>(null);
  const [localError, setLocalError] = React.useState<string | null>(null);
  const [busyMode, setBusyMode] = React.useState<'preview' | 'commit' | null>(null);

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (file.size > MAX_CSV_BYTES) {
      setLocalError(__('Selected CSV file or payload exceeds the 2 MiB limit.'));
      setPreview(null);
      return;
    }
    const text = await file.text();
    const bytes = new TextEncoder().encode(text).length;
    if (bytes > MAX_CSV_BYTES) {
      setLocalError(__('Selected CSV file or payload exceeds the 2 MiB limit.'));
      setPreview(null);
      return;
    }
    setLocalError(null);
    setPreview(null);
    onChange({ csvText: text });
  };

  const handleDryRun = async () => {
    if (!csrfToken) return;
    const bytes = new TextEncoder().encode(csvText).length;
    if (bytes > MAX_CSV_BYTES) {
      setLocalError(__('Selected CSV file or payload exceeds the 2 MiB limit.'));
      setPreview(null);
      return;
    }
    setLocalError(null);
    setBusyMode('preview');
    try {
      const res = await importRedirectsCsv(csvText, true, conflictPolicy, csrfToken);
      setPreview(res);
    } catch (err) {
      setPreview(null);
      const msg = err instanceof Error ? err.message : __('Could not import redirects CSV.');
      setLocalError(msg);
      onApiError(err);
    } finally {
      setBusyMode(null);
    }
  };

  const handleCommit = async () => {
    if (!csrfToken || !preview || preview.error_count > 0) return;
    setLocalError(null);
    setBusyMode('commit');
    try {
      const res = await importRedirectsCsv(csvText, false, conflictPolicy, csrfToken);
      onCommitted(res);
    } catch (err) {
      setPreview(null);
      const msg = err instanceof Error ? err.message : __('Could not import redirects CSV.');
      setLocalError(msg);
      onApiError(err);
    } finally {
      setBusyMode(null);
    }
  };

  const canCommit =
    preview !== null &&
    preview.error_count === 0 &&
    preview.create_count + preview.update_count > 0 &&
    busyMode === null;

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && busyMode === null) {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [busyMode, onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={__('Import Redirects from CSV')}
      data-testid="csv-import-modal"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && busyMode === null) {
          onClose();
        }
      }}
    >
      <div className="flex max-h-[90vh] w-full max-w-3xl flex-col rounded-xl border border-zinc-200 bg-white p-5 shadow-xl">
        <div className="mb-4 flex items-center justify-between gap-2">
          <h2 className="text-base font-semibold text-zinc-900">
            {__('Import Redirects from CSV')}
          </h2>
          <AdminButton variant="ghost" size="sm" onClick={onClose} disabled={busyMode !== null}>
            {_x('Cancel', 'redirects')}
          </AdminButton>
        </div>

        <div className="space-y-3 overflow-y-auto pr-1">
          {localError && <AdminAlert tone="error">{localError}</AdminAlert>}

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div>
              <label htmlFor="csv-file-input" className="mb-1 block text-xs font-medium text-zinc-700">
                {__('Choose .csv file (max 2 MiB / 5,000 rows)')}
              </label>
              <input
                id="csv-file-input"
                type="file"
                accept=".csv,text/csv"
                onChange={(e) => {
                  void handleFileChange(e);
                }}
                data-testid="csv-file-input"
                className="block w-full text-xs text-zinc-700 file:mr-2 file:rounded-md file:border file:border-zinc-300 file:bg-white file:px-2.5 file:py-1.5 file:text-xs file:font-medium hover:file:bg-zinc-50"
              />
            </div>

            <div>
              <label htmlFor="csv-conflict-policy" className="mb-1 block text-xs font-medium text-zinc-700">
                {__('Duplicate source path policy')}
              </label>
              <select
                id="csv-conflict-policy"
                value={conflictPolicy}
                onChange={(e) => {
                  setPreview(null);
                  setLocalError(null);
                  onChange({ conflictPolicy: e.target.value as ConflictPolicy });
                }}
                data-testid="csv-conflict-policy"
                className="w-full rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
              >
                <option value="skip">{__('Skip existing rules')}</option>
                <option value="overwrite">{__('Overwrite existing rules (keep hits)')}</option>
                <option value="error">{__('Fail if duplicate exists')}</option>
              </select>
            </div>
          </div>

          <div>
            <label htmlFor="csv-textarea" className="mb-1 block text-xs font-medium text-zinc-700">
              {__('Or paste CSV content')}
            </label>
            <textarea
              id="csv-textarea"
              rows={6}
              value={csvText}
              onChange={(e) => {
                setPreview(null);
                setLocalError(null);
                onChange({ csvText: e.target.value });
              }}
              placeholder={__('source_path,target_url,status_code,enabled,note\n/old.html,/new-page,301,true,legacy')}
              data-testid="csv-textarea"
              className="w-full rounded-md border border-zinc-300 px-3 py-2 font-mono text-xs text-zinc-900 focus:border-od-accent focus:outline-none"
            />
          </div>

          {preview && (
            <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50 p-3">
              <div className="text-xs font-medium text-zinc-800" data-testid="csv-preview-summary">
                {__('Total: {{total}} · Create: {{create}} · Update: {{update}} · Skip: {{skip}} · Errors: {{errors}}', {
                  total: preview.total_rows,
                  create: preview.create_count,
                  update: preview.update_count,
                  skip: preview.skip_count,
                  errors: preview.error_count,
                })}
              </div>
              {preview.truncated && (
                <div className="text-xs text-amber-700">
                  {__('Showing first {{shown}} of {{total}} rows (errors prioritized).', {
                    shown: preview.rows.length,
                    total: preview.total_rows,
                  })}
                </div>
              )}
              <div className="max-h-56 overflow-auto rounded border border-zinc-200 bg-white">
                <table className="w-full border-collapse text-left text-xs">
                  <thead className="sticky top-0 bg-zinc-100 text-zinc-600">
                    <tr>
                      <th className="px-2 py-1.5">#</th>
                      <th className="px-2 py-1.5">{__('Status')}</th>
                      <th className="px-2 py-1.5">{__('Source Path')}</th>
                      <th className="px-2 py-1.5">{__('Target URL')}</th>
                      <th className="px-2 py-1.5">{__('Actions')}</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-zinc-100">
                    {preview.rows.map((row) => (
                      <tr key={row.row_number}>
                        <td className="px-2 py-1.5 text-zinc-500">{row.row_number}</td>
                        <td className="px-2 py-1.5">
                          <AdminBadge tone={rowTone(row.status)}>
                            {csvRowStatusLabel(row.status, __)}
                          </AdminBadge>
                        </td>
                        <td className="px-2 py-1.5 font-mono text-zinc-900">{row.source_path}</td>
                        <td className="px-2 py-1.5 font-mono text-zinc-700">{row.target_url}</td>
                        <td className="px-2 py-1.5 text-rose-700">{row.error ?? ''}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2 border-t border-zinc-100 pt-3">
          <AdminButton variant="secondary" onClick={onClose} disabled={busyMode !== null}>
            {_x('Cancel', 'redirects')}
          </AdminButton>
          <AdminButton
            variant="secondary"
            onClick={() => {
              void handleDryRun();
            }}
            disabled={!csvText.trim() || busyMode !== null || !csrfToken}
            data-testid="csv-dry-run-btn"
          >
            {busyMode === 'preview' ? __('Previewing…') : __('Dry-Run Preview')}
          </AdminButton>
          <AdminButton
            variant="primary"
            onClick={() => {
              void handleCommit();
            }}
            disabled={!canCommit}
            data-testid="csv-commit-btn"
          >
            {busyMode === 'commit' ? __('Importing…') : __('Confirm Import')}
          </AdminButton>
        </div>
      </div>
    </div>
  );
}
