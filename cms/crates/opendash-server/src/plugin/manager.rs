use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::http::StatusCode;
use opendash_core::hooks::{
    EnvelopeKind, ExecutionPrincipal, HookEvent, HookExecutionMode, PluginEnvelopeV2,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::storage::{
    entry_lifecycle::{
        EntryCommitReceipt, EntryMutationKind, LifecycleAuthority, LifecycleCommand,
        LifecycleConfirmationEvidence, LifecycleOperation, LifecycleScope, SecondaryOutcome,
    },
    plugin as plugin_storage,
};

use super::{
    host_ops::StagedLifecycleOperation,
    lifecycle_confirmation::{
        ConfirmationStoreError, LifecycleActionOrigin, LifecycleConfirmationStore,
        NewPendingConfirmation, PluginLifecycleConfirmation,
    },
    manifest::{PluginManifest, PluginPermissions},
    package::PluginPackage,
    runtime::{PluginError, PluginRuntime},
};

#[derive(Debug, Clone)]
pub struct CachedActivePlugin {
    pub manifest: PluginManifest,
    #[allow(dead_code)]
    pub wasm_blob: Vec<u8>,
    #[allow(dead_code)]
    pub revision_generation: i64,
    #[allow(dead_code)]
    pub revision_digest: Option<String>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<PluginLifecycleConfirmation>,
}

/// Host-only internal outcome of executing or confirming a plugin action.
/// Never serialized directly to HTTP; routes serialize `response` and dispatch hooks
/// strictly from the trusted server-owned `committed` receipt.
#[derive(Debug, Clone)]
pub struct ActionCommitOutcome {
    pub response: PluginRunResponse,
    pub committed: Option<EntryCommitReceipt>,
    pub secondary: SecondaryOutcome,
}

fn lifecycle_data_value(receipt: &EntryCommitReceipt, op: &str) -> Value {
    serde_json::json!({
        "operation": op,
        "id": receipt.id,
        "type_key": receipt.type_key,
        "locale": receipt.locale,
        "version": receipt.version_after.or(receipt.version_before),
        "publication_status": receipt
            .publication_after
            .as_deref()
            .or(receipt.publication_before.as_deref()),
        "trashed_at": receipt.trashed_at,
        "published_at": receipt.published_at,
    })
}

fn map_lifecycle_api_error_code(err: &crate::error::ApiError) -> &'static str {
    let msg = err.1.as_ref();
    if msg.starts_with("confirmation_required:") {
        "confirmation_required"
    } else if msg.starts_with("entry_trashed:") {
        "entry_trashed"
    } else if msg.starts_with("entry_not_trashed:") {
        "entry_not_trashed"
    } else if msg.starts_with("reading_page_assigned:") {
        "reading_page_assigned"
    } else if msg.starts_with("provider_inactive:") {
        "provider_inactive"
    } else if msg.starts_with("locale_disabled:") {
        "locale_disabled"
    } else if msg.starts_with("locale_deleted:") {
        "locale_deleted"
    } else if msg.starts_with("entry_purge_forbidden:") {
        "entry_purge_forbidden"
    } else if msg.starts_with("entry_lifecycle_forbidden:") {
        "entry_lifecycle_forbidden"
    } else if err.0 == StatusCode::UNAUTHORIZED {
        "unauthorized"
    } else if err.0 == StatusCode::FORBIDDEN {
        "rbac_revoked"
    } else if err.0 == StatusCode::PRECONDITION_FAILED {
        "version_conflict"
    } else if err.0 == StatusCode::NOT_FOUND {
        "not_found"
    } else {
        "content_lifecycle_fenced"
    }
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
    confirmation_store: Arc<LifecycleConfirmationStore>,
    allow_loopback: bool,
    plugins_root: PathBuf,
    secrets_root: PathBuf,
    manifest_cache: Arc<RwLock<Option<BTreeMap<String, CachedActivePlugin>>>>,
    recovery: crate::config::RecoveryState,
}

impl PluginManager {
    pub fn new(db_pool: SqlitePool, allow_loopback: bool) -> Self {
        let (plugins_root, secrets_root) = match crate::paths::SitePaths::from_env() {
            Ok(p) => (p.plugins_dir(), p.secrets_dir()),
            Err(_) => (
                std::env::temp_dir().join("opendash_plugins_default"),
                std::env::temp_dir().join("opendash_secrets_default"),
            ),
        };
        Self::with_roots(db_pool, allow_loopback, plugins_root, secrets_root)
    }

    pub fn with_plugins_root(
        db_pool: SqlitePool,
        allow_loopback: bool,
        plugins_root: PathBuf,
    ) -> Self {
        let secrets_root = crate::paths::SitePaths::from_env()
            .map(|p| p.secrets_dir())
            .unwrap_or_else(|_| {
                plugins_root
                    .parent()
                    .unwrap_or(&plugins_root)
                    .join("database")
                    .join("secrets")
            });
        Self::with_roots(db_pool, allow_loopback, plugins_root, secrets_root)
    }

