//! Encrypted backup archive codec and bounded verifier (M8-S04-T01).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use age::secrecy::{ExposeSecret, SecretString};
use opendash_core::backup::{
    BackupManifestV1, MAX_BACKUP_DECRYPTED_ZIP_BYTES, MAX_BACKUP_MANIFEST_BYTES,
    MAX_BACKUP_SINGLE_FILE_BYTES, validate_manifest_v1,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::backup::SnapshotReceipt;
use crate::error::{ApiError, ApiResult};
use crate::package::{ensure_safe_flat_entry, preflight_and_open_backup_zip, read_bounded_entry};
use crate::secrets::{
    create_private_file, ensure_no_reparse_or_symlink, ensure_private_directory_tree,
};

pub const MAX_ENCRYPTED_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024 + 2 * 1024 * 1024;
pub const MAX_AGE_HEADER_BYTES: usize = 8 * 1024;
pub const BACKUP_SCRYPT_WORK_FACTOR: u8 = 16;
pub const MIN_PASSPHRASE_SCALARS: usize = 16;
pub const MAX_PASSPHRASE_SCALARS: usize = 128;
pub const MAX_PASSPHRASE_UTF8_BYTES: usize = 512;
const STREAM_BUF_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct ArtifactReceipt {
    pub archive_path: PathBuf,
    pub archive_size: u64,
    pub archive_sha256: String,
    pub manifest: BackupManifestV1,
}

#[derive(Debug, Clone)]
pub struct ValidatedArchive {
    pub zip_path: PathBuf,
    pub payload_dir: PathBuf,
    pub manifest: BackupManifestV1,
}

pub fn validate_passphrase(passphrase: &SecretString) -> ApiResult<()> {
    let raw = passphrase.expose_secret();
    let scalar_count = raw.chars().count();
    if !(MIN_PASSPHRASE_SCALARS..=MAX_PASSPHRASE_SCALARS).contains(&scalar_count) {
        return Err(ApiError::bad_request(
            "Mật khẩu backup phải có từ 16 đến 128 ký tự Unicode.",
        ));
    }
    if raw.len() > MAX_PASSPHRASE_UTF8_BYTES {
        return Err(ApiError::bad_request(
            "Mật khẩu backup vượt quá 512 byte UTF-8.",
        ));
    }
    Ok(())
}

struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
    written: u64,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            written: 0,
        }
    }

    fn finish(self) -> (W, String, u64) {
        let digest = hex_lower(&self.hasher.finalize());
        (self.inner, digest, self.written)
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn write_encrypted_archive(
    snapshot: &SnapshotReceipt,
    passphrase: &SecretString,
    out_path: &Path,
) -> ApiResult<ArtifactReceipt> {
    validate_passphrase(passphrase)?;
    validate_manifest_v1(&snapshot.manifest).map_err(ApiError::bad_request)?;

    ensure_no_reparse_or_symlink(&snapshot.snapshot_dir)?;
    ensure_no_reparse_or_symlink(&snapshot.manifest_path)?;

    let out_parent = out_path
        .parent()
        .ok_or_else(|| ApiError::bad_request("Đường dẫn tệp backup đầu ra không hợp lệ."))?;
    ensure_private_directory_tree(out_parent)?;
    ensure_no_reparse_or_symlink(out_path)?;

    if out_path.exists() {
        return Err(ApiError::conflict("Tệp backup đầu ra đã tồn tại."));
    }

    let staging_zip_path = snapshot.snapshot_dir.join(".archive-staging.zip");
    let part_path = out_parent.join(format!(
        ".{}.part",
        out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("backup.odbackup.age")
    ));

    let res = write_encrypted_archive_inner(
        snapshot,
        passphrase,
        &staging_zip_path,
        &part_path,
        out_path,
    );
    if res.is_err() {
        let _ = fs::remove_file(&part_path);
        let _ = fs::remove_file(&staging_zip_path);
    } else {
        let _ = fs::remove_file(&staging_zip_path);
    }
    res
}

fn write_encrypted_archive_inner(
    snapshot: &SnapshotReceipt,
    passphrase: &SecretString,
    staging_zip_path: &Path,
    part_path: &Path,
    out_path: &Path,
) -> ApiResult<ArtifactReceipt> {
    let zip_file = create_private_file(staging_zip_path)?;
    let mut zip = ZipWriter::new(BufWriter::new(zip_file));
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(false);

    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    for record in &snapshot.manifest.files {
        let src_path = snapshot.snapshot_dir.join(&record.entry_name);
        ensure_no_reparse_or_symlink(&src_path)?;
        let mut src = File::open(&src_path)
            .map_err(|_| ApiError::internal("Không mở được payload snapshot để đóng gói."))?;
        zip.start_file(&record.entry_name, opts)
            .map_err(|_| ApiError::internal("Không ghi được entry payload vào backup ZIP."))?;

        let mut hasher = Sha256::new();
        let mut actual_size = 0u64;
        loop {
            let n = src
                .read(&mut buf)
                .map_err(|_| ApiError::internal("Lỗi đọc payload snapshot."))?;
            if n == 0 {
                break;
            }
            actual_size = actual_size
                .checked_add(n as u64)
                .ok_or_else(|| ApiError::internal("Tràn kích thước payload."))?;
            hasher.update(&buf[..n]);
            zip.write_all(&buf[..n])
                .map_err(|_| ApiError::internal("Lỗi ghi payload vào backup ZIP."))?;
        }
        let actual_sha = hex_lower(&hasher.finalize());
        if actual_size != record.size || actual_sha != record.sha256 {
            return Err(ApiError::conflict(
                "Payload snapshot thay đổi hoặc không khớp manifest.",
            ));
        }
    }

    let manifest_bytes = fs::read(&snapshot.manifest_path)
        .map_err(|_| ApiError::internal("Không đọc được manifest.json của snapshot."))?;
    if manifest_bytes.len() > MAX_BACKUP_MANIFEST_BYTES {
        return Err(ApiError::bad_request("manifest.json vượt quá 8 MiB."));
    }
    zip.start_file("manifest.json", opts)
        .map_err(|_| ApiError::internal("Không ghi được manifest.json vào backup ZIP."))?;
    zip.write_all(&manifest_bytes)
        .map_err(|_| ApiError::internal("Lỗi ghi manifest.json vào backup ZIP."))?;

    let mut zip_writer_inner = zip
        .finish()
        .map_err(|_| ApiError::internal("Không hoàn tất được backup ZIP."))?;
    zip_writer_inner
        .flush()
        .map_err(|_| ApiError::internal("Lỗi flush backup ZIP."))?;
    let zip_file_handle = zip_writer_inner
        .into_inner()
        .map_err(|_| ApiError::internal("Lỗi hoàn tất buffer backup ZIP."))?;
    zip_file_handle
        .sync_all()
        .map_err(|_| ApiError::internal("Lỗi fsync backup ZIP."))?;
    let zip_len = zip_file_handle
        .metadata()
        .map_err(|_| ApiError::internal("Không đọc được kích thước backup ZIP."))?
        .len();
    drop(zip_file_handle);

    if zip_len > MAX_BACKUP_DECRYPTED_ZIP_BYTES {
        return Err(ApiError::bad_request(
            "Tổng dung lượng backup ZIP vượt quá 4 GiB.",
        ));
    }

    let part_file = create_private_file(part_path)?;
    let hashing_writer = HashingWriter::new(BufWriter::new(part_file));

    let mut recipient = age::scrypt::Recipient::new(passphrase.clone());
    recipient.set_work_factor(BACKUP_SCRYPT_WORK_FACTOR);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .map_err(|_| ApiError::internal("Không khởi tạo được age encryptor."))?;
    let mut age_writer = encryptor
        .wrap_output(hashing_writer)
        .map_err(|_| ApiError::internal("Không mở được luồng mã hóa age."))?;

    let mut zip_src = File::open(staging_zip_path)
        .map_err(|_| ApiError::internal("Không mở lại được backup ZIP để mã hóa."))?;
    loop {
        let n = zip_src
            .read(&mut buf)
            .map_err(|_| ApiError::internal("Lỗi đọc backup ZIP để mã hóa."))?;
        if n == 0 {
            break;
        }
        age_writer
            .write_all(&buf[..n])
            .map_err(|_| ApiError::internal("Lỗi ghi luồng mã hóa age."))?;
    }
    drop(zip_src);

    let mut finished_hash_writer = age_writer
        .finish()
        .map_err(|_| ApiError::internal("Không hoàn tất được thẻ xác thực age AEAD."))?;
    finished_hash_writer
        .flush()
        .map_err(|_| ApiError::internal("Lỗi flush tệp mã hóa age."))?;
    let (buf_writer, archive_sha256, archive_size) = finished_hash_writer.finish();
    let part_file_handle = buf_writer
        .into_inner()
        .map_err(|_| ApiError::internal("Lỗi hoàn tất buffer tệp mã hóa age."))?;
    part_file_handle
        .sync_all()
        .map_err(|_| ApiError::internal("Lỗi fsync tệp mã hóa age."))?;
    drop(part_file_handle);

    fs::rename(part_path, out_path)
        .map_err(|_| ApiError::internal("Không đổi tên hoàn tất tệp backup đã mã hóa."))?;

    Ok(ArtifactReceipt {
        archive_path: out_path.to_path_buf(),
        archive_size,
        archive_sha256,
        manifest: snapshot.manifest.clone(),
    })
}

pub fn preflight_age_header_file(file: &mut File) -> ApiResult<()> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ApiError::bad_request("Không đọc được header gói backup."))?;
    let mut header_buf = vec![0u8; MAX_AGE_HEADER_BYTES + 1];
    let mut total = 0usize;
    while total < header_buf.len() {
        let n = file
            .read(&mut header_buf[total..])
            .map_err(|_| ApiError::bad_request("Không đọc được header gói backup."))?;
        if n == 0 {
            break;
        }
        total += n;
    }
    let slice = &header_buf[..total];
    let mac_line_start = slice
        .windows(5)
        .position(|w| w == b"\n--- ")
        .map(|idx| idx + 1)
        .ok_or_else(|| {
            ApiError::bad_request("Header age của gói backup không hợp lệ hoặc vượt quá 8 KiB.")
        })?;
    let line_end = slice[mac_line_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| mac_line_start + p)
        .ok_or_else(|| {
            ApiError::bad_request("Header age của gói backup không hợp lệ hoặc vượt quá 8 KiB.")
        })?;
    if line_end > MAX_AGE_HEADER_BYTES {
        return Err(ApiError::bad_request(
            "Header age của gói backup vượt quá 8 KiB.",
        ));
    }

    let header_str = std::str::from_utf8(&slice[..line_end])
        .map_err(|_| ApiError::bad_request("Header age không phải UTF-8 hợp lệ."))?;
    let mut lines = header_str.lines();
    if lines.next() != Some("age-encryption.org/v1") {
        return Err(ApiError::bad_request("Phiên bản header age không hợp lệ."));
    }

    let stanza_line = lines
        .next()
        .ok_or_else(|| ApiError::bad_request("Thiếu stanza scrypt trong header age."))?;
    let parts: Vec<&str> = stanza_line.split(' ').collect();
    if parts.len() != 4 || parts[0] != "->" || parts[1] != "scrypt" {
        return Err(ApiError::bad_request(
            "Gói backup chỉ hỗ trợ đúng một stanza scrypt.",
        ));
    }
    let salt_b64 = parts[2];
    if salt_b64.is_empty()
        || salt_b64.len() > 32
        || !salt_b64
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
    {
        return Err(ApiError::bad_request(
            "Salt scrypt trong header age không hợp lệ.",
        ));
    }
    let log_n: u8 = parts[3]
        .parse()
        .map_err(|_| ApiError::bad_request("Work factor scrypt không hợp lệ."))?;
    if log_n == 0 || log_n > BACKUP_SCRYPT_WORK_FACTOR {
        return Err(ApiError::bad_request(
            "Work factor scrypt vượt quá giới hạn cho phép.",
        ));
    }

    let body_line = lines
        .next()
        .ok_or_else(|| ApiError::bad_request("Thiếu thân stanza scrypt trong header age."))?;
    if body_line.is_empty()
        || body_line.len() > 128
        || body_line.starts_with("->")
        || body_line.starts_with("---")
    {
        return Err(ApiError::bad_request("Thân stanza scrypt không hợp lệ."));
    }

    let mac_line = lines
        .next()
        .ok_or_else(|| ApiError::bad_request("Thiếu dòng MAC trong header age."))?;
    if !mac_line.starts_with("--- ") || lines.next().is_some() {
        return Err(ApiError::bad_request(
            "Header age chứa nhiều stanza hoặc cấu trúc không hợp lệ.",
        ));
    }

    file.seek(SeekFrom::Start(0))
        .map_err(|_| ApiError::bad_request("Không tua lại được gói backup."))?;
    Ok(())
}

