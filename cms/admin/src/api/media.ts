import { __, _x } from '../i18n/index.tsx';
import { ApiClientError, requestJson } from './client.ts';

export type MediaKind = 'image' | 'video';
export type MediaVariantSpec = 'thumbnail' | 'medium' | 'large' | 'full';
export type MediaVariantFormat = 'jpeg' | 'png' | 'webp';

export interface MediaVariantView {
  spec: MediaVariantSpec;
  format: MediaVariantFormat;
  width: number;
  height: number;
  byte_size: number;
  url: string;
}

export interface MediaAsset {
  id: string;
  content_sha256: string;
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
  variants_ready_version: number;
  created_at: string;
  updated_at: string;
}

export interface MediaAssetDetails extends MediaAsset {
  variants: MediaVariantView[];
  webp_srcset: string | null;
  fallback_srcset: string | null;
  etag: string;
}

export interface MediaVariantsStatus {
  total_images: number;
  ready_images: number;
  pending_images: number;
  variant_files_bytes: number;
  active_run_id: string | null;
}

export interface RegenerateVariantsRequest {
  after_id?: string;
  batch_size?: number;
  force?: boolean;
  run_id?: string;
}

export interface RegenerateCumulativeStats {
  total_processed: number;
  total_succeeded: number;
  total_failed: number;
  total_skipped_concurrent: number;
  last_after_id: string | null;
}

export interface RegenerateVariantsResponse {
  run_id: string;
  batch_processed: number;
  batch_succeeded: number;
  batch_failed: number;
  batch_skipped_concurrent: number;
  failed_ids: string[];
  remaining: number;
  next_after_id: string | null;
  done: boolean;
  cumulative: RegenerateCumulativeStats;
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
      defaultErrorMessage: __("Could not delete media."),
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
          body.error || __("This media is in use and cannot be deleted."),
          body.pins,
        );
      }
      throw new Error(body?.error || __("This media is busy or being deleted."));
    }
    throw err;
  }
}

export function formatMediaIfMatch(
  generation: number,
  version: number,
  etag?: string,
): string {
  const trimmed = etag?.trim();
  if (trimmed && /^"gv\d+-v\d+"$/.test(trimmed)) {
    return trimmed;
  }
  return `"gv${generation}-v${version}"`;
}

export const mediaThumbnailUrl = (id: string, version?: number): string => {
  const base = `/api/admin/media/${encodeURIComponent(id)}/thumbnail`;
  return typeof version === 'number' && Number.isInteger(version) && version > 0
    ? `${base}?v=${version}`
    : base;
};

export const mediaOriginalUrl = (id: string, version?: number): string => {
  const base = `/api/admin/media/${encodeURIComponent(id)}/file`;
  return typeof version === 'number' && Number.isInteger(version) && version > 0
    ? `${base}?v=${version}`
    : base;
};

export function getMedia(id: string): Promise<MediaAssetDetails> {
  return requestJson<MediaAssetDetails>(`/api/admin/media/${encodeURIComponent(id)}`, {
    defaultErrorMessage: __("Could not load media details."),
  });
}

export function replaceMedia(
  id: string,
  file: File,
  expected: { generation: number; version: number; etag?: string },
  csrfToken: string,
): Promise<MediaAssetDetails> {
  const isMp4 = file.type === 'video/mp4' || file.name.toLowerCase().endsWith('.mp4');
  const contentType = isMp4 ? 'video/mp4' : file.type || 'application/octet-stream';
  return requestJson<MediaAssetDetails>(`/api/admin/media/${encodeURIComponent(id)}/file`, {
    method: 'PUT',
    csrfToken,
    ifMatch: formatMediaIfMatch(expected.generation, expected.version, expected.etag),
    contentType,
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: __("Could not replace the media file."),
    oversizeErrorMessage: isMp4
      ? _x("The video exceeds 100 MiB.", "api.media")
      : __("The image exceeds 8 MiB."),
  });
}

export function getMediaVariantsStatus(): Promise<MediaVariantsStatus> {
  return requestJson<MediaVariantsStatus>('/api/admin/media/variants/status', {
    defaultErrorMessage: __("Could not load responsive variant status."),
  });
}

export function regenerateMediaVariants(
  csrfToken: string,
  input: RegenerateVariantsRequest = {},
): Promise<RegenerateVariantsResponse> {
  return requestJson<RegenerateVariantsResponse>('/api/admin/media/variants/regenerate', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not regenerate responsive media variants."),
  });
}

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
    defaultErrorMessage: __("Could not load the media library."),
  });
}

export function uploadMedia(file: File, csrfToken: string): Promise<MediaAsset> {
  return requestJson<MediaAsset>('/api/admin/media', {
    method: 'POST',
    csrfToken,
    contentType: file.type,
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: __("Could not upload the image."),
    oversizeErrorMessage: __("The image exceeds 8 MiB."),
  });
}

export function uploadVideo(file: File, csrfToken: string): Promise<MediaAsset> {
  return requestJson<MediaAsset>('/api/admin/media/video', {
    method: 'POST',
    csrfToken,
    contentType: file.type || 'video/mp4',
    uploadFilename: file.name,
    body: file,
    defaultErrorMessage: __("Could not upload the MP4 video."),
    oversizeErrorMessage: _x("The video exceeds 100 MiB.", "api.media"),
  });
}
