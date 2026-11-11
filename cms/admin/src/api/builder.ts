import { BlockData, BlockRegistry } from '../types.ts';
import { requestBuilderHtml, requestJson, requestText } from './client.ts';

export async function getRegistry(): Promise<BlockRegistry> {
  return requestJson<BlockRegistry>('/registry', {
    method: 'GET',
    defaultErrorMessage: 'Không thể tải danh sách block.',
  });
}

export async function getPage(): Promise<BlockData | null> {
  return requestJson<BlockData | null>('/page', {
    method: 'GET',
    defaultErrorMessage: 'Không thể tải trang.',
  });
}

export async function uploadTheme(file: File, csrfToken: string): Promise<void> {
  await requestText('/theme', {
    method: 'POST',
    csrfToken,
    contentType: 'application/zip',
    body: file,
    defaultErrorMessage: 'Không thể tải lên theme ZIP.',
  });
}

export async function previewBlock(block: BlockData, csrfToken: string) {
  return requestBuilderHtml('/preview', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(block),
    defaultErrorMessage: 'Không thể xem trước block.',
  });
}

export async function saveBlock(block: BlockData, csrfToken: string) {
  return requestBuilderHtml('/save', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(block),
    defaultErrorMessage: 'Không thể lưu block.',
  });
}
