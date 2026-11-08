import { requestJson } from './client.ts';

export interface MediaAsset {
  id: string;
  filename: string;
  mime_type: 'image/jpeg' | 'image/png' | 'image/webp';
  extension: 'jpg' | 'png' | 'webp';
  byte_size: number;
  width: number;
  height: number;
  thumb_width: number;
  thumb_height: number;
  created_at: string;
}

export const mediaThumbnailUrl = (id: string): string =>
  `/api/admin/media/${encodeURIComponent(id)}/thumbnail`;

export const mediaOriginalUrl = (id: string): string =>
  `/api/admin/media/${encodeURIComponent(id)}/file`;

export function listMedia(limit = 48, offset = 0): Promise<MediaAsset[]> {
  return requestJson<MediaAsset[]>(`/api/admin/media?limit=${limit}&offset=${offset}`, {
    defaultErrorMessage: 'Không thể tải thư viện ảnh.',
  });
}

export function uploadMedia(file: File, csrfToken: string): Promise<MediaAsset> {
  return requestJson<MediaAsset>('/api/admin/media', {
    method: 'POST',
    csrfToken,
    contentType: file.type,
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: 'Không thể upload ảnh.',
    oversizeErrorMessage: 'File ảnh vượt quá 8 MiB.',
  });
}
