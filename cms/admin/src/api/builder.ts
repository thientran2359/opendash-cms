import { BlockData, BlockRegistry } from '../types.ts';
import { ApiClientError, handleResponseText } from './client.ts';

export async function getRegistry(): Promise<BlockRegistry> {
  const res = await fetch('/registry', {
    method: 'GET',
    credentials: 'same-origin',
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiClientError(res.status, text || 'Không thể tải danh sách block.');
  }
  return res.json();
}

export async function getPage(): Promise<BlockData | null> {
  const res = await fetch('/page', {
    method: 'GET',
    credentials: 'same-origin',
  });
  if (!res.ok) {
    const text = await res.text();
    throw new ApiClientError(res.status, text || 'Không thể tải trang.');
  }
  return res.json();
}

export async function uploadTheme(file: File, csrfToken: string): Promise<void> {
  const res = await fetch('/theme', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/zip',
      'X-CSRF-Token': csrfToken,
    },
    body: file,
  });
  await handleResponseText(res, 'Không thể tải lên theme ZIP.');
}

export async function previewBlock(block: BlockData, csrfToken: string): Promise<string> {
  const res = await fetch('/preview', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrfToken,
    },
    body: JSON.stringify(block),
  });
  return handleResponseText(res, 'Không thể xem trước block.');
}

export async function saveBlock(block: BlockData, csrfToken: string): Promise<string> {
  const res = await fetch('/save', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      'X-CSRF-Token': csrfToken,
    },
    body: JSON.stringify(block),
  });
  return handleResponseText(res, 'Không thể lưu block.');
}
