import { __, _x } from '../i18n/index.tsx';
import { AdminI18nBootstrap, AuthResponse } from '../types.ts';
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
    defaultErrorMessage: __("You are not signed in."),
  });
}

export async function setupAdmin(username: string, password: string): Promise<void> {
  await requestText('/auth/setup', {
    method: 'POST',
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: __("Could not set up the administrator."),
  });
}

export async function login(username: string, password: string): Promise<AuthResponse> {
  return requestJson<AuthResponse>('/auth/login', {
    method: 'POST',
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: __("Could not sign in."),
  });
}

export async function logout(csrfToken: string | null): Promise<void> {
  await requestText('/auth/logout', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: __("Could not sign out."),
  });
}

export function listUsers(limit: number, offset: number): Promise<UserPage> {
  return requestJson<UserPage>('/api/admin/users?limit=' + limit + '&offset=' + offset, {
    defaultErrorMessage: _x("Could not load users.", "api.auth"),
  });
}

export function createUser(username: string, password: string, role: UserRole, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users', {
    method: 'POST', contentType: 'application/json', csrfToken,
    body: JSON.stringify({ username, password, role }),
    defaultErrorMessage: __("Could not create the user."),
  });
}

export function updateUser(id: number, version: number, changes: { username?: string; role?: UserRole }, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users/' + id, {
    method: 'PATCH', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify(changes),
    defaultErrorMessage: __("Could not update the user."),
  });
}

export function resetUserPassword(id: number, version: number, password: string, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users/' + id + '/password', {
    method: 'POST', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify({ password }),
    defaultErrorMessage: __("Could not reset the password."),
  });
}

export async function removeUser(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText('/api/admin/users/' + id, {
    method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: __("Could not delete the account."),
  });
}

export type { AdminI18nBootstrap, CmsLocaleOption } from '../types.ts';

export function getAdminI18nBootstrap(signal?: AbortSignal): Promise<AdminI18nBootstrap> {
  return requestJson<AdminI18nBootstrap>('/api/i18n/admin', {
    signal,
    defaultErrorMessage: __("Could not load the CMS language settings."),
  });
}
