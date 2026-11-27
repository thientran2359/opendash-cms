import { __ } from '../i18n/index.tsx';
import { requestJson } from './client.ts';

export interface PublicationCounts {
  draft: number;
  published: number;
}

export interface DashboardCounts {
  post: PublicationCounts;
  page: PublicationCounts;
}

export function getDashboard(signal?: AbortSignal): Promise<DashboardCounts> {
  return requestJson<DashboardCounts>('/api/admin/dashboard', {
    signal,
    defaultErrorMessage: __("Could not load post and page statistics."),
  });
}
