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
}

impl PluginManager {
    pub fn new(db_pool: SqlitePool, allow_loopback: bool) -> Self {
        Self {
            db_pool,
            runtime: PluginRuntime::new(),
            allow_loopback,
        }
    }

    pub fn runtime(&self) -> &PluginRuntime {
        &self.runtime
    }

    pub fn allow_loopback(&self) -> bool {
        self.allow_loopback
    }

    pub async fn install_plugin(&self, zip_bytes: &[u8]) -> Result<PluginSummary, String> {
        let package =
            PluginPackage::from_zip(zip_bytes, self.runtime.engine()).map_err(|e| e.to_string())?;

        let manifest_json = serde_json::to_string(&package.manifest)
            .map_err(|e| format!("Lỗi serialize manifest: {e}"))?;

        plugin_storage::upsert_plugin(
            &self.db_pool,
            &package.manifest,
            &manifest_json,
            &package.wasm_bytes,
        )
        .await?;

        self.get_plugin(&package.manifest.id)
            .await?
            .ok_or_else(|| "Không tìm thấy plugin vừa cài đặt.".to_string())
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

        let row_opt = plugin_storage::load_plugin_executable(&self.db_pool, id)
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

        let Some((wasm_blob, manifest_json, enabled)) = row_opt else {
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

        let run_res = self
            .runtime
            .run_plugin(
                wasm_blob,
                manifest,
                input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
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
