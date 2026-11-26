use super::*;

pub(super) use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) use axum::http::StatusCode;

pub(super) use opendash_core::hooks::{
    EnvelopeKind, ExecutionPrincipal, ExtensionOwnerKind, ExtensionRuntimeKind, HookEvent,
    HookExecutionMode, PluginEnvelopeV2,
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::Value;

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::SqlitePool;

pub(super) use tokio::sync::RwLock;

pub(super) use crate::storage::{
    entries::OwnerFence,
    entry_lifecycle::{
        EntryCommitReceipt, EntryMutationKind, LifecycleAuthority, LifecycleCommand,
        LifecycleConfirmationEvidence, LifecycleOperation, LifecycleScope, SecondaryOutcome,
    },
    plugin as plugin_storage,
};

pub(super) use crate::plugin::{
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

pub(super) fn lifecycle_data_value(receipt: &EntryCommitReceipt, op: &str) -> Value {
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

pub(super) fn map_lifecycle_api_error_code(err: &crate::error::ApiError) -> &'static str {
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
    } else if msg.starts_with("locale_parked:") {
        "locale_parked"
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

pub(super) struct ActionLeaseGuard {
    pub(super) leases: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<RuntimeOwner>>>,
    pub(super) owner: RuntimeOwner,
}

impl Drop for ActionLeaseGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.leases.lock() {
            guard.remove(&self.owner);
        }
    }
}

pub struct PluginManager {
    pub(super) db_pool: SqlitePool,
    pub(super) runtime: PluginRuntime,
    pub(super) active_action_leases:
        std::sync::Arc<std::sync::Mutex<std::collections::HashSet<RuntimeOwner>>>,
    pub(super) confirmation_store: Arc<LifecycleConfirmationStore>,
    pub(super) allow_loopback: bool,
    pub(super) plugins_root: PathBuf,
    pub(super) themes_root: PathBuf,
    pub(super) secrets_root: PathBuf,
    pub(super) manifest_cache: Arc<RwLock<Option<BTreeMap<String, CachedActivePlugin>>>>,
    pub(super) active_theme_cache: Arc<RwLock<Option<Option<CachedActiveTheme>>>>,
    pub(super) recovery: crate::config::RecoveryState,
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
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
    pub committed: Option<EntryCommitReceipt>,
}
