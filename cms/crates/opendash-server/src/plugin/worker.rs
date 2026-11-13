use std::{
    io::{Read, Write},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimitsBuilder, TrapCode};

use super::{
    host_ops::{HostContext, HostOperationResponse},
    manifest::PluginManifest,
    runtime::{INVOCATION_FUEL, MAX_LINEAR_MEMORY_BYTES, register_host_imports},
};

pub const IPC_PREFIX: &str = "__OPENDASH_IPC__:";

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerInvocationRequest {
    pub manifest: PluginManifest,
    pub input: Value,
    pub allow_loopback: bool,
    pub correlation_id: String,
    pub deadline_ms: u64,
    #[serde(default)]
    pub execution_mode: opendash_core::hooks::HookExecutionMode,
}

pub fn run_worker_from_stdio() {
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();

    // 1. Read wasm bytes
    let mut wasm_len_buf = [0u8; 4];
    if handle.read_exact(&mut wasm_len_buf).is_err() {
        emit_ipc_error("Không thể đọc độ dài wasm bytes từ stdin.");
        return;
    }
    let wasm_len = u32::from_le_bytes(wasm_len_buf) as usize;
    let mut wasm_bytes = vec![0u8; wasm_len];
    if handle.read_exact(&mut wasm_bytes).is_err() {
        emit_ipc_error("Không thể đọc dữ liệu wasm bytes từ stdin.");
        return;
    }

    // 2. Read request JSON
    let mut req_len_buf = [0u8; 4];
    if handle.read_exact(&mut req_len_buf).is_err() {
        emit_ipc_error("Không thể đọc độ dài request JSON từ stdin.");
        return;
    }
    let req_len = u32::from_le_bytes(req_len_buf) as usize;
    let mut req_bytes = vec![0u8; req_len];
    if handle.read_exact(&mut req_bytes).is_err() {
        emit_ipc_error("Không thể đọc dữ liệu request JSON từ stdin.");
        return;
    }

    let req: WorkerInvocationRequest = match serde_json::from_slice(&req_bytes) {
        Ok(r) => r,
        Err(e) => {
            emit_ipc_error(&format!("Lỗi deserialize WorkerInvocationRequest: {e}"));
            return;
        }
    };

    drop(handle);

    // 3. Setup Wasmi and execute
    let deadline = Instant::now() + Duration::from_millis(req.deadline_ms);
    let cancellation_token = Arc::new(AtomicBool::new(false));

    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);

    let input_bytes = match serde_json::to_vec(&req.input) {
        Ok(b) => b,
        Err(e) => {
            emit_ipc_error(&format!("Lỗi serialize input: {e}"));
            return;
        }
    };

    let limits = StoreLimitsBuilder::new()
        .memory_size(MAX_LINEAR_MEMORY_BYTES)
        .instances(1)
        .tables(10)
        .build();

    let kv_ipc = Arc::new(|key: &str| -> Result<HostOperationResponse, String> {
        let msg = json!({
            "ipc": "kv_get",
            "key": key
        });
        println!("{}{}", IPC_PREFIX, msg);
        let _ = std::io::stdout().flush();

        let mut line = String::new();
        let stdin = std::io::stdin();
        stdin
            .read_line(&mut line)
            .map_err(|e| format!("Lỗi đọc IPC stdin: {e}"))?;

        let val: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("Lỗi parse phản hồi IPC kv_get: {e}"))?;

        let found = val.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
        let value = val.get("value").cloned();

        Ok(HostOperationResponse {
            ok: true,
            error: None,
            found: Some(found),
            value,
            status: None,
            headers: None,
            body: None,
        })
    });

    let host_ctx = HostContext {
        plugin_id: req.manifest.id.clone(),
        manifest: req.manifest,
        input_bytes,
        output_bytes: None,
        host_calls_count: 0,
        db_pool: None,
        kv_ipc: Some(kv_ipc),
        _correlation_id: req.correlation_id,
        allow_loopback: req.allow_loopback,
        limits,
        staged_kv_writes: Default::default(),
        cancellation_token,
        deadline,
        execution_mode: req.execution_mode,
    };

    let mut store = Store::new(&engine, host_ctx);
    store.limiter(|ctx: &mut HostContext| &mut ctx.limits);
    if let Err(e) = store.set_fuel(INVOCATION_FUEL) {
        emit_ipc_error(&format!("Không thể cấp fuel: {e}"));
        return;
    }

    let module = match Module::new(&engine, &wasm_bytes) {
        Ok(m) => m,
        Err(e) => {
            emit_ipc_error(&format!("Module WebAssembly không hợp lệ: {e}"));
            return;
        }
    };

    let mut linker = <Linker<HostContext>>::new(&engine);
    if let Err(e) = register_host_imports(&mut linker) {
        emit_ipc_error(&format!("Lỗi linker: {e}"));
        return;
    }

    let instance = match linker.instantiate_and_start(&mut store, &module) {
        Ok(inst) => inst,
        Err(e) => match e.as_trap_code() {
            Some(TrapCode::OutOfFuel) => {
                emit_ipc_typed_error("out_of_fuel", "Plugin hết fuel (vượt quá giới hạn CPU).");
                return;
            }
            _ => {
                emit_ipc_error(&format!("Lỗi khởi tạo module: {e}"));
                return;
            }
        },
    };

    let run_fn = match instance.get_typed_func::<(), i32>(&store, "opendash_run") {
        Ok(f) => f,
        Err(e) => {
            emit_ipc_error(&format!("Không tìm thấy export opendash_run: {e}"));
            return;
        }
    };

    let run_res = run_fn.call(&mut store, ());
    let exit_code = match run_res {
        Ok(code) => code,
        Err(e) => match e.as_trap_code() {
            Some(TrapCode::OutOfFuel) => {
                emit_ipc_typed_error("out_of_fuel", "Plugin hết fuel (vượt quá giới hạn CPU).");
                return;
            }
            _ => {
                let msg = e.to_string();
                if msg.contains("deadline exceeded") || msg.contains("cancelled") {
                    emit_ipc_typed_error("timeout", "Quá thời gian thực thi (deadline 5 giây).");
                } else if msg.contains("memory") || msg.contains("limit") {
                    emit_ipc_typed_error("memory_limit", "Vượt quá giới hạn bộ nhớ 16 MB.");
                } else {
                    emit_ipc_typed_error("trap", &format!("Guest trap: {msg}"));
                }
                return;
            }
        },
    };

    if exit_code != 0 {
        emit_ipc_typed_error(
            "guest_failed",
            &format!("Plugin kết thúc với mã lỗi {exit_code}."),
        );
        return;
    }

    let ctx = store.into_data();
    let output_bytes = match ctx.output_bytes {
        Some(b) => b,
        None => {
            emit_ipc_typed_error("invalid_output", "Plugin không gọi host_write_output.");
            return;
        }
    };
    let output_val = match serde_json::from_slice::<Value>(&output_bytes) {
        Ok(v) => v,
        Err(e) => {
            emit_ipc_typed_error(
                "invalid_output",
                &format!("Output không phải JSON hợp lệ: {e}"),
            );
            return;
        }
    };

    let res_msg = json!({
        "ipc": "result",
        "ok": true,
        "code": exit_code,
        "output": output_val,
        "staged_kv_writes": ctx.staged_kv_writes
    });
    println!("{}{}", IPC_PREFIX, res_msg);
    let _ = std::io::stdout().flush();
}

fn emit_ipc_error(msg: &str) {
    let res = json!({
        "ipc": "result",
        "ok": false,
        "error_type": "internal",
        "error": msg
    });
    println!("{}{}", IPC_PREFIX, res);
    let _ = std::io::stdout().flush();
}

fn emit_ipc_typed_error(error_type: &str, msg: &str) {
    let res = json!({
        "ipc": "result",
        "ok": false,
        "error_type": error_type,
        "error": msg
    });
    println!("{}{}", IPC_PREFIX, res);
    let _ = std::io::stdout().flush();
}
