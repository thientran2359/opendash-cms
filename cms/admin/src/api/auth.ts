import { AuthResponse, AuthUser } from '../types.ts';
import { ApiClientError, handleResponseText } from './client.ts';

export async function getAuthMe(): Promise<AuthResponse> {
  const res = await fetch('/auth/me', {
    method: 'GET',
    credentials: 'same-origin',
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiClientError(res.status, text || 'Chưa đăng nhập.');
  }
  return res.json();
}

export async function setupAdmin(username: string, password: string): Promise<void> {
  const res = await fetch('/auth/setup', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  await handleResponseText(res, 'Không thể thiết lập quản trị viên.');
}

export async function login(username: string, password: string): Promise<AuthResponse> {
  const res = await fetch('/auth/login', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiClientError(res.status, text || 'Không thể đăng nhập.');
  }
  return res.json();
}

export async function logout(csrfToken: string | null): Promise<void> {
  const headers: Record<string, string> = {};
  if (csrfToken) {
    headers['X-CSRF-Token'] = csrfToken;
  }
  const res = await fetch('/auth/logout', {
    method: 'POST',
    credentials: 'same-origin',
    headers,
  });
  await handleResponseText(res, 'Không thể đăng xuất.');
}

export async function createEditor(
  username: string,
  password: string,
  csrfToken: string,
): Promise<AuthUser> {
  const res = await fetch('/api/admin/users', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrfToken,
    },
    body: JSON.stringify({ username, password }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiClientError(res.status, text || 'Không thể tạo tài khoản biên tập viên.');
  }
  return res.json();
}
