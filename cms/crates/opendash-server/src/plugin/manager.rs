use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::storage::plugin as plugin_storage;

use super::{
    manifest::{PluginManifest, PluginPermissions},
    package::PluginPackage,
    runtime::{PluginError, PluginRuntime},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSummary {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub permissions: PluginPermissions,
    pub last_error: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub active_revision_id: Option<i64>,
    #[serde(default)]
    pub revision_generation: i64,
    #[serde(default)]
    pub cas_version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRunResponse {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub code: Option<String>,
    pub correlation_id: String,
}

pub struct PluginManager {
    db_pool: SqlitePool,
    runtime: PluginRuntime,
    allow_loopback: bool,
    plugins_root: std::path::PathBuf,
}

impl PluginManager {
    pub fn new(db_pool: SqlitePool, allow_loopback: bool) -> Self {
        let plugins_root = crate::paths::SitePaths::from_env()
            .map(|p| p.plugins_dir())
            .unwrap_or_else(|_| std::env::temp_dir().join("opendash_plugins_default"));
        Self::with_plugins_root(db_pool, allow_loopback, plugins_root)
    }

    pub fn with_plugins_root(
        db_pool: SqlitePool,
        allow_loopback: bool,
        plugins_root: std::path::PathBuf,
    ) -> Self {
        // Layout dirs created lazily; startup quarantine runs once in main.rs.
        Self {
            db_pool,
            runtime: PluginRuntime::new(),
            allow_loopback,
            plugins_root,
        }
    }

    pub fn plugins_root(&self) -> &std::path::Path {
        &self.plugins_root
    }

    pub fn runtime(&self) -> &PluginRuntime {
        &self.runtime
    }

    pub fn allow_loopback(&self) -> bool {
        self.allow_loopback
    }

    pub async fn install_plugin(&self, zip_bytes: &[u8]) -> Result<PluginSummary, String> {
        self.install_plugin_checked(zip_bytes)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn install_plugin_checked(
        &self,
        zip_bytes: &[u8],
    ) -> Result<PluginSummary, plugin_storage::PluginStorageError> {
        let package = PluginPackage::from_zip(zip_bytes, self.runtime.engine())
            .map_err(|e| plugin_storage::PluginStorageError::BadRequest(e.to_string()))?;

        let manifest_json = serde_json::to_string(&package.manifest).map_err(|e| {
            plugin_storage::PluginStorageError::Internal(format!("Lỗi serialize manifest: {e}"))
        })?;

        plugin_storage::install_plugin_revision(
            &self.db_pool,
            &self.plugins_root,
            &package.manifest,
            &manifest_json,
            &package.wasm_bytes,
        )
        .await?;

        self.get_plugin(&package.manifest.id)
            .await
            .map_err(plugin_storage::PluginStorageError::Internal)?
            .ok_or_else(|| {
                plugin_storage::PluginStorageError::Internal(
                    "Không tìm thấy plugin vừa cài đặt.".to_string(),
                )
            })
    }

    pub async fn disable_plugin_cas(
        &self,
        id: &str,
        expected_cas_version: Option<i64>,
    ) -> Result<i64, plugin_storage::PluginStorageError> {
        plugin_storage::set_plugin_enabled_cas(&self.db_pool, id, false, expected_cas_version).await
    }

    pub async fn enable_plugin_cas(
        &self,
        id: &str,
        expected_cas_version: Option<i64>,
    ) -> Result<i64, plugin_storage::PluginStorageError> {
        plugin_storage::set_plugin_enabled_cas(&self.db_pool, id, true, expected_cas_version).await
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginSummary>, String> {
        plugin_storage::list_plugins(&self.db_pool).await
    }

    pub async fn get_plugin(&self, id: &str) -> Result<Option<PluginSummary>, String> {
        plugin_storage::get_plugin(&self.db_pool, id).await
    }

    pub async fn disable_plugin(&self, id: &str) -> Result<(), String> {
        plugin_storage::set_plugin_enabled(&self.db_pool, id, false).await
    }

    pub async fn enable_plugin(&self, id: &str) -> Result<(), String> {
        plugin_storage::set_plugin_enabled(&self.db_pool, id, true).await
    }

    pub async fn run_plugin(
        &self,
        id: &str,
        input: Value,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let token = crate::auth::generate_random_token();
        let correlation_id = format!("run_{}", &token[..16.min(token.len())]);

        self.run_plugin_correlated(id, input, correlation_id).await
    }

    pub async fn hook_subscriptions(
        &self,
        event: opendash_core::hooks::HookEvent,
    ) -> Result<Vec<String>, String> {
        plugin_storage::hook_subscriptions(&self.db_pool, event).await
    }

    pub async fn content_hook_subscriptions(
        &self,
        event: opendash_core::hooks::HookEvent,
        type_key: &str,
    ) -> Result<Vec<(i32, String)>, String> {
        plugin_storage::content_hook_subscriptions(&self.db_pool, event, type_key).await
    }

    pub async fn record_hook_failure(&self, id: &str, correlation_id: &str, message: &str) {
        plugin_storage::record_plugin_execution_result(
            &self.db_pool,
            id,
            correlation_id,
            "error",
            message,
            Some(message),
        )
        .await;
    }

    pub async fn run_plugin_correlated(
        &self,
        id: &str,
        input: Value,
        correlation_id: String,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        self.run_plugin_with_mode(
            id,
            input,
            correlation_id,
            opendash_core::hooks::HookExecutionMode::Standard,
            None,
        )
        .await
    }

    pub async fn run_plugin_with_mode(
        &self,
        id: &str,
        input: Value,
        correlation_id: String,
        execution_mode: opendash_core::hooks::HookExecutionMode,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let row_opt = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Lỗi cơ sở dữ liệu: {e}")),
                        code: Some("db_error".into()),
                        correlation_id: correlation_id.clone(),
                    },
                )
            })?;

        let Some((wasm_blob, manifest_json, enabled, revision_generation, revision_digest_opt)) = row_opt else {
            return Err((
                StatusCode::NOT_FOUND,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some("Plugin không tồn tại.".into()),
                    code: Some("not_found".into()),
                    correlation_id,
                },
            ));
        };

        if !enabled {
            return Err((
                StatusCode::BAD_REQUEST,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some("Plugin chưa được bật hoặc đã bị vô hiệu hóa.".into()),
                    code: Some("disabled".into()),
                    correlation_id,
                },
            ));
        }

        let manifest = serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Manifest trong database không hợp lệ: {e}")),
                    code: Some("corrupt_manifest".into()),
                    correlation_id: correlation_id.clone(),
                },
            )
        })?;

        if let Some(digest) = &revision_digest_opt {
            if let Err(err) = plugin_storage::verify_plugin_revision_durability(&self.plugins_root, digest) {
                return Err((
                    StatusCode::CONFLICT,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err),
                        code: Some("corrupt_revision".into()),
                        correlation_id,
                    },
                ));
            }
        }

        let run_res = self
            .runtime
            .run_plugin_with_mode(
                wasm_blob,
                manifest,
                input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(revision_generation),
                execution_mode,
                timeout_override,
            )
            .await;

        match run_res {
            Ok(val) => {
                plugin_storage::record_plugin_execution_result(
                    &self.db_pool,
                    id,
                    &correlation_id,
                    "info",
                    "Thực thi thành công",
                    None,
                )
                .await;

                Ok(PluginRunResponse {
                    success: true,
                    data: Some(val),
                    error: None,
                    code: None,
                    correlation_id,
                })
            }
            Err(err) => {
                let (code_str, err_msg) = match &err {
                    PluginError::Disabled => ("disabled", err.to_string()),
                    PluginError::NotFound => ("not_found", err.to_string()),
                    PluginError::OutOfFuel => ("out_of_fuel", err.to_string()),
                    PluginError::Timeout => ("timeout", err.to_string()),
                    PluginError::BadPointer => ("bad_pointer", err.to_string()),
                    PluginError::GuestFailed(_) => ("guest_failed", err.to_string()),
                    PluginError::Trap(_) => ("trap", err.to_string()),
                    PluginError::InvalidOutput(_) => ("invalid_output", err.to_string()),
                    PluginError::ConcurrencyLimit => ("concurrency_limit", err.to_string()),
                    PluginError::ModuleCompilation(_) => ("module_compilation", err.to_string()),
                    PluginError::Internal(_) => ("internal_error", err.to_string()),
                };

                plugin_storage::record_plugin_execution_result(
                    &self.db_pool,
                    id,
                    &correlation_id,
                    "error",
                    &err_msg,
                    Some(&err_msg),
                )
                .await;

                Err((
                    StatusCode::BAD_REQUEST,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err_msg),
                        code: Some(code_str.to_string()),
                        correlation_id,
                    },
                ))
            }
        }
    }
}
