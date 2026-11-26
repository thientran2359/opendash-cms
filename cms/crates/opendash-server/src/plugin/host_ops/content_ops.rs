use super::*;

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

pub(super) fn dispatch_host_read(ctx: &HostContext, req: HostReadRequest) -> HostOperationResponse {
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
            crate::plugin::runtime::execute_host_read_request_for_owner(
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

pub(super) fn staged_entry_size(key: &str, val: Option<&Value>) -> usize {
    key.len()
        + val
            .and_then(|v| serde_json::to_vec(v).ok().map(|b| b.len()))
            .unwrap_or(0)
}

pub(super) fn handle_http_request(
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
        _ => crate::plugin::egress::MAX_HTTP_CALLS_PER_INVOCATION,
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

    match crate::plugin::egress::execute_scoped_http_request(
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
