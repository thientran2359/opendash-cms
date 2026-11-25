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
    let archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid_zip_err)?;
    // zip 9.3.0 deduplicates equal raw central-directory names in its
    // IndexMap before by_index() is called. Compare its raw entry count first
    // so an overwritten regular member can never bypass our path/cap checks.
    if archive.has_duplicate_names() {
        return Err("ZIP chứa đường dẫn trùng lặp.");
    }
    Ok(archive)
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

/// Validates a relative ASCII path inside theme/plugin packages:
/// no empty/dot/dot-dot segments, backslashes, drive letters/colons, control chars, or leading/trailing slashes.
pub fn validate_theme_relative_path(raw: &str) -> Result<String, &'static str> {
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
                validate_theme_relative_path(trimmed).map_err(V2ZipError::BadRequest)?;
            }
            if entry.size() > 0 {
                return Err(V2ZipError::BadRequest(
                    "Mục thư mục trong ZIP không được chứa dữ liệu.",
                ));
            }
            continue;
        }

        let rel_path = validate_theme_relative_path(&name).map_err(V2ZipError::BadRequest)?;
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

        total_decoded =
            total_decoded
                .checked_add(data.len())
                .ok_or(V2ZipError::PayloadTooLarge(
                    "Tổng dung lượng giải nén ZIP v2 vượt quá 32 MiB.",
                ))?;
        files.insert(rel_path, data);
    }

    Ok(files)
}

pub const BACKUP_MAX_ZIP_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const BACKUP_MAX_CD_BYTES: u32 = 16 * 1024 * 1024;
pub const BACKUP_MAX_ENTRIES: usize = 20_001;
pub const BACKUP_MAX_ENTRY_NAME_BYTES: usize = 13;

