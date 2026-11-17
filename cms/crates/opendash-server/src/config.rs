//! Typed RuntimeConfigV1 loader, strict precedence, and RecoveryState policy (M8-S05-T02).

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use opendash_core::backup::PortableRuntimeConfigV1;

use crate::error::{ApiError, ApiResult};
use crate::paths::SitePaths;
use crate::restore::RestoreReadyMarker;
use crate::secrets::{
    create_private_file, ensure_no_reparse_or_symlink, ensure_private_directory_tree,
};

const MAX_SITE_CONFIG_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone)]
pub struct RuntimeConfigV1 {
    pub portable: PortableRuntimeConfigV1,
    pub ready_marker: Option<RestoreReadyMarker>,
}

#[derive(Clone, Debug)]
pub struct RecoveryState {
    safe_mode: Arc<AtomicBool>,
}

impl RecoveryState {
    pub fn new(safe_mode: bool) -> Self {
        Self {
            safe_mode: Arc::new(AtomicBool::new(safe_mode)),
        }
    }

    pub fn is_safe_mode(&self) -> bool {
        self.safe_mode.load(Ordering::SeqCst)
    }

    pub fn set_safe_mode(&self, val: bool) {
        self.safe_mode.store(val, Ordering::SeqCst);
    }
}

pub fn load_ready_marker(paths: &SitePaths) -> ApiResult<Option<RestoreReadyMarker>> {
    let p = paths.site_root().join("READY.json");
    let meta = match fs::symlink_metadata(&p) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ApiError::internal("Không đọc được thông tin READY.json.")),
    };
    ensure_no_reparse_or_symlink(&p)?;
    if !meta.is_file() || meta.len() > MAX_SITE_CONFIG_BYTES {
        return Err(ApiError::bad_request(
            "READY.json không hợp lệ hoặc vượt kích thước.",
        ));
    }
    let raw = fs::read(&p).map_err(|_| ApiError::internal("Không đọc được READY.json."))?;
    let marker: RestoreReadyMarker = serde_json::from_slice(&raw)
        .map_err(|_| ApiError::bad_request("READY.json không đúng định dạng."))?;
    Ok(Some(marker))
}

pub fn load_portable_config(paths: &SitePaths) -> ApiResult<PortableRuntimeConfigV1> {
    let cfg_path = paths.site_config_path();
    let meta = match fs::symlink_metadata(&cfg_path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PortableRuntimeConfigV1::default());
        }
        Err(_) => {
            return Err(ApiError::internal(
                "Không đọc được metadata config/site.json.",
            ));
        }
    };
    ensure_no_reparse_or_symlink(&cfg_path)?;
    if !meta.is_file() || meta.len() > MAX_SITE_CONFIG_BYTES {
        return Err(ApiError::bad_request(
            "config/site.json không hợp lệ hoặc vượt quá 64 KiB.",
        ));
    }
    let raw =
        fs::read(&cfg_path).map_err(|_| ApiError::internal("Không đọc được config/site.json."))?;
    let cfg: PortableRuntimeConfigV1 = serde_json::from_slice(&raw)
        .map_err(|_| ApiError::bad_request("config/site.json không đúng định dạng."))?;
    cfg.validate().map_err(ApiError::bad_request)?;
    Ok(cfg)
}

impl RuntimeConfigV1 {
    pub fn load(paths: &SitePaths) -> ApiResult<Self> {
        let mut portable = load_portable_config(paths)?;
        let ready_marker = load_ready_marker(paths)?;

        // Precedence: defaults < config/site.json < explicit valid env overrides.
        if let Ok(port_str) = std::env::var("OPENDASH_PORT")
            && !port_str.trim().is_empty()
        {
            let port: u16 = port_str
                .trim()
                .parse()
                .map_err(|_| ApiError::bad_request("OPENDASH_PORT không phải cổng hợp lệ."))?;
            portable.port = port;
        }
        if let Ok(origin) = std::env::var("OPENDASH_PUBLIC_ORIGIN")
            && !origin.trim().is_empty()
        {
            portable.trusted_public_origin = Some(origin.trim().to_string());
        }
        if let Ok(lvl) = std::env::var("OPENDASH_LOG_LEVEL")
            && !lvl.trim().is_empty()
        {
            portable.log_level = lvl.trim().to_string();
        }

        // If READY.json exists and no config/site.json cleared safe mode, keep safe mode on.
        if ready_marker.is_some() && !paths.site_config_path().exists() {
            portable.recovery_safe_mode = true;
        }

        portable.validate().map_err(ApiError::bad_request)?;
        if let Some(ref raw) = portable.trusted_public_origin {
            portable.trusted_public_origin = Some(
                opendash_core::backup::canonicalize_trusted_origin(raw)
                    .map_err(ApiError::bad_request)?,
            );
        }
        Ok(Self {
            portable,
            ready_marker,
        })
    }
}

pub fn persist_clear_safe_mode(paths: &SitePaths) -> ApiResult<PortableRuntimeConfigV1> {
    let mut cfg = load_portable_config(paths)?;
    cfg.recovery_safe_mode = false;
    cfg.validate().map_err(ApiError::bad_request)?;

    let cfg_dir = paths.config_dir();
    ensure_private_directory_tree(&cfg_dir)?;
    let cfg_path = paths.site_config_path();
    let part_path = cfg_dir.join(".site.json.part");
    let bytes = serde_json::to_vec_pretty(&cfg)
        .map_err(|_| ApiError::internal("Không tuần tự hóa được config/site.json."))?;

    let mut f = create_private_file(&part_path)?;
    f.write_all(&bytes)
        .map_err(|_| ApiError::internal("Không ghi được config/site.json."))?;
    f.sync_all()
        .map_err(|_| ApiError::internal("Không fsync được config/site.json."))?;
    drop(f);

    if cfg_path.exists() {
        let _ = fs::remove_file(&cfg_path);
    }
    fs::rename(&part_path, &cfg_path)
        .map_err(|_| ApiError::internal("Không cập nhật được config/site.json."))?;
    Ok(cfg)
}

pub fn build_recovery_state_for_root(site_root: &Path, db_path: &Path) -> RecoveryState {
    let paths = SitePaths::from_parts(site_root.to_path_buf(), db_path.to_path_buf());
    match RuntimeConfigV1::load(&paths) {
        Ok(cfg) => RecoveryState::new(cfg.portable.recovery_safe_mode),
        Err(_) => RecoveryState::new(false),
    }
}

impl RuntimeConfigV1 {
    pub fn trusted_origin_normalized(&self) -> Option<String> {
        self.portable
            .trusted_public_origin
            .as_deref()
            .and_then(|o| opendash_core::backup::canonicalize_trusted_origin(o).ok())
    }

    pub fn trusted_origin_is_https(&self) -> bool {
        self.trusted_origin_normalized()
            .is_some_and(|o| o.starts_with("https://"))
    }

    pub fn trusted_host(&self) -> Option<String> {
        let origin = self.trusted_origin_normalized()?;
        let rest = origin
            .strip_prefix("https://")
            .or_else(|| origin.strip_prefix("http://"))?;
        Some(rest.to_string())
    }

    pub fn redacted_summary_json(&self) -> serde_json::Value {
        serde_json::json!({
            "version": self.portable.version,
            "listen_ip": self.portable.listen_ip,
            "port": self.portable.port,
            "trusted_public_origin": self.portable.trusted_public_origin,
            "log_level": self.portable.log_level,
            "recovery_safe_mode": self.portable.recovery_safe_mode,
        })
    }
}
