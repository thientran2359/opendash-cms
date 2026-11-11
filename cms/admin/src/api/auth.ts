import { AuthResponse, AuthUser } from '../types.ts';
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
    defaultErrorMessage: 'Chưa đăng nhập.',
  });
}

export async function setupAdmin(username: string, password: string): Promise<void> {
  await requestText('/auth/setup', {
    method: 'POST',
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: 'Không thể thiết lập quản trị viên.',
  });
}

export async function login(username: string, password: string): Promise<AuthResponse> {
  return requestJson<AuthResponse>('/auth/login', {
    method: 'POST',
    contentType: 'application/json',
    body: JSON.stringify({ username, password }),
    defaultErrorMessage: 'Không thể đăng nhập.',
  });
}

export async function logout(csrfToken: string | null): Promise<void> {
  await requestText('/auth/logout', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: 'Không thể đăng xuất.',
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
    defaultErrorMessage: 'Không thể tạo tài khoản biên tập viên.',
  });
}

export function listUsers(limit: number, offset: number): Promise<UserPage> {
  return requestJson<UserPage>('/api/admin/users?limit=' + limit + '&offset=' + offset, {
    defaultErrorMessage: 'Không thể tải danh sách người dùng.',
  });
}

export function createUser(username: string, password: string, role: UserRole, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users', {
    method: 'POST', contentType: 'application/json', csrfToken,
    body: JSON.stringify({ username, password, role }),
    defaultErrorMessage: 'Không thể tạo người dùng.',
  });
}

export function updateUser(id: number, version: number, changes: { username?: string; role?: UserRole }, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users/' + id, {
    method: 'PATCH', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify(changes),
    defaultErrorMessage: 'Không thể cập nhật người dùng.',
  });
}

export function resetUserPassword(id: number, version: number, password: string, csrfToken: string): Promise<UserSummary> {
  return requestJson<UserSummary>('/api/admin/users/' + id + '/password', {
    method: 'POST', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify({ password }),
    defaultErrorMessage: 'Không thể đặt lại mật khẩu.',
  });
}

export async function removeUser(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText('/api/admin/users/' + id, {
    method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: 'Không thể xóa tài khoản.',
  });
}
