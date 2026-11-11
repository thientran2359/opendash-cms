import React from 'react';
import type { AuthUser } from '../types.ts';

/**
 * Stable Forge-to-Harbor integration seam for Issue #36.
 * Harbor owns this file only after Issue #35 is merged.
 */
export interface MediaRouteProps {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
}

export const MediaRoute: React.FC<MediaRouteProps> = ({ user }) => (
  <main className="od-pending rounded-2xl border border-slate-200 bg-white p-6 shadow-sm sm:p-9">
    <div className="text-xs font-semibold uppercase tracking-widest text-sky-700">Đang phát triển</div>
    <h2 className="mt-3 text-xl font-semibold text-slate-900">Thư viện ảnh</h2>
    <p className="mt-3 max-w-xl leading-7 text-slate-600">
      {user
        ? 'Thư viện độc lập sẽ được Harbor kết nối tại đây. Hiện có thể chọn và tải ảnh từ màn hình Nội dung tùy chỉnh hoặc Bài viết/Trang.'
        : 'Đăng nhập để quản lý ảnh.'}
    </p>
  </main>
);
