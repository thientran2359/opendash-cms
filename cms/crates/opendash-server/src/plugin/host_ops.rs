use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use opendash_core::hooks::{
    ExecutionPrincipal, ExtensionOwnerKind, HookExecutionMode, sanitize_plain_text,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::manifest::PluginManifest;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuntimeOwner {
    pub kind: ExtensionOwnerKind,
    pub id: String,
}

impl RuntimeOwner {
    pub fn plugin(id: impl Into<String>) -> Self {
        Self {
            kind: ExtensionOwnerKind::Plugin,
            id: id.into(),
        }
    }

    pub fn theme(id: impl Into<String>) -> Self {
        Self {
            kind: ExtensionOwnerKind::Theme,
            id: id.into(),
        }
    }
}

pub const MAX_HOST_CALLS: usize = 100;
pub const MAX_RENDER_HOST_CALLS: usize = 64;
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

impl HostContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .load(std::sync::atomic::Ordering::Relaxed)
            || Instant::now() >= self.deadline
    }

    fn has_any_kv_stage(&self) -> bool {
        !self.staged_kv.is_empty()
    }

    fn has_content_stage(&self) -> bool {
        self.staged_content_command.is_some() || self.staged_lifecycle_command.is_some()
    }
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

impl HostOperationResponse {
    pub fn ok_data(data: Value) -> Self {
        Self {
            ok: true,
            error: None,
            data: Some(data),
            found: None,
            value: None,
            status: None,
            headers: None,
            body: None,
        }
    }

