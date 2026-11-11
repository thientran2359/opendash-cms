import React from 'react';
import type { AuthUser } from '../types.ts';
import { MediaLibraryPage } from '../features/media/MediaLibraryPage.tsx';

/**
 * Stable Forge-to-Harbor integration seam for Issue #36.
 * Harbor owns this file only after Issue #35 is merged.
 */
export interface MediaRouteProps {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
}

export const MediaRoute: React.FC<MediaRouteProps> = ({ user, csrfToken, onApiError }) => (
  <MediaLibraryPage user={user} csrfToken={csrfToken} onApiError={onApiError} />
);
