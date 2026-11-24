use std::{
    io::{Read, Write},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

use opendash_core::hooks::{ExecutionPrincipal, HookExecutionMode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimitsBuilder, TrapCode};

use super::{
    host_ops::{
        HostContext, HostOperationResponse, HostReadRequest, HostToWorkerFrame,
        MAX_IPC_FRAME_BYTES, WorkerExecutionResult, WorkerToHostFrame,
    },
    manifest::PluginManifest,
    runtime::{INVOCATION_FUEL, MAX_LINEAR_MEMORY_BYTES, register_host_imports},
};

pub const IPC_STREAM_MAGIC: &[u8; 8] = b"\x00ODIPC2\x00";

fn default_system_principal() -> ExecutionPrincipal {
    ExecutionPrincipal::System
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerInvocationRequest {
    pub manifest: PluginManifest,
    pub input: Value,
    pub allow_loopback: bool,
    pub correlation_id: String,
    pub deadline_ms: u64,
    #[serde(default)]
    pub execution_mode: HookExecutionMode,
    #[serde(default = "default_system_principal")]
    pub principal: ExecutionPrincipal,
    #[serde(default)]
    pub action_capabilities: Option<Vec<String>>,
}

pub fn read_ipc_frame<R: Read, T: DeserializeOwned>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<T, String> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .map_err(|e| format!("Không thể đọc độ dài IPC frame: {e}"))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > max_bytes {
        return Err(format!(
            "Kích thước IPC frame không hợp lệ ({len} bytes, tối đa {max_bytes})."
        ));
    }
    let mut payload = vec![0u8; len];
    reader
        .read_exact(&mut payload)
        .map_err(|e| format!("Không thể đọc nội dung IPC frame: {e}"))?;
    serde_json::from_slice(&payload).map_err(|e| format!("Lỗi parse IPC frame JSON: {e}"))
}

pub fn write_ipc_frame<W: Write, T: Serialize>(
    writer: &mut W,
    value: &T,
    max_bytes: usize,
) -> Result<(), String> {
    let payload = serde_json::to_vec(value).map_err(|e| format!("Lỗi serialize IPC frame: {e}"))?;
    if payload.is_empty() || payload.len() > max_bytes {
        return Err(format!(
            "IPC frame vượt quá giới hạn ({} bytes, tối đa {max_bytes}).",
            payload.len()
        ));
    }
    let len = u32::try_from(payload.len())
        .map_err(|_| "IPC frame quá lớn cho u32 length prefix.".to_string())?;
    writer
        .write_all(&len.to_le_bytes())
        .map_err(|e| format!("Lỗi ghi độ dài IPC frame: {e}"))?;
    writer
        .write_all(&payload)
        .map_err(|e| format!("Lỗi ghi dữ liệu IPC frame: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Lỗi flush IPC frame: {e}"))
}

pub fn run_worker_from_stdio() {
    {
        let mut stdout = std::io::stdout().lock();
        if stdout.write_all(IPC_STREAM_MAGIC).is_err() || stdout.flush().is_err() {
            return;
        }
    }

    // 1. Read wasm bytes and request frame from stdin
    let (wasm_bytes, req) = {
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();

        let mut wasm_len_buf = [0u8; 4];
        if handle.read_exact(&mut wasm_len_buf).is_err() {
            emit_ipc_error("Không thể đọc độ dài wasm bytes từ stdin.");
            return;
        }
        let wasm_len = u32::from_le_bytes(wasm_len_buf) as usize;
        if wasm_len == 0 || wasm_len > super::package::MAX_WASM_BYTES {
            emit_ipc_error("Độ dài wasm bytes vượt quá giới hạn.");
            return;
        }
        let mut wasm_bytes = vec![0u8; wasm_len];
        if handle.read_exact(&mut wasm_bytes).is_err() {
            emit_ipc_error("Không thể đọc dữ liệu wasm bytes từ stdin.");
            return;
        }

        let req: WorkerInvocationRequest = match read_ipc_frame(&mut handle, MAX_IPC_FRAME_BYTES) {
            Ok(r) => r,
            Err(e) => {
                emit_ipc_error(&format!("Lỗi đọc WorkerInvocationRequest: {e}"));
                return;
            }
        };
        (wasm_bytes, req)
    };

    // 2. Setup Wasmi and execute
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

    let ipc_io_lock = Arc::new(Mutex::new(()));
    let read_ipc = Arc::new(
        move |read_req: HostReadRequest| -> Result<HostOperationResponse, String> {
            let _guard = ipc_io_lock
                .lock()
                .map_err(|_| "IPC mutex bị khóa.".to_string())?;
            {
                let mut stdout = std::io::stdout().lock();
                write_ipc_frame(
                    &mut stdout,
                    &WorkerToHostFrame::ReadRequest(read_req),
                    MAX_IPC_FRAME_BYTES,
                )?;
            }
            let mut stdin = std::io::stdin().lock();
            let frame: HostToWorkerFrame = read_ipc_frame(&mut stdin, MAX_IPC_FRAME_BYTES)?;
            match frame {
                HostToWorkerFrame::ReadResponse(resp) => Ok(resp),
            }
        },
    );

    let host_ctx = HostContext {
        plugin_id: req.manifest.id.clone(),
        manifest: req.manifest,
        input_bytes,
        output_bytes: None,
        last_resp_bytes: None,
        host_calls_count: 0,
        http_calls_count: 0,
        db_pool: None,
        secrets_root: None,
        read_ipc: Some(read_ipc),
        _correlation_id: req.correlation_id,
        allow_loopback: req.allow_loopback,
        limits,
        staged_kv: Default::default(),
        staged_kv_bytes: 0,
        staged_logs: vec![],
        cancellation_token,
        deadline,
        execution_mode: req.execution_mode,
        principal: req.principal,
        staged_content_command: None,
        staged_content_delete: None,
        action_capabilities: req.action_capabilities,
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
        emit_ipc_frame(WorkerExecutionResult {
            ok: false,
            code: exit_code,
            error_type: Some("guest_failed".into()),
            error: Some(format!("Plugin kết thúc với mã lỗi {exit_code}.")),
            output: None,
            staged_kv: Default::default(),
            staged_logs: vec![],
            staged_content_command: None,
            staged_content_delete: None,
        });
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

    emit_ipc_frame(WorkerExecutionResult {
        ok: true,
        code: exit_code,
        error_type: None,
        error: None,
        output: Some(output_val),
        staged_kv: ctx.staged_kv,
        staged_logs: ctx.staged_logs,
        staged_content_command: ctx.staged_content_command,
        staged_content_delete: ctx.staged_content_delete,
    });
}

fn emit_ipc_frame(result: WorkerExecutionResult) {
    let mut stdout = std::io::stdout().lock();
    let _ = write_ipc_frame(
        &mut stdout,
        &WorkerToHostFrame::Result(result),
        MAX_IPC_FRAME_BYTES,
    );
}

fn emit_ipc_error(msg: &str) {
    emit_ipc_typed_error("internal", msg);
}

fn emit_ipc_typed_error(error_type: &str, msg: &str) {
    emit_ipc_frame(WorkerExecutionResult {
        ok: false,
        code: 1,
        error_type: Some(error_type.into()),
        error: Some(msg.into()),
        output: None,
        staged_kv: Default::default(),
        staged_logs: vec![],
        staged_content_command: None,
        staged_content_delete: None,
    });
}
