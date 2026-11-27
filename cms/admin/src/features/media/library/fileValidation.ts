import { __, _x } from '../../../i18n/index.tsx';
const ACCEPTED_MIME_TYPES = new Set(['image/jpeg', 'image/png', 'image/webp']);
const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_DIMENSION = 6000;
const MAX_PIXELS = 20_000_000;
const MAX_VIDEO_BYTES = 100 * 1024 * 1024;

export class MediaUploadValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MediaUploadValidationError';
  }
}

/** Friendly browser preflight only. Rust still validates actual bytes and dimensions. */
export async function validateImageUpload(file: File): Promise<void> {
  if (!ACCEPTED_MIME_TYPES.has(file.type)) {
    throw new MediaUploadValidationError(__("Only JPEG, PNG or WebP images are supported."));
  }
  if (file.size === 0) {
    throw new MediaUploadValidationError(__("The image is empty (0 bytes). Select another image."));
  }
  if (file.size > MAX_FILE_BYTES) {
    throw new MediaUploadValidationError(__("The image exceeds 8 MiB. Reduce its size before uploading."));
  }

  // createImageBitmap verifies that the browser can decode the selected file.
  // An image with valid-looking MIME but invalid bytes must not appear uploaded.
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(file);
  } catch {
    throw new MediaUploadValidationError(__("Could not read the image. The file may be corrupted or have an invalid format."));
  }
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width > MAX_DIMENSION
      || bitmap.height > MAX_DIMENSION || bitmap.width * bitmap.height > MAX_PIXELS) {
      throw new MediaUploadValidationError(__("The image exceeds 6000 px on either side or 20 million pixels."));
    }
  } finally {
    bitmap.close();
  }
}

/** Friendly browser preflight for MP4 video uploads. Rust still validates MP4 boxes & codecs. */
export function validateVideoUpload(file: File): void {
  const isMp4Type = file.type === 'video/mp4' || file.name.toLowerCase().endsWith('.mp4');
  if (!isMp4Type) {
    throw new MediaUploadValidationError(__("Only MP4 video (video/mp4) is supported."));
  }
  if (file.size === 0) {
    throw new MediaUploadValidationError(__("The video is empty (0 bytes). Select another file."));
  }
  if (file.size > MAX_VIDEO_BYTES) {
    throw new MediaUploadValidationError(_x("The video exceeds 100 MiB.", "feature.media"));
  }
}
