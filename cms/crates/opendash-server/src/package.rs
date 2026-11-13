//! Shared safe ZIP package inspection and extraction primitives.
//!
//! Centralizes ZIP archive opening, entry path traversal checks, symlink/directory
//! rejection, and bounded reading for Theme and Plugin ZIP packages.

use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Read},
};

use zip::{ZipArchive, read::ZipFile};

pub const V2_MAX_ZIP_BYTES: usize = 8 * 1024 * 1024;
pub const V2_MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
pub const V2_MAX_FILES: usize = 256;
pub const V2_MAX_TEXT_BYTES: usize = 256 * 1024;
pub const V2_MAX_RASTER_BYTES: usize = 8 * 1024 * 1024;
pub const V2_MAX_EXPANSION_RATIO: u64 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2ZipError {
    PayloadTooLarge(&'static str),
    BadRequest(&'static str),
}

/// Opens a ZIP archive from raw bytes after verifying the compressed size ceiling.
pub fn open_bounded_zip<'a>(
    bytes: &'a [u8],
    max_zip_bytes: usize,
    oversize_err: &'static str,
    invalid_zip_err: &'static str,
) -> Result<ZipArchive<Cursor<&'a [u8]>>, &'static str> {
    if bytes.len() > max_zip_bytes {
        return Err(oversize_err);
    }
    ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid_zip_err)
}

/// Validates that a ZIP entry name is a flat, safe file name (no `/`, `\`, `..`,
/// symlinks, or subdirectories).
pub fn ensure_safe_flat_entry<R: Read>(
    file: &ZipFile<'_, R>,
    unsafe_path_err: &'static str,
    symlink_err: &'static str,
    dir_err: &'static str,
) -> Result<String, &'static str> {
    let name_cow = file.name().map_err(|_| unsafe_path_err)?;
    let name = name_cow.to_string();

    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(unsafe_path_err);
    }

    if let Some(mode) = file.unix_mode()
        && (mode & 0o170000) == 0o120000
    {
        return Err(symlink_err);
    }

    if file.is_dir() {
        return Err(dir_err);
    }

    Ok(name)
}

/// Reads an entry's bytes up to `read_cap + 1` so callers can detect oversize
/// uncompressed payloads without allocating unbounded memory (zip-bomb protection).
pub fn read_bounded_entry<R: Read>(
    file: &mut ZipFile<'_, R>,
    read_cap: usize,
    read_err: &'static str,
) -> Result<Vec<u8>, &'static str> {
    let mut buffer = Vec::new();
    file.by_ref()
        .take((read_cap + 1) as u64)
        .read_to_end(&mut buffer)
        .map_err(|_| read_err)?;
    Ok(buffer)
}

/// Checks if a `Content-Type` header is acceptable for a ZIP upload.
/// When `strict_only_application_zip` is true, requires exact `application/zip`.
pub fn is_valid_zip_content_type(
    content_type: Option<&str>,
    strict_only_application_zip: bool,
) -> bool {
    match content_type {
        Some(ct) if strict_only_application_zip => ct == "application/zip",
        Some(ct) => {
            ct.starts_with("application/zip")
                || ct.starts_with("application/x-zip-compressed")
                || ct.starts_with("application/octet-stream")
        }
        None => !strict_only_application_zip,
    }
}

/// Validates a relative ASCII path for v2 theme packages:
/// no empty/dot/dot-dot segments, backslashes, drive letters/colons, control chars, or leading/trailing slashes.
pub fn validate_v2_relative_path(raw: &str) -> Result<String, &'static str> {
    if raw.is_empty()
        || !raw.is_ascii()
        || raw.starts_with('/')
        || raw.ends_with('/')
        || raw.contains('\\')
        || raw.contains(':')
        || raw.bytes().any(|b| b < 0x20 || b == 0x7f)
    {
        return Err("Đường dẫn trong ZIP v2 không hợp lệ.");
    }
    for segment in raw.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err("Đường dẫn trong ZIP v2 chứa segment không an toàn.");
        }
    }
    Ok(raw.to_string())
}

