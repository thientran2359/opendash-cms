import { tStatic } from '../i18n/index.tsx';
import { ApiClientError, requestJson } from './client.ts';

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
  lifecycle_state: 'active' | 'deleting';
  generation: number;
  version: number;
  created_at: string;
}

export interface MediaPin {
  source_kind: string;
  entity_id: string;
}

export interface MediaPinSummary {
  total_pins: number;
  pins: MediaPin[];
}

export interface MediaDeleteJob {
  job_id: string;
  media_id: string;
  generation: number;
  expected_version: number;
  phase: 'tombstoned' | 'moving' | 'quarantined' | 'finalized';
  error_code: string | null;
}

export class MediaDeleteConflictError extends Error {
  readonly pins: MediaPinSummary;

  constructor(message: string, pins: MediaPinSummary) {
    super(message);
    this.name = 'MediaDeleteConflictError';
    this.pins = pins;
  }
}

export async function deleteMedia(
  id: string,
  generation: number,
  version: number,
  csrfToken: string,
): Promise<MediaDeleteJob> {
  try {
    return await requestJson<MediaDeleteJob>(`/api/admin/media/${encodeURIComponent(id)}`, {
      method: 'DELETE',
      csrfToken,
      ifMatch: `"gv${generation}-v${version}"`,
      defaultErrorMessage: tStatic('api.media.delete_media'),
    });
  } catch (err) {
    if (err instanceof ApiClientError && err.status === 409) {
      let body: { error?: string; pins?: MediaPinSummary } | null = null;
      try {
        body = JSON.parse(err.message) as { error?: string; pins?: MediaPinSummary };
      } catch {
        body = null;
      }
      if (body?.pins) {
        throw new MediaDeleteConflictError(
          body.error || tStatic('api.media.this_media_is_in_use_and_cannot_be_deleted'),
          body.pins,
        );
      }
      throw new Error(body?.error || tStatic('api.media.this_media_is_busy_or_being_deleted'));
    }
    throw err;
  }
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
    defaultErrorMessage: tStatic('api.media.load_the_media_library'),
  });
}

export function uploadMedia(file: File, csrfToken: string): Promise<MediaAsset> {
  return requestJson<MediaAsset>('/api/admin/media', {
    method: 'POST',
    csrfToken,
    contentType: file.type,
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: tStatic('api.media.upload_the_image'),
    oversizeErrorMessage: tStatic('api.media.the_image_exceeds_8_mib'),
  });
}

export function uploadVideo(file: File, csrfToken: string): Promise<MediaAsset> {
  return requestJson<MediaAsset>('/api/admin/media/video', {
    method: 'POST',
    csrfToken,
    contentType: file.type || 'video/mp4',
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: tStatic('api.media.upload_the_mp4_video'),
    oversizeErrorMessage: tStatic('api.media.the_video_exceeds_100_mib'),
  });
}
