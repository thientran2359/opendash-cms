import { requestJson } from './client.ts';
import type { Role } from '../types.ts';

export interface UserProfile {
  id: number;
  username: string;
  role: Role;
  version: number;
}

export interface UpdateProfileInput {
  username?: string;
  current_password?: string;
  new_password?: string;
}

export function getProfile(): Promise<UserProfile> {
  return requestJson('/api/admin/profile', {
    defaultErrorMessage: 'Không thể tải hồ sơ cá nhân.',
  });
}

export function updateProfile(
  version: number,
  input: UpdateProfileInput,
  csrfToken: string,
): Promise<UserProfile> {
  return requestJson('/api/admin/profile', {
    method: 'PUT',
    csrfToken,
    ifMatchVersion: version,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: 'Không thể cập nhật hồ sơ cá nhân.',
  });
}
