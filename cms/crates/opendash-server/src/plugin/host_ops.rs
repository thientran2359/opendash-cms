use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use wasmi::StoreLimits;

use super::manifest::PluginManifest;

pub const MAX_HOST_CALLS: usize = 100;
pub const MAX_KV_KEY_BYTES: usize = 256;
pub const MAX_KV_VALUE_BYTES: usize = 512 * 1024;
pub const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(2);

pub struct HostContext {
    pub plugin_id: String,
    pub manifest: PluginManifest,
    pub input_bytes: Vec<u8>,
    pub output_bytes: Option<Vec<u8>>,
    pub host_calls_count: usize,
    pub http_calls_count: usize,
    pub db_pool: Option<SqlitePool>,
    pub kv_ipc: Option<Arc<dyn Fn(&str) -> Result<HostOperationResponse, String> + Send + Sync>>,
    pub _correlation_id: String,
    pub allow_loopback: bool,
    pub limits: StoreLimits,
    pub staged_kv_writes: BTreeMap<String, Value>,
    pub cancellation_token: Arc<AtomicBool>,
    pub deadline: Instant,
    pub execution_mode: opendash_core::hooks::HookExecutionMode,
    pub staged_content_command: Option<StagedContentCommand>,
    pub action_capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedContentCommand {
    pub type_key: String,
    #[serde(default)]
    pub entry_id: Option<i64>,
    #[serde(default)]
    pub expected_version: Option<i64>,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub term_ids: Option<Vec<i64>>,
}

impl HostContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .load(std::sync::atomic::Ordering::Relaxed)
            || Instant::now() >= self.deadline
    }
}

#[derive(Deserialize)]
#[serde(tag = "op")]
pub enum HostOperationRequest {
    #[serde(rename = "kv_get")]
    KvGet { key: String },
    #[serde(rename = "kv_set")]
    KvSet { key: String, value: Value },
    #[serde(rename = "content_save")]
    ContentSave {
        type_key: String,
        #[serde(default)]
        entry_id: Option<i64>,
        #[serde(default)]
        expected_version: Option<i64>,
        slug: String,
        title: String,
        #[serde(default)]
        values: BTreeMap<String, Value>,
        #[serde(default)]
        term_ids: Option<Vec<i64>>,
    },
    #[serde(rename = "http_request")]
    HttpRequest {
        method: String,
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        body: Option<String>,
    },
}

#[derive(Serialize)]
pub struct HostOperationResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl HostOperationResponse {
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            found: None,
            value: None,
            status: None,
            headers: None,
            body: None,
        }
    }
}

