//! Pure canonical backup manifest v1 DTOs, portable config v1, and domain logical-key validation.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub const BACKUP_FORMAT_VERSION: u32 = 1;
pub const RESTORE_POLICY_VERSION: u32 = 1;
pub const MAX_BACKUP_PAYLOADS: usize = 20_000;
pub const MAX_BACKUP_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_BACKUP_DECRYPTED_ZIP_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_BACKUP_DB_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_BACKUP_SINGLE_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortableRuntimeConfigV1 {
    pub version: u32,
    pub listen_ip: String,
    pub port: u16,
    pub trusted_public_origin: Option<String>,
    pub log_level: String,
    pub recovery_safe_mode: bool,
}

impl Default for PortableRuntimeConfigV1 {
    fn default() -> Self {
        Self {
            version: 1,
            listen_ip: "127.0.0.1".to_string(),
            port: 18766,
            trusted_public_origin: None,
            log_level: "info".to_string(),
            recovery_safe_mode: false,
        }
    }
}

impl PortableRuntimeConfigV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 {
            return Err("Phiên bản config/site.json không được hỗ trợ.");
        }
        if self.listen_ip != "127.0.0.1" && self.listen_ip != "::1" {
            return Err("listen_ip chỉ cho phép 127.0.0.1 hoặc ::1.");
        }
        if self.port == 0 {
            return Err("port phải khác 0.");
        }
        if !matches!(
            self.log_level.as_str(),
            "error" | "warn" | "info" | "debug" | "trace"
        ) {
            return Err("log_level không hợp lệ.");
        }
        if let Some(origin) = &self.trusted_public_origin {
            canonicalize_trusted_origin(origin)?;
        }
        Ok(())
    }
}

/// Parses and canonicalizes `trusted_public_origin` as `scheme://host[:non_default_port]`.
/// Accepts an optional single trailing `/` (e.g. `https://cms.example.com/`), normalizes
/// scheme/host casing and omits default `:80`/`:443` ports, and rejects credentials,
/// non-root paths, query strings, and fragments.
pub fn canonicalize_trusted_origin(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("trusted_public_origin không phải URL hợp lệ.");
    }
    let parsed =
        url::Url::parse(trimmed).map_err(|_| "trusted_public_origin không phải URL hợp lệ.")?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("trusted_public_origin phải dùng http/https và có host.");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("trusted_public_origin không được chứa thông tin xác thực.");
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err("trusted_public_origin không được chứa path.");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("trusted_public_origin không được chứa query hoặc fragment.");
    }
    let host = parsed
        .host_str()
        .ok_or("trusted_public_origin phải dùng http/https và có host.")?;
    let authority = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    Ok(format!("{}://{authority}", parsed.scheme()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupFileKind {
    Db,
    MediaOriginal,
    MediaThumbnail,
    ThemePackageFile,
    PluginPackageFile,
    SmtpSecret,
    ExtensionSecret,
    QuarantineObject,
    PortableConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupFileRecord {
    pub entry_name: String,
    pub kind: BackupFileKind,
    pub logical_key: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifestV1 {
    pub format_version: u32,
    pub app_version: String,
    pub source_sha: String,
    pub schema_version: i64,
    pub migration_checksums: Vec<(i64, String)>,
    pub created_at: String,
    pub files: Vec<BackupFileRecord>,
    pub restore_policy_version: u32,
}

pub fn format_flat_entry_name(index_one_based: usize) -> Result<String, &'static str> {
    if index_one_based == 0 || index_one_based > MAX_BACKUP_PAYLOADS {
        return Err("Chỉ số entry vượt quá giới hạn 20.000 payload.");
    }
    Ok(format!("{index_one_based:08}.bin"))
}

pub fn is_valid_flat_entry_name(name: &str) -> bool {
    name.len() == 12
        && name.ends_with(".bin")
        && name[..8].bytes().all(|b| b.is_ascii_digit())
        && &name[..8] != "00000000"
}

pub fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_windows_reserved_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

pub fn validate_safe_relative_key(key: &str) -> Result<(), &'static str> {
    if key.is_empty() || key.len() > 512 || !key.is_ascii() {
        return Err("Logical key phải là ASCII khác rỗng và <= 512 bytes.");
    }
    if key.starts_with('/') || key.ends_with('/') || key.contains('\\') || key.contains(':') {
        return Err("Logical key chứa ký tự đường dẫn không an toàn.");
    }
    for seg in key.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." || seg.ends_with(' ') || seg.ends_with('.') {
            return Err("Logical key chứa thành phần đường dẫn không hợp lệ.");
        }
        if seg.bytes().any(|b| b < 0x20 || b == 0x7f) || is_windows_reserved_name(seg) {
            return Err("Logical key chứa tên dành riêng hoặc ký tự điều khiển.");
        }
    }
    Ok(())
}

