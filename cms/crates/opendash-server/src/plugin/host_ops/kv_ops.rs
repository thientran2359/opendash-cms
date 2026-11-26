use super::*;

impl HostContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .load(std::sync::atomic::Ordering::Relaxed)
            || Instant::now() >= self.deadline
    }

    pub(super) fn has_any_kv_stage(&self) -> bool {
        !self.staged_kv.is_empty()
    }

    pub(super) fn has_content_stage(&self) -> bool {
        self.staged_content_command.is_some() || self.staged_lifecycle_command.is_some()
    }
}

pub(super) fn stage_lifecycle_command(
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

pub(super) fn handle_kv_get(ctx: &mut HostContext, key: &str) -> HostOperationResponse {
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

pub(super) fn handle_kv_list(
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

pub(super) fn check_kv_write_preconditions(
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

pub(super) fn handle_kv_set(
    ctx: &mut HostContext,
    key: &str,
    value: Value,
) -> HostOperationResponse {
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

pub(super) fn handle_kv_delete(ctx: &mut HostContext, key: &str) -> HostOperationResponse {
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
