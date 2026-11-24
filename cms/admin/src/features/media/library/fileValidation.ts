import { tStatic } from '../../../i18n/index.tsx';
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
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.only_jpeg_png_or_webp_images_are_supported'));
  }
  if (file.size === 0) {
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.the_image_is_empty_0_bytes_select_another_image'));
  }
  if (file.size > MAX_FILE_BYTES) {
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.the_image_exceeds_8_mib_reduce_its_size_before_uploading'));
  }

  // createImageBitmap verifies that the browser can decode the selected file.
  // An image with valid-looking MIME but invalid bytes must not appear uploaded.
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(file);
  } catch {
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.could_not_read_the_image_the_file_may_be_corrupted_or_have_an_invalid_format'));
  }
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width > MAX_DIMENSION
      || bitmap.height > MAX_DIMENSION || bitmap.width * bitmap.height > MAX_PIXELS) {
      throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.the_image_exceeds_6000_px_on_either_side_or_20_million_pixels'));
    }
  } finally {
    bitmap.close();
  }
}

/** Friendly browser preflight for MP4 video uploads. Rust still validates MP4 boxes & codecs. */
export function validateVideoUpload(file: File): void {
  const isMp4Type = file.type === 'video/mp4' || file.name.toLowerCase().endsWith('.mp4');
  if (!isMp4Type) {
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.only_mp4_video_video_mp4_is_supported'));
  }
  if (file.size === 0) {
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.the_video_is_empty_0_bytes_select_another_file'));
  }
  if (file.size > MAX_VIDEO_BYTES) {
    throw new MediaUploadValidationError(tStatic('feature.media.filevalidation.the_video_exceeds_100_mib'));
  }
}