pub fn handle_host_call(ctx: &mut HostContext, req_json: &str) -> HostOperationResponse {
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }

    ctx.host_calls_count += 1;
    if ctx.host_calls_count > MAX_HOST_CALLS {
        return HostOperationResponse::err("Vượt quá số lần gọi host tối đa (100).");
    }

    let req: HostOperationRequest = match serde_json::from_str(req_json) {
        Ok(r) => r,
        Err(_) => return HostOperationResponse::err("Thao tác host không đúng định dạng JSON."),
    };

    match req {
        HostOperationRequest::KvGet { key } => handle_kv_get(ctx, &key),
        HostOperationRequest::KvSet { key, value } => {
            if ctx.execution_mode != opendash_core::hooks::HookExecutionMode::Standard {
                return HostOperationResponse::err("Ghi KV bị cấm trong save hook.");
            }
            if let Some(ref caps) = ctx.action_capabilities {
                if !caps.iter().any(|c| c == "kv_write") {
                    return HostOperationResponse::err("permission_denied: action:kv_write");
                }
            }
            if ctx.staged_content_command.is_some() {
                return HostOperationResponse::err(
                    "Không cho phép trộn ghi KV và ghi nội dung trong cùng một action (V1).",
                );
            }
            handle_kv_set(ctx, &key, value)
        }
        HostOperationRequest::HttpRequest {
            method,
            url,
            headers,
            body,
        } => {
            if ctx.execution_mode != opendash_core::hooks::HookExecutionMode::Standard {
                return HostOperationResponse::err("Gọi HTTP ngoài bị cấm trong save hook.");
            }
            if let Some(ref caps) = ctx.action_capabilities {
                if !caps.iter().any(|c| c == "http") {
                    return HostOperationResponse::err("permission_denied: action:http");
                }
            }
            if let Ok(parsed) = url::Url::parse(&url) {
                let path = parsed.path();
                if path.starts_with("/api/admin") || path == "/save" || path == "/preview" || path == "/theme" {
                    return HostOperationResponse::err("Cấm gọi HTTP loopback tới Admin/CMS nội bộ.");
                }
            }
            handle_http_request(ctx, &method, &url, headers, body)
        }
        HostOperationRequest::ContentSave {
            type_key,
            entry_id,
            expected_version,
            slug,
            title,
            values,
            term_ids,
        } => {
            if ctx.execution_mode != opendash_core::hooks::HookExecutionMode::Standard {
                return HostOperationResponse::err("Ghi nội dung bị cấm trong save hook.");
            }
            if let Some(ref caps) = ctx.action_capabilities {
                if !caps.iter().any(|c| c == "content_write") {
                    return HostOperationResponse::err("permission_denied: action:content_write");
                }
            }
            if !ctx.staged_kv_writes.is_empty() {
                return HostOperationResponse::err(
                    "Không cho phép trộn ghi KV và ghi nội dung trong cùng một action (V1).",
                );
            }
            if ctx.staged_content_command.is_some() {
                return HostOperationResponse::err(
                    "Mỗi action chỉ được thực thi tối đa một lệnh ghi nội dung (V1).",
                );
            }
            if !ctx.manifest.can_write_content_type(&type_key) {
                return HostOperationResponse::err("permission_denied: content_write");
            }
            ctx.staged_content_command = Some(StagedContentCommand {
                type_key,
                entry_id,
                expected_version,
                slug,
                title,
                values,
                term_ids,
            });
            HostOperationResponse {
                ok: true,
                error: None,
                found: None,
                value: None,
                status: None,
                headers: None,
                body: None,
            }
        }
    }
}

fn handle_kv_get(ctx: &mut HostContext, key: &str) -> HostOperationResponse {
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }
    if !ctx.manifest.has_sqlite_perm("kv_read") {
        return HostOperationResponse::err("permission_denied: sqlite:kv_read");
    }
    if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
        return HostOperationResponse::err("Key không hợp lệ hoặc quá dài.");
    }

    // 1. Check staged writes in current invocation first
    if let Some(staged) = ctx.staged_kv_writes.get(key) {
        return HostOperationResponse {
            ok: true,
            error: None,
            found: Some(true),
            value: Some(staged.clone()),
            status: None,
            headers: None,
            body: None,
        };
    }

    // 2. Query via IPC closure if in worker process mode
    if let Some(ref ipc) = ctx.kv_ipc {
        return match ipc(key) {
            Ok(resp) => resp,
            Err(e) => HostOperationResponse::err(format!("Lỗi IPC kv_get: {e}")),
        };
    }

    // 3. Query database synchronously via tokio handle bounded by remaining deadline
    let Some(ref db) = ctx.db_pool else {
        return HostOperationResponse::err("Không có kết nối cơ sở dữ liệu khả dụng.");
    };

    let remaining = ctx.deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return HostOperationResponse::err("Lỗi cơ sở dữ liệu: Quá thời gian truy vấn (timeout).");
    }
    let db_timeout = std::cmp::min(Duration::from_secs(2), remaining);

    let plugin_id = ctx.plugin_id.clone();
    let db = db.clone();
    let key_clone = key.to_string();

    let query_res = tokio::runtime::Handle::current().block_on(async move {
        tokio::time::timeout(
            db_timeout,
            crate::storage::plugin_kv::get(&db, &plugin_id, &key_clone),
        )
        .await
    });

    match query_res {
        Ok(Ok(Some(val_str))) => match serde_json::from_str::<Value>(&val_str) {
            Ok(v) => HostOperationResponse {
                ok: true,
                error: None,
                found: Some(true),
                value: Some(v),
                status: None,
                headers: None,
                body: None,
            },
            Err(_) => HostOperationResponse::err("Dữ liệu trong database không phải JSON hợp lệ."),
        },
        Ok(Ok(None)) => HostOperationResponse {
            ok: true,
            error: None,
            found: Some(false),
            value: None,
            status: None,
            headers: None,
            body: None,
        },
        Ok(Err(e)) => HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
        Err(_) => {
            HostOperationResponse::err("Lỗi cơ sở dữ liệu: Quá thời gian truy vấn (timeout).")
        }
    }
}

