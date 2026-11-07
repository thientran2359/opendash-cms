import { AuthResponse, AuthUser } from '../types.ts';
import { requestJson, requestText } from './client.ts';

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
