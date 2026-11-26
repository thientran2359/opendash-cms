use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::http::StatusCode;
use opendash_core::hooks::{
    EnvelopeKind, ExecutionPrincipal, ExtensionOwnerKind, ExtensionRuntimeKind, HookEvent,
    HookExecutionMode, PluginEnvelopeV2,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::storage::{
    entries::OwnerFence,
    entry_lifecycle::{
        EntryCommitReceipt, EntryMutationKind, LifecycleAuthority, LifecycleCommand,
        LifecycleConfirmationEvidence, LifecycleOperation, LifecycleScope, SecondaryOutcome,
    },
    plugin as plugin_storage,
};

use super::{
    host_ops::{RuntimeOwner, StagedLifecycleOperation},
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

#[derive(Debug, Clone)]
pub struct CachedActiveTheme {
    pub theme_id: String,
    pub revision_id: String,
    #[allow(dead_code)]
    pub revision_digest: String,
    pub selection_version: i64,
    pub manifest: PluginManifest,
    pub js_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionSubscriber {
    pub priority: i32,
    pub owner: RuntimeOwner,
    pub theme_fence: Option<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFilterSubscriber {
    pub priority: i32,
    pub owner: RuntimeOwner,
    pub revision_key: String,
    pub render_cache: opendash_core::plugin::RenderCachePolicy,
    pub render_trusted_html: bool,
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
    leases: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<RuntimeOwner>>>,
    owner: RuntimeOwner,
}

impl Drop for ActionLeaseGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.leases.lock() {
            guard.remove(&self.owner);
        }
    }
}

pub struct PluginManager {
    db_pool: SqlitePool,
    runtime: PluginRuntime,
    active_action_leases: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<RuntimeOwner>>>,
    confirmation_store: Arc<LifecycleConfirmationStore>,
    allow_loopback: bool,
    plugins_root: PathBuf,
    themes_root: PathBuf,
    secrets_root: PathBuf,
    manifest_cache: Arc<RwLock<Option<BTreeMap<String, CachedActivePlugin>>>>,
    active_theme_cache: Arc<RwLock<Option<Option<CachedActiveTheme>>>>,
    recovery: crate::config::RecoveryState,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

impl PluginManager {
    pub fn new(db_pool: SqlitePool, allow_loopback: bool) -> Self {
        let (plugins_root, themes_root, secrets_root) = match crate::paths::SitePaths::from_env() {
            Ok(p) => (p.plugins_dir(), p.themes_dir(), p.secrets_dir()),
            Err(_) => (
                std::env::temp_dir().join("opendash_plugins_default"),
                std::env::temp_dir().join("opendash_themes_default"),
                std::env::temp_dir().join("opendash_secrets_default"),
            ),
        };
        Self::with_roots(db_pool, allow_loopback, plugins_root, secrets_root)
            .with_themes_root(themes_root)
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
        let themes_root = plugins_root
            .parent()
            .unwrap_or(&plugins_root)
            .join("themes");
        Self::with_roots(db_pool, allow_loopback, plugins_root, secrets_root)
            .with_themes_root(themes_root)
    }

    pub fn with_roots(
        db_pool: SqlitePool,
        allow_loopback: bool,
        plugins_root: PathBuf,
        secrets_root: PathBuf,
    ) -> Self {
        let themes_root = plugins_root
            .parent()
            .unwrap_or(&plugins_root)
            .join("themes");
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
            themes_root,
            secrets_root,
            manifest_cache: Arc::new(RwLock::new(None)),
            active_theme_cache: Arc::new(RwLock::new(None)),
            recovery: crate::config::RecoveryState::new(false),
        }
    }

    pub fn with_themes_root(mut self, themes_root: PathBuf) -> Self {
        self.themes_root = themes_root;
        self
    }

    #[allow(dead_code)]
    pub fn themes_root(&self) -> &std::path::Path {
        &self.themes_root
    }

    pub fn engine(&self) -> &wasmi::Engine {
        self.runtime.engine()
    }

    pub async fn invalidate_cache(&self) {
        *self.manifest_cache.write().await = None;
    }

    pub async fn invalidate_active_theme_cache(&self) {
        *self.active_theme_cache.write().await = None;
    }

    async fn ensure_active_theme_cache(&self) -> Result<Option<CachedActiveTheme>, String> {
        if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
            && dev.dev_theme_id().is_some()
        {
            return match dev.load_dev_theme() {
                Ok(Some(snap)) => {
                    let Some(runtime_manifest) = snap.manifest.to_runtime_manifest() else {
                        return Ok(None);
                    };
                    let Some(js_bytes) = snap.functions_js else {
                        return Ok(None);
                    };
                    let db_sel = crate::storage::theme::get_active_selection(&self.db_pool)
                        .await
                        .map_err(|e| e.1.to_string())?;
                    let revision_id =
                        if db_sel.theme_id.as_deref() == Some(snap.manifest.theme_id.as_str()) {
                            db_sel
                                .revision_id
                                .unwrap_or_else(|| format!("dev:{}", snap.manifest.theme_id))
                        } else {
                            format!("dev:{}", snap.manifest.theme_id)
                        };
                    Ok(Some(CachedActiveTheme {
                        theme_id: snap.manifest.theme_id,
                        revision_id,
                        revision_digest: snap.revision_digest,
                        selection_version: db_sel.version,
                        manifest: runtime_manifest,
                        js_bytes,
                    }))
                }
                Ok(None) => Ok(None),
                Err(err) => {
                    eprintln!("Dev theme functions load failed: {err}");
                    Ok(None)
                }
            };
        }
        {
            let guard = self.active_theme_cache.read().await;
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        let snap_opt = crate::storage::theme::load_active_theme_functions_snapshot(&self.db_pool)
            .await
            .map_err(|e| e.1.to_string())?;
        let built = match snap_opt {
            None => None,
            Some(snap) => {
                let parsed: opendash_core::theme::ThemeV3Manifest =
                    match serde_json::from_str(&snap.manifest_json) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Invalid active theme manifest {}: {e}", snap.theme_id);
                            *self.active_theme_cache.write().await = Some(None);
                            return Ok(None);
                        }
                    };
                match parsed.to_runtime_manifest() {
                    None => None,
                    Some(runtime_manifest) => {
                        let rev_dir = self
                            .themes_root
                            .join("revisions")
                            .join(&snap.revision_digest);
                        let marker = rev_dir.join(".complete");
                        let marker_ok = std::fs::read_to_string(&marker)
                            .ok()
                            .is_some_and(|s| s.trim() == snap.revision_digest);
                        let func_path = rev_dir.join("functions.js");
                        let no_symlink = crate::secrets::ensure_no_reparse_or_symlink(&rev_dir)
                            .is_ok()
                            && crate::secrets::ensure_no_reparse_or_symlink(&func_path).is_ok();
                        let expected_hex =
                            snap.file_digests.get("functions.js").map(String::as_str);
                        let js_bytes_opt = if marker_ok && no_symlink {
                            std::fs::read(&func_path).ok().filter(|bytes| {
                                expected_hex == Some(sha256_hex(bytes).as_str())
                                    && std::str::from_utf8(bytes).is_ok()
                            })
                        } else {
                            None
                        };
                        js_bytes_opt.map(|js_bytes| CachedActiveTheme {
                            theme_id: snap.theme_id,
                            revision_id: snap.revision_id,
                            revision_digest: snap.revision_digest,
                            selection_version: snap.selection_version,
                            manifest: runtime_manifest,
                            js_bytes,
                        })
                    }
                }
            }
        };
        *self.active_theme_cache.write().await = Some(built.clone());
        Ok(built)
    }

    pub async fn active_theme_manifest(&self) -> Option<(String, String, i64, PluginManifest)> {
        let cached = self.ensure_active_theme_cache().await.ok()??;
        Some((
            cached.theme_id,
            cached.revision_id,
            cached.selection_version,
            cached.manifest,
        ))
    }

    pub async fn cached_owner_manifest(&self, owner: &RuntimeOwner) -> Option<PluginManifest> {
        match owner.kind {
            ExtensionOwnerKind::Plugin => self.cached_plugin_manifest(&owner.id).await,
            ExtensionOwnerKind::Theme => {
                let cached = self.ensure_active_theme_cache().await.ok()??;
                (cached.theme_id == owner.id).then_some(cached.manifest)
            }
        }
    }

    async fn ensure_manifest_cache(&self) -> Result<BTreeMap<String, CachedActivePlugin>, String> {
        let dev_overlay = crate::dev_mode::DevOverlayConfig::global();
        if dev_overlay.is_none() {
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
            if row.wasm_blob.is_empty() && row.revision_digest.is_none() {
                // Dev-only placeholder synced via /api/admin/dev/sync; only active when mounted via --dev-plugin.
                continue;
            }
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
        if let Some(dev) = dev_overlay {
            for snap in dev.load_all_dev_plugins_best_effort(self.runtime.engine()) {
                let db_gen = built
                    .get(&snap.manifest.id)
                    .map(|existing| existing.revision_generation)
                    .unwrap_or(0);
                built.insert(
                    snap.manifest.id.clone(),
                    CachedActivePlugin {
                        manifest: snap.manifest,
                        wasm_blob: snap.code_bytes,
                        revision_generation: db_gen,
                        revision_digest: Some(snap.revision_digest),
                    },
                );
            }
            return Ok(built);
        }
        *self.manifest_cache.write().await = Some(built.clone());
        Ok(built)
    }

    pub async fn cached_plugin_manifest(&self, id: &str) -> Option<PluginManifest> {
        let cache = self.ensure_manifest_cache().await.ok()?;
        cache.get(id).map(|item| item.manifest.clone())
    }

    #[allow(dead_code)]
    pub async fn filter_public_content_payload_for_plugin(
        &self,
        plugin_id: &str,
        type_key: &str,
        payload: &mut Value,
    ) {
        self.filter_public_content_payload_for_owner(
            &RuntimeOwner::plugin(plugin_id),
            type_key,
            payload,
        )
        .await;
    }

    pub async fn filter_public_content_payload_for_owner(
        &self,
        owner: &RuntimeOwner,
        type_key: &str,
        payload: &mut Value,
    ) {
        let manifest_opt = self.cached_owner_manifest(owner).await;
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
                                    && opendash_core::plugin::plugin_can_read_field_for_owner(
                                        owner.kind,
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
        self.execute_owner_action(
            ExtensionOwnerKind::Plugin,
            Some(id),
            action_id,
            actor,
            input,
            content_registry,
            hooks,
            correlation_id,
            origin,
            maintenance_epoch,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_owner_action(
        &self,
        owner_kind: ExtensionOwnerKind,
        id_opt: Option<&str>,
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

        if self.recovery.is_safe_mode() {
            return Err(mk_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "safe_mode",
                "Hệ thống đang ở safe mode sau khi khôi phục; gói mở rộng bị tạm khóa.".into(),
            ));
        }

        let (owner_id, code_bytes, manifest, owner_fence, runtime_kind) = match owner_kind {
            ExtensionOwnerKind::Plugin => {
                let id = id_opt.ok_or_else(|| {
                    mk_err(
                        StatusCode::NOT_FOUND,
                        "not_found",
                        "Plugin không tồn tại.".into(),
                    )
                })?;
                if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
                    && dev.has_dev_plugin(id)
                {
                    let snap = dev
                        .load_dev_plugin_by_id(id, self.runtime.engine())
                        .map_err(|err| mk_err(StatusCode::CONFLICT, "dev_plugin_invalid", err))?
                        .ok_or_else(|| {
                            mk_err(
                                StatusCode::NOT_FOUND,
                                "not_found",
                                "Plugin không tồn tại.".into(),
                            )
                        })?;
                    let db_gen =
                        plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|(_, _, enabled, gen_val, _)| enabled.then_some(gen_val))
                            .unwrap_or(0);
                    let rt_kind = snap.manifest.runtime_kind();
                    (
                        id.to_owned(),
                        snap.code_bytes,
                        snap.manifest,
                        OwnerFence::Plugin {
                            plugin_id: id.to_owned(),
                            expected_generation: db_gen,
                        },
                        rt_kind,
                    )
                } else {
                    let row_opt =
                        plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                            .await
                            .map_err(|e| {
                                mk_err(
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    "db_error",
                                    format!("Lỗi cơ sở dữ liệu: {e}"),
                                )
                            })?;
                    let Some((
                        wasm_blob,
                        manifest_json,
                        enabled,
                        revision_generation,
                        revision_digest_opt,
                    )) = row_opt
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
                    if wasm_blob.is_empty() && revision_digest_opt.is_none() {
                        return Err(mk_err(
                            StatusCode::NOT_FOUND,
                            "dev_plugin_unmounted",
                            "Plugin chỉ có ở chế độ dev; hãy khởi chạy kèm --dev-plugin hoặc cài bản ZIP chính thức.".into(),
                        ));
                    }
                    let manifest =
                        serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
                            mk_err(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "corrupt_manifest",
                                format!("Manifest không hợp lệ: {e}"),
                            )
                        })?;
                    if let Some(digest) = &revision_digest_opt
                        && let Err(err) = plugin_storage::verify_plugin_revision_durability(
                            &self.plugins_root,
                            digest,
                        )
                    {
                        return Err(mk_err(StatusCode::CONFLICT, "corrupt_revision", err));
                    }
                    let rt_kind = manifest.runtime_kind();
                    (
                        id.to_owned(),
                        wasm_blob,
                        manifest,
                        OwnerFence::Plugin {
                            plugin_id: id.to_owned(),
                            expected_generation: revision_generation,
                        },
                        rt_kind,
                    )
                }
            }
            ExtensionOwnerKind::Theme => {
                let cached = self
                    .ensure_active_theme_cache()
                    .await
                    .map_err(|e| mk_err(StatusCode::INTERNAL_SERVER_ERROR, "theme_cache_error", e))?
                    .ok_or_else(|| {
                        mk_err(
                            StatusCode::NOT_FOUND,
                            "theme_functions_not_found",
                            "Theme hiện hành không có functions.js.".into(),
                        )
                    })?;
                if let Some(expected_id) = id_opt
                    && cached.theme_id != expected_id
                {
                    return Err(mk_err(
                        StatusCode::NOT_FOUND,
                        "theme_not_active",
                        "Theme không còn đang được kích hoạt.".into(),
                    ));
                }
                let fence = OwnerFence::Theme {
                    theme_id: cached.theme_id.clone(),
                    revision_id: cached.revision_id.clone(),
                    selection_version: cached.selection_version,
                };
                (
                    cached.theme_id,
                    cached.js_bytes,
                    cached.manifest,
                    fence,
                    ExtensionRuntimeKind::Js,
                )
            }
        };

        let owner = RuntimeOwner {
            kind: owner_kind,
            id: owner_id.clone(),
        };
        let _lease_guard = {
            if self.recovery.is_safe_mode() {
                return Err(mk_err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "safe_mode",
                    "Hệ thống đang ở safe mode sau khi khôi phục; gói mở rộng bị tạm khóa.".into(),
                ));
            }
            let mut leases = self.active_action_leases.lock().map_err(|_| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "lease_lock",
                    "Lỗi khóa action lease.".into(),
                )
            })?;
            if !leases.insert(owner.clone()) {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "recursive_dispatch",
                    "Không cho phép gọi đệ quy action trên cùng gói mở rộng.".into(),
                ));
            }
            ActionLeaseGuard {
                leases: std::sync::Arc::clone(&self.active_action_leases),
                owner: owner.clone(),
            }
        };

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
                "content_read" => {
                    !manifest.permissions.content_read.is_empty()
                        || manifest.permissions.content_read_all_public
                }
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
                    format!("Gói mở rộng thiếu quyền manifest cho capability '{cap}'."),
                ));
            }
        }

        let mut filtered_input = input;
        self.filter_action_input_for_owner(
            owner_kind,
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
            .run_owner_action_with_principal(
                owner_kind,
                runtime_kind,
                code_bytes,
                manifest.clone(),
                runtime_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(owner_fence.clone()),
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
                plugin_storage::record_owner_execution_result(
                    &self.db_pool,
                    owner_kind,
                    &owner_id,
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
                    "Gói mở rộng không có quyền ghi loại nội dung này.".into(),
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
                if !opendash_core::plugin::plugin_can_write_field_for_owner(
                    owner_kind,
                    &manifest,
                    Some(&action_decl.capabilities),
                    actor_role,
                    &cmd.type_key,
                    field_entry,
                ) {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "field_edit_forbidden",
                        format!(
                            "Gói mở rộng hoặc tài khoản không có quyền ghi trường '{field_key}'."
                        ),
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
                owner_fence: owner_fence.clone(),
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
            plugin_storage::record_owner_execution_result(
                &self.db_pool,
                owner_kind,
                &owner_id,
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
                    format!("Gói mở rộng không có quyền '{required_cap}' cho loại nội dung này."),
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
                owner_fence: owner_fence.clone(),
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
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        owner_kind,
                        &owner_id,
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
                        plugin_storage::record_owner_execution_result(
                            &self.db_pool,
                            owner_kind,
                            &owner_id,
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

                    // Purge or immediate-mode public Restore requires explicit host-owned confirmation
                    // (scoped to Plugin actions).
                    let OwnerFence::Plugin {
                        expected_generation: revision_generation,
                        ..
                    } = owner_fence
                    else {
                        return Err(mk_err(
                            StatusCode::FORBIDDEN,
                            "content_lifecycle_denied",
                            "Xác nhận thao tác vòng đời chỉ hỗ trợ cho plugin đã khai báo.".into(),
                        ));
                    };
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
                                plugin_id: owner_id.clone(),
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

        plugin_storage::record_owner_execution_result(
            &self.db_pool,
            owner_kind,
            &owner_id,
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
            owner_fence: OwnerFence::Plugin {
                plugin_id: id.to_string(),
                expected_generation: pending.expected_generation,
            },
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

    #[allow(dead_code)]
    pub async fn hook_subscriptions(&self, event: HookEvent) -> Result<Vec<String>, String> {
        let cache = self.ensure_manifest_cache().await?;
        Ok(cache
            .into_iter()
            .filter_map(|(id, item)| item.manifest.hooks.contains(&event).then_some(id))
            .collect())
    }

    pub async fn owner_hook_subscriptions(
        &self,
        event: HookEvent,
    ) -> Result<Vec<ExtensionSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            if item.manifest.hooks.contains(&event) {
                subs.push(ExtensionSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    theme_fence: None,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await?
            && theme.manifest.hooks.contains(&event)
        {
            subs.push(ExtensionSubscriber {
                priority: theme.manifest.hook_priority,
                owner: RuntimeOwner::theme(theme.theme_id),
                theme_fence: Some((theme.revision_id, theme.selection_version)),
            });
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }

    #[allow(dead_code)]
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

    pub async fn owner_content_hook_subscriptions(
        &self,
        event: HookEvent,
        type_key: &str,
    ) -> Result<Vec<ExtensionSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            if item.manifest.hooks.contains(&event) && item.manifest.can_read_content_type(type_key)
            {
                subs.push(ExtensionSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    theme_fence: None,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await?
            && theme.manifest.hooks.contains(&event)
            && theme.manifest.can_read_content_type(type_key)
        {
            subs.push(ExtensionSubscriber {
                priority: theme.manifest.hook_priority,
                owner: RuntimeOwner::theme(theme.theme_id),
                theme_fence: Some((theme.revision_id, theme.selection_version)),
            });
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
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

    pub async fn owner_filter_subscriptions(
        &self,
        event: HookEvent,
        type_key: Option<&str>,
    ) -> Result<Vec<ExtensionSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            let matches_type = type_key.is_none_or(|tk| item.manifest.can_read_content_type(tk));
            if item.manifest.filters.contains(&event) && matches_type {
                subs.push(ExtensionSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    theme_fence: None,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await? {
            let matches_type = type_key.is_none_or(|tk| theme.manifest.can_read_content_type(tk));
            if theme.manifest.filters.contains(&event) && matches_type {
                subs.push(ExtensionSubscriber {
                    priority: theme.manifest.hook_priority,
                    owner: RuntimeOwner::theme(theme.theme_id),
                    theme_fence: Some((theme.revision_id, theme.selection_version)),
                });
            }
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }

    pub fn is_safe_mode(&self) -> bool {
        self.recovery.is_safe_mode()
    }

    pub async fn owner_render_filter_subscriptions(
        &self,
        event: HookEvent,
    ) -> Result<Vec<RenderFilterSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            if item.manifest.filters.contains(&event) {
                let revision_key = item
                    .revision_digest
                    .unwrap_or_else(|| format!("gen:{}", item.revision_generation));
                subs.push(RenderFilterSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    revision_key,
                    render_cache: item.manifest.render_cache,
                    render_trusted_html: item.manifest.permissions.render_trusted_html,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await?
            && theme.manifest.filters.contains(&event)
        {
            subs.push(RenderFilterSubscriber {
                priority: theme.manifest.hook_priority,
                owner: RuntimeOwner::theme(theme.theme_id),
                revision_key: format!("{}:v{}", theme.revision_digest, theme.selection_version),
                render_cache: theme.manifest.render_cache,
                render_trusted_html: theme.manifest.permissions.render_trusted_html,
            });
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }

    #[allow(dead_code)]
    pub async fn filter_content_payload_for_plugin(
        &self,
        plugin_id: &str,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: &mut Value,
    ) {
        self.filter_content_payload_for_owner(
            &RuntimeOwner::plugin(plugin_id),
            type_key,
            actor_role,
            payload,
        )
        .await;
    }

    pub async fn filter_content_payload_for_owner(
        &self,
        owner: &RuntimeOwner,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: &mut Value,
    ) {
        let manifest_opt = self.cached_owner_manifest(owner).await;
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
                                opendash_core::plugin::plugin_can_read_field_for_owner(
                                    owner.kind, manifest, None, actor_role, type_key, fe,
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

    async fn filter_action_input_for_owner(
        &self,
        owner_kind: ExtensionOwnerKind,
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
                            opendash_core::plugin::plugin_can_read_field_for_owner(
                                owner_kind,
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

    #[allow(dead_code)]
    pub async fn record_hook_failure(&self, id: &str, correlation_id: &str, message: &str) {
        self.record_owner_hook_failure(&RuntimeOwner::plugin(id), correlation_id, message)
            .await;
    }

    pub async fn record_owner_hook_failure(
        &self,
        owner: &RuntimeOwner,
        correlation_id: &str,
        message: &str,
    ) {
        plugin_storage::record_owner_execution_result(
            &self.db_pool,
            owner.kind,
            &owner.id,
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
        self.run_owner_dispatch(
            &RuntimeOwner::plugin(id),
            kind,
            legacy_input,
            event_str,
            occurred_at_ms,
            correlation_id,
            locale,
            principal,
            dispatch_depth,
            payload,
            execution_mode,
            timeout_override,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_owner_dispatch(
        &self,
        owner: &RuntimeOwner,
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
        if matches!(kind, EnvelopeKind::Cron) {
            return Err((
                StatusCode::BAD_REQUEST,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some("Envelope kind cron chưa mở ở V1.".into()),
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
        match owner.kind {
            ExtensionOwnerKind::Plugin => {
                self.run_plugin_with_principal(
                    &owner.id,
                    Some(v2_envelope),
                    legacy_input,
                    correlation_id,
                    execution_mode,
                    principal,
                    timeout_override,
                )
                .await
            }
            ExtensionOwnerKind::Theme => {
                self.run_theme_with_principal(
                    &owner.id,
                    v2_envelope,
                    correlation_id,
                    execution_mode,
                    principal,
                    timeout_override,
                )
                .await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_theme_with_principal(
        &self,
        expected_theme_id: &str,
        v2_input: Value,
        correlation_id: String,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(
                        "Hệ thống đang ở safe mode sau khi khôi phục; theme functions bị tạm khóa."
                            .into(),
                    ),
                    code: Some("safe_mode".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }
        let cached = match self.ensure_active_theme_cache().await {
            Ok(Some(c)) if c.theme_id == expected_theme_id => c,
            Ok(_) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some("Theme functions không tồn tại hoặc đã đổi theme.".into()),
                        code: Some("not_found".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }
            Err(err) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err),
                        code: Some("theme_cache_error".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }
        };

        let fence = OwnerFence::Theme {
            theme_id: cached.theme_id.clone(),
            revision_id: cached.revision_id.clone(),
            selection_version: cached.selection_version,
        };
        let run_res = self
            .runtime
            .run_owner_action_with_principal(
                ExtensionOwnerKind::Theme,
                ExtensionRuntimeKind::Js,
                cached.js_bytes,
                cached.manifest,
                v2_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(fence),
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
                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        ExtensionOwnerKind::Theme,
                        &cached.theme_id,
                        &correlation_id,
                        "info",
                        "Thực thi thành công",
                        None,
                    )
                    .await;
                }
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
                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        ExtensionOwnerKind::Theme,
                        &cached.theme_id,
                        &correlation_id,
                        "error",
                        &err_msg,
                        Some(&err_msg),
                    )
                    .await;
                }
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

        let (wasm_blob, manifest, revision_generation) = if let Some(dev) =
            crate::dev_mode::DevOverlayConfig::global()
            && dev.has_dev_plugin(id)
        {
            let snap = dev
                .load_dev_plugin_by_id(id, self.runtime.engine())
                .map_err(|err| {
                    (
                        StatusCode::CONFLICT,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some(err),
                            code: Some("dev_plugin_invalid".into()),
                            correlation_id: correlation_id.clone(),
                            confirmation: None,
                        },
                    )
                })?
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some("Plugin không tồn tại.".into()),
                            code: Some("not_found".into()),
                            correlation_id: correlation_id.clone(),
                            confirmation: None,
                        },
                    )
                })?;
            let db_gen = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                .await
                .ok()
                .flatten()
                .and_then(|(_, _, enabled, gen_val, _)| enabled.then_some(gen_val))
                .unwrap_or(0);
            (snap.code_bytes, snap.manifest, db_gen)
        } else {
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
            if wasm_blob.is_empty() && revision_digest_opt.is_none() {
                return Err((
                    StatusCode::NOT_FOUND,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(
                            "Plugin chỉ có ở chế độ dev; hãy khởi chạy kèm --dev-plugin hoặc cài bản ZIP chính thức."
                                .into(),
                        ),
                        code: Some("dev_plugin_unmounted".into()),
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
            (wasm_blob, manifest, revision_generation)
        };

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
                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_plugin_execution_result(
                        &self.db_pool,
                        id,
                        &correlation_id,
                        "info",
                        "Thực thi thành công",
                        None,
                    )
                    .await;
                }

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

                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_plugin_execution_result(
                        &self.db_pool,
                        id,
                        &correlation_id,
                        "error",
                        &err_msg,
                        Some(&err_msg),
                    )
                    .await;
                }

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

    pub async fn resolve_extension_route(
        &self,
        pkg: &str,
        method: opendash_core::plugin::RouteMethod,
        subpath: &str,
    ) -> Result<Option<ResolvedExtensionRoute>, (StatusCode, String)> {
        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Hệ thống đang ở safe mode sau khi khôi phục; route mở rộng tạm khóa.".into(),
            ));
        }
        if opendash_core::id::validate_package_id(pkg).is_err() {
            return Ok(None);
        }

        // 1. Check mounted dev plugin first, then enabled plugin in DB
        if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
            && dev.has_dev_plugin(pkg)
        {
            if let Ok(Some(snap)) = dev.load_dev_plugin_by_id(pkg, self.runtime.engine()) {
                let db_gen =
                    plugin_storage::load_plugin_executable_with_generation(&self.db_pool, pkg)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|(_, _, enabled, gen_val, _)| enabled.then_some(gen_val))
                        .unwrap_or(0);
                for route in &snap.manifest.routes {
                    if route.method == method
                        && let Some(params) =
                            opendash_core::plugin::match_route_path(&route.path, subpath)
                    {
                        let runtime_kind =
                            ExtensionRuntimeKind::from_entrypoint(&snap.manifest.entrypoint);
                        return Ok(Some(ResolvedExtensionRoute {
                            owner: RuntimeOwner::plugin(snap.manifest.id.clone()),
                            route: route.clone(),
                            params,
                            code_bytes: snap.code_bytes,
                            runtime_kind,
                            owner_fence: OwnerFence::Plugin {
                                plugin_id: snap.manifest.id.clone(),
                                expected_generation: db_gen,
                            },
                            manifest: snap.manifest,
                        }));
                    }
                }
            }
        } else if let Ok(Some((
            wasm_blob,
            manifest_json,
            enabled,
            revision_generation,
            revision_digest,
        ))) =
            plugin_storage::load_plugin_executable_with_generation(&self.db_pool, pkg).await
            && enabled
            && !(wasm_blob.is_empty() && revision_digest.is_none())
            && let Ok(manifest) = serde_json::from_str::<PluginManifest>(&manifest_json)
        {
            for route in &manifest.routes {
                if route.method == method
                    && let Some(params) =
                        opendash_core::plugin::match_route_path(&route.path, subpath)
                {
                    if let Some(digest) = &revision_digest
                        && let Err(err) = plugin_storage::verify_plugin_revision_durability(
                            &self.plugins_root,
                            digest,
                        )
                    {
                        return Err((StatusCode::CONFLICT, err));
                    }
                    let runtime_kind = ExtensionRuntimeKind::from_entrypoint(&manifest.entrypoint);
                    return Ok(Some(ResolvedExtensionRoute {
                        owner: RuntimeOwner::plugin(manifest.id.clone()),
                        route: route.clone(),
                        params,
                        code_bytes: wasm_blob,
                        runtime_kind,
                        owner_fence: OwnerFence::Plugin {
                            plugin_id: manifest.id.clone(),
                            expected_generation: revision_generation,
                        },
                        manifest,
                    }));
                }
            }
        }

        // 2. Check active theme functions with matching theme ID and route
        if let Ok(Some(cached)) = self.ensure_active_theme_cache().await
            && cached.theme_id == pkg
        {
            for route in &cached.manifest.routes {
                if route.method == method
                    && let Some(params) =
                        opendash_core::plugin::match_route_path(&route.path, subpath)
                {
                    return Ok(Some(ResolvedExtensionRoute {
                        owner: RuntimeOwner::theme(cached.theme_id.clone()),
                        route: route.clone(),
                        params,
                        code_bytes: cached.js_bytes,
                        runtime_kind: ExtensionRuntimeKind::Js,
                        owner_fence: OwnerFence::Theme {
                            theme_id: cached.theme_id,
                            revision_id: cached.revision_id,
                            selection_version: cached.selection_version,
                        },
                        manifest: cached.manifest,
                    }));
                }
            }
        }

        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_resolved_route(
        &self,
        resolved: ResolvedExtensionRoute,
        payload: Value,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout: std::time::Duration,
        actor: Option<&crate::auth::SessionContext>,
        hooks: &crate::hooks::HookDispatcher,
    ) -> Result<ExtensionRouteExecOutput, (StatusCode, PluginRunResponse)> {
        let correlation_id = format!("rte_{}", &crate::auth::generate_random_token()[..16]);
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.to_string()),
                    correlation_id: correlation_id.clone(),
                    confirmation: None,
                },
            )
        };

        if self.recovery.is_safe_mode() {
            return Err(mk_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "safe_mode",
                "Hệ thống đang ở safe mode sau khi khôi phục; route mở rộng tạm khóa.".into(),
            ));
        }

        let owner_kind = resolved.owner.kind;
        let owner_id = resolved.owner.id.clone();
        let manifest = resolved.manifest;
        let route_decl = resolved.route;
        let owner_fence = resolved.owner_fence;

        let occurred_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let event_str = format!("route:{}", route_decl.id);
        let runtime_input = serde_json::to_value(PluginEnvelopeV2::new(
            EnvelopeKind::Route,
            &event_str,
            occurred_at_ms,
            correlation_id.clone(),
            None,
            principal,
            0,
            payload,
        ))
        .unwrap_or(Value::Null);

        let caps_opt = if route_decl.capabilities.is_empty() {
            None
        } else {
            Some(route_decl.capabilities.clone())
        };
        let required_role = if route_decl.access == opendash_core::plugin::RouteAccess::Admin {
            "admin".to_string()
        } else {
            "editor".to_string()
        };
        let actor_fence = actor.map(|a| crate::plugin::runtime::ActionActorFence {
            user_id: a.user.id,
            required_role: required_role.clone(),
        });

        let exec_res = self
            .runtime
            .run_owner_action_with_principal(
                owner_kind,
                resolved.runtime_kind,
                resolved.code_bytes,
                manifest.clone(),
                runtime_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(owner_fence.clone()),
                execution_mode,
                principal,
                Some(timeout),
                caps_opt.clone(),
                actor_fence,
            )
            .await;

        let exec_out = match exec_res {
            Ok(out) => out,
            Err(err) => {
                let (status, code_str) = match &err {
                    PluginError::Disabled => (StatusCode::CONFLICT, "generation_changed"),
                    PluginError::Timeout => {
                        if execution_mode == HookExecutionMode::RouteGet {
                            (StatusCode::SERVICE_UNAVAILABLE, "timeout")
                        } else {
                            (StatusCode::GATEWAY_TIMEOUT, "timeout")
                        }
                    }
                    PluginError::ConcurrencyLimit => {
                        (StatusCode::SERVICE_UNAVAILABLE, "concurrency_limit")
                    }
                    PluginError::Internal(m) if m.starts_with("rbac_revoked:") => {
                        (StatusCode::FORBIDDEN, "rbac_revoked")
                    }
                    _ => (StatusCode::BAD_REQUEST, "route_failed"),
                };
                let msg = err.to_string();
                if execution_mode != HookExecutionMode::RouteGet {
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        owner_kind,
                        &owner_id,
                        &correlation_id,
                        "error",
                        &msg,
                        Some(&msg),
                    )
                    .await;
                }
                return Err(mk_err(status, code_str, msg));
            }
        };

        let render_trusted_html = manifest.permissions.render_trusted_html;

        if execution_mode == HookExecutionMode::RoutePost {
            if let Some(cmd) = exec_out.staged_content_command {
                let Some(actor) = actor else {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_write_denied",
                        "Ghi nội dung bị cấm trên public route.".into(),
                    ));
                };
                if principal == ExecutionPrincipal::Public {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_write_denied",
                        "Ghi nội dung bị cấm trên public route.".into(),
                    ));
                }
                self.commit_staged_content_with_actor(
                    owner_kind,
                    &owner_id,
                    &manifest,
                    caps_opt.as_deref(),
                    owner_fence,
                    &required_role,
                    actor,
                    cmd,
                    hooks,
                    &correlation_id,
                )
                .await?;
            } else if let Some(del) = exec_out.staged_lifecycle_command {
                if del.operation != StagedLifecycleOperation::Trash {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Chỉ content_delete được phép trên custom route; restore/purge yêu cầu action xác nhận.".into(),
                    ));
                }
                let Some(actor) = actor else {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Xóa nội dung bị cấm trên public route.".into(),
                    ));
                };
                if principal == ExecutionPrincipal::Public {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Xóa nội dung bị cấm trên public route.".into(),
                    ));
                }
                if !manifest.can_delete_content_type(&del.type_key) {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Gói mở rộng không có quyền xóa loại nội dung này.".into(),
                    ));
                }
                let fence = crate::storage::entries::PluginActionFence {
                    owner_fence,
                    required_role,
                };
                if let Err(err) = plugin_storage::delete_entry_with_plugin_fence(
                    &self.db_pool,
                    actor,
                    &del.type_key,
                    del.entry_id,
                    del.expected_version,
                    &fence,
                )
                .await
                {
                    return Err(mk_err(err.0, "content_delete_fenced", err.1.to_string()));
                }
            }
            plugin_storage::record_owner_execution_result(
                &self.db_pool,
                owner_kind,
                &owner_id,
                &correlation_id,
                "info",
                "Thực thi route thành công",
                None,
            )
            .await;
        }

        Ok(ExtensionRouteExecOutput {
            output: exec_out.output,
            render_trusted_html,
            correlation_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_staged_content_with_actor(
        &self,
        owner_kind: ExtensionOwnerKind,
        _owner_id: &str,
        manifest: &PluginManifest,
        caps_opt: Option<&[String]>,
        owner_fence: OwnerFence,
        required_role: &str,
        actor: &crate::auth::SessionContext,
        cmd: crate::plugin::host_ops::StagedContentCommand,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: &str,
    ) -> Result<crate::storage::entries::ContentEntryRecord, (StatusCode, PluginRunResponse)> {
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.to_string()),
                    correlation_id: correlation_id.to_owned(),
                    confirmation: None,
                },
            )
        };
        if !manifest.can_write_content_type(&cmd.type_key) {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "content_write_denied",
                "Gói mở rộng không có quyền ghi loại nội dung này.".into(),
            ));
        }
        let actor_role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        let effective_schema =
            crate::storage::content::load_effective_content_schema(&self.db_pool, &cmd.type_key)
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
            if !opendash_core::plugin::plugin_can_write_field_for_owner(
                owner_kind,
                manifest,
                caps_opt,
                actor_role,
                &cmd.type_key,
                field_entry,
            ) {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "field_edit_forbidden",
                    format!("Gói mở rộng hoặc tài khoản không có quyền ghi trường '{field_key}'."),
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
                    correlation_id: correlation_id.to_owned(),
                    confirmation: None,
                },
            ));
        }

        let fence = crate::storage::entries::PluginActionFence {
            owner_fence,
            required_role: required_role.to_owned(),
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
        let _ = crate::storage::plugin_projections::recompute_entry_projections(
            &self.db_pool,
            hooks,
            record.id,
        )
        .await;
        hooks
            .dispatch_content_after_save_for_actor(
                &cmd.type_key,
                actor_role,
                serde_json::to_value(&record).unwrap_or_default(),
            )
            .await;
        Ok(record)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedExtensionRoute {
    pub owner: RuntimeOwner,
    pub manifest: PluginManifest,
    pub route: opendash_core::plugin::ExtensionRouteDecl,
    pub params: std::collections::BTreeMap<String, String>,
    pub code_bytes: Vec<u8>,
    pub runtime_kind: ExtensionRuntimeKind,
    pub owner_fence: OwnerFence,
}

#[derive(Debug, Clone)]
pub struct ExtensionRouteExecOutput {
    pub output: Value,
    pub render_trusted_html: bool,
    pub correlation_id: String,
}