pub fn validate_logical_key(kind: BackupFileKind, key: &str) -> Result<(), &'static str> {
    validate_safe_relative_key(key)?;
    match kind {
        BackupFileKind::Db => {
            if key == "db/main.sqlite" {
                Ok(())
            } else {
                Err("DB logical_key phải là db/main.sqlite.")
            }
        }
        BackupFileKind::PortableConfig => {
            if key == "config/site.json" {
                Ok(())
            } else {
                Err("Config logical_key phải là config/site.json.")
            }
        }
        BackupFileKind::MediaOriginal => {
            let rest = key
                .strip_prefix("media/")
                .ok_or("Media original key phải bắt đầu bằng media/.")?;
            let (shard, fname) = rest
                .split_once('/')
                .ok_or("Media original key thiếu shard.")?;
            let (id, ext) = fname
                .rsplit_once('.')
                .ok_or("Media original key thiếu phần mở rộng.")?;
            if shard.len() == 2
                && is_valid_sha256_hex(id)
                && id.starts_with(shard)
                && matches!(ext, "jpg" | "png" | "webp" | "mp4")
            {
                Ok(())
            } else {
                Err("Media original logical_key không hợp lệ.")
            }
        }
        BackupFileKind::MediaThumbnail => {
            let rest = key
                .strip_prefix("media/")
                .ok_or("Media thumbnail key phải bắt đầu bằng media/.")?;
            let (shard, fname) = rest
                .split_once('/')
                .ok_or("Media thumbnail key thiếu shard.")?;
            let id = fname
                .strip_suffix(".thumb.png")
                .ok_or("Media thumbnail key phải kết thúc bằng .thumb.png.")?;
            if shard.len() == 2 && is_valid_sha256_hex(id) && id.starts_with(shard) {
                Ok(())
            } else {
                Err("Media thumbnail logical_key không hợp lệ.")
            }
        }
        BackupFileKind::SmtpSecret => {
            let rest = key
                .strip_prefix("secrets/smtp/")
                .ok_or("Secret logical_key phải bắt đầu bằng secrets/smtp/.")?;
            let stem = rest
                .strip_suffix(".json")
                .or_else(|| rest.strip_suffix(".bin"))
                .ok_or("Secret logical_key phải có đuôi .json hoặc .bin.")?;
            if is_valid_sha256_hex(stem) {
                Ok(())
            } else {
                Err("Secret logical_key phải là 64-hex.")
            }
        }
        BackupFileKind::ExtensionSecret => {
            let rest = key
                .strip_prefix("secrets/")
                .ok_or("Extension secret logical_key phải bắt đầu bằng secrets/.")?;
            let (subdir, tail) = rest
                .split_once('/')
                .ok_or("Extension secret logical_key thiếu thư mục sở hữu.")?;
            if !matches!(subdir, "plugins" | "themes") {
                return Err("Extension secret chỉ hỗ trợ secrets/plugins/ hoặc secrets/themes/.");
            }
            let (package_id, fname) = tail
                .split_once('/')
                .ok_or("Extension secret logical_key thiếu package_id.")?;
            if fname.contains('/') || crate::id::validate_package_id(package_id).is_err() {
                return Err(
                    "Extension secret logical_key có package_id hoặc tên tệp không hợp lệ.",
                );
            }
            let stem = fname
                .strip_suffix(".json")
                .ok_or("Extension secret logical_key phải có đuôi .json.")?;
            if is_valid_sha256_hex(stem) {
                Ok(())
            } else {
                Err("Extension secret logical_key phải là 64-hex.")
            }
        }
        BackupFileKind::QuarantineObject => {
            let rest = key
                .strip_prefix("quarantine/")
                .ok_or("Quarantine logical_key phải bắt đầu bằng quarantine/.")?;
            let (job_id, fname) = rest
                .split_once('/')
                .ok_or("Quarantine logical_key thiếu job_id.")?;
            if !job_id.is_empty()
                && job_id.len() <= 128
                && job_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                && !fname.contains('/')
            {
                Ok(())
            } else {
                Err("Quarantine logical_key không hợp lệ.")
            }
        }
        BackupFileKind::ThemePackageFile => {
            if key.starts_with("themes/") && key.split('/').count() >= 4 {
                Ok(())
            } else {
                Err("Theme logical_key không hợp lệ.")
            }
        }
        BackupFileKind::PluginPackageFile => {
            if key.starts_with("plugins/") && key.split('/').count() >= 4 {
                Ok(())
            } else {
                Err("Plugin logical_key không hợp lệ.")
            }
        }
    }
}