/// Reads and validates all entries of a v2 theme ZIP archive enforcing size,
/// file count, expansion ratio, zero-compressed-size bomb, symlink, and case-collision limits.
pub fn read_v2_zip_entries<F>(
    bytes: &[u8],
    entry_cap: F,
) -> Result<BTreeMap<String, Vec<u8>>, V2ZipError>
where
    F: Fn(&str) -> Result<usize, &'static str>,
{
    let mut archive = open_bounded_zip(
        bytes,
        V2_MAX_ZIP_BYTES,
        "Gói theme ZIP v2 vượt quá giới hạn 8 MiB.",
        "Gói theme ZIP không hợp lệ.",
    )
    .map_err(|msg| {
        if bytes.len() > V2_MAX_ZIP_BYTES {
            V2ZipError::PayloadTooLarge(msg)
        } else {
            V2ZipError::BadRequest(msg)
        }
    })?;

    let mut files = BTreeMap::new();
    let mut seen_lower = HashSet::new();
    let mut total_decoded: usize = 0;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| V2ZipError::BadRequest("Không đọc được mục trong ZIP."))?;

        if let Some(mode) = entry.unix_mode()
            && (mode & 0o170000) == 0o120000
        {
            return Err(V2ZipError::BadRequest("ZIP không được chứa symlink."));
        }

        let name = entry
            .name()
            .map_err(|_| V2ZipError::BadRequest("Đường dẫn ZIP không hợp lệ."))?
            .to_string();

        if entry.is_dir() {
            let trimmed = name.trim_end_matches('/');
            if !trimmed.is_empty() {
                validate_v2_relative_path(trimmed).map_err(V2ZipError::BadRequest)?;
            }
            if entry.size() > 0 {
                return Err(V2ZipError::BadRequest(
                    "Mục thư mục trong ZIP không được chứa dữ liệu.",
                ));
            }
            continue;
        }

        let rel_path = validate_v2_relative_path(&name).map_err(V2ZipError::BadRequest)?;
        if files.len() >= V2_MAX_FILES {
            return Err(V2ZipError::PayloadTooLarge(
                "Gói theme ZIP v2 vượt quá giới hạn 256 file.",
            ));
        }

        let lower = rel_path.to_ascii_lowercase();
        if !seen_lower.insert(lower) {
            return Err(V2ZipError::BadRequest(
                "ZIP v2 chứa đường dẫn trùng lặp hoặc xung đột chữ hoa/thường.",
            ));
        }

        let max_for_entry = entry_cap(&rel_path).map_err(V2ZipError::BadRequest)?;
        let remaining_total = V2_MAX_DECODED_BYTES.saturating_sub(total_decoded);
        let read_cap = max_for_entry.min(remaining_total);

        if entry.size() > max_for_entry as u64 {
            return Err(V2ZipError::PayloadTooLarge(
                "File trong ZIP v2 vượt quá giới hạn kích thước cho phép.",
            ));
        }
        if entry.size() > remaining_total as u64 {
            return Err(V2ZipError::PayloadTooLarge(
                "Tổng dung lượng giải nén ZIP v2 vượt quá 32 MiB.",
            ));
        }

        let compressed_size = entry.compressed_size();
        let data = read_bounded_entry(
            &mut entry,
            read_cap,
            "Không thể đọc nội dung file trong ZIP v2.",
        )
        .map_err(V2ZipError::BadRequest)?;

        if data.len() > max_for_entry {
            return Err(V2ZipError::PayloadTooLarge(
                "File giải nén trong ZIP v2 vượt quá giới hạn kích thước.",
            ));
        }
        if data.len() > remaining_total {
            return Err(V2ZipError::PayloadTooLarge(
                "Tổng dung lượng giải nén ZIP v2 vượt quá 32 MiB.",
            ));
        }

        let actual_len = data.len() as u64;
        if (compressed_size == 0 && actual_len > 0)
            || (compressed_size > 0
                && actual_len > compressed_size.saturating_mul(V2_MAX_EXPANSION_RATIO))
        {
            return Err(V2ZipError::BadRequest(
                "ZIP v2 bị từ chối do tỷ lệ giải nén bất thường (zip bomb).",
            ));
        }

        total_decoded = total_decoded
            .checked_add(data.len())
            .ok_or(V2ZipError::PayloadTooLarge(
                "Tổng dung lượng giải nén ZIP v2 vượt quá 32 MiB.",
            ))?;
        files.insert(rel_path, data);
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_zip_content_types() {
        assert!(is_valid_zip_content_type(Some("application/zip"), true));
        assert!(!is_valid_zip_content_type(
            Some("application/octet-stream"),
            true
        ));
        assert!(is_valid_zip_content_type(
            Some("application/x-zip-compressed"),
            false
        ));
        assert!(!is_valid_zip_content_type(Some("text/plain"), false));
    }
}
