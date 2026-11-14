import { requestJson } from './client.ts';

export type MediaKind = 'image' | 'video';

export interface MediaAsset {
  id: string;
  kind: MediaKind;
  filename: string;
  mime_type: 'image/jpeg' | 'image/png' | 'image/webp' | 'video/mp4';
  extension: 'jpg' | 'png' | 'webp' | 'mp4';
  byte_size: number;
  width: number;
  height: number;
  thumb_width: number | null;
  thumb_height: number | null;
  duration_ms: number | null;
  video_codec: string | null;
  audio_codec: string | null;
  created_at: string;
}

export const mediaThumbnailUrl = (id: string): string =>
  `/api/admin/media/${encodeURIComponent(id)}/thumbnail`;

export const mediaOriginalUrl = (id: string): string =>
  `/api/admin/media/${encodeURIComponent(id)}/file`;

export function listMedia(
  limit = 48,
  offset = 0,
  kind?: MediaKind,
): Promise<MediaAsset[]> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  if (kind) {
    params.set('kind', kind);
  }
  return requestJson<MediaAsset[]>(`/api/admin/media?${params.toString()}`, {
    defaultErrorMessage: 'Không thể tải thư viện media.',
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

export function uploadVideo(file: File, csrfToken: string): Promise<MediaAsset> {
  return requestJson<MediaAsset>('/api/admin/media/video', {
    method: 'POST',
    csrfToken,
    contentType: file.type || 'video/mp4',
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: 'Không thể upload video MP4.',
    oversizeErrorMessage: 'File video vượt quá 100 MiB.',
  });
}
