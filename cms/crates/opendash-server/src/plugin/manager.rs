use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

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

        let mut conn = self
            .db_pool
            .acquire()
            .await
            .map_err(|e| format!("Lỗi kết nối cơ sở dữ liệu: {e}"))?;

        sqlx::query(
            "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled, last_error, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, NULL, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 version = excluded.version,
                 name = excluded.name,
                 description = excluded.description,
                 manifest_json = excluded.manifest_json,
                 wasm_blob = excluded.wasm_blob,
                 enabled = 1,
                 last_error = NULL,
                 updated_at = excluded.updated_at",
        )
        .bind(&package.manifest.id)
        .bind(&package.manifest.version)
        .bind(&package.manifest.name)
        .bind(&package.manifest.description)
        .bind(&manifest_json)
        .bind(&package.wasm_bytes)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("Lỗi lưu plugin vào database: {e}"))?;

        self.get_plugin(&package.manifest.id)
            .await?
            .ok_or_else(|| "Không tìm thấy plugin vừa cài đặt.".to_string())
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginSummary>, String> {
        let rows = sqlx::query(
            "SELECT id, version, name, description, manifest_json, enabled, last_error, installed_at, updated_at
             FROM plugins ORDER BY id ASC",
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| format!("Lỗi truy vấn plugins: {e}"))?;

        let mut list = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let version: String = row.get("version");
            let name: String = row.get("name");
            let description: String = row.get("description");
            let manifest_json: String = row.get("manifest_json");
            let enabled: bool = row.get::<i64, _>("enabled") == 1;
            let last_error: Option<String> = row.get("last_error");
            let installed_at: String = row.get("installed_at");
            let updated_at: String = row.get("updated_at");

            let permissions =
                if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&manifest_json) {
                    manifest.permissions
                } else {
                    PluginPermissions {
                        sqlite: vec![],
                        http: vec![],
                    }
                };

            list.push(PluginSummary {
                id,
                version,
                name,
                description,
                enabled,
                permissions,
                last_error,
                installed_at,
                updated_at,
            });
        }

        Ok(list)
    }

    pub async fn get_plugin(&self, id: &str) -> Result<Option<PluginSummary>, String> {
        let row_opt = sqlx::query(
            "SELECT id, version, name, description, manifest_json, enabled, last_error, installed_at, updated_at
             FROM plugins WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| format!("Lỗi truy vấn plugin: {e}"))?;

        let Some(row) = row_opt else {
            return Ok(None);
        };

        let manifest_json: String = row.get("manifest_json");
        let permissions =
            if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&manifest_json) {
                manifest.permissions
            } else {
                PluginPermissions {
                    sqlite: vec![],
                    http: vec![],
                }
            };

        Ok(Some(PluginSummary {
            id: row.get("id"),
            version: row.get("version"),
            name: row.get("name"),
            description: row.get("description"),
            enabled: row.get::<i64, _>("enabled") == 1,
            permissions,
            last_error: row.get("last_error"),
            installed_at: row.get("installed_at"),
            updated_at: row.get("updated_at"),
        }))
    }

    pub async fn disable_plugin(&self, id: &str) -> Result<(), String> {
        let res = sqlx::query(
            "UPDATE plugins SET enabled = 0, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&self.db_pool)
        .await
        .map_err(|e| format!("Lỗi cập nhật trạng thái plugin: {e}"))?;

        if res.rows_affected() == 0 {
            return Err("Plugin không tồn tại.".to_string());
        }
        Ok(())
    }

    pub async fn enable_plugin(&self, id: &str) -> Result<(), String> {
        let res = sqlx::query(
            "UPDATE plugins SET enabled = 1, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&self.db_pool)
        .await
        .map_err(|e| format!("Lỗi cập nhật trạng thái plugin: {e}"))?;

        if res.rows_affected() == 0 {
            return Err("Plugin không tồn tại.".to_string());
        }
        Ok(())
    }

    pub async fn run_plugin(
        &self,
        id: &str,
        input: Value,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let token = crate::auth::generate_random_token();
        let correlation_id = format!("run_{}", &token[..16.min(token.len())]);

        let row_opt =
            sqlx::query("SELECT wasm_blob, manifest_json, enabled FROM plugins WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.db_pool)
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

        let Some(row) = row_opt else {
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

        let enabled: bool = row.get::<i64, _>("enabled") == 1;
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

        let wasm_blob: Vec<u8> = row.get("wasm_blob");
        let manifest_json: String = row.get("manifest_json");
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
                let _ = sqlx::query(
                    "INSERT INTO plugin_logs (plugin_id, correlation_id, level, message) VALUES (?, ?, 'info', 'Thực thi thành công')",
                )
                .bind(id)
                .bind(&correlation_id)
                .execute(&self.db_pool)
                .await;

                let _ = sqlx::query("UPDATE plugins SET last_error = NULL, updated_at = datetime('now') WHERE id = ?")
                    .bind(id)
                    .execute(&self.db_pool)
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

                let _ = sqlx::query(
                    "INSERT INTO plugin_logs (plugin_id, correlation_id, level, message) VALUES (?, ?, 'error', ?)",
                )
                .bind(id)
                .bind(&correlation_id)
                .bind(&err_msg)
                .execute(&self.db_pool)
                .await;

                let _ = sqlx::query(
                    "UPDATE plugins SET last_error = ?, updated_at = datetime('now') WHERE id = ?",
                )
                .bind(&err_msg)
                .bind(id)
                .execute(&self.db_pool)
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