    pub fn with_roots(
        db_pool: SqlitePool,
        allow_loopback: bool,
        plugins_root: PathBuf,
        secrets_root: PathBuf,
    ) -> Self {
        // Layout dirs created lazily; startup quarantine runs once in main.rs.
        Self {
            db_pool,
            runtime: PluginRuntime::with_secrets_root(secrets_root.clone()),
            active_action_leases: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            confirmation_store: Arc::new(LifecycleConfirmationStore::new()),
            allow_loopback,
            plugins_root,
            secrets_root,
            manifest_cache: Arc::new(RwLock::new(None)),
            recovery: crate::config::RecoveryState::new(false),
        }
    }

    pub async fn invalidate_cache(&self) {
        *self.manifest_cache.write().await = None;
    }

    async fn ensure_manifest_cache(&self) -> Result<BTreeMap<String, CachedActivePlugin>, String> {
        {
            let guard = self.manifest_cache.read().await;
            if let Some(ref map) = *guard {
                return Ok(map.clone());
            }
        }
        let rows = plugin_storage::load_all_enabled_plugins_for_cache(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;
        let mut built = BTreeMap::new();
        for row in rows {
            match PluginManifest::from_json_str(&row.manifest_json) {
                Ok(manifest) if manifest.id == row.id => {
                    built.insert(
                        row.id,
                        CachedActivePlugin {
                            manifest,
                            wasm_blob: row.wasm_blob,
                            revision_generation: row.revision_generation,
                            revision_digest: row.revision_digest,
                        },
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("Invalid cached manifest {}: {error}", row.id);
                }
            }
        }
        *self.manifest_cache.write().await = Some(built.clone());
        Ok(built)
    }

    pub async fn cached_plugin_manifest(&self, id: &str) -> Option<PluginManifest> {
        let cache = self.ensure_manifest_cache().await.ok()?;
        cache.get(id).map(|item| item.manifest.clone())
    }

    pub async fn filter_public_content_payload_for_plugin(
        &self,
        plugin_id: &str,
        type_key: &str,
        payload: &mut Value,
    ) {
        let manifest_opt = self.cached_plugin_manifest(plugin_id).await;
        let schema_opt =
            crate::storage::content::load_effective_content_schema(&self.db_pool, type_key)
                .await
                .ok()
                .flatten();

        for field_map_key in ["candidate_values", "values"] {
            if let Some(Value::Object(map)) = payload.get_mut(field_map_key) {
                match (&manifest_opt, &schema_opt) {
                    (Some(manifest), Some(effective)) => {
                        map.retain(|k, _| {
                            effective.effective_fields.get(k).is_some_and(|fe| {
                                fe.definition.public_exposure
                                    == opendash_core::content::PublicExposure::Public
                                    && opendash_core::plugin::plugin_can_read_field(
                                        manifest,
                                        None,
                                        opendash_core::content::FieldRole::Editor,
                                        type_key,
                                        fe,
                                    )
                            })
                        });
                    }
                    _ => {
                        map.clear();
                    }
                }
            }
        }
    }

    pub fn with_recovery_state(mut self, recovery: crate::config::RecoveryState) -> Self {
        self.recovery = recovery;
        self
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
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: String,
        origin: LifecycleActionOrigin,
        maintenance_epoch: u64,
    ) -> Result<ActionCommitOutcome, (StatusCode, PluginRunResponse)> {
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.into()),
                    correlation_id: correlation_id.clone(),
                    confirmation: None,
                },
            )
        };

        let _lease_guard = {
            if self.recovery.is_safe_mode() {
                return Err(mk_err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "safe_mode",
                    "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.".into(),
                ));
            }
            let mut leases = self.active_action_leases.lock().map_err(|_| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "lease_lock",
                    "Lỗi khóa action lease.".into(),
                )
            })?;
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
            .map_err(|e| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    format!("Lỗi cơ sở dữ liệu: {e}"),
                )
            })?;

        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(
                        "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.".into(),
                    ),
                    code: Some("safe_mode".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }
        let Some((wasm_blob, manifest_json, enabled, revision_generation, revision_digest_opt)) =
            row_opt
        else {
            return Err(mk_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Plugin không tồn tại.".into(),
            ));
        };
        if !enabled {
            return Err(mk_err(
                StatusCode::BAD_REQUEST,
                "disabled",
                "Plugin chưa được bật hoặc đã bị vô hiệu hóa.".into(),
            ));
        }

        let manifest = serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
            mk_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "corrupt_manifest",
                format!("Manifest không hợp lệ: {e}"),
            )
        })?;
        let Some(action_decl) = manifest.action(action_id).cloned() else {
            return Err(mk_err(
                StatusCode::NOT_FOUND,
                "action_not_found",
                "Action không tồn tại trong manifest.".into(),
            ));
        };

        if action_decl.required_role == "admin" && actor.user.role != crate::auth::Role::Admin {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "rbac_denied",
                "Tài khoản không đủ quyền thực thi action này.".into(),
            ));
        }
        let actor_role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        let declares_http = action_decl.capabilities.iter().any(|c| c == "http");
        let declares_lifecycle_confirm_write = action_decl
            .capabilities
            .iter()
            .any(|c| c == "content_restore" || c == "content_purge");
        if declares_http && declares_lifecycle_confirm_write {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "capability_denied",
                "Action khai báo content_restore hoặc content_purge không được kết hợp capability http.".into(),
            ));
        }
        for cap in &action_decl.capabilities {
            let allowed = match cap.as_str() {
                "kv_read" => manifest.has_sqlite_perm("kv_read"),
                "kv_list" => manifest.has_sqlite_perm("kv_read"),
                "kv_write" | "kv_delete" => manifest.has_sqlite_perm("kv_write"),
                "content_read" => !manifest.permissions.content_read.is_empty(),
                "content_read_private" => !manifest.permissions.content_read_private.is_empty(),
                "content_write" => !manifest.permissions.content_write.is_empty(),
                "content_write_private" => !manifest.permissions.content_write_private.is_empty(),
                "content_delete" => !manifest.permissions.content_delete.is_empty(),
                "content_trash_read" => !manifest.permissions.content_trash_read.is_empty(),
                "content_restore" => !manifest.permissions.content_restore.is_empty(),
                "content_purge" => !manifest.permissions.content_purge.is_empty(),
                "http" => {
                    !manifest.permissions.http.is_empty()
                        || !manifest.permissions.network.is_empty()
                }
                "taxonomy_read" => manifest.permissions.taxonomy_read,
                "media_read" => manifest.permissions.media_read,
                "site_options_read" => manifest.permissions.site_options_read,
                "plugin_settings_read" => manifest.permissions.plugin_settings_read,
                "plugin_secrets_read" => !manifest.permissions.plugin_secrets_read.is_empty(),
                "log" => true,
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
            if let Err(err) =
                plugin_storage::verify_plugin_revision_durability(&self.plugins_root, digest)
            {
                return Err(mk_err(StatusCode::CONFLICT, "corrupt_revision", err));
            }
        }

        let mut filtered_input = input;
        self.filter_action_input_for_plugin(
            &manifest,
            &action_decl.capabilities,
            actor_role,
            &mut filtered_input,
        )
        .await;

        let principal = match actor.user.role {
            crate::auth::Role::Admin => ExecutionPrincipal::Admin,
            crate::auth::Role::Editor => ExecutionPrincipal::Editor,
        };
        let runtime_input = if manifest.manifest_schema >= 3 {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let locale = filtered_input
                .get("locale")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            serde_json::to_value(PluginEnvelopeV2::new(
                EnvelopeKind::Action,
                format!("action:{action_id}"),
                now_ms,
                correlation_id.clone(),
                locale,
                principal,
                0,
                filtered_input,
            ))
            .unwrap_or(Value::Null)
        } else {
            filtered_input
        };

        let action_res = self
            .runtime
            .run_plugin_action_with_principal(
                wasm_blob,
                manifest.clone(),
                runtime_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(revision_generation),
                HookExecutionMode::Standard,
                principal,
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
            let _ = &content_registry;
            let effective_schema = crate::storage::content::load_effective_content_schema(
                &self.db_pool,
                &cmd.type_key,
            )
            .await
            .map_err(|e| mk_err(e.0, "schema_load_failed", e.1.to_string()))?
            .ok_or_else(|| {
                mk_err(
                    StatusCode::NOT_FOUND,
                    "content_type_not_found",
                    "Không tìm thấy loại nội dung.".into(),
                )
            })?;

            for field_key in cmd.values.keys() {
                let Some(field_entry) = effective_schema.effective_fields.get(field_key) else {
                    return Err(mk_err(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "unknown_field_key",
                        format!(
                            "Trường '{field_key}' không tồn tại trong schema của '{}'.",
                            cmd.type_key
                        ),
                    ));
                };
                if !opendash_core::plugin::plugin_can_write_field(
                    &manifest,
                    Some(&action_decl.capabilities),
                    actor_role,
                    &cmd.type_key,
                    field_entry,
                ) {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "field_edit_forbidden",
                        format!("Plugin hoặc tài khoản không có quyền ghi trường '{field_key}'."),
                    ));
                }
            }

            let schema = opendash_core::content::ContentTypeDefinition {
                key: effective_schema.base.descriptor.key.clone(),
                label: effective_schema.base.descriptor.label.clone(),
                settings: effective_schema.base.descriptor.settings.clone(),
                fields: effective_schema
                    .effective_fields
                    .values()
                    .map(|e| {
                        let mut def = e.definition.clone();
                        if !e.effective_required() {
                            def.required = false;
                        }
                        def
                    })
                    .collect(),
            };

            let entry_input = opendash_core::content::ContentEntryInput {
                slug: cmd.slug,
                title: cmd.title,
                locale: cmd.locale.clone(),
                values: cmd.values,
                term_ids: cmd.term_ids,
            };
            if let Err(msg) = entry_input.validate_against_effective(&effective_schema) {
                return Err(mk_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_entry_input",
                    msg.to_string(),
                ));
            }
            if let Err(err) = crate::storage::media::require_existing_effective_references(
                &self.db_pool,
                &effective_schema,
                &entry_input.values,
            )
            .await
            {
                return Err(mk_err(err.0, "media_reference_error", err.1.to_string()));
            }

            let (hook_locale, hook_group_id) = match cmd.entry_id {
                Some(entry_id) => {
                    match crate::storage::entries::get_entry_locale_and_group(
                        &self.db_pool,
                        &cmd.type_key,
                        entry_id,
                    )
                    .await
                    {
                        Ok(Some((existing_locale, group_id))) => {
                            if let Some(raw) = cmd.locale.as_deref()
                                && raw.trim().to_ascii_lowercase() != existing_locale
                            {
                                return Err(mk_err(
                                    StatusCode::BAD_REQUEST,
                                    "locale_immutable",
                                    "Không thể đổi ngôn ngữ của nội dung đã tạo.".into(),
                                ));
                            }
                            (Some(existing_locale), Some(group_id))
                        }
                        Ok(None) => (cmd.locale.clone(), None),
                        Err(err) => {
                            return Err(mk_err(err.0, "entry_lookup_failed", err.1.to_string()));
                        }
                    }
                }
                None => {
                    let effective_locale = crate::storage::settings::resolve_requested_locale(
                        &self.db_pool,
                        cmd.locale.as_deref(),
                    )
                    .await
                    .map_err(|err| mk_err(err.0, "invalid_locale", err.1.to_string()))?;
                    (Some(effective_locale), None)
                }
            };

            let operation = if cmd.entry_id.is_some() {
                "update"
            } else {
                "create"
            };
            let candidate = serde_json::json!({
                "operation": operation,
                "type_key": &cmd.type_key,
                "entry_id": cmd.entry_id,
                "expected_version": cmd.expected_version,
                "locale": hook_locale,
                "translation_group_id": hook_group_id,
                "title": &entry_input.title,
                "slug": &entry_input.slug,
                "candidate_values": &entry_input.values,
                "term_ids": &entry_input.term_ids,
            });
            if let Err(veto) = hooks
                .dispatch_content_before_save_for_actor(&cmd.type_key, actor_role, candidate)
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
                        confirmation: None,
                    },
                ));
            }

            let fence = crate::storage::entries::PluginActionFence {
                plugin_id: id.to_string(),
                expected_generation: revision_generation,
                required_role: action_decl.required_role.clone(),
            };
            let version_before = cmd.expected_version;
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
            let projection = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
                &self.db_pool,
                hooks,
                record.id,
                true,
            )
            .await;
            hooks
                .dispatch_content_after_save_for_actor(
                    &cmd.type_key,
                    actor_role,
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
            let receipt = EntryCommitReceipt {
                operation: EntryMutationKind::Save,
                id: record.id,
                type_key: record.content_type_key.clone(),
                locale: record.locale.clone(),
                translation_group_id: record.translation_group_id.clone(),
                version_before,
                version_after: Some(record.version),
                publication_before: None,
                publication_after: Some(record.publication_status.clone()),
                trashed_at: None,
                published_at: record.published_at.clone(),
            };
            return Ok(ActionCommitOutcome {
                response: PluginRunResponse {
                    success: true,
                    data: Some(serde_json::json!({
                        "output": exec_out.output,
                        "entry": record,
                    })),
                    error: None,
                    code: None,
                    correlation_id,
                    confirmation: None,
                },
                committed: Some(receipt),
                secondary: SecondaryOutcome {
                    projection,
                    hook_enqueued: 0,
                    hook_dropped: 0,
                },
            });
        }

        if let Some(staged) = exec_out.staged_lifecycle_command {
            // Exhaustive conversion from wire StagedLifecycleOperation to storage LifecycleOperation.
            let domain_op = match staged.operation {
                StagedLifecycleOperation::Trash => LifecycleOperation::Trash,
                StagedLifecycleOperation::Restore => LifecycleOperation::Restore,
                StagedLifecycleOperation::Purge => LifecycleOperation::Purge,
            };
            let (required_cap, has_manifest_grant) = match domain_op {
                LifecycleOperation::Trash => (
                    "content_delete",
                    manifest.can_delete_content_type(&staged.type_key),
                ),
                LifecycleOperation::Restore => (
                    "content_restore",
                    manifest.can_restore_content_type(&staged.type_key),
                ),
                LifecycleOperation::Purge => (
                    "content_purge",
                    manifest.can_purge_content_type(&staged.type_key),
                ),
            };
            if !action_decl.capabilities.iter().any(|c| c == required_cap) || !has_manifest_grant {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "content_lifecycle_denied",
                    format!("Plugin không có quyền '{required_cap}' cho loại nội dung này."),
                ));
            }
            if domain_op == LifecycleOperation::Purge && actor.user.role != crate::auth::Role::Admin
            {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "entry_purge_forbidden",
                    "Chỉ Quản trị viên được phép xóa vĩnh viễn nội dung.".into(),
                ));
            }

            let command = LifecycleCommand {
                operation: domain_op,
                type_key: staged.type_key.clone(),
                entry_id: staged.entry_id,
                expected_version: staged.expected_version,
            };
            let fence = crate::storage::entries::PluginActionFence {
                plugin_id: id.to_string(),
                expected_generation: revision_generation,
                required_role: action_decl.required_role.clone(),
            };

            match domain_op {
                LifecycleOperation::Trash => {
                    let authority = LifecycleAuthority {
                        session: actor,
                        plugin_fence: Some(&fence),
                        confirmation: LifecycleConfirmationEvidence::None,
                        scope: LifecycleScope::Ordinary,
                    };
                    let receipt = crate::storage::entry_lifecycle::execute_lifecycle_transition(
                        &self.db_pool,
                        &command,
                        &authority,
                    )
                    .await
                    .map_err(|err| {
                        mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string())
                    })?;
                    plugin_storage::record_plugin_execution_result(
                        &self.db_pool,
                        id,
                        &correlation_id,
                        "info",
                        "Thực thi action thành công",
                        None,
                    )
                    .await;
                    return Ok(ActionCommitOutcome {
                        response: PluginRunResponse {
                            success: true,
                            data: Some(serde_json::json!({
                                "output": exec_out.output,
                                "deleted_entry_id": receipt.id,
                                "deleted_type_key": receipt.type_key,
                                "lifecycle": lifecycle_data_value(&receipt, "trash"),
                            })),
                            error: None,
                            code: None,
                            correlation_id,
                            confirmation: None,
                        },
                        committed: Some(receipt),
                        secondary: SecondaryOutcome::none(),
                    });
                }
                LifecycleOperation::Restore | LifecycleOperation::Purge => {
                    let preview =
                        crate::storage::entry_lifecycle::preflight_plugin_lifecycle_confirmation(
                            &self.db_pool,
                            actor,
                            &command,
                            &fence,
                        )
                        .await
                        .map_err(|err| {
                            mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string())
                        })?;

                    if !preview.needs_confirmation {
                        let authority = LifecycleAuthority {
                            session: actor,
                            plugin_fence: Some(&fence),
                            confirmation: LifecycleConfirmationEvidence::None,
                            scope: LifecycleScope::Ordinary,
                        };
                        let receipt =
                            crate::storage::entry_lifecycle::execute_lifecycle_transition(
                                &self.db_pool,
                                &command,
                                &authority,
                            )
                            .await
                            .map_err(|err| {
                                mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string())
                            })?;
                        let projection =
                            crate::storage::entry_lifecycle::recompute_projections_nonfatal(
                                &self.db_pool,
                                hooks,
                                receipt.id,
                                receipt.publication_after.as_deref() == Some("published"),
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
                        return Ok(ActionCommitOutcome {
                            response: PluginRunResponse {
                                success: true,
                                data: Some(serde_json::json!({
                                    "output": exec_out.output,
                                    "lifecycle": lifecycle_data_value(&receipt, "restore"),
                                })),
                                error: None,
                                code: None,
                                correlation_id,
                                confirmation: None,
                            },
                            committed: Some(receipt),
                            secondary: SecondaryOutcome {
                                projection,
                                hook_enqueued: 0,
                                hook_dropped: 0,
                            },
                        });
                    }

                    // Purge or immediate-mode public Restore requires explicit host-owned confirmation.
                    let site_id = crate::storage::auth::compute_site_instance_id(&self.db_pool)
                        .await
                        .unwrap_or_else(|| "default-site".into());
                    let display = self
                        .confirmation_store
                        .insert(
                            maintenance_epoch,
                            NewPendingConfirmation {
                                command,
                                site_id,
                                session_token_hash: actor.token_hash.clone(),
                                actor_id: actor.user.id,
                                actor_role: actor.user.role,
                                plugin_id: id.to_string(),
                                action_id: action_id.to_string(),
                                expected_generation: revision_generation,
                                origin,
                                correlation_id: correlation_id.clone(),
                                initiating_maintenance_epoch: maintenance_epoch,
                                locale: preview.locale,
                                title: preview.title,
                                slug: preview.slug,
                            },
                        )
                        .map_err(|err| match err {
                            ConfirmationStoreError::ScopeAlreadyPending => mk_err(
                                StatusCode::CONFLICT,
                                "confirmation_pending_limit",
                                "Đang có một yêu cầu xác nhận chưa hoàn tất cho phiên và action này.".into(),
                            ),
                            ConfirmationStoreError::GlobalCapacityReached => mk_err(
                                StatusCode::TOO_MANY_REQUESTS,
                                "confirmation_capacity_reached",
                                "Quá nhiều yêu cầu xác nhận đang chờ trên hệ thống; hãy thử lại sau.".into(),
                            ),
                            ConfirmationStoreError::ItemTooLarge => mk_err(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "confirmation_too_large",
                                "Dữ liệu xác nhận vượt quá giới hạn cho phép.".into(),
                            ),
                            ConfirmationStoreError::LockPoisoned => mk_err(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "confirmation_store_error",
                                "Lỗi nội bộ bộ nhớ xác nhận.".into(),
                            ),
                        })?;

                    let prompt_msg = if domain_op == LifecycleOperation::Purge {
                        "Thao tác xóa vĩnh viễn nội dung cần xác nhận rõ ràng không thể hoàn tác từ Quản trị viên."
                    } else {
                        "Khôi phục nội dung chế độ lưu ngay sẽ hiển thị công khai ngay lập tức; cần xác nhận rõ ràng."
                    };
                    return Err((
                        StatusCode::CONFLICT,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some(prompt_msg.into()),
                            code: Some("confirmation_required".into()),
                            correlation_id,
                            confirmation: Some(display),
                        },
                    ));
                }
            }
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

        Ok(ActionCommitOutcome {
            response: PluginRunResponse {
                success: true,
                data: Some(exec_out.output),
                error: None,
                code: None,
                correlation_id,
                confirmation: None,
            },
            committed: None,
            secondary: SecondaryOutcome::none(),
        })
    }

    /// Resolves a host-owned single-use lifecycle confirmation (`confirm` or `cancel`)
    /// without ever re-invoking guest Wasm or accepting replacement targets.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_plugin_lifecycle_confirmation(
        &self,
        id: &str,
        action_id: &str,
        actor: &crate::auth::SessionContext,
        confirmation_id: &str,
        decision: &str,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: String,
        maintenance_epoch: u64,
    ) -> Result<ActionCommitOutcome, (StatusCode, PluginRunResponse)> {
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.into()),
                    correlation_id: correlation_id.clone(),
                    confirmation: None,
                },
            )
        };

        if self.recovery.is_safe_mode() {
            return Err(mk_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "safe_mode",
                "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.".into(),
            ));
        }
        if !matches!(decision, "confirm" | "cancel") {
            return Err(mk_err(
                StatusCode::BAD_REQUEST,
                "invalid_decision",
                "Quyết định xác nhận chỉ chấp nhận 'confirm' hoặc 'cancel'.".into(),
            ));
        }

        let site_id = crate::storage::auth::compute_site_instance_id(&self.db_pool)
            .await
            .unwrap_or_else(|| "default-site".into());
        let pending_opt = self
            .confirmation_store
            .consume(
                confirmation_id,
                &site_id,
                &actor.token_hash,
                actor.user.id,
                id,
                action_id,
                maintenance_epoch,
            )
            .map_err(|_| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "confirmation_store_error",
                    "Lỗi nội bộ bộ nhớ xác nhận.".into(),
                )
            })?;

        let Some(pending) = pending_opt else {
            return Err(mk_err(
                StatusCode::CONFLICT,
                "confirmation_invalid",
                "Yêu cầu xác nhận không hợp lệ, đã hết hạn hoặc đã được sử dụng.".into(),
            ));
        };

        if decision == "cancel" {
            return Ok(ActionCommitOutcome {
                response: PluginRunResponse {
                    success: false,
                    data: None,
                    error: None,
                    code: Some("confirmation_cancelled".into()),
                    correlation_id,
                    confirmation: None,
                },
                committed: None,
                secondary: SecondaryOutcome::none(),
            });
        }

        // Reauthorize installed plugin, manifest, generation, capability, and origin before commit.
        if actor.user.role != pending.actor_role {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "rbac_revoked",
                "Quyền của tài khoản đã thay đổi kể từ khi tạo yêu cầu xác nhận.".into(),
            ));
        }
        let row_opt = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
            .await
            .map_err(|e| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    format!("Lỗi cơ sở dữ liệu: {e}"),
                )
            })?;
        let Some((_, manifest_json, enabled, revision_generation, _)) = row_opt else {
            return Err(mk_err(
                StatusCode::CONFLICT,
                "generation_changed",
                "Plugin đã bị gỡ hoặc thay đổi phiên bản.".into(),
            ));
        };
        if !enabled || revision_generation != pending.expected_generation {
            return Err(mk_err(
                StatusCode::CONFLICT,
                "generation_changed",
                "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc chờ xác nhận.".into(),
            ));
        }
        let manifest = serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
            mk_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "corrupt_manifest",
                format!("Manifest không hợp lệ: {e}"),
            )
        })?;
        let Some(action_decl) = manifest.action(action_id).cloned() else {
            return Err(mk_err(
                StatusCode::NOT_FOUND,
                "action_not_found",
                "Action không tồn tại trong manifest.".into(),
            ));
        };
        if action_decl.required_role == "admin" && actor.user.role != crate::auth::Role::Admin {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "rbac_denied",
                "Tài khoản không đủ quyền thực thi action này.".into(),
            ));
        }
        let (required_cap, has_manifest_grant, evidence, op_label) = match pending.command.operation
        {
            LifecycleOperation::Purge => (
                "content_purge",
                manifest.can_purge_content_type(&pending.command.type_key),
                LifecycleConfirmationEvidence::ExplicitPurge {
                    type_key: pending.command.type_key.clone(),
                    entry_id: pending.command.entry_id,
                    expected_version: pending.command.expected_version,
                    session_token_hash: actor.token_hash.clone(),
                },
                "purge",
            ),
            LifecycleOperation::Restore => (
                "content_restore",
                manifest.can_restore_content_type(&pending.command.type_key),
                LifecycleConfirmationEvidence::PublicRestore {
                    type_key: pending.command.type_key.clone(),
                    entry_id: pending.command.entry_id,
                    expected_version: pending.command.expected_version,
                    session_token_hash: actor.token_hash.clone(),
                },
                "restore",
            ),
            LifecycleOperation::Trash => {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "confirmation_invalid",
                    "Yêu cầu xác nhận không hợp lệ, đã hết hạn hoặc đã được sử dụng.".into(),
                ));
            }
        };
        if !action_decl.capabilities.iter().any(|c| c == required_cap)
            || !has_manifest_grant
            || action_decl.capabilities.iter().any(|c| c == "http")
        {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "capability_denied",
                format!("Plugin không còn quyền '{required_cap}' hợp lệ."),
            ));
        }

        if let LifecycleActionOrigin::ProviderRow { type_key, entry_id } = &pending.origin {
            if type_key != &pending.command.type_key || *entry_id != pending.command.entry_id {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "confirmation_invalid",
                    "Yêu cầu xác nhận không khớp phạm vi hàng dữ liệu ban đầu.".into(),
                ));
            }
            let actor_role = match actor.user.role {
                crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
                crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
            };
            let list_cfg = crate::storage::admin_extensions::effective_list(
                &self.db_pool,
                type_key,
                actor_role,
            )
            .await
            .map_err(|err| mk_err(err.0, "provider_config_error", err.1.to_string()))?;
            if !list_cfg.columns.iter().any(|col| {
                col.owner_kind == "plugin"
                    && col.owner_id == id
                    && col.action_id.as_deref() == Some(action_id)
            }) {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "provider_action_revoked",
                    "Action không còn thuộc cấu hình cột hiệu lực.".into(),
                ));
            }
        }

        let fence = crate::storage::entries::PluginActionFence {
            plugin_id: id.to_string(),
            expected_generation: pending.expected_generation,
            required_role: action_decl.required_role.clone(),
        };
        let authority = LifecycleAuthority {
            session: actor,
            plugin_fence: Some(&fence),
            confirmation: evidence,
            scope: LifecycleScope::Ordinary,
        };
        let receipt = crate::storage::entry_lifecycle::execute_lifecycle_transition(
            &self.db_pool,
            &pending.command,
            &authority,
        )
        .await
        .map_err(|err| mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string()))?;

        let projection = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
            &self.db_pool,
            hooks,
            receipt.id,
            pending.command.operation == LifecycleOperation::Restore
                && receipt.publication_after.as_deref() == Some("published"),
        )
        .await;
        plugin_storage::record_plugin_execution_result(
            &self.db_pool,
            id,
            &correlation_id,
            "info",
            "Xác nhận và hoàn tất thao tác vòng đời thành công",
            None,
        )
        .await;

        Ok(ActionCommitOutcome {
            response: PluginRunResponse {
                success: true,
                data: Some(serde_json::json!({
                    "lifecycle": lifecycle_data_value(&receipt, op_label),
                })),
                error: None,
                code: None,
                correlation_id,
                confirmation: None,
            },
            committed: Some(receipt),
            secondary: SecondaryOutcome {
                projection,
                hook_enqueued: 0,
                hook_dropped: 0,
            },
        })
    }

    pub fn plugins_root(&self) -> &std::path::Path {
        &self.plugins_root
    }

    pub fn secrets_root(&self) -> &std::path::Path {
        &self.secrets_root
    }

    #[allow(dead_code)]
    pub fn plugin_secrets_dir(&self, plugin_id: &str) -> PathBuf {
        self.secrets_root.join("plugins").join(plugin_id)
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
            package.admin_extensions.as_ref(),
            &package.blocks,
        )
        .await?;
        self.invalidate_cache().await;

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
        let cas =
            plugin_storage::set_plugin_enabled_cas(&self.db_pool, id, false, expected_cas_version)
                .await?;
        self.invalidate_cache().await;
        Ok(cas)
    }

    pub async fn enable_plugin_cas(
        &self,
        id: &str,
        expected_cas_version: Option<i64>,
    ) -> Result<i64, plugin_storage::PluginStorageError> {
        let cas =
            plugin_storage::set_plugin_enabled_cas(&self.db_pool, id, true, expected_cas_version)
                .await?;
        self.invalidate_cache().await;
        Ok(cas)
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginSummary>, String> {
        plugin_storage::list_plugins(&self.db_pool).await
    }

    pub async fn get_plugin(&self, id: &str) -> Result<Option<PluginSummary>, String> {
        plugin_storage::get_plugin(&self.db_pool, id).await
    }

    pub async fn disable_plugin(&self, id: &str) -> Result<(), String> {
        plugin_storage::set_plugin_enabled(&self.db_pool, id, false).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    pub async fn enable_plugin(&self, id: &str) -> Result<(), String> {
        plugin_storage::set_plugin_enabled(&self.db_pool, id, true).await?;
        self.invalidate_cache().await;
        Ok(())
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

    pub async fn hook_subscriptions(&self, event: HookEvent) -> Result<Vec<String>, String> {
        let cache = self.ensure_manifest_cache().await?;
        Ok(cache
            .into_iter()
            .filter_map(|(id, item)| item.manifest.hooks.contains(&event).then_some(id))
            .collect())
    }

    pub async fn content_hook_subscriptions(
        &self,
        event: HookEvent,
        type_key: &str,
    ) -> Result<Vec<(i32, String)>, String> {
        let cache = self.ensure_manifest_cache().await?;
        let mut subs: Vec<(i32, String)> = cache
            .into_iter()
            .filter_map(|(id, item)| {
                (item.manifest.hooks.contains(&event)
                    && item.manifest.can_read_content_type(type_key))
                .then_some((item.manifest.hook_priority, id))
            })
            .collect();
        subs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(subs)
    }

    #[allow(dead_code)]
    pub async fn filter_subscriptions(
        &self,
        event: HookEvent,
        type_key: Option<&str>,
    ) -> Result<Vec<(i32, String)>, String> {
        let cache = self.ensure_manifest_cache().await?;
        let mut subs: Vec<(i32, String)> = cache
            .into_iter()
            .filter_map(|(id, item)| {
                let matches_type =
                    type_key.is_none_or(|tk| item.manifest.can_read_content_type(tk));
                (item.manifest.filters.contains(&event) && matches_type)
                    .then_some((item.manifest.hook_priority, id))
            })
            .collect();
        subs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(subs)
    }

    pub async fn filter_content_payload_for_plugin(
        &self,
        plugin_id: &str,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: &mut Value,
    ) {
        let manifest_opt = match self.ensure_manifest_cache().await {
            Ok(cache) => cache.get(plugin_id).map(|c| c.manifest.clone()),
            Err(_) => None,
        };
        let schema_opt =
            crate::storage::content::load_effective_content_schema(&self.db_pool, type_key)
                .await
                .ok()
                .flatten();

        for field_map_key in ["candidate_values", "values"] {
            if let Some(Value::Object(map)) = payload.get_mut(field_map_key) {
                match (&manifest_opt, &schema_opt) {
                    (Some(manifest), Some(effective)) => {
                        map.retain(|k, _| {
                            effective.effective_fields.get(k).is_some_and(|fe| {
                                opendash_core::plugin::plugin_can_read_field(
                                    manifest, None, actor_role, type_key, fe,
                                )
                            })
                        });
                    }
                    _ => {
                        map.clear();
                    }
                }
            }
        }
    }

    async fn filter_action_input_for_plugin(
        &self,
        manifest: &PluginManifest,
        action_caps: &[String],
        actor_role: opendash_core::content::FieldRole,
        input: &mut Value,
    ) {
        let Some(type_key) = input
            .get("type_key")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return;
        };
        let schema_opt =
            crate::storage::content::load_effective_content_schema(&self.db_pool, &type_key)
                .await
                .ok()
                .flatten();
        if let Some(Value::Object(map)) = input.get_mut("values") {
            match &schema_opt {
                Some(effective) => {
                    map.retain(|k, _| {
                        effective.effective_fields.get(k).is_some_and(|fe| {
                            opendash_core::plugin::plugin_can_read_field(
                                manifest,
                                Some(action_caps),
                                actor_role,
                                &type_key,
                                fe,
                            )
                        })
                    });
                }
                None => {
                    map.clear();
                }
            }
        }
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
        execution_mode: HookExecutionMode,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        self.run_plugin_with_principal(
            id,
            None,
            input,
            correlation_id,
            execution_mode,
            ExecutionPrincipal::Admin,
            timeout_override,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_dispatch(
        &self,
        id: &str,
        kind: EnvelopeKind,
        legacy_input: Value,
        event_str: &str,
        occurred_at_ms: u64,
        correlation_id: String,
        locale: Option<String>,
        principal: ExecutionPrincipal,
        dispatch_depth: u8,
        payload: Value,
        execution_mode: HookExecutionMode,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        if matches!(kind, EnvelopeKind::Route | EnvelopeKind::Cron) {
            return Err((
                StatusCode::BAD_REQUEST,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some("Envelope kind route/cron chưa mở ở V1.".into()),
                    code: Some("unsupported_envelope_kind".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }
        let v2_envelope = serde_json::to_value(PluginEnvelopeV2::new(
            kind,
            event_str,
            occurred_at_ms,
            correlation_id.clone(),
            locale,
            principal,
            dispatch_depth,
            payload,
        ))
        .unwrap_or(Value::Null);
        self.run_plugin_with_principal(
            id,
            Some(v2_envelope),
            legacy_input,
            correlation_id,
            execution_mode,
            principal,
            timeout_override,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_plugin_with_principal(
        &self,
        id: &str,
        v2_input: Option<Value>,
        legacy_input: Value,
        correlation_id: String,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
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
                        confirmation: None,
                    },
                )
            })?;

        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(
                        "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.".into(),
                    ),
                    code: Some("safe_mode".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }
        let Some((wasm_blob, manifest_json, enabled, revision_generation, revision_digest_opt)) =
            row_opt
        else {
            return Err((
                StatusCode::NOT_FOUND,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some("Plugin không tồn tại.".into()),
                    code: Some("not_found".into()),
                    correlation_id,
                    confirmation: None,
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
                    confirmation: None,
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
                    confirmation: None,
                },
            )
        })?;

        if let Some(digest) = &revision_digest_opt {
            if let Err(err) =
                plugin_storage::verify_plugin_revision_durability(&self.plugins_root, digest)
            {
                return Err((
                    StatusCode::CONFLICT,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err),
                        code: Some("corrupt_revision".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }
        }

        let input = if manifest.manifest_schema >= 3 {
            v2_input.unwrap_or(legacy_input)
        } else {
            legacy_input
        };

        let run_res = self
            .runtime
            .run_plugin_action_with_principal(
                wasm_blob,
                manifest,
                input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(revision_generation),
                execution_mode,
                principal,
                timeout_override,
                None,
                None,
            )
            .await
            .map(|r| r.output);

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
                    confirmation: None,
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
                        confirmation: None,
                    },
                ))
            }
        }
    }
}
