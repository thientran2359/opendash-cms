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

struct ActionLeaseGuard {
    leases: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    plugin_id: String,
}

impl Drop for ActionLeaseGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.leases.lock() {
            guard.remove(&self.plugin_id);
        }
    }
}

pub struct PluginManager {
    db_pool: SqlitePool,
    runtime: PluginRuntime,
    active_action_leases: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
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
            active_action_leases: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            allow_loopback,
            plugins_root,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_plugin_action(
        &self,
        id: &str,
        action_id: &str,
        actor: &crate::auth::SessionContext,
        input: Value,
        content_registry: &std::sync::Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>,
            >,
        >,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: String,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.into()),
                    correlation_id: correlation_id.clone(),
                },
            )
        };

        let _lease_guard = {
            let mut leases = self
                .active_action_leases
                .lock()
                .map_err(|_| mk_err(StatusCode::INTERNAL_SERVER_ERROR, "lease_lock", "Lỗi khóa action lease.".into()))?;
            if !leases.insert(id.to_string()) {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "recursive_dispatch",
                    "Không cho phép gọi đệ quy action trên cùng plugin.".into(),
                ));
            }
            ActionLeaseGuard {
                leases: std::sync::Arc::clone(&self.active_action_leases),
                plugin_id: id.to_string(),
            }
        };

        let row_opt = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
            .await
            .map_err(|e| mk_err(StatusCode::INTERNAL_SERVER_ERROR, "db_error", format!("Lỗi cơ sở dữ liệu: {e}")))?;

        let Some((wasm_blob, manifest_json, enabled, revision_generation, revision_digest_opt)) = row_opt else {
            return Err(mk_err(StatusCode::NOT_FOUND, "not_found", "Plugin không tồn tại.".into()));
        };
        if !enabled {
            return Err(mk_err(
                StatusCode::BAD_REQUEST,
                "disabled",
                "Plugin chưa được bật hoặc đã bị vô hiệu hóa.".into(),
            ));
        }

        let manifest = serde_json::from_str::<PluginManifest>(&manifest_json)
            .map_err(|e| mk_err(StatusCode::INTERNAL_SERVER_ERROR, "corrupt_manifest", format!("Manifest không hợp lệ: {e}")))?;
        let Some(action_decl) = manifest.action(action_id).cloned() else {
            return Err(mk_err(StatusCode::NOT_FOUND, "action_not_found", "Action không tồn tại trong manifest.".into()));
        };

        if action_decl.required_role == "admin" && actor.user.role != crate::auth::Role::Admin {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "rbac_denied",
                "Tài khoản không đủ quyền thực thi action này.".into(),
            ));
        }
        for cap in &action_decl.capabilities {
            let allowed = match cap.as_str() {
                "kv_read" => manifest.has_sqlite_perm("kv_read"),
                "kv_write" => manifest.has_sqlite_perm("kv_write"),
                "content_read" => !manifest.permissions.content_read.is_empty(),
                "content_write" => !manifest.permissions.content_write.is_empty(),
                "http" => !manifest.permissions.http.is_empty(),
                _ => false,
            };
            if !allowed {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "capability_denied",
                    format!("Plugin thiếu quyền manifest cho capability '{cap}'."),
                ));
            }
        }

        if let Some(digest) = &revision_digest_opt {
            if let Err(err) = plugin_storage::verify_plugin_revision_durability(&self.plugins_root, digest) {
                return Err(mk_err(StatusCode::CONFLICT, "corrupt_revision", err));
            }
        }

        let action_res = self
            .runtime
            .run_plugin_action(
                wasm_blob,
                manifest.clone(),
                input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(revision_generation),
                opendash_core::hooks::HookExecutionMode::Standard,
                None,
                Some(action_decl.capabilities.clone()),
                Some(crate::plugin::runtime::ActionActorFence {
                    user_id: actor.user.id,
                    required_role: action_decl.required_role.clone(),
                }),
            )
            .await;

        let exec_out = match action_res {
            Ok(out) => out,
            Err(err) => {
                let (status, code_str) = match &err {
                    PluginError::Disabled => (StatusCode::CONFLICT, "generation_changed"),
                    PluginError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "timeout"),
                    PluginError::Internal(m) if m.starts_with("rbac_revoked:") => {
                        (StatusCode::FORBIDDEN, "rbac_revoked")
                    }
                    _ => (StatusCode::BAD_REQUEST, "action_failed"),
                };
                let msg = err.to_string();
                plugin_storage::record_plugin_execution_result(
                    &self.db_pool,
                    id,
                    &correlation_id,
                    "error",
                    &msg,
                    Some(&msg),
                )
                .await;
                return Err(mk_err(status, code_str, msg));
            }
        };

        if let Some(cmd) = exec_out.staged_content_command {
            if !manifest.can_write_content_type(&cmd.type_key) {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "content_write_denied",
                    "Plugin không có quyền ghi loại nội dung này.".into(),
                ));
            }
            let schema = {
                let reg = content_registry.read().await;
                reg.get(&cmd.type_key).map(|d| d.descriptor.clone())
            }
            .ok_or_else(|| mk_err(StatusCode::NOT_FOUND, "content_type_not_found", "Không tìm thấy loại nội dung.".into()))?;

            let entry_input = opendash_core::content::ContentEntryInput {
                slug: cmd.slug,
                title: cmd.title,
                values: cmd.values,
                term_ids: cmd.term_ids,
            };
            if let Err(msg) = entry_input.validate_against(&schema) {
                return Err(mk_err(StatusCode::BAD_REQUEST, "invalid_entry_input", msg.to_string()));
            }
            if let Err(err) =
                crate::storage::media::require_existing_references(&self.db_pool, &schema, &entry_input).await
            {
                return Err(mk_err(err.0, "media_reference_error", err.1.to_string()));
            }

            let operation = if cmd.entry_id.is_some() { "update" } else { "create" };
            let candidate = serde_json::json!({
                "operation": operation,
                "type_key": &cmd.type_key,
                "entry_id": cmd.entry_id,
                "expected_version": cmd.expected_version,
                "title": &entry_input.title,
                "slug": &entry_input.slug,
                "candidate_values": &entry_input.values,
                "term_ids": &entry_input.term_ids,
            });
            if let Err(veto) = hooks
                .dispatch_content_before_save(&cmd.type_key, candidate)
                .await
            {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    PluginRunResponse {
                        success: false,
                        data: serde_json::to_value(&veto).ok(),
                        error: Some(veto.message),
                        code: Some(veto.code),
                        correlation_id,
                    },
                ));
            }

            let fence = crate::storage::entries::PluginActionFence {
                plugin_id: id.to_string(),
                expected_generation: revision_generation,
                required_role: action_decl.required_role.clone(),
            };
            let saved_res = match cmd.entry_id {
                Some(entry_id) => {
                    let expected_ver = cmd.expected_version.ok_or_else(|| {
                        mk_err(
                            StatusCode::PRECONDITION_REQUIRED,
                            "missing_expected_version",
                            "Cập nhật nội dung yêu cầu expected_version.".into(),
                        )
                    })?;
                    crate::storage::entries::update_entry_with_fence(
                        &self.db_pool,
                        actor,
                        &cmd.type_key,
                        &schema,
                        entry_id,
                        expected_ver,
                        &entry_input,
                        Some(&fence),
                    )
                    .await
                }
                None => {
                    crate::storage::entries::create_entry_with_fence(
                        &self.db_pool,
                        actor,
                        &cmd.type_key,
                        &schema,
                        &entry_input,
                        Some(&fence),
                    )
                    .await
                }
            };
            let record = match saved_res {
                Ok(r) => r,
                Err(err) => {
                    return Err(mk_err(err.0, "content_commit_fenced", err.1.to_string()));
                }
            };
            hooks
                .dispatch_content_after_save(
                    &cmd.type_key,
                    serde_json::to_value(&record).unwrap_or_default(),
                )
                .await;
            plugin_storage::record_plugin_execution_result(
                &self.db_pool,
                id,
                &correlation_id,
                "info",
                "Thực thi action thành công",
                None,
            )
            .await;
            return Ok(PluginRunResponse {
                success: true,
                data: Some(serde_json::json!({
                    "output": exec_out.output,
                    "entry": record,
                })),
                error: None,
                code: None,
                correlation_id,
            });
        }

        plugin_storage::record_plugin_execution_result(
            &self.db_pool,
            id,
            &correlation_id,
            "info",
            "Thực thi action thành công",
            None,
        )
        .await;

        Ok(PluginRunResponse {
            success: true,
            data: Some(exec_out.output),
            error: None,
            code: None,
            correlation_id,
        })
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
