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
    throw new MediaUploadValidationError('Chỉ hỗ trợ ảnh JPEG, PNG hoặc WebP.');
  }
  if (file.size === 0) {
    throw new MediaUploadValidationError('Ảnh trống (0 byte). Hãy chọn ảnh khác.');
  }
  if (file.size > MAX_FILE_BYTES) {
    throw new MediaUploadValidationError('Ảnh vượt quá 8 MiB. Hãy giảm dung lượng trước khi tải lên.');
  }

  // createImageBitmap verifies that the browser can decode the selected file.
  // An image with valid-looking MIME but invalid bytes must not appear uploaded.
  let bitmap: ImageBitmap;
  try {
    bitmap = await createImageBitmap(file);
  } catch {
    throw new MediaUploadValidationError('Không thể đọc ảnh. File có thể bị hỏng hoặc sai định dạng.');
  }
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width > MAX_DIMENSION
      || bitmap.height > MAX_DIMENSION || bitmap.width * bitmap.height > MAX_PIXELS) {
      throw new MediaUploadValidationError('Ảnh vượt giới hạn 6000 px mỗi chiều hoặc 20 triệu điểm ảnh.');
    }
  } finally {
    bitmap.close();
  }
}

/** Friendly browser preflight for MP4 video uploads. Rust still validates MP4 boxes & codecs. */
export function validateVideoUpload(file: File): void {
  const isMp4Type = file.type === 'video/mp4' || file.name.toLowerCase().endsWith('.mp4');
  if (!isMp4Type) {
    throw new MediaUploadValidationError('Chỉ hỗ trợ video MP4 (video/mp4).');
  }
  if (file.size === 0) {
    throw new MediaUploadValidationError('Video trống (0 byte). Hãy chọn file khác.');
  }
  if (file.size > MAX_VIDEO_BYTES) {
    throw new MediaUploadValidationError('Video vượt quá 100 MiB.');
  }
}
