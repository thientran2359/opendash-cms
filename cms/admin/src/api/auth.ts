import { tStatic } from '../i18n/index.tsx';
import { AdminI18nBootstrap, AuthResponse, AuthUser } from '../types.ts';
import { requestJson, requestText } from './client.ts';

export type UserRole = 'admin' | 'editor';
export interface UserSummary {
  id: number;
  username: string;
  role: UserRole;
  version: number;
}
export interface UserPage {
  items: UserSummary[];
  total: number;
}

export async function getAuthMe(): Promise<AuthResponse> {
  return requestJson<AuthResponse>('/auth/me', {
    method: 'GET',
    defaultErrorMessage: tStatic("api.auth.not_signed_in"),
  });
}

export async function setupAdmin(username: string, password: string): Promise<void> {
  await requestText('/auth/setup', {
    method: 'POST',
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: tStatic("api.auth.setup_failed"),
  });
}

export async function login(username: string, password: string): Promise<AuthResponse> {
  return requestJson<AuthResponse>('/auth/login', {
    method: 'POST',
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: tStatic("api.auth.login_failed"),
  });
}

export async function logout(csrfToken: string | null): Promise<void> {
  await requestText('/auth/logout', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: tStatic("api.auth.logout_failed"),
  });
}

export async function createEditor(
  username: string,
  password: string,
  csrfToken: string,
): Promise<AuthUser> {
  return requestJson<AuthUser>('/api/admin/users', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: tStatic("api.auth.create_editor_failed"),
  });
}

export function listUsers(limit: number, offset: number): Promise<UserPage> {
  return requestJson<UserPage>('/api/admin/users?limit=' + limit + '&offset=' + offset, {
    defaultErrorMessage: tStatic("api.auth.list_users_failed"),
  });
}

export function createUser(username: string, password: string, role: UserRole, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users', {
    method: 'POST', contentType: 'application/json', csrfToken,
    body: JSON.stringify({ username, password, role }),
    defaultErrorMessage: tStatic("api.auth.create_user_failed"),
  });
}

export function updateUser(id: number, version: number, changes: { username?: string; role?: UserRole }, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users/' + id, {
    method: 'PATCH', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify(changes),
    defaultErrorMessage: tStatic("api.auth.update_user_failed"),
  });
}

export function resetUserPassword(id: number, version: number, password: string, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users/' + id + '/password', {
    method: 'POST', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify({ password }),
    defaultErrorMessage: tStatic("api.auth.reset_password_failed"),
  });
}

export async function removeUser(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText('/api/admin/users/' + id, {
    method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: tStatic("api.auth.remove_user_failed"),
  });
}

export type { AdminI18nBootstrap, CmsLocaleOption } from '../types.ts';

export function getAdminI18nBootstrap(signal?: AbortSignal): Promise<AdminI18nBootstrap> {
  return requestJson<AdminI18nBootstrap>('/api/i18n/admin', {
    signal,
    defaultErrorMessage: tStatic("api.auth.bootstrap_failed"),
  });
}
