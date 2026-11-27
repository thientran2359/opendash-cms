use super::*;

pub(super) fn make_err(error_type: &str, message: impl Into<String>) -> WorkerExecutionResult {
    WorkerExecutionResult {
        ok: false,
        code: 1,
        error_type: Some(error_type.to_owned()),
        error: Some(message.into()),
        output: None,
        staged_kv: Default::default(),
        staged_logs: Vec::new(),
        staged_content_command: None,
        staged_lifecycle_command: None,
    }
}

pub(super) fn classify_js_error(err: &JsError, cancelled: bool) -> (&'static str, String) {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    if cancelled || lower.contains("deadline exceeded") || lower.contains("cancelled") {
        ("timeout", "Quá thời gian thực thi JavaScript.".to_owned())
    } else if lower.contains("runtimelimit")
        || lower.contains("iteration loops")
        || lower.contains("loop iteration")
        || lower.contains("recursive calls")
        || lower.contains("recursion")
        || lower.contains("stack size")
        || lower.contains("runtime limit")
    {
        (
            "out_of_fuel",
            format!("JavaScript vượt quá giới hạn thực thi (RuntimeLimits): {msg}"),
        )
    } else {
        ("trap", format!("Lỗi thực thi JavaScript: {msg}"))
    }
}

pub fn execute_js_worker(
    js_source: &str,
    host_ctx: HostContext,
    input: Value,
) -> WorkerExecutionResult {
    let _guard = ActiveHostGuard;
    ACTIVE_HOST.with(|cell| {
        *cell.borrow_mut() = Some(JsHostBridge {
            ctx: host_ctx,
            in_registration: true,
        });
    });

    let mut ctx = Context::default();
    configure_runtime_limits(&mut ctx);

    let host_fn = NativeFunction::from_fn_ptr(js_internal_host_call);
    if let Err(err) = ctx.register_global_property(
        js_string!("__od_internal_host_call"),
        host_fn.to_js_function(ctx.realm()),
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    ) {
        return make_err("internal", format!("Không thể khởi tạo host bridge: {err}"));
    }

    let dispatch_val = match ctx.eval(Source::from_bytes(OD_BOOTSTRAP_JS.as_bytes())) {
        Ok(val) => val,
        Err(err) => {
            return make_err(
                "internal",
                format!("Không thể khởi tạo OpenDash JS SDK: {err}"),
            );
        }
    };
    let Some(dispatch_fn) = dispatch_val.as_callable() else {
        return make_err("internal", "OpenDash JS SDK dispatcher không hợp lệ.");
    };

    // Phase 1: Top-level script evaluation (registration only; host calls forbidden)
    if let Err(err) = ctx.eval(Source::from_bytes(js_source.as_bytes())) {
        let cancelled =
            ACTIVE_HOST.with(|c| c.borrow().as_ref().is_some_and(|b| b.ctx.is_cancelled()));
        let (kind, msg) = classify_js_error(&err, cancelled);
        return make_err(kind, msg);
    }

    // Phase 2: Handler dispatch (host calls permitted under HostPolicy)
    let cancelled_before_dispatch = ACTIVE_HOST.with(|cell| {
        if let Some(bridge) = cell.borrow_mut().as_mut() {
            bridge.in_registration = false;
            bridge.ctx.is_cancelled()
        } else {
            true
        }
    });
    if cancelled_before_dispatch {
        return make_err("timeout", "Quá thời gian thực thi JavaScript.");
    }

    let env_js = match JsValue::from_json(&input, &mut ctx) {
        Ok(v) => v,
        Err(err) => {
            return make_err("internal", format!("Không thể chuyển input sang JS: {err}"));
        }
    };

    let call_res = dispatch_fn.call(&JsValue::undefined(), &[env_js], &mut ctx);
    let result_val = match call_res {
        Ok(v) => v,
        Err(err) => {
            let cancelled =
                ACTIVE_HOST.with(|c| c.borrow().as_ref().is_some_and(|b| b.ctx.is_cancelled()));
            let (kind, msg) = classify_js_error(&err, cancelled);
            return make_err(kind, msg);
        }
    };

    let result_json = match result_val.to_json(&mut ctx) {
        Ok(Some(v)) => v,
        Ok(None) => Value::Null,
        Err(err) => {
            return make_err(
                "invalid_output",
                format!("Không thể chuyển kết quả JS sang JSON: {err}"),
            );
        }
    };

    let bridge = ACTIVE_HOST.with(|cell| cell.borrow_mut().take());
    let Some(bridge) = bridge else {
        return make_err("internal", "HostContext bị mất sau khi thực thi JS.");
    };
    if bridge.ctx.is_cancelled() {
        return make_err("timeout", "Quá thời gian thực thi JavaScript.");
    }

    if result_json.get("ok").and_then(Value::as_bool) != Some(true) {
        let err_type = result_json
            .get("error_type")
            .and_then(Value::as_str)
            .unwrap_or("invalid_output");
        let err_msg = result_json
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Lỗi thực thi JS handler.");
        return make_err(err_type, err_msg);
    }

    let output_val = result_json.get("value").cloned().unwrap_or(Value::Null);
    match serde_json::to_vec(&output_val) {
        Ok(bytes) if bytes.len() <= MAX_JS_OUTPUT_BYTES => {}
        Ok(_) => {
            return make_err(
                "invalid_output",
                "Đầu ra JSON từ JavaScript vượt quá giới hạn 1 MiB.",
            );
        }
        Err(err) => {
            return make_err(
                "invalid_output",
                format!("Không thể serialize đầu ra JSON từ JavaScript: {err}"),
            );
        }
    }

    WorkerExecutionResult {
        ok: true,
        code: 0,
        error_type: None,
        error: None,
        output: Some(output_val),
        staged_kv: bridge.ctx.staged_kv,
        staged_logs: bridge.ctx.staged_logs,
        staged_content_command: bridge.ctx.staged_content_command,
        staged_lifecycle_command: bridge.ctx.staged_lifecycle_command,
    }
}
