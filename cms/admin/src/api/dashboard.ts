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
    defaultErrorMessage: 'Không tải được thống kê bài viết và trang.',
  });
}
