use std::{
    collections::BTreeMap,
    net::IpAddr,
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

pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
                || v4.octets()[0] >= 224
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || if let Some(v4_mapped) = v6.to_ipv4_mapped() {
                    is_forbidden_ip(IpAddr::V4(v4_mapped))
                } else {
                    false
                }
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xff00) == 0xff00
        }
    }
}

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

    // 1. Check manifest allowlist
    if !ctx.manifest.is_http_allowed(target_url) {
        return HostOperationResponse::err(
            "permission_denied: URL không nằm trong allowlist HTTP của manifest.",
        );
    }

    let parsed_url = match url::Url::parse(target_url) {
        Ok(u) => u,
        Err(_) => return HostOperationResponse::err("URL không hợp lệ."),
    };

    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return HostOperationResponse::err("Chỉ hỗ trợ scheme http hoặc https.");
    }

    let Some(raw_host) = parsed_url.host_str() else {
        return HostOperationResponse::err("URL không chứa host hợp lệ.");
    };
    let clean_host = raw_host.trim_start_matches('[').trim_end_matches(']');
    let port = parsed_url.port_or_known_default().unwrap_or(80);

    // 2. Resolve DNS & verify target IP
    let target_socket_addr = if let Ok(ip) = clean_host.parse::<IpAddr>() {
        if is_forbidden_ip(ip) && !(ctx.allow_loopback && ip.is_loopback()) {
            return HostOperationResponse::err(
                "permission_denied: Không được phép truy cập mạng loopback hoặc private network.",
            );
        }
        std::net::SocketAddr::new(ip, port)
    } else if clean_host.eq_ignore_ascii_case("localhost") {
        if !ctx.allow_loopback {
            return HostOperationResponse::err(
                "permission_denied: Không được phép truy cập mạng loopback hoặc private network.",
            );
        }
        std::net::SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port)
    } else {
        let remaining = ctx.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
        }
        let dns_timeout = std::cmp::min(Duration::from_secs(1), remaining);
        let lookup = format!("{}:{}", clean_host, port);
        let lookup_res = if let Ok(h) = tokio::runtime::Handle::try_current() {
            h.block_on(async { tokio::time::timeout(dns_timeout, tokio::net::lookup_host(&lookup)).await })
        } else if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
            rt.block_on(async { tokio::time::timeout(dns_timeout, tokio::net::lookup_host(&lookup)).await })
        } else {
            return HostOperationResponse::err("Lỗi DNS: không khởi tạo được runtime.");
        };
        #[cfg(any())]
        let _unused = tokio::runtime::Handle::current().block_on(async {
            tokio::time::timeout(dns_timeout, tokio::net::lookup_host(lookup)).await
        });
        let addrs_iter = match lookup_res {
            Ok(Ok(addrs)) => addrs,
            Ok(Err(_)) => {
                return HostOperationResponse::err("Lỗi DNS: không thể phân giải tên miền.");
            }
            Err(_) => {
                return HostOperationResponse::err("Lỗi DNS: quá thời gian phân giải tên miền.");
            }
        };
        let addrs_vec: Vec<std::net::SocketAddr> = addrs_iter.collect();
        if addrs_vec.is_empty() {
            return HostOperationResponse::err("Lỗi DNS: không tìm thấy địa chỉ IP.");
        }
        for addr in &addrs_vec {
            let ip = addr.ip();
            if is_forbidden_ip(ip) && !(ctx.allow_loopback && ip.is_loopback()) {
                return HostOperationResponse::err(
                    "permission_denied: Tên miền phân giải về địa chỉ loopback hoặc private network bị cấm.",
                );
            }
        }
        addrs_vec[0]
    };

    let remaining = ctx.deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }
    let http_timeout = std::cmp::min(HTTP_TIMEOUT, remaining);

    let method_upper = method.to_ascii_uppercase();
    if method_upper != "GET" && method_upper != "POST" {
        return HostOperationResponse::err("Chỉ hỗ trợ method GET hoặc POST.");
    }

    // 3. Build blocking client with strict timeout, pinned IP, NO environment proxy, and NO redirect
    let client = match reqwest::blocking::Client::builder()
        .timeout(http_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .resolve(clean_host, target_socket_addr)
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return HostOperationResponse::err("Không thể khởi tạo HTTP client.");
        }
    };

    let mut req_builder = match method_upper.as_str() {
        "GET" => client.get(parsed_url),
        "POST" => client.post(parsed_url),
        _ => unreachable!(),
    };

    if headers.len() > 50 {
        return HostOperationResponse::err("Số lượng header HTTP vượt quá 50.");
    }
    for (k, v) in headers {
        if k.len() > 128 || v.len() > 4096 {
            return HostOperationResponse::err("Header key hoặc value vượt quá giới hạn độ dài.");
        }
        req_builder = req_builder.header(k, v);
    }

    if let Some(b) = body {
        if b.len() > MAX_HTTP_BODY_BYTES {
            return HostOperationResponse::err("Body request HTTP vượt quá giới hạn 1 MB.");
        }
        req_builder = req_builder.body(b);
    }

    match req_builder.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut resp_headers = BTreeMap::new();
            for (k, v) in resp.headers() {
                if let Ok(val_str) = v.to_str() {
                    resp_headers.insert(k.as_str().to_string(), val_str.to_string());
                }
            }

            // Stream bounded read: take MAX_HTTP_BODY_BYTES + 1
            let mut body_buf = Vec::new();
            let mut reader = std::io::Read::take(resp, (MAX_HTTP_BODY_BYTES + 1) as u64);
            if std::io::Read::read_to_end(&mut reader, &mut body_buf).is_err() {
                return HostOperationResponse::err("Lỗi khi đọc response HTTP.");
            }

            if body_buf.len() > MAX_HTTP_BODY_BYTES {
                return HostOperationResponse::err("Response HTTP vượt quá giới hạn 1 MB.");
            }

            let resp_text = match String::from_utf8(body_buf) {
                Ok(t) => t,
                Err(_) => {
                    return HostOperationResponse::err("Response HTTP không phải UTF-8 hợp lệ.");
                }
            };

            HostOperationResponse {
                ok: true,
                error: None,
                found: None,
                value: None,
                status: Some(status),
                headers: Some(resp_headers),
                body: Some(resp_text),
            }
        }
        Err(e) => {
            if e.is_timeout() {
                HostOperationResponse::err("Lỗi kết nối HTTP: Quá thời gian chờ (timeout).")
            } else if e.is_redirect() {
                HostOperationResponse::err(
                    "Lỗi kết nối HTTP: Bị chặn chuyển hướng (redirect policy).",
                )
            } else {
                HostOperationResponse::err("Lỗi kết nối HTTP: Không thể hoàn thành yêu cầu.")
            }
        }
    }
}
