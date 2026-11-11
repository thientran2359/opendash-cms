import type { BlockRegistry } from '../types.ts';
import type { BuilderDocument } from '../builder/document.ts';
import { requestBlob, requestJson } from './client.ts';

export interface BuilderDiagnostic { node_id: string; code: string }
export interface HomepageOpen {
  document: BuilderDocument;
  source: 'empty' | 'legacy' | 'layout';
  version: number;
}
export interface HomepagePreview {
  html: string;
  diagnostics: BuilderDiagnostic[];
}
export interface HomepageSaved extends HomepageOpen, HomepagePreview {}

export async function getRegistry(): Promise<BlockRegistry> {
  return requestJson<BlockRegistry>('/registry', {
    method: 'GET',
    defaultErrorMessage: 'Không thể tải danh sách block.',
  });
}

export function getHomepage(): Promise<HomepageOpen> {
  return requestJson('/api/admin/builder/homepage', {
    method: 'GET',
    defaultErrorMessage: 'Không thể mở homepage.',
  });
}

export function previewHomepage(document: BuilderDocument, csrfToken: string, signal?: AbortSignal): Promise<HomepagePreview> {
  return requestJson('/api/admin/builder/homepage/preview', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(document),
    signal,
    defaultErrorMessage: 'Không thể xem trước homepage.',
  });
}

export function saveHomepage(document: BuilderDocument, version: number, csrfToken: string): Promise<HomepageSaved> {
  return requestJson('/api/admin/builder/homepage', {
    method: 'PUT',
    csrfToken,
    contentType: 'application/json',
    ifMatchVersion: version,
    body: JSON.stringify(document),
    defaultErrorMessage: 'Không thể lưu homepage.',
  });
}

export function getPrivatePreviewMedia(path: string, signal: AbortSignal): Promise<Blob> {
  return requestBlob(path, {
    signal,
    defaultErrorMessage: 'Không tải được ảnh riêng tư cho Preview.',
  });
}
