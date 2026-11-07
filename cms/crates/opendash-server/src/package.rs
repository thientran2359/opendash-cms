//! Shared safe ZIP package inspection and extraction primitives.
//!
//! Centralizes ZIP archive opening, entry path traversal checks, symlink/directory
//! rejection, and bounded reading for Theme and Plugin ZIP packages.

use std::io::{Cursor, Read};

use zip::{ZipArchive, read::ZipFile};

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