/// Preflights a file-backed backup ZIP32 archive before calling `ZipArchive::new`.
/// Rejects ZIP64, oversized central directory, comments, extra fields, non-Store
/// compression, encrypted entries, excessive entry counts, and overlapping offsets.
pub fn preflight_and_open_backup_zip(
    file: std::fs::File,
) -> Result<ZipArchive<std::io::BufReader<std::fs::File>>, &'static str> {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = std::io::BufReader::new(file);
    let file_len = reader
        .seek(SeekFrom::End(0))
        .map_err(|_| "Không xác định được kích thước tệp backup ZIP.")?;
    if file_len < 22 || file_len > BACKUP_MAX_ZIP_BYTES {
        return Err("Kích thước tệp backup ZIP nằm ngoài giới hạn hợp lệ.");
    }

    let eocd_offset = file_len - 22;
    reader
        .seek(SeekFrom::Start(eocd_offset))
        .map_err(|_| "Không đọc được EOCD của backup ZIP.")?;
    let mut eocd = [0u8; 22];
    reader
        .read_exact(&mut eocd)
        .map_err(|_| "Không đọc đủ EOCD của backup ZIP.")?;

    if &eocd[0..4] != b"PK\x05\x06" {
        return Err("Backup ZIP thiếu ZIP32 EOCD hợp lệ hoặc chứa comment cuối tệp.");
    }
    let disk_num = u16::from_le_bytes([eocd[4], eocd[5]]);
    let cd_disk = u16::from_le_bytes([eocd[6], eocd[7]]);
    let entries_disk = u16::from_le_bytes([eocd[8], eocd[9]]);
    let entries_total = u16::from_le_bytes([eocd[10], eocd[11]]);
    let cd_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]);
    let cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]);
    let comment_len = u16::from_le_bytes([eocd[20], eocd[21]]);

    if disk_num != 0
        || cd_disk != 0
        || entries_disk != entries_total
        || entries_total == u16::MAX
        || entries_total == 0
        || (entries_total as usize) > BACKUP_MAX_ENTRIES
        || cd_size == u32::MAX
        || cd_offset == u32::MAX
        || cd_size > BACKUP_MAX_CD_BYTES
        || comment_len != 0
    {
        return Err("Header ZIP32 EOCD của backup không hợp lệ hoặc vượt ngưỡng.");
    }

    if (cd_offset as u64)
        .checked_add(cd_size as u64)
        .is_none_or(|end| end != eocd_offset)
    {
        return Err("Offset/kích thước Central Directory của backup ZIP không khớp.");
    }

    if eocd_offset >= 20 {
        let mut loc_sig = [0u8; 4];
        reader
            .seek(SeekFrom::Start(eocd_offset - 20))
            .map_err(|_| "Không kiểm tra được ZIP64 locator.")?;
        reader
            .read_exact(&mut loc_sig)
            .map_err(|_| "Không đọc được ZIP64 locator.")?;
        if &loc_sig == b"PK\x06\x07" {
            return Err("Backup ZIP64 không được hỗ trợ trong format v1.");
        }
    }

    reader
        .seek(SeekFrom::Start(cd_offset as u64))
        .map_err(|_| "Không seek được tới Central Directory của backup ZIP.")?;
    let mut cd_buf = vec![0u8; cd_size as usize];
    reader
        .read_exact(&mut cd_buf)
        .map_err(|_| "Không đọc trọn vẹn Central Directory của backup ZIP.")?;

    let mut pos = 0usize;
    let mut spans: Vec<(u64, u64)> = Vec::with_capacity(entries_total as usize);
    for _ in 0..entries_total {
        if pos + 46 > cd_buf.len() {
            return Err("Central Directory backup ZIP bị cắt cụt.");
        }
        let hdr = &cd_buf[pos..pos + 46];
        if &hdr[0..4] != b"PK\x01\x02" {
            return Err("Entry Central Directory backup ZIP sai chữ ký.");
        }
        let flags = u16::from_le_bytes([hdr[8], hdr[9]]);
        let method = u16::from_le_bytes([hdr[10], hdr[11]]);
        let comp_size = u32::from_le_bytes([hdr[20], hdr[21], hdr[22], hdr[23]]);
        let uncomp_size = u32::from_le_bytes([hdr[24], hdr[25], hdr[26], hdr[27]]);
        let name_len = u16::from_le_bytes([hdr[28], hdr[29]]) as usize;
        let extra_len = u16::from_le_bytes([hdr[30], hdr[31]]) as usize;
        let entry_comment_len = u16::from_le_bytes([hdr[32], hdr[33]]) as usize;
        let disk_start = u16::from_le_bytes([hdr[34], hdr[35]]);
        let local_offset = u32::from_le_bytes([hdr[42], hdr[43], hdr[44], hdr[45]]);

        if (flags & 0x0001) != 0
            || method != 0
            || comp_size == u32::MAX
            || uncomp_size == u32::MAX
            || comp_size != uncomp_size
            || name_len == 0
            || name_len > BACKUP_MAX_ENTRY_NAME_BYTES
            || extra_len != 0
            || entry_comment_len != 0
            || disk_start != 0
            || local_offset == u32::MAX
        {
            return Err("Entry backup ZIP vi phạm profile Store/ZIP32.");
        }

        let name_start = pos + 46;
        let name_end = name_start + name_len;
        if name_end > cd_buf.len() {
            return Err("Tên entry backup ZIP vượt quá Central Directory.");
        }
        let name_bytes = &cd_buf[name_start..name_end];
        if !name_bytes
            .iter()
            .all(|b| b.is_ascii() && !b.is_ascii_control())
            || name_bytes.contains(&b'/')
            || name_bytes.contains(&b'\\')
        {
            return Err("Tên entry backup ZIP không hợp lệ.");
        }

        let span_start = local_offset as u64;
        let span_len = 30u64 + (name_len as u64) + (comp_size as u64);
        let span_end = span_start
            .checked_add(span_len)
            .ok_or("Offset entry backup ZIP bị tràn số.")?;
        if span_end > (cd_offset as u64) {
            return Err("Payload entry backup ZIP vượt quá vùng dữ liệu trước Central Directory.");
        }
        spans.push((span_start, span_end));
        pos = name_end;
    }
    if pos != cd_buf.len() {
        return Err("Central Directory backup ZIP có dữ liệu dư thừa.");
    }

    spans.sort_unstable_by_key(|s| s.0);
    for w in spans.windows(2) {
        if w[0].1 > w[1].0 {
            return Err("Các entry trong backup ZIP có vùng byte chồng chéo.");
        }
    }

    reader
        .seek(SeekFrom::Start(0))
        .map_err(|_| "Không tua lại được backup ZIP.")?;
    ZipArchive::new(reader).map_err(|_| "Không mở được backup ZIP sau preflight.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn zip_with_members(files: &[(&str, &[u8])], directories: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut bytes));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for directory in directories {
                writer.add_directory(*directory, options).unwrap();
            }
            for (name, data) in files {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        bytes
    }

    // Fixed, ordinary ZIP32 fixture with two central-directory regular entries
    // both named "one.txt" (payloads "a" and "b"). The pinned zip writer
    // correctly refuses to create duplicates, so the fixture was generated
    // once with Python's stdlib zipfile; no runtime byte scanner is involved.
    const DUPLICATE_REGULAR_ZIP: &[u8] = &[
        0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x75, 0x5d, 0x43,
        0xbe, 0xb7, 0xe8, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00,
        0x6f, 0x6e, 0x65, 0x2e, 0x74, 0x78, 0x74, 0x61, 0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x75, 0x5d, 0xf9, 0xef, 0xbe, 0x71, 0x01, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x6f, 0x6e, 0x65, 0x2e, 0x74, 0x78, 0x74,
        0x62, 0x50, 0x4b, 0x01, 0x02, 0x14, 0x03, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x75, 0x5d, 0x43, 0xbe, 0xb7, 0xe8, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x6f, 0x6e, 0x65, 0x2e, 0x74, 0x78, 0x74, 0x50, 0x4b, 0x01, 0x02, 0x14, 0x03,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x75, 0x5d, 0xf9, 0xef, 0xbe, 0x71, 0x01,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x26, 0x00, 0x00, 0x00, 0x6f, 0x6e, 0x65, 0x2e, 0x74,
        0x78, 0x74, 0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x6a,
        0x00, 0x00, 0x00, 0x4c, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

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

    #[test]
    fn shared_zip_reader_rejects_exact_regular_duplicates_before_indexmap_dedup() {
        let raw = ZipArchive::new(Cursor::new(DUPLICATE_REGULAR_ZIP)).unwrap();
        assert!(raw.has_duplicate_names());
        assert_eq!(raw.len(), 1, "zip 9.3.0 deduplicates exact names");
        let failure = open_bounded_zip(
            DUPLICATE_REGULAR_ZIP,
            V2_MAX_ZIP_BYTES,
            "too big",
            "invalid",
        );
        assert!(matches!(failure, Err(message) if message.contains("trùng lặp")));
        assert!(matches!(
            read_v2_zip_entries(DUPLICATE_REGULAR_ZIP, |_| Ok(V2_MAX_TEXT_BYTES)),
            Err(V2ZipError::BadRequest(message)) if message.contains("trùng lặp")
        ));
    }

    #[test]
    fn shared_zip_reader_preserves_case_collision_rejection_for_distinct_raw_names() {
        let bytes = zip_with_members(
            &[("Assets/Cover.PNG", b"a"), ("assets/cover.png", b"b")],
            &[],
        );
        assert!(
            !ZipArchive::new(Cursor::new(bytes.as_slice()))
                .unwrap()
                .has_duplicate_names()
        );
        assert!(matches!(
            read_v2_zip_entries(&bytes, |_| Ok(V2_MAX_TEXT_BYTES)),
            Err(V2ZipError::BadRequest(message))
                if message.contains("trùng lặp") || message.contains("chữ hoa")
        ));
    }

    #[test]
    fn shared_zip_reader_accepts_distinct_files_and_ignores_safe_empty_directories() {
        let bytes = zip_with_members(
            &[("theme.json", b"{}"), ("assets/hero.png", b"png")],
            &["assets/", "assets/images/", "assets/.empty/"],
        );
        let files = read_v2_zip_entries(&bytes, |_| Ok(V2_MAX_TEXT_BYTES)).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files["theme.json"], b"{}");
        assert_eq!(files["assets/hero.png"], b"png");
        assert!(open_bounded_zip(&bytes, V2_MAX_ZIP_BYTES, "too big", "invalid").is_ok());
    }

    #[test]
    fn shared_zip_reader_rejects_malformed_and_truncated_archives() {
        assert!(matches!(
            read_v2_zip_entries(b"not a zip", |_| Ok(V2_MAX_TEXT_BYTES)),
            Err(V2ZipError::BadRequest(_))
        ));
        let mut truncated = zip_with_members(&[("theme.json", b"{}")], &[]);
        truncated.truncate(truncated.len() - 22);
        assert!(matches!(
            read_v2_zip_entries(&truncated, |_| Ok(V2_MAX_TEXT_BYTES)),
            Err(V2ZipError::BadRequest(_))
        ));
    }
}