fn handle_kv_set(ctx: &mut HostContext, key: &str, value: Value) -> HostOperationResponse {
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }
    if !ctx.manifest.has_sqlite_perm("kv_write") {
        return HostOperationResponse::err("permission_denied: sqlite:kv_write");
    }
    if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
        return HostOperationResponse::err("Key không hợp lệ hoặc quá dài.");
    }

    let serialized = match serde_json::to_string(&value) {
        Ok(s) => s,
        Err(_) => return HostOperationResponse::err("Không thể chuyển đổi value thành JSON."),
    };

    if serialized.len() > MAX_KV_VALUE_BYTES {
        return HostOperationResponse::err("Value vượt quá giới hạn 512 KB.");
    }

    // Stage write in memory. It will be committed atomically on invocation success.
    // If the invocation traps or times out, staged writes are dropped immediately.
    ctx.staged_kv_writes.insert(key.to_string(), value);

    HostOperationResponse {
        ok: true,
        error: None,
        found: None,
        value: None,
        status: None,
        headers: None,
        body: None,
    }
}

#[allow(unused_imports)]
pub use super::egress::is_forbidden_ip;

fn handle_http_request(
    ctx: &mut HostContext,
    method: &str,
    target_url: &str,
    headers: BTreeMap<String, String>,
    body: Option<String>,
) -> HostOperationResponse {
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }
    if ctx.http_calls_count >= super::egress::MAX_HTTP_CALLS_PER_INVOCATION {
        return HostOperationResponse::err(
            "quota_exceeded: Vượt quá giới hạn 10 HTTP requests mỗi lần chạy.",
        );
    }
    ctx.http_calls_count += 1;

    match super::egress::execute_scoped_http_request(
        &ctx.manifest,
        method,
        target_url,
        headers,
        body,
        ctx.allow_loopback,
        ctx.deadline,
    ) {
        Ok(resp) => HostOperationResponse {
            ok: true,
            error: None,
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
                sqlite: vec![],
                http: vec![],
                network: vec![NetworkScopeGrant {
                    scheme: "http".into(),
                    host: "127.0.0.1".into(),
                    port: Some(1),
                    methods: vec!["GET".into()],
                    path_prefix: Some("/".into()),
                }],
                content_read: vec![],
                content_write: vec![],
            },
            hooks: vec![],
            hook_priority: 0,
            actions: vec![],
            declarations: Default::default(),
        };
        let mut ctx = HostContext {
            plugin_id: manifest.id.clone(),
            manifest,
            input_bytes: vec![],
            output_bytes: None,
            host_calls_count: 0,
            http_calls_count: super::super::egress::MAX_HTTP_CALLS_PER_INVOCATION,
            db_pool: None,
            kv_ipc: None,
            _correlation_id: "quota".into(),
            allow_loopback: true,
            limits: Default::default(),
            staged_kv_writes: Default::default(),
            cancellation_token: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(5),
            execution_mode: opendash_core::hooks::HookExecutionMode::Standard,
            staged_content_command: None,
            action_capabilities: None,
        };
        let res = handle_http_request(
            &mut ctx,
            "GET",
            "http://127.0.0.1:1/",
            BTreeMap::new(),
            None,
        );
        assert!(!res.ok);
        assert!(
            res.error.as_deref().unwrap_or("").contains("quota_exceeded"),
            "expected quota_exceeded, got {:?}",
            res.error
        );
    }
}
