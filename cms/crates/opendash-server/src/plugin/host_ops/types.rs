use super::*;

pub(super) use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

pub(super) use opendash_core::hooks::{
    ExecutionPrincipal, ExtensionOwnerKind, HookExecutionMode, sanitize_plain_text,
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::{Value, json};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::plugin::manifest::PluginManifest;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuntimeOwner {
    pub kind: ExtensionOwnerKind,
    pub id: String,
}

pub const MAX_HOST_CALLS: usize = 100;

pub const MAX_RENDER_HOST_CALLS: usize = 64;

pub const MAX_L10N_CALLS: usize = 256;

pub const MAX_RENDER_HTTP_CALLS: usize = 2;

pub const MAX_KV_KEY_BYTES: usize = 256;

pub const MAX_KV_VALUE_BYTES: usize = 512 * 1024;

pub const MAX_IPC_FRAME_BYTES: usize = 2 * 1024 * 1024;

pub const MAX_STAGED_KV_OPS: usize = 64;

pub const MAX_STAGED_KV_TOTAL_BYTES: usize = 2 * 1024 * 1024;

pub const MAX_STAGED_LOGS: usize = 20;

pub const MAX_LOG_MESSAGE_CHARS: usize = 512;

pub const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;

pub const HTTP_TIMEOUT: Duration = Duration::from_secs(2);

pub struct HostContext {
    pub owner: RuntimeOwner,
    pub plugin_id: String,
    pub manifest: PluginManifest,
    pub input_bytes: Vec<u8>,
    pub output_bytes: Option<Vec<u8>>,
    pub last_resp_bytes: Option<Vec<u8>>,
    pub host_calls_count: usize,
    pub http_calls_count: usize,
    pub l10n_calls_count: usize,
    pub default_locale: Option<String>,
    pub db_pool: Option<SqlitePool>,
    pub secrets_root: Option<PathBuf>,
    pub read_ipc:
        Option<Arc<dyn Fn(HostReadRequest) -> Result<HostOperationResponse, String> + Send + Sync>>,
    pub _correlation_id: String,
    pub allow_loopback: bool,
    pub staged_kv: BTreeMap<String, Option<Value>>,
    pub staged_kv_bytes: usize,
    pub staged_logs: Vec<StagedPluginLog>,
    pub cancellation_token: Arc<AtomicBool>,
    pub deadline: Instant,
    pub execution_mode: HookExecutionMode,
    pub principal: ExecutionPrincipal,
    pub staged_content_command: Option<StagedContentCommand>,
    pub staged_lifecycle_command: Option<StagedLifecycleCommand>,
    pub action_capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedContentCommand {
    pub type_key: String,
    #[serde(default)]
    pub entry_id: Option<i64>,
    #[serde(default)]
    pub expected_version: Option<i64>,
    #[serde(default)]
    pub locale: Option<String>,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub term_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagedLifecycleOperation {
    Trash,
    Restore,
    Purge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedLifecycleCommand {
    pub operation: StagedLifecycleOperation,
    pub type_key: String,
    pub entry_id: i64,
    pub expected_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedPluginLog {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostReadRequest {
    KvGet {
        key: String,
    },
    KvList {
        #[serde(default)]
        prefix: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    ContentGet {
        type_key: String,
        entry_id: i64,
        #[serde(default)]
        locale: Option<String>,
    },
    ContentList {
        type_key: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        published_only: Option<bool>,
    },
    ContentTrashGet {
        type_key: String,
        entry_id: i64,
        #[serde(default)]
        locale: Option<String>,
    },
    ContentTrashList {
        type_key: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
    },
    TaxonomyList {
        kind: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    MediaGet {
        media_id: String,
    },
    SiteOptionsGet {
        #[serde(default)]
        locale: Option<String>,
    },
    PluginSettingsGet,
    PluginSecretGet {
        key: String,
    },
    L10nTranslate {
        #[serde(default)]
        text_domain: Option<String>,
        msgid: String,
        #[serde(default)]
        msgid_plural: Option<String>,
        #[serde(default)]
        count: Option<i64>,
        #[serde(default)]
        context: Option<String>,
        #[serde(default)]
        locale: Option<String>,
    },
    L10nTranslateBatch {
        items: Vec<L10nTranslateItem>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L10nTranslateItem {
    #[serde(default)]
    pub text_domain: Option<String>,
    pub msgid: String,
    #[serde(default)]
    pub msgid_plural: Option<String>,
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostOperationRequest {
    KvGet {
        key: String,
    },
    KvSet {
        key: String,
        value: Value,
    },
    KvDelete {
        key: String,
    },
    KvList {
        #[serde(default)]
        prefix: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    HttpRequest {
        method: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    ContentSave {
        type_key: String,
        #[serde(default)]
        entry_id: Option<i64>,
        #[serde(default)]
        expected_version: Option<i64>,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        slug: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        values: BTreeMap<String, Value>,
        #[serde(default)]
        term_ids: Option<Vec<i64>>,
    },
    ContentDelete {
        type_key: String,
        entry_id: i64,
        expected_version: i64,
    },
    ContentRestore {
        type_key: String,
        entry_id: i64,
        expected_version: i64,
    },
    ContentPurge {
        type_key: String,
        entry_id: i64,
        expected_version: i64,
    },
    ContentGet {
        type_key: String,
        entry_id: i64,
        #[serde(default)]
        locale: Option<String>,
    },
    ContentList {
        type_key: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        published_only: Option<bool>,
    },
    ContentTrashGet {
        type_key: String,
        entry_id: i64,
        #[serde(default)]
        locale: Option<String>,
    },
    ContentTrashList {
        type_key: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
    },
    TaxonomyList {
        kind: String,
        #[serde(default)]
        locale: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    MediaGet {
        media_id: String,
    },
    SiteOptionsGet {
        #[serde(default)]
        locale: Option<String>,
    },
    PluginSettingsGet,
    PluginSecretGet {
        key: String,
    },
    L10nTranslate {
        #[serde(default)]
        text_domain: Option<String>,
        msgid: String,
        #[serde(default)]
        msgid_plural: Option<String>,
        #[serde(default)]
        count: Option<i64>,
        #[serde(default)]
        context: Option<String>,
        #[serde(default)]
        locale: Option<String>,
    },
    L10nTranslateBatch {
        items: Vec<L10nTranslateItem>,
    },
    Log {
        level: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostOperationResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerExecutionResult {
    pub ok: bool,
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub error_type: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub staged_kv: BTreeMap<String, Option<Value>>,
    #[serde(default)]
    pub staged_logs: Vec<StagedPluginLog>,
    #[serde(default)]
    pub staged_content_command: Option<StagedContentCommand>,
    #[serde(default)]
    pub staged_lifecycle_command: Option<StagedLifecycleCommand>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum WorkerToHostFrame {
    ReadRequest(HostReadRequest),
    Result(WorkerExecutionResult),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum HostToWorkerFrame {
    ReadResponse(HostOperationResponse),
}

#[allow(unused_imports)]
pub use crate::plugin::egress::is_forbidden_ip;