pub fn decrypt_and_validate_archive(
    input_path: &Path,
    passphrase: &SecretString,
    private_dir: &Path,
) -> ApiResult<ValidatedArchive> {
    validate_passphrase(passphrase)?;
    ensure_no_reparse_or_symlink(input_path)?;
    ensure_private_directory_tree(private_dir)?;

    let meta = fs::metadata(input_path)
        .map_err(|_| ApiError::bad_request("Không đọc được thông tin gói backup."))?;
    if !meta.is_file() || meta.len() > MAX_ENCRYPTED_ARCHIVE_BYTES {
        return Err(ApiError::bad_request(
            "Kích thước gói backup không hợp lệ hoặc vượt quá giới hạn.",
        ));
    }

    let zip_path = private_dir.join("archive.zip");
    let payload_dir = private_dir.join("payloads");

    let res = decrypt_and_validate_archive_inner(input_path, passphrase, &zip_path, &payload_dir);
    if res.is_err() {
        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&payload_dir);
    }
    res
}

fn decrypt_and_validate_archive_inner(
    input_path: &Path,
    passphrase: &SecretString,
    zip_path: &Path,
    payload_dir: &Path,
) -> ApiResult<ValidatedArchive> {
    let mut enc_file =
        File::open(input_path).map_err(|_| ApiError::bad_request("Không mở được gói backup."))?;
    preflight_age_header_file(&mut enc_file)?;

    let decryptor = age::Decryptor::new_buffered(BufReader::new(enc_file))
        .map_err(|_| ApiError::bad_request("Không thể giải mã hoặc xác thực gói backup."))?;
    let mut identity = age::scrypt::Identity::new(passphrase.clone());
    identity.set_max_work_factor(BACKUP_SCRYPT_WORK_FACTOR);

    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_| ApiError::bad_request("Không thể giải mã hoặc xác thực gói backup."))?;

    let mut zip_file = create_private_file(zip_path)?;
    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    let mut total_zip_bytes = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|_| ApiError::bad_request("Không thể giải mã hoặc xác thực gói backup."))?;
        if n == 0 {
            break;
        }
        total_zip_bytes = total_zip_bytes
            .checked_add(n as u64)
            .ok_or_else(|| ApiError::bad_request("Gói backup giải mã vượt quá 4 GiB."))?;
        if total_zip_bytes > MAX_BACKUP_DECRYPTED_ZIP_BYTES {
            return Err(ApiError::bad_request("Gói backup giải mã vượt quá 4 GiB."));
        }
        zip_file
            .write_all(&buf[..n])
            .map_err(|_| ApiError::internal("Không ghi được ZIP giải mã vào staging."))?;
    }
    zip_file
        .sync_all()
        .map_err(|_| ApiError::internal("Không fsync được ZIP giải mã."))?;
    drop(zip_file);

    let zip_read_file =
        File::open(zip_path).map_err(|_| ApiError::internal("Không mở lại được ZIP giải mã."))?;
    let mut archive =
        preflight_and_open_backup_zip(zip_read_file).map_err(ApiError::bad_request)?;

    let manifest = {
        let mut mf = archive
            .by_name("manifest.json")
            .map_err(|_| ApiError::bad_request("Backup ZIP thiếu manifest.json."))?;
        let _ = ensure_safe_flat_entry(
            &mf,
            "Tên manifest.json không hợp lệ.",
            "manifest.json không được là symlink.",
            "manifest.json không được là thư mục.",
        )
        .map_err(ApiError::bad_request)?;
        let raw = read_bounded_entry(
            &mut mf,
            MAX_BACKUP_MANIFEST_BYTES,
            "Không đọc được manifest.json trong backup.",
        )
        .map_err(ApiError::bad_request)?;
        if raw.len() > MAX_BACKUP_MANIFEST_BYTES {
            return Err(ApiError::bad_request("manifest.json vượt quá 8 MiB."));
        }
        let parsed: BackupManifestV1 = serde_json::from_slice(&raw)
            .map_err(|_| ApiError::bad_request("manifest.json trong backup không hợp lệ."))?;
        validate_manifest_v1(&parsed).map_err(ApiError::bad_request)?;
        parsed
    };

    if archive.len() != manifest.files.len() + 1 {
        return Err(ApiError::bad_request(
            "Số lượng entry trong backup ZIP không khớp manifest.json.",
        ));
    }

    ensure_private_directory_tree(payload_dir)?;
    let expected_by_entry: HashMap<&str, &opendash_core::backup::BackupFileRecord> = manifest
        .files
        .iter()
        .map(|r| (r.entry_name.as_str(), r))
        .collect();
    if expected_by_entry.len() != manifest.files.len() {
        return Err(ApiError::bad_request(
            "manifest.json chứa entry_name trùng lặp.",
        ));
    }

    let mut seen_entries = HashSet::new();
    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|_| ApiError::bad_request("Không đọc được entry trong backup ZIP."))?;
        let name = ensure_safe_flat_entry(
            &entry,
            "Tên entry backup không hợp lệ.",
            "Entry backup không được là symlink.",
            "Entry backup không được là thư mục.",
        )
        .map_err(ApiError::bad_request)?;
        if !seen_entries.insert(name.clone()) {
            return Err(ApiError::bad_request("Backup ZIP chứa entry trùng lặp."));
        }
        if name == "manifest.json" {
            continue;
        }

        let record = expected_by_entry.get(name.as_str()).ok_or_else(|| {
            ApiError::bad_request("Backup ZIP chứa entry thừa không có trong manifest.")
        })?;

        let dest_path = payload_dir.join(&record.entry_name);
        let mut dest_file = create_private_file(&dest_path)?;
        let mut hasher = Sha256::new();
        let mut actual_size = 0u64;
        loop {
            let n = entry
                .read(&mut buf)
                .map_err(|_| ApiError::bad_request("Lỗi đọc payload từ backup ZIP."))?;
            if n == 0 {
                break;
            }
            actual_size = actual_size
                .checked_add(n as u64)
                .ok_or_else(|| ApiError::bad_request("Kích thước payload bị tràn."))?;
            if actual_size > MAX_BACKUP_SINGLE_FILE_BYTES {
                return Err(ApiError::bad_request(
                    "Payload backup vượt quá giới hạn 512 MiB.",
                ));
            }
            hasher.update(&buf[..n]);
            dest_file
                .write_all(&buf[..n])
                .map_err(|_| ApiError::internal("Không ghi được payload vào staging."))?;
        }
        dest_file
            .sync_all()
            .map_err(|_| ApiError::internal("Không fsync được payload vào staging."))?;

        let actual_sha = hex_lower(&hasher.finalize());
        if actual_size != record.size || actual_sha != record.sha256 {
            return Err(ApiError::bad_request(
                "Checksum hoặc kích thước payload không khớp manifest.json.",
            ));
        }
    }

    Ok(ValidatedArchive {
        zip_path: zip_path.to_path_buf(),
        payload_dir: payload_dir.to_path_buf(),
        manifest,
    })
}