pub fn validate_manifest_v1(manifest: &BackupManifestV1) -> Result<(), &'static str> {
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err("format_version không được hỗ trợ.");
    }
    if manifest.restore_policy_version != RESTORE_POLICY_VERSION {
        return Err("restore_policy_version không được hỗ trợ.");
    }
    if manifest.files.len() > MAX_BACKUP_PAYLOADS {
        return Err("Số lượng tệp trong manifest vượt quá 20.000.");
    }

    let mut total_bytes: u64 = 0;
    let mut seen_entries: HashSet<String> = HashSet::with_capacity(manifest.files.len());
    let mut seen_keys_ci: HashSet<String> = HashSet::with_capacity(manifest.files.len());
    let mut has_db = false;
    let mut prev_key: Option<&str> = None;

    for rec in &manifest.files {
        if !is_valid_flat_entry_name(&rec.entry_name) {
            return Err("entry_name trong manifest không hợp lệ.");
        }
        if !seen_entries.insert(rec.entry_name.clone()) {
            return Err("entry_name bị trùng lặp trong manifest.");
        }
        if !is_valid_sha256_hex(&rec.sha256) {
            return Err("sha256 trong manifest không hợp lệ.");
        }
        validate_logical_key(rec.kind, &rec.logical_key)?;
        if let Some(prev) = prev_key {
            if rec.logical_key.as_str() <= prev {
                return Err("files trong manifest phải được sắp xếp tăng dần và không trùng.");
            }
        }
        prev_key = Some(rec.logical_key.as_str());

        let key_ci = rec.logical_key.to_ascii_lowercase();
        if !seen_keys_ci.insert(key_ci) {
            return Err("Xung đột chữ hoa/thường trong logical_key.");
        }

        if rec.kind == BackupFileKind::Db {
            has_db = true;
            if rec.size > MAX_BACKUP_DB_BYTES {
                return Err("Kích thước DB vượt giới hạn 1 GiB.");
            }
        } else if rec.size > MAX_BACKUP_SINGLE_FILE_BYTES {
            return Err("Kích thước một tệp vượt giới hạn 512 MiB.");
        }

        total_bytes = total_bytes
            .checked_add(rec.size)
            .ok_or("Tổng kích thước backup bị tràn số.")?;
        if total_bytes > MAX_BACKUP_DECRYPTED_ZIP_BYTES {
            return Err("Tổng kích thước backup vượt giới hạn 4 GiB.");
        }
    }

    if !has_db {
        return Err("Manifest thiếu bản ghi db/main.sqlite.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_canonical_keys_and_rejects_traversal_and_case_collisions() {
        let h = "a".repeat(64);
        assert!(validate_logical_key(BackupFileKind::Db, "db/main.sqlite").is_ok());
        assert!(validate_logical_key(BackupFileKind::PortableConfig, "config/site.json").is_ok());
        assert!(
            validate_logical_key(BackupFileKind::MediaOriginal, &format!("media/aa/{h}.png"))
                .is_ok()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::MediaThumbnail,
                &format!("media/aa/{h}.thumb.png")
            )
            .is_ok()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::SmtpSecret,
                &format!("secrets/smtp/{h}.json")
            )
            .is_ok()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::ExtensionSecret,
                &format!("secrets/plugins/demo-plugin/{h}.json")
            )
            .is_ok()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::ExtensionSecret,
                &format!("secrets/themes/demo-theme/{h}.json")
            )
            .is_ok()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::ExtensionSecret,
                &format!("secrets/smtp/{h}.json")
            )
            .is_err()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::ExtensionSecret,
                &format!("secrets/plugins/BAD_ID/{h}.json")
            )
            .is_err()
        );
        assert!(
            validate_logical_key(
                BackupFileKind::ExtensionSecret,
                "secrets/plugins/demo-plugin/not-hex.json"
            )
            .is_err()
        );
        assert!(validate_logical_key(BackupFileKind::Db, "db/../main.sqlite").is_err());
        assert!(validate_logical_key(BackupFileKind::Db, "db/CON").is_err());
        assert!(validate_logical_key(BackupFileKind::MediaOriginal, "media/aa/CON.png").is_err());
    }

    #[test]
    fn canonicalizes_trusted_origin_and_rejects_invalid_parts() {
        assert_eq!(
            canonicalize_trusted_origin("https://cms.example.com/").unwrap(),
            "https://cms.example.com"
        );
        assert_eq!(
            canonicalize_trusted_origin("HTTPS://CMS.Example.COM:443/").unwrap(),
            "https://cms.example.com"
        );
        assert_eq!(
            canonicalize_trusted_origin("http://cms.example.com:80").unwrap(),
            "http://cms.example.com"
        );
        assert_eq!(
            canonicalize_trusted_origin("https://cms.example.com:8443/").unwrap(),
            "https://cms.example.com:8443"
        );
        assert!(canonicalize_trusted_origin("https://user:pass@cms.example.com").is_err());
        assert!(canonicalize_trusted_origin("https://cms.example.com/admin").is_err());
        assert!(canonicalize_trusted_origin("https://cms.example.com//").is_err());
        assert!(canonicalize_trusted_origin("https://cms.example.com/?a=1").is_err());
        assert!(canonicalize_trusted_origin("https://cms.example.com/#frag").is_err());
        assert!(canonicalize_trusted_origin("ftp://cms.example.com").is_err());
        assert!(canonicalize_trusted_origin("   ").is_err());
    }
}