    pub fn ok_empty() -> Self {
        Self {
            ok: true,
            error: None,
            data: None,
            found: None,
            value: None,
            status: None,
            headers: None,
            body: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            data: None,
            found: None,
            value: None,
            status: None,
            headers: None,
            body: None,
        }
    }
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

/// Shared HostPolicy enforcement for both Worker pre-checks and Host IPC handlers.
pub fn check_read_policy(
    manifest: &PluginManifest,
    execution_mode: HookExecutionMode,
    principal: ExecutionPrincipal,
    action_caps: Option<&[String]>,
    req: &HostReadRequest,
) -> Result<(), &'static str> {
    if execution_mode == HookExecutionMode::FilterContactBeforeSubmit
        && !matches!(req, HostReadRequest::PluginSecretGet { .. })
    {
        return Err("Thao tác đọc dữ liệu bị cấm trong filter:contact/before_submit.");
    }
    match req {
        HostReadRequest::KvGet { .. } => {
            if !manifest.has_sqlite_perm("kv_read") {
                return Err("permission_denied: sqlite:kv_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "kv_read")
            {
                return Err("permission_denied: action:kv_read");
            }
        }
        HostReadRequest::KvList { .. } => {
            if !matches!(
                execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::FilterRender
                    | HookExecutionMode::RouteGet
                    | HookExecutionMode::RoutePost
            ) {
                return Err("kv_list chỉ được phép trong action hoặc async post-commit hook.");
            }
            if !manifest.has_sqlite_perm("kv_read") {
                return Err("permission_denied: sqlite:kv_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "kv_list")
            {
                return Err("permission_denied: action:kv_list");
            }
        }
        HostReadRequest::ContentGet { type_key, .. }
        | HostReadRequest::ContentList { type_key, .. } => {
            if !manifest.can_read_content_type(type_key) {
                return Err("permission_denied: content_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "content_read")
            {
                return Err("permission_denied: action:content_read");
            }
        }
        HostReadRequest::ContentTrashGet { type_key, .. }
        | HostReadRequest::ContentTrashList { type_key, .. } => {
            if execution_mode != HookExecutionMode::Standard {
                return Err("content_trash_read chỉ được phép trong action mode.");
            }
            if !matches!(
                principal,
                ExecutionPrincipal::Admin | ExecutionPrincipal::Editor
            ) {
                return Err("permission_denied: authenticated_actor_required");
            }
            if !action_caps.is_some_and(|caps| caps.iter().any(|c| c == "content_trash_read")) {
                return Err("permission_denied: action:content_trash_read");
            }
            if !manifest.can_read_trash_content_type(type_key) {
                return Err("permission_denied: content_trash_read");
            }
        }
        HostReadRequest::TaxonomyList { .. } => {
            if !manifest.permissions.taxonomy_read {
                return Err("permission_denied: taxonomy_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "taxonomy_read")
            {
                return Err("permission_denied: action:taxonomy_read");
            }
        }
        HostReadRequest::MediaGet { .. } => {
            if !manifest.permissions.media_read {
                return Err("permission_denied: media_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "media_read")
            {
                return Err("permission_denied: action:media_read");
            }
        }
        HostReadRequest::SiteOptionsGet { .. } => {
            if !manifest.permissions.site_options_read {
                return Err("permission_denied: site_options_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "site_options_read")
            {
                return Err("permission_denied: action:site_options_read");
            }
        }
        HostReadRequest::PluginSettingsGet => {
            if !manifest.permissions.plugin_settings_read {
                return Err("permission_denied: plugin_settings_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "plugin_settings_read")
            {
                return Err("permission_denied: action:plugin_settings_read");
            }
        }
        HostReadRequest::PluginSecretGet { key } => {
            if !matches!(
                execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::FilterContactBeforeSubmit
                    | HookExecutionMode::RoutePost
            ) {
                return Err("plugin_secret_get bị cấm trong hook/filter hiện tại.");
            }
            if !manifest
                .permissions
                .plugin_secrets_read
                .iter()
                .any(|k| k == key)
            {
                return Err("permission_denied: plugin_secrets_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "plugin_secrets_read")
            {
                return Err("permission_denied: action:plugin_secrets_read");
            }
        }
    }
    Ok(())
}

pub fn handle_host_call(ctx: &mut HostContext, req_json: &str) -> HostOperationResponse {
    ctx.last_resp_bytes = None;
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }

    ctx.host_calls_count += 1;
    let max_calls = if matches!(
        ctx.execution_mode,
        HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
    ) {
        MAX_RENDER_HOST_CALLS
    } else {
        MAX_HOST_CALLS
    };
    if ctx.host_calls_count > max_calls {
        return HostOperationResponse::err(format!(
            "Vượt quá số lần gọi host tối đa ({max_calls})."
        ));
    }

    let req: HostOperationRequest = match serde_json::from_str(req_json) {
        Ok(r) => r,
        Err(_) => return HostOperationResponse::err("Thao tác host không đúng định dạng JSON."),
    };

    match req {
        HostOperationRequest::KvGet { key } => handle_kv_get(ctx, &key),
        HostOperationRequest::KvSet { key, value } => handle_kv_set(ctx, &key, value),
        HostOperationRequest::KvDelete { key } => handle_kv_delete(ctx, &key),
        HostOperationRequest::KvList { prefix, limit } => handle_kv_list(ctx, prefix, limit),
        HostOperationRequest::HttpRequest {
            method,
            url,
            headers,
            body,
            timeout_ms,
        } => {
            if ctx.execution_mode == HookExecutionMode::FilterRender {
                if ctx.manifest.render_cache == opendash_core::plugin::RenderCachePolicy::None {
                    return HostOperationResponse::err(
                        "Gọi HTTP ngoài trong filter:render/* yêu cầu bật render_cache khác none.",
                    );
                }
            } else if !matches!(
                ctx.execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::FilterContactBeforeSubmit
                    | HookExecutionMode::RouteGet
                    | HookExecutionMode::RoutePost
            ) {
                return HostOperationResponse::err(
                    "Gọi HTTP ngoài bị cấm trong chế độ thực thi này.",
                );
            }
            if let Some(ref caps) = ctx.action_capabilities {
                if caps
                    .iter()
                    .any(|c| c == "content_restore" || c == "content_purge")
                {
                    return HostOperationResponse::err(
                        "permission_denied: http_forbidden_with_lifecycle_write",
                    );
                }
                if !caps.iter().any(|c| c == "http") {
                    return HostOperationResponse::err("permission_denied: action:http");
                }
            }
            if let Ok(parsed) = url::Url::parse(&url)
                && super::egress::is_forbidden_internal_path(parsed.path())
            {
                return HostOperationResponse::err(
                    "Cấm gọi HTTP loopback tới Admin/CMS nội bộ (/api/admin, /save, /preview, /theme, /p/, /api/p/, /api/public/p/).",
                );
            }
            handle_http_request(ctx, &method, &url, headers, body, timeout_ms)
        }
        HostOperationRequest::ContentSave {
            type_key,
            entry_id,
            expected_version,
            locale,
            slug,
            title,
            status: _,
            values,
            term_ids,
        } => {
            if !matches!(
                ctx.execution_mode,
                HookExecutionMode::Standard | HookExecutionMode::RoutePost
            ) {
                return HostOperationResponse::err("Ghi nội dung bị cấm ngoài action mode.");
            }
            if ctx.principal == ExecutionPrincipal::Public {
                return HostOperationResponse::err(
                    "permission_denied: content_save bị cấm ở public route.",
                );
            }
            if let Some(ref caps) = ctx.action_capabilities
                && !caps.iter().any(|c| c == "content_write")
            {
                return HostOperationResponse::err("permission_denied: action:content_write");
            }
            if ctx.has_any_kv_stage() || ctx.staged_lifecycle_command.is_some() {
                return HostOperationResponse::err(
                    "ERR_MIXED_WRITE_SCOPE: Không cho phép trộn ghi nội dung với KV hoặc xóa nội dung trong cùng action.",
                );
            }
            if ctx.staged_content_command.is_some() {
                return HostOperationResponse::err(
                    "Mỗi action chỉ được thực thi tối đa một lệnh ghi nội dung.",
                );
            }
            if !ctx.manifest.can_write_content_type(&type_key) {
                return HostOperationResponse::err("permission_denied: content_write");
            }
            ctx.staged_content_command = Some(StagedContentCommand {
                type_key,
                entry_id,
                expected_version,
                locale,
                slug: slug.unwrap_or_default(),
                title: title.unwrap_or_default(),
                values,
                term_ids,
            });
            HostOperationResponse::ok_empty()
        }
        HostOperationRequest::ContentDelete {
            type_key,
            entry_id,
            expected_version,
        } => stage_lifecycle_command(
            ctx,
            StagedLifecycleOperation::Trash,
            type_key,
            entry_id,
            expected_version,
        ),
        HostOperationRequest::ContentRestore {
            type_key,
            entry_id,
            expected_version,
        } => stage_lifecycle_command(
            ctx,
            StagedLifecycleOperation::Restore,
            type_key,
            entry_id,
            expected_version,
        ),
        HostOperationRequest::ContentPurge {
            type_key,
            entry_id,
            expected_version,
        } => stage_lifecycle_command(
            ctx,
            StagedLifecycleOperation::Purge,
            type_key,
            entry_id,
            expected_version,
        ),
        HostOperationRequest::ContentGet {
            type_key,
            entry_id,
            locale,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentGet {
                type_key,
                entry_id,
                locale,
            },
        ),
        HostOperationRequest::ContentList {
            type_key,
            locale,
            limit,
            offset,
            published_only,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentList {
                type_key,
                locale,
                limit,
                offset,
                published_only,
            },
        ),
        HostOperationRequest::ContentTrashGet {
            type_key,
            entry_id,
            locale,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentTrashGet {
                type_key,
                entry_id,
                locale,
            },
        ),
        HostOperationRequest::ContentTrashList {
            type_key,
            locale,
            limit,
            offset,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentTrashList {
                type_key,
                locale,
                limit,
                offset,
            },
        ),
        HostOperationRequest::TaxonomyList {
            kind,
            locale,
            limit,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::TaxonomyList {
                kind,
                locale,
                limit,
            },
        ),
        HostOperationRequest::MediaGet { media_id } => {
            dispatch_host_read(ctx, HostReadRequest::MediaGet { media_id })
        }
        HostOperationRequest::SiteOptionsGet { locale } => {
            dispatch_host_read(ctx, HostReadRequest::SiteOptionsGet { locale })
        }
        HostOperationRequest::PluginSettingsGet => {
            dispatch_host_read(ctx, HostReadRequest::PluginSettingsGet)
        }
        HostOperationRequest::PluginSecretGet { key } => {
            if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
                return HostOperationResponse::err("Khóa secret không hợp lệ.");
            }
            dispatch_host_read(ctx, HostReadRequest::PluginSecretGet { key })
        }
        HostOperationRequest::Log { level, message } => {
            if ctx.execution_mode == HookExecutionMode::FilterContactBeforeSubmit {
                return HostOperationResponse::err(
                    "Ghi log bị cấm trong filter:contact/before_submit.",
                );
            }
            if !matches!(level.as_str(), "info" | "warn" | "error") {
                return HostOperationResponse::err("level log chỉ hỗ trợ info, warn hoặc error.");
            }
            if ctx.staged_logs.len() >= MAX_STAGED_LOGS {
                return HostOperationResponse::err(
                    "quota_exceeded: Vượt quá giới hạn 20 log mỗi lần chạy.",
                );
            }
            let clean = sanitize_plain_text(&message, MAX_LOG_MESSAGE_CHARS);
            ctx.staged_logs.push(StagedPluginLog {
                level,
                message: clean,
            });
            HostOperationResponse::ok_data(json!({ "staged": true }))
        }
    }
}

fn stage_lifecycle_command(
    ctx: &mut HostContext,
    operation: StagedLifecycleOperation,
    type_key: String,
    entry_id: i64,
    expected_version: i64,
) -> HostOperationResponse {
    let mode_allowed = match operation {
        StagedLifecycleOperation::Trash => matches!(
            ctx.execution_mode,
            HookExecutionMode::Standard | HookExecutionMode::RoutePost
        ),
        StagedLifecycleOperation::Restore | StagedLifecycleOperation::Purge => {
            ctx.execution_mode == HookExecutionMode::Standard
        }
    };
    if !mode_allowed {
        return HostOperationResponse::err(match operation {
            StagedLifecycleOperation::Trash => "content_delete chỉ được phép trong action mode.",
            StagedLifecycleOperation::Restore => "content_restore chỉ được phép trong action mode.",
            StagedLifecycleOperation::Purge => "content_purge chỉ được phép trong action mode.",
        });
    }
    if !matches!(
        ctx.principal,
        ExecutionPrincipal::Admin | ExecutionPrincipal::Editor
    ) {
        return HostOperationResponse::err("permission_denied: authenticated_actor_required");
    }
    if operation == StagedLifecycleOperation::Purge && ctx.principal != ExecutionPrincipal::Admin {
        return HostOperationResponse::err("permission_denied: admin_required");
    }
    let (cap_name, has_manifest_perm) = match operation {
        StagedLifecycleOperation::Trash => (
            "content_delete",
            ctx.manifest.can_delete_content_type(&type_key),
        ),
        StagedLifecycleOperation::Restore => (
            "content_restore",
            ctx.manifest.can_restore_content_type(&type_key),
        ),
        StagedLifecycleOperation::Purge => (
            "content_purge",
            ctx.manifest.can_purge_content_type(&type_key),
        ),
    };
    let cap_allowed = match operation {
        // Legacy content_delete checks declared action capabilities when present; new restore/purge
        // require an explicit declared action (never unrestricted /run).
        StagedLifecycleOperation::Trash => ctx
            .action_capabilities
            .as_deref()
            .is_none_or(|caps| caps.iter().any(|c| c == cap_name)),
        StagedLifecycleOperation::Restore | StagedLifecycleOperation::Purge => ctx
            .action_capabilities
            .as_deref()
            .is_some_and(|caps| caps.iter().any(|c| c == cap_name)),
    };
    if !cap_allowed {
        return HostOperationResponse::err(format!("permission_denied: action:{cap_name}"));
    }
    if ctx.has_any_kv_stage() || ctx.staged_content_command.is_some() {
        return HostOperationResponse::err(
            "ERR_MIXED_WRITE_SCOPE: Không cho phép trộn thao tác vòng đời nội dung với KV hoặc content_save.",
        );
    }
    if ctx.staged_lifecycle_command.is_some() {
        return HostOperationResponse::err(
            "Mỗi action chỉ được thực thi tối đa một lệnh vòng đời nội dung.",
        );
    }
    if !has_manifest_perm {
        return HostOperationResponse::err(format!("permission_denied: {cap_name}"));
    }
    if entry_id <= 0 || expected_version < 1 {
        return HostOperationResponse::err("entry_id hoặc expected_version không hợp lệ.");
    }
    let op_str = match operation {
        StagedLifecycleOperation::Trash => "trash",
        StagedLifecycleOperation::Restore => "restore",
        StagedLifecycleOperation::Purge => "purge",
    };
    ctx.staged_lifecycle_command = Some(StagedLifecycleCommand {
        operation,
        type_key,
        entry_id,
        expected_version,
    });
    HostOperationResponse::ok_data(json!({
        "staged": true,
        "operation": op_str,
        "entry_id": entry_id,
    }))
}

fn dispatch_host_read(ctx: &HostContext, req: HostReadRequest) -> HostOperationResponse {
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }
    if let Err(msg) = check_read_policy(
        &ctx.manifest,
        ctx.execution_mode,
        ctx.principal,
        ctx.action_capabilities.as_deref(),
        &req,
    ) {
        return HostOperationResponse::err(msg);
    }

    if let Some(ref ipc) = ctx.read_ipc {
        return match ipc(req) {
            Ok(resp) => resp,
            Err(e) => HostOperationResponse::err(format!("Lỗi IPC host_read: {e}")),
        };
    }

    let Some(ref db) = ctx.db_pool else {
        return HostOperationResponse::err("Không có kết nối cơ sở dữ liệu khả dụng.");
    };

    let remaining = ctx.deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return HostOperationResponse::err("Lỗi cơ sở dữ liệu: Quá thời gian truy vấn (timeout).");
    }
    let db_timeout = std::cmp::min(Duration::from_secs(2), remaining);

    let owner_kind = ctx.owner.kind;
    let plugin_id = ctx.plugin_id.clone();
    let manifest = ctx.manifest.clone();
    let execution_mode = ctx.execution_mode;
    let principal = ctx.principal;
    let action_capabilities = ctx.action_capabilities.clone();
    let secrets_root = ctx.secrets_root.clone();
    let db = db.clone();

    let query_res = tokio::runtime::Handle::current().block_on(async move {
        tokio::time::timeout(
            db_timeout,
            super::runtime::execute_host_read_request_for_owner(
                &db,
                owner_kind,
                &plugin_id,
                &manifest,
                execution_mode,
                principal,
                action_capabilities.as_deref(),
                secrets_root.as_deref(),
                req,
            ),
        )
        .await
    });

    match query_res {
        Ok(resp) => resp,
        Err(_) => {
            HostOperationResponse::err("Lỗi cơ sở dữ liệu: Quá thời gian truy vấn (timeout).")
        }
    }
}

fn handle_kv_get(ctx: &mut HostContext, key: &str) -> HostOperationResponse {
    if let Err(msg) = check_read_policy(
        &ctx.manifest,
        ctx.execution_mode,
        ctx.principal,
        ctx.action_capabilities.as_deref(),
        &HostReadRequest::KvGet {
            key: key.to_string(),
        },
    ) {
        return HostOperationResponse::err(msg);
    }
    if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
        return HostOperationResponse::err("Key không hợp lệ hoặc quá dài.");
    }

    // 1. Check staged writes/tombstones in current invocation first
    if let Some(staged_opt) = ctx.staged_kv.get(key) {
        return match staged_opt {
            Some(val) => HostOperationResponse {
                ok: true,
                error: None,
                data: None,
                found: Some(true),
                value: Some(val.clone()),
                status: None,
                headers: None,
                body: None,
            },
            None => HostOperationResponse {
                ok: true,
                error: None,
                data: None,
                found: Some(false),
                value: None,
                status: None,
                headers: None,
                body: None,
            },
        };
    }

    dispatch_host_read(
        ctx,
        HostReadRequest::KvGet {
            key: key.to_string(),
        },
    )
}

fn handle_kv_list(
    ctx: &mut HostContext,
    prefix: Option<String>,
    limit: Option<usize>,
) -> HostOperationResponse {
    let max_limit = if matches!(
        ctx.execution_mode,
        HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
    ) {
        50
    } else {
        100
    };
    let clamped = limit.unwrap_or(50).clamp(1, max_limit);
    let prefix_val = prefix.unwrap_or_default();
    if prefix_val.len() > MAX_KV_KEY_BYTES {
        return HostOperationResponse::err("Prefix quá dài.");
    }
    let fetch_limit = clamped.saturating_add(ctx.staged_kv.len()).min(200);
    let resp = dispatch_host_read(
        ctx,
        HostReadRequest::KvList {
            prefix: (!prefix_val.is_empty()).then(|| prefix_val.clone()),
            limit: Some(fetch_limit),
        },
    );
    if !resp.ok {
        return resp;
    }

    let mut merged: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(items) = resp
        .data
        .as_ref()
        .and_then(|d| d.get("items"))
        .and_then(Value::as_array)
    {
        for item in items {
            if let (Some(k), Some(v)) = (item.get("key").and_then(Value::as_str), item.get("value"))
                && (prefix_val.is_empty() || k.starts_with(&prefix_val))
            {
                merged.insert(k.to_string(), v.clone());
            }
        }
    }
    for (k, opt_v) in &ctx.staged_kv {
        if prefix_val.is_empty() || k.starts_with(&prefix_val) {
            match opt_v {
                Some(v) => {
                    merged.insert(k.clone(), v.clone());
                }
                None => {
                    merged.remove(k);
                }
            }
        }
    }
    let items: Vec<Value> = merged
        .into_iter()
        .take(clamped)
        .map(|(k, v)| json!({ "key": k, "value": v }))
        .collect();
    HostOperationResponse::ok_data(json!({ "items": items }))
}

fn check_kv_write_preconditions(
    ctx: &HostContext,
    cap: &str,
    key: &str,
) -> Result<(), HostOperationResponse> {
    if ctx.is_cancelled() {
        return Err(HostOperationResponse::err(
            "Quá thời gian thực thi (deadline exceeded).",
        ));
    }
    if !matches!(
        ctx.execution_mode,
        HookExecutionMode::Standard
            | HookExecutionMode::AsyncPostCommit
            | HookExecutionMode::RoutePost
    ) {
        return Err(HostOperationResponse::err(
            "Ghi/xóa KV bị cấm trong hook đồng bộ hiện tại.",
        ));
    }
    if let Some(ref caps) = ctx.action_capabilities
        && !caps.iter().any(|c| c == cap)
    {
        return Err(HostOperationResponse::err(format!(
            "permission_denied: action:{cap}"
        )));
    }
    if ctx.has_content_stage() {
        return Err(HostOperationResponse::err(
            "ERR_MIXED_WRITE_SCOPE: Không cho phép trộn ghi/xóa KV và ghi/xóa nội dung trong cùng một action.",
        ));
    }
    if !ctx.manifest.has_sqlite_perm("kv_write") {
        return Err(HostOperationResponse::err(
            "permission_denied: sqlite:kv_write",
        ));
    }
    if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
        return Err(HostOperationResponse::err("Key không hợp lệ hoặc quá dài."));
    }
    if !ctx.staged_kv.contains_key(key) && ctx.staged_kv.len() >= MAX_STAGED_KV_OPS {
        return Err(HostOperationResponse::err(
            "quota_exceeded: Vượt quá 64 thao tác KV mỗi lần chạy.",
        ));
    }
    Ok(())
}

fn staged_entry_size(key: &str, val: Option<&Value>) -> usize {
    key.len()
        + val
            .and_then(|v| serde_json::to_vec(v).ok().map(|b| b.len()))
            .unwrap_or(0)
}

fn handle_kv_set(ctx: &mut HostContext, key: &str, value: Value) -> HostOperationResponse {
    if let Err(resp) = check_kv_write_preconditions(ctx, "kv_write", key) {
        return resp;
    }
    let serialized_len = match serde_json::to_vec(&value) {
        Ok(b) => b.len(),
        Err(_) => return HostOperationResponse::err("Không thể chuyển đổi value thành JSON."),
    };
    if serialized_len > MAX_KV_VALUE_BYTES {
        return HostOperationResponse::err("Value vượt quá giới hạn 512 KB.");
    }
    let old_size = ctx
        .staged_kv
        .get(key)
        .map(|opt| staged_entry_size(key, opt.as_ref()))
        .unwrap_or(0);
    let new_size = key.len() + serialized_len;
    let next_total = ctx.staged_kv_bytes.saturating_sub(old_size) + new_size;
    if next_total > MAX_STAGED_KV_TOTAL_BYTES {
        return HostOperationResponse::err(
            "quota_exceeded: Tổng kích thước staged_kv vượt quá 2 MiB.",
        );
    }
    ctx.staged_kv_bytes = next_total;
    ctx.staged_kv.insert(key.to_string(), Some(value));
    HostOperationResponse::ok_empty()
}

fn handle_kv_delete(ctx: &mut HostContext, key: &str) -> HostOperationResponse {
    if let Err(resp) = check_kv_write_preconditions(ctx, "kv_delete", key) {
        return resp;
    }
    let old_size = ctx
        .staged_kv
        .get(key)
        .map(|opt| staged_entry_size(key, opt.as_ref()))
        .unwrap_or(0);
    let new_size = key.len();
    let next_total = ctx.staged_kv_bytes.saturating_sub(old_size) + new_size;
    if next_total > MAX_STAGED_KV_TOTAL_BYTES {
        return HostOperationResponse::err(
            "quota_exceeded: Tổng kích thước staged_kv vượt quá 2 MiB.",
        );
    }
    ctx.staged_kv_bytes = next_total;
    ctx.staged_kv.insert(key.to_string(), None);
    HostOperationResponse::ok_data(json!({ "deleted": true }))
}

#[allow(unused_imports)]
pub use super::egress::is_forbidden_ip;

fn handle_http_request(
    ctx: &mut HostContext,
    method: &str,
    target_url: &str,
    headers: BTreeMap<String, String>,
    body: Option<String>,
    timeout_ms: Option<u64>,
) -> HostOperationResponse {
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }
    let call_cap = match ctx.execution_mode {
        HookExecutionMode::FilterContactBeforeSubmit => 1,
        HookExecutionMode::FilterRender | HookExecutionMode::RouteGet => MAX_RENDER_HTTP_CALLS,
        _ => super::egress::MAX_HTTP_CALLS_PER_INVOCATION,
    };
    if ctx.http_calls_count >= call_cap {
        return HostOperationResponse::err(format!(
            "quota_exceeded: Vượt quá giới hạn {call_cap} HTTP requests mỗi lần chạy."
        ));
    }
    ctx.http_calls_count += 1;

    let effective_deadline = match timeout_ms {
        Some(ms) => {
            let capped = ms.clamp(1, HTTP_TIMEOUT.as_millis() as u64);
            ctx.deadline
                .min(Instant::now() + Duration::from_millis(capped))
        }
        None => ctx.deadline,
    };

    match super::egress::execute_scoped_http_request(
        &ctx.manifest,
        method,
        target_url,
        headers,
        body,
        ctx.allow_loopback,
        effective_deadline,
    ) {
        Ok(resp) => HostOperationResponse {
            ok: true,
            error: None,
            data: None,
            found: None,
            value: None,
            status: Some(resp.status),
            headers: Some(resp.headers),
            body: Some(resp.body),
        },
        Err(err) => HostOperationResponse::err(err),
    }
}

#[cfg(test)]
mod egress_quota_tests {
    use super::*;
    use opendash_core::plugin::{NetworkScopeGrant, PluginPermissions};

    fn test_ctx(manifest: PluginManifest, caps: Option<Vec<String>>) -> HostContext {
        let owner = RuntimeOwner::plugin(manifest.id.clone());
        HostContext {
            owner,
            plugin_id: manifest.id.clone(),
            manifest,
            input_bytes: vec![],
            output_bytes: None,
            last_resp_bytes: None,
            host_calls_count: 0,
            http_calls_count: 0,
            db_pool: None,
            secrets_root: None,
            read_ipc: None,
            _correlation_id: "test".into(),
            allow_loopback: true,
            staged_kv: Default::default(),
            staged_kv_bytes: 0,
            staged_logs: vec![],
            cancellation_token: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(5),
            execution_mode: HookExecutionMode::Standard,
            principal: ExecutionPrincipal::Admin,
            staged_content_command: None,
            staged_lifecycle_command: None,
            action_capabilities: caps,
        }
    }

    #[test]
    fn http_sub_quota_blocks_11th_call_per_invocation() {
        let manifest = PluginManifest {
            id: "quota-test".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 2,
            min_host_version: None,
            name: "Quota Test".into(),
            description: "Quota test".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                network: vec![NetworkScopeGrant {
                    scheme: "http".into(),
                    host: "127.0.0.1".into(),
                    port: Some(1),
                    methods: vec!["GET".into()],
                    path_prefix: Some("/".into()),
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ctx = test_ctx(manifest, None);
        ctx.http_calls_count = super::super::egress::MAX_HTTP_CALLS_PER_INVOCATION;
        let res = handle_http_request(
            &mut ctx,
            "GET",
            "http://127.0.0.1:1/",
            BTreeMap::new(),
            None,
            None,
        );
        assert!(!res.ok);
        assert!(
            res.error
                .as_deref()
                .unwrap_or("")
                .contains("quota_exceeded"),
            "expected quota_exceeded, got {:?}",
            res.error
        );
    }

    #[test]
    fn content_save_stages_optional_locale_and_hook_envelope_extracts_locale() {
        let manifest = PluginManifest {
            id: "locale-test".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 2,
            min_host_version: None,
            name: "Locale Test".into(),
            description: "Locale test".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                content_write: vec!["post".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ctx = test_ctx(manifest, Some(vec!["content_write".into()]));
        let req_json = r#"{"op":"content_save","type_key":"post","locale":"en","slug":"en-post","title":"EN Post"}"#;
        let resp = handle_host_call(&mut ctx, req_json);
        assert!(resp.ok, "unexpected error: {:?}", resp.error);
        let staged = ctx.staged_content_command.expect("staged command");
        assert_eq!(staged.locale.as_deref(), Some("en"));

        let env = opendash_core::hooks::HookEnvelope::new(
            opendash_core::hooks::HookEvent::ContentAfterSave,
            1_700_000_000_000,
            "locale-test",
            serde_json::json!({
                "id": 1,
                "locale": "en",
                "translation_group_id": "tg_123"
            }),
        );
        assert_eq!(env.locale.as_deref(), Some("en"));
        assert_eq!(env.translation_group_id.as_deref(), Some("tg_123"));
    }

    #[test]
    fn host_policy_enforces_tombstones_mixed_scope_logs_and_api_p_block() {
        let manifest = PluginManifest {
            id: "policy-v3".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 3,
            min_host_version: Some("0.1.0".into()),
            name: "Policy v3".into(),
            description: "Policy v3".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec!["kv_read".into(), "kv_write".into()],
                http: vec!["http://127.0.0.1:8080/".into()],
                content_read: vec!["post".into()],
                content_write: vec!["post".into()],
                content_delete: vec!["post".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        // 1. /api/p/ loopback blocked even when allow_loopback = true
        let mut ctx = test_ctx(manifest.clone(), None);
        let api_p_resp = handle_host_call(
            &mut ctx,
            r#"{"op":"http_request","method":"GET","url":"http://127.0.0.1:8080/api/p/foo/bar"}"#,
        );
        assert!(!api_p_resp.ok);
        assert!(api_p_resp.error.unwrap_or_default().contains("/api/p/"));

        // 2. kv_set then kv_delete tombstone visible immediately in kv_get
        assert!(handle_host_call(&mut ctx, r#"{"op":"kv_set","key":"k1","value":{"x":1}}"#).ok);
        assert!(handle_host_call(&mut ctx, r#"{"op":"kv_delete","key":"k1"}"#).ok);
        let get_after_del = handle_host_call(&mut ctx, r#"{"op":"kv_get","key":"k1"}"#);
        assert!(get_after_del.ok);
        assert_eq!(get_after_del.found, Some(false));

        // 3. ERR_MIXED_WRITE_SCOPE forbids combining kv_* with content_delete
        let mix_del = handle_host_call(
            &mut ctx,
            r#"{"op":"content_delete","type_key":"post","entry_id":1,"expected_version":1}"#,
        );
        assert!(!mix_del.ok);
        assert!(
            mix_del
                .error
                .unwrap_or_default()
                .contains("ERR_MIXED_WRITE_SCOPE")
        );

        // 4. log cap 20 & control char stripping
        for i in 0..20 {
            let log_req = serde_json::json!({
                "op": "log",
                "level": "info",
                "message": format!("msg\n{i}")
            });
            assert!(handle_host_call(&mut ctx, &log_req.to_string()).ok);
        }
        assert!(
            !handle_host_call(
                &mut ctx,
                r#"{"op":"log","level":"info","message":"overflow"}"#
            )
            .ok
        );
        assert_eq!(ctx.staged_logs[0].message, "msg0");
    }
}
