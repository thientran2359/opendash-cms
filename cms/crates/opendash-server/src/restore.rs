//! Validate restore, rebase to separate site root, and atomic READY publish (M8-S05-T01).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use opendash_core::backup::{
    BackupFileKind, BackupFileRecord, PortableRuntimeConfigV1, validate_logical_key,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::backup_archive::ValidatedArchive;
use crate::error::{ApiError, ApiResult};
use crate::secrets::{
    create_private_file, ensure_no_reparse_or_symlink, ensure_private_directory_tree,
};

const STREAM_BUF_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreFailpoint {
    AfterExtract,
    AfterDbSanitize,
    AfterReadyMarker,
}

#[derive(Debug, Clone)]
pub struct RestorePolicy {
    pub job_id: String,
    pub source_site_root: PathBuf,
    pub source_db_path: PathBuf,
    pub failpoint: Option<RestoreFailpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreReadyMarker {
    pub version: u32,
    pub job_id: String,
    pub schema_version: i64,
    pub db_sha256: String,
    pub file_count: usize,
    pub recovery_safe_mode: bool,
}

#[derive(Debug, Clone)]
pub struct RestoreReceipt {
    pub job_id: String,
    pub target_root: PathBuf,
    pub schema_version: i64,
    pub db_sha256: String,
    pub file_count: usize,
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

fn sha256_file(path: &Path) -> ApiResult<String> {
    let mut f = File::open(path).map_err(|_| ApiError::internal("Không mở được tệp để băm."))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|_| ApiError::internal("Lỗi đọc tệp để băm."))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn resolve_logical_dest(stage_root: &Path, record: &BackupFileRecord) -> ApiResult<PathBuf> {
    validate_logical_key(record.kind, &record.logical_key).map_err(ApiError::bad_request)?;
    let segments: Vec<&str> = record.logical_key.split('/').collect();

    match record.kind {
        BackupFileKind::Db => Ok(stage_root.join("database").join("opendash.sqlite")),
        BackupFileKind::PortableConfig => Ok(stage_root.join("config").join("site.json")),
        BackupFileKind::MediaOriginal | BackupFileKind::MediaThumbnail => {
            let mut p = stage_root.join("uploads");
            for seg in segments {
                p.push(seg);
            }
            Ok(p)
        }
        BackupFileKind::QuarantineObject => {
            let mut p = stage_root.join("uploads").join(".quarantine");
            for seg in segments.into_iter().skip(1) {
                p.push(seg);
            }
            Ok(p)
        }
        BackupFileKind::ThemePackageFile | BackupFileKind::PluginPackageFile => {
            let mut p = stage_root.to_path_buf();
            for seg in segments {
                p.push(seg);
            }
            Ok(p)
        }
        BackupFileKind::SmtpSecret => {
            let mut p = stage_root.to_path_buf();
            for seg in segments {
                p.push(seg);
            }
            Ok(p)
        }
        BackupFileKind::ExtensionSecret => {
            let mut p = stage_root.join("database");
            for seg in segments {
                p.push(seg);
            }
            Ok(p)
        }
        BackupFileKind::LanguagePack => {
            let mut p = stage_root.to_path_buf();
            for seg in segments {
                p.push(seg);
            }
            Ok(p)
        }
    }
}

pub async fn restore_to_new_root(
    validated: &ValidatedArchive,
    policy: &RestorePolicy,
    generated_target: &Path,
) -> ApiResult<RestoreReceipt> {
    if generated_target.exists() {
        return Err(ApiError::conflict("Thư mục đích khôi phục đã tồn tại."));
    }
    if generated_target == policy.source_site_root
        || generated_target == policy.source_db_path
        || policy.source_db_path.starts_with(generated_target)
    {
        return Err(ApiError::bad_request(
            "Thư mục đích khôi phục không được trùng hoặc chứa site nguồn.",
        ));
    }

    let target_parent = generated_target
        .parent()
        .ok_or_else(|| ApiError::bad_request("Đường dẫn đích khôi phục không hợp lệ."))?;
    ensure_private_directory_tree(target_parent)?;
    ensure_no_reparse_or_symlink(target_parent)?;

    let stage_root = target_parent.join(format!(".restore-{}", policy.job_id));
    if stage_root.exists() {
        return Err(ApiError::conflict("Thư mục staging khôi phục đã tồn tại."));
    }
    ensure_private_directory_tree(&stage_root)?;

    let mut seen_db = false;
    let mut seen_config = false;
    let mut buf = vec![0u8; STREAM_BUF_BYTES];

    for record in &validated.manifest.files {
        let src_path = validated.payload_dir.join(&record.entry_name);
        let dest_path = resolve_logical_dest(&stage_root, record)?;
        if let Some(parent) = dest_path.parent() {
            if record.kind == BackupFileKind::ExtensionSecret {
                let db_dir = stage_root.join("database");
                let sec_dir = db_dir.join("secrets");
                ensure_private_directory_tree(&db_dir)?;
                ensure_private_directory_tree(&sec_dir)?;
                if let Some(subdir_dir) = parent.parent() {
                    ensure_private_directory_tree(subdir_dir)?;
                }
            }
            ensure_private_directory_tree(parent)?;
        }

        if record.kind == BackupFileKind::Db {
            seen_db = true;
        }
        if record.kind == BackupFileKind::PortableConfig {
            seen_config = true;
            let raw = fs::read(&src_path)
                .map_err(|_| ApiError::internal("Không đọc được portable config từ backup."))?;
            let mut cfg: PortableRuntimeConfigV1 = serde_json::from_slice(&raw).map_err(|_| {
                ApiError::bad_request("config/site.json trong backup không hợp lệ.")
            })?;
            cfg.validate().map_err(ApiError::bad_request)?;
            cfg.recovery_safe_mode = true;
            let cfg_bytes = serde_json::to_vec_pretty(&cfg)
                .map_err(|_| ApiError::internal("Không tuần tự hóa được config khôi phục."))?;
            let mut dst = create_private_file(&dest_path)?;
            dst.write_all(&cfg_bytes)
                .map_err(|_| ApiError::internal("Không ghi được config khôi phục."))?;
            dst.sync_all()
                .map_err(|_| ApiError::internal("Không fsync được config khôi phục."))?;
            continue;
        }

        let mut src = File::open(&src_path)
            .map_err(|_| ApiError::internal("Không mở được payload khôi phục."))?;
        let mut dst = create_private_file(&dest_path)?;
        let mut hasher = Sha256::new();
        let mut actual_size = 0u64;
        loop {
            let n = src
                .read(&mut buf)
                .map_err(|_| ApiError::internal("Lỗi đọc payload khôi phục."))?;
            if n == 0 {
                break;
            }
            actual_size += n as u64;
            hasher.update(&buf[..n]);
            dst.write_all(&buf[..n])
                .map_err(|_| ApiError::internal("Lỗi ghi payload khôi phục."))?;
        }
        dst.sync_all()
            .map_err(|_| ApiError::internal("Lỗi fsync payload khôi phục."))?;
        drop(dst);
        if record.kind == BackupFileKind::ExtensionSecret {
            ensure_no_reparse_or_symlink(&dest_path)?;
            crate::secrets::verify_file_acl(&dest_path)?;
        }

        if actual_size != record.size || hex_lower(&hasher.finalize()) != record.sha256 {
            return Err(ApiError::bad_request(
                "Payload khôi phục không khớp manifest.",
            ));
        }
        if record.kind == BackupFileKind::LanguagePack {
            let raw = fs::read(&dest_path).map_err(|_| {
                ApiError::internal(String::from("Không đọc lại được tệp ngôn ngữ khôi phục."))
            })?;
            let is_po = record.logical_key.ends_with(".po");
            crate::l10n::registry::validate_language_pack_bytes(is_po, &raw)
                .map_err(|err| ApiError::bad_request(err.message))?;
        }
    }

    if !seen_db {
        return Err(ApiError::bad_request(
            "Backup thiếu database/opendash.sqlite.",
        ));
    }
    if !seen_config {
        let cfg_dir = stage_root.join("config");
        ensure_private_directory_tree(&cfg_dir)?;
        let mut cfg = PortableRuntimeConfigV1::default();
        cfg.recovery_safe_mode = true;
        let cfg_bytes = serde_json::to_vec_pretty(&cfg).unwrap();
        let mut dst = create_private_file(&cfg_dir.join("site.json"))?;
        dst.write_all(&cfg_bytes)
            .map_err(|_| ApiError::internal("Không ghi được config mặc định."))?;
        dst.sync_all()
            .map_err(|_| ApiError::internal("Không fsync được config mặc định."))?;
    }

    // A restored root must never be published with the source site's
    // recovery_safe_mode=false. The startup loader deliberately accepts false
    // *after* authenticated activation, so the restore worker must guarantee
    // the initial on-disk state before publishing READY.json.
    let staged_paths = crate::paths::SitePaths::from_parts(
        stage_root.clone(),
        stage_root.join("database/opendash.sqlite"),
    );
    if !crate::config::load_portable_config(&staged_paths)?.recovery_safe_mode {
        return Err(ApiError::internal(
            "Từ chối publish restore: config/site.json chưa bật Safe Mode.",
        ));
    }

    if policy.failpoint == Some(RestoreFailpoint::AfterExtract) {
        return Err(ApiError::internal("Simulated failpoint: AfterExtract"));
    }

    let stage_db_path = stage_root.join("database").join("opendash.sqlite");
    let final_schema_version =
        crate::storage::restore::validate_and_rebase_copy(&stage_db_path).await?;
    crate::storage::restore::verify_restored_db_references(&stage_db_path, &stage_root).await?;

    if policy.failpoint == Some(RestoreFailpoint::AfterDbSanitize) {
        return Err(ApiError::internal("Simulated failpoint: AfterDbSanitize"));
    }

    let db_sha256 = sha256_file(&stage_db_path)?;
    let marker = RestoreReadyMarker {
        version: 1,
        job_id: policy.job_id.clone(),
        schema_version: final_schema_version,
        db_sha256: db_sha256.clone(),
        file_count: validated.manifest.files.len(),
        recovery_safe_mode: true,
    };
    let marker_bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|_| ApiError::internal("Không tuần tự hóa được READY.json."))?;
    let mut ready_file = create_private_file(&stage_root.join("READY.json"))?;
    ready_file
        .write_all(&marker_bytes)
        .map_err(|_| ApiError::internal("Không ghi được READY.json."))?;
    ready_file
        .sync_all()
        .map_err(|_| ApiError::internal("Không fsync được READY.json."))?;
    drop(ready_file);

    if policy.failpoint == Some(RestoreFailpoint::AfterReadyMarker) {
        return Err(ApiError::internal("Simulated failpoint: AfterReadyMarker"));
    }

    if generated_target.exists() {
        return Err(ApiError::conflict(
            "Thư mục đích khôi phục xuất hiện trước bước publish.",
        ));
    }
    fs::rename(&stage_root, generated_target)
        .map_err(|_| ApiError::internal("Không thể publish thư mục khôi phục."))?;

    Ok(RestoreReceipt {
        job_id: policy.job_id.clone(),
        target_root: generated_target.to_path_buf(),
        schema_version: final_schema_version,
        db_sha256,
        file_count: validated.manifest.files.len(),
    })
}
