import { __ } from '../i18n/index.tsx';
import { requestBlob, requestJson, requestText } from './client.ts';

export interface ManualRedirectItem {
  id: number;
  source_path: string;
  target_url: string;
  status_code: 301 | 302;
  enabled: boolean;
  note: string;
  hit_count: number;
  last_hit_at: string | null;
  version: number;
  created_at: string;
  updated_at: string;
  shadowed_by_live_route: boolean;
  shadowed_target_kind: string | null;
}

export interface RedirectListPage {
  items: ManualRedirectItem[];
  total: number;
  page: number;
  per_page: number;
  shadowed_count: number;
}

export interface ListRedirectsParams {
  page?: number;
  per_page?: number;
  q?: string;
  status?: '301' | '302' | '';
  enabled?: 'true' | 'false' | '';
  shadowed?: 'true' | 'false' | '';
}

export interface CreateManualRedirectInput {
  source_path: string;
  target_url: string;
  status_code: 301 | 302;
  enabled?: boolean;
  note?: string;
  resolve_not_found?: boolean;
}

export interface UpdateManualRedirectInput {
  source_path: string;
  target_url: string;
  status_code: 301 | 302;
  enabled: boolean;
  note?: string;
}

export type ConflictPolicy = 'skip' | 'overwrite' | 'error';
export type CsvImportRowStatus = 'create' | 'update' | 'skip_existing' | 'error';

export interface CsvPreviewRow {
  row_number: number;
  source_path: string;
  target_url: string;
  status_code: number | null;
  enabled: boolean | null;
  note: string;
  status: CsvImportRowStatus;
  error_code?: string;
  error?: string;
}

export interface CsvImportResponse {
  dry_run: boolean;
  total_rows: number;
  create_count: number;
  update_count: number;
  skip_count: number;
  error_count: number;
  truncated: boolean;
  rows: CsvPreviewRow[];
  error?: string;
  code?: string;
}

export interface NotFoundLogItem {
  id: number;
  path: string;
  hit_count: number;
  first_seen_at: string;
  last_seen_at: string;
  last_referrer: string | null;
  last_user_agent: string | null;
}

export interface NotFoundLogPage {
  items: NotFoundLogItem[];
  total: number;
  page: number;
  per_page: number;
  max_rows: number;
}

export type NotFoundSort = 'hits_desc' | 'recent_desc' | 'oldest_asc';

export interface ListNotFoundLogsParams {
  page?: number;
  per_page?: number;
  q?: string;
  sort?: NotFoundSort;
}

export async function listManualRedirects(params: ListRedirectsParams = {}): Promise<RedirectListPage> {
  const qs = new URLSearchParams();
  if (params.page) qs.set('page', String(params.page));
  if (params.per_page) qs.set('per_page', String(params.per_page));
  if (params.q && params.q.trim()) qs.set('q', params.q.trim());
  if (params.status) qs.set('status', params.status);
  if (params.enabled) qs.set('enabled', params.enabled);
  if (params.shadowed) qs.set('shadowed', params.shadowed);
  const suffix = qs.toString() ? `?${qs.toString()}` : '';
  return requestJson<RedirectListPage>(`/api/admin/redirects${suffix}`, {
    defaultErrorMessage: __('Could not load manual redirects.'),
  });
}

export async function getManualRedirect(id: number): Promise<ManualRedirectItem> {
  return requestJson<ManualRedirectItem>(`/api/admin/redirects/${id}`, {
    defaultErrorMessage: __('Could not load the latest redirect rule.'),
  });
}

export async function createManualRedirect(
  input: CreateManualRedirectInput,
  csrfToken: string,
): Promise<ManualRedirectItem> {
  return requestJson<ManualRedirectItem>('/api/admin/redirects', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __('Could not save manual redirect.'),
  });
}

export async function updateManualRedirect(
  id: number,
  version: number,
  input: UpdateManualRedirectInput,
  csrfToken: string,
): Promise<ManualRedirectItem> {
  return requestJson<ManualRedirectItem>(`/api/admin/redirects/${id}`, {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __('Could not save manual redirect.'),
  });
}

export async function deleteManualRedirect(id: number, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/redirects/${id}`, {
    method: 'DELETE',
    csrfToken,
    defaultErrorMessage: __('Could not delete manual redirect.'),
  });
}

export async function importRedirectsCsv(
  csv_text: string,
  dry_run: boolean,
  conflict_policy: ConflictPolicy,
  csrfToken: string,
): Promise<CsvImportResponse> {
  return requestJson<CsvImportResponse>('/api/admin/redirects/import', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ csv_text, dry_run, conflict_policy }),
    oversizeErrorMessage: __('Selected CSV file or payload exceeds the 2 MiB limit.'),
    defaultErrorMessage: __('Could not import redirects CSV.'),
  });
}

export async function exportRedirectsCsvBlob(): Promise<Blob> {
  return requestBlob('/api/admin/redirects/export', {
    defaultErrorMessage: __('Could not export redirects CSV.'),
  });
}

export async function listNotFoundLogs(params: ListNotFoundLogsParams = {}): Promise<NotFoundLogPage> {
  const qs = new URLSearchParams();
  if (params.page) qs.set('page', String(params.page));
  if (params.per_page) qs.set('per_page', String(params.per_page));
  if (params.q && params.q.trim()) qs.set('q', params.q.trim());
  if (params.sort) qs.set('sort', params.sort);
  const suffix = qs.toString() ? `?${qs.toString()}` : '';
  return requestJson<NotFoundLogPage>(`/api/admin/404-logs${suffix}`, {
    defaultErrorMessage: __('Could not load 404 logs.'),
  });
}

export async function deleteNotFoundLog(id: number, csrfToken: string): Promise<void> {
  await requestText(`/api/admin/404-logs/${id}`, {
    method: 'DELETE',
    csrfToken,
    defaultErrorMessage: __('Could not delete 404 log entry.'),
  });
}

export async function bulkDeleteNotFoundLogs(
  ids: number[],
  csrfToken: string,
): Promise<{ deleted: number }> {
  return requestJson<{ deleted: number }>('/api/admin/404-logs/bulk-delete', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ ids }),
    defaultErrorMessage: __('Could not delete selected 404 logs.'),
  });
}

export async function clearNotFoundLogs(
  older_than_days: number | null,
  csrfToken: string,
): Promise<{ deleted: number }> {
  return requestJson<{ deleted: number }>('/api/admin/404-logs/clear', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ older_than_days }),
    defaultErrorMessage: __('Could not clear 404 logs.'),
  });
}
