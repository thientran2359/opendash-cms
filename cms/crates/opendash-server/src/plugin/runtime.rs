use crate::storage::plugin_kv;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, watch};
use wasmi::{AsContextMut, Caller, Config, Engine, Error as WasmiError, Linker};
#[cfg(test)]
use wasmi::{Module, Store, StoreLimitsBuilder, TrapCode};

use super::host_ops::{self, HostContext};
use super::manifest::PluginManifest;

pub const INVOCATION_FUEL: u64 = 20_000_000;
pub const MAX_LINEAR_MEMORY_BYTES: usize = 16 * 1024 * 1024; // 16 MB
pub const INVOCATION_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct ActionActorFence {
    pub user_id: i64,
    pub required_role: String,
}

#[derive(Debug, Clone)]
pub struct ActionExecutionOutput {
    pub output: Value,
    pub staged_content_command: Option<host_ops::StagedContentCommand>,
}
pub const MAX_CONCURRENT_INVOCATIONS: usize = 4;

#[cfg(test)]
pub(crate) static TEST_STAGE_WRITE_BARRIER_HIT: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(crate) static TEST_STAGE_WRITE_PAUSE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(crate) static TEST_IPC_KV_GET_WAITING: AtomicBool = AtomicBool::new(false);

struct StagedCommitGuard {
    conn: Option<sqlx::pool::PoolConnection<sqlx::Sqlite>>,
    _initial_busy_timeout: i64,
    committed: bool,
}

impl Drop for StagedCommitGuard {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(mut conn) = self.conn.take() {
                conn.close_on_drop();
            }
        }
    }
}

struct ProcessGuard {
    child: Arc<std::sync::Mutex<Option<std::process::Child>>>,
    worker_thread: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
    active_workers: Arc<AtomicUsize>,
    cancellation_token: Arc<AtomicBool>,
    cancel_tx: watch::Sender<bool>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl ProcessGuard {
    fn cleanup(&mut self) {
        self.cancellation_token.store(true, Ordering::SeqCst);
        let _ = self.cancel_tx.send(true);
        if let Ok(mut lock) = self.child.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Ok(mut lock) = self.worker_thread.lock() {
            if let Some(handle) = lock.take() {
                let _ = handle.join();
            }
        }
        if self.permit.is_some() {
            self.active_workers.fetch_sub(1, Ordering::SeqCst);
            drop(self.permit.take());
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[derive(Debug)]
pub enum PluginError {
    Disabled,
    NotFound,
    ModuleCompilation(String),
    OutOfFuel,
    Timeout,
    BadPointer,
    GuestFailed(i32),
    Trap(String),
    InvalidOutput(String),
    ConcurrencyLimit,
    Internal(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Plugin chưa được bật hoặc đã bị vô hiệu hóa."),
            Self::NotFound => write!(f, "Plugin không tồn tại."),
            Self::ModuleCompilation(msg) => write!(f, "Module WebAssembly không hợp lệ: {msg}"),
            Self::OutOfFuel => write!(
                f,
                "Plugin hết fuel (vượt quá giới hạn CPU {INVOCATION_FUEL} đơn vị)."
            ),
            Self::Timeout => write!(f, "Quá thời gian thực thi (deadline 5 giây)."),
            Self::BadPointer => {
                write!(
                    f,
                    "Lỗi con trỏ bộ nhớ (Bad Pointer hoặc ngoài biên memory)."
                )
            }
            Self::GuestFailed(code) => write!(f, "Plugin kết thúc với mã lỗi {code}."),
            Self::Trap(msg) => write!(f, "Guest trap: {msg}"),
            Self::InvalidOutput(msg) => write!(f, "Dữ liệu đầu ra từ plugin không hợp lệ: {msg}"),
            Self::ConcurrencyLimit => write!(f, "Không thể lấy permit điều phối plugin."),
            Self::Internal(msg) => write!(f, "Lỗi máy chủ nội bộ: {msg}"),
        }
    }
}

impl std::error::Error for PluginError {}

#[derive(Clone)]
pub struct PluginRuntime {
    engine: Engine,
    semaphore: Arc<Semaphore>,
    active_workers: Arc<AtomicUsize>,
}

impl PluginRuntime {
    pub fn new() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        Self {
            engine,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_INVOCATIONS)),
            active_workers: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn active_workers(&self) -> usize {
        self.active_workers.load(Ordering::SeqCst)
    }

    pub fn semaphore(&self) -> &Arc<Semaphore> {
        &self.semaphore
    }

    pub async fn run_plugin(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
    ) -> Result<Value, PluginError> {
        self.run_plugin_with_generation(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            None,
        )
        .await
    }

    pub async fn run_plugin_with_generation(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
    ) -> Result<Value, PluginError> {
        self.run_plugin_with_mode(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            expected_generation,
            opendash_core::hooks::HookExecutionMode::Standard,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_with_mode(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
        execution_mode: opendash_core::hooks::HookExecutionMode,
        timeout_override: Option<Duration>,
    ) -> Result<Value, PluginError> {
        self.run_plugin_action(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            expected_generation,
            execution_mode,
            timeout_override,
            None,
            None,
        )
        .await
        .map(|res| res.output)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_action(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
        execution_mode: opendash_core::hooks::HookExecutionMode,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        let cancellation_token = Arc::new(AtomicBool::new(false));

        // Fail-fast concurrency control to prevent RAM exhaustion
        let permit = match self.semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => return Err(PluginError::ConcurrencyLimit),
        };

        let start = Instant::now();
        let effective_budget = timeout_override.unwrap_or(INVOCATION_DEADLINE).min(INVOCATION_DEADLINE);
        let deadline = start + effective_budget;
        let remaining = effective_budget;

        // In production and tests, always run in isolated worker process (no in-process fallback)
        let current_exe = std::env::current_exe().map_err(|e| {
            PluginError::Internal(format!("Không thể lấy đường dẫn executable hiện tại: {e}"))
        })?;

        let mut cmd = std::process::Command::new(current_exe);
        if cfg!(test) {
            cmd.args([
                "plugin::tests::tests::test_worker_entry",
                "--exact",
                "--nocapture",
            ]);
        } else {
            cmd.arg("--plugin-worker");
        }
        cmd.env("OPENDASH_WORKER_CHILD", "1");
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| {
            PluginError::Internal(format!("Không khởi động được worker process: {e}"))
        })?;

        let mut child_stdin = child.stdin.take().ok_or_else(|| {
            PluginError::Internal("Không mở được stdin của worker process".into())
        })?;
        let child_stdout = child.stdout.take().ok_or_else(|| {
            PluginError::Internal("Không mở được stdout của worker process".into())
        })?;

        let child_ref = Arc::new(std::sync::Mutex::new(Some(child)));
        let worker_thread_ref = Arc::new(std::sync::Mutex::new(None));
        self.active_workers.fetch_add(1, Ordering::SeqCst);

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let mut guard = ProcessGuard {
            child: Arc::clone(&child_ref),
            worker_thread: Arc::clone(&worker_thread_ref),
            active_workers: Arc::clone(&self.active_workers),
            cancellation_token: Arc::clone(&cancellation_token),
            cancel_tx,
            permit: Some(permit),
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tokio_handle = tokio::runtime::Handle::current();
        let worker_cancel = Arc::clone(&cancellation_token);
        let manifest_clone = manifest.clone();
        let db_pool_clone = db_pool.clone();
        let child_ref_worker = Arc::clone(&child_ref);

        let thread_handle = std::thread::Builder::new()
            .name(format!("plugin-worker-{}", manifest.id))
            .spawn(move || {
                use std::io::{BufRead, BufReader, Write};

                let cancel_watch = cancel_rx;
                let run = move || -> Result<(Value, BTreeMap<String, Value>, Option<host_ops::StagedContentCommand>), PluginError> {
                    let wasm_len = wasm_bytes.len() as u32;
                    child_stdin
                        .write_all(&wasm_len.to_le_bytes())
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi wasm len: {e}")))?;
                    child_stdin
                        .write_all(&wasm_bytes)
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi wasm bytes: {e}")))?;

                    let req = crate::plugin::worker::WorkerInvocationRequest {
                        manifest: manifest_clone.clone(),
                        input,
                        allow_loopback,
                        correlation_id,
                        deadline_ms: remaining.as_millis() as u64,
                        execution_mode,
                        action_capabilities: action_capabilities.clone(),
                    };
                    let req_json = serde_json::to_vec(&req)
                        .map_err(|e| PluginError::Internal(format!("Lỗi serialize request: {e}")))?;
                    let req_len = req_json.len() as u32;
                    child_stdin
                        .write_all(&req_len.to_le_bytes())
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi request len: {e}")))?;
                    child_stdin
                        .write_all(&req_json)
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi request bytes: {e}")))?;
                    child_stdin
                        .flush()
                        .map_err(|e| PluginError::Internal(format!("Lỗi flush stdin: {e}")))?;

                    let reader = BufReader::new(child_stdout);
                    for line_res in reader.lines() {
                        let line = match line_res {
                            Ok(l) => l,
                            Err(_) => break, // EOF or pipe broken
                        };
                        if let Some(payload) = line.strip_prefix(crate::plugin::worker::IPC_PREFIX) {
                            let msg: Value = serde_json::from_str(payload)
                                .map_err(|e| PluginError::Internal(format!("Lỗi parse IPC: {e}")))?;
                            let ipc_type = msg.get("ipc").and_then(|v| v.as_str()).unwrap_or("");
                            match ipc_type {
                                "kv_get" => {
                                    if worker_cancel.load(Ordering::Relaxed) || Instant::now() >= deadline || *cancel_watch.borrow() {
                                        return Err(PluginError::Timeout);
                                    }
                                    let key = msg.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                    let remaining_kv = deadline.saturating_duration_since(Instant::now());
                                    if remaining_kv.is_zero() {
                                        return Err(PluginError::Timeout);
                                    }

                                    let plugin_id = manifest_clone.id.clone();
                                    let key_str = key.to_string();
                                    let db = db_pool_clone.clone();

                                    let query_fut = async move {
                                        plugin_kv::get(&db, &plugin_id, &key_str)
                                        .await
                                    };

                                    #[cfg(test)]
                                    TEST_IPC_KV_GET_WAITING.store(true, Ordering::SeqCst);

                                    let mut watch_rx = cancel_watch.clone();
                                    let val_opt = match tokio_handle.block_on(async {
                                        tokio::select! {
                                            biased;
                                            _ = watch_rx.changed() => {
                                                Err(PluginError::Timeout)
                                            }
                                            res = query_fut => {
                                                match res {
                                                    Ok(opt) => Ok(opt),
                                                    Err(e) => Err(PluginError::Internal(format!("Lỗi DB kv_get: {e}"))),
                                                }
                                            }
                                            _ = tokio::time::sleep(remaining_kv) => {
                                                Err(PluginError::Timeout)
                                            }
                                        }
                                    }) {
                                        Ok(opt) => opt,
                                        Err(err) => return Err(err),
                                    };

                                    let resp_val = match val_opt {
                                        Some(s) => match serde_json::from_str::<Value>(&s) {
                                            Ok(v) => json!({"found": true, "value": v}),
                                            Err(_) => json!({"found": true, "value": Value::Null}),
                                        },
                                        None => json!({"found": false, "value": Value::Null}),
                                    };
                                    let resp_str = serde_json::to_string(&resp_val).unwrap();
                                    if child_stdin.write_all(format!("{resp_str}\n").as_bytes()).is_err() {
                                        break;
                                    }
                                    if child_stdin.flush().is_err() {
                                        break;
                                    }
                                }
                                "result" => {
                                    let ok = msg.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                                    if let Ok(mut lock) = child_ref_worker.lock() {
                                        if let Some(ref mut c) = *lock {
                                            let _ = c.wait();
                                        }
                                    }
                                    if ok {
                                        let output = msg.get("output").cloned().unwrap_or(Value::Null);
                                        let staged_writes: BTreeMap<String, Value> = msg
                                            .get("staged_kv_writes")
                                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                                            .unwrap_or_default();
                                        let staged_content: Option<host_ops::StagedContentCommand> = msg
                                            .get("staged_content_command")
                                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                                            .flatten();
                                        return Ok((output, staged_writes, staged_content));
                                    } else {
                                        let error_type = msg
                                            .get("error_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("internal");
                                        let err_msg = msg
                                            .get("error")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Lỗi thực thi plugin");
                                        match error_type {
                                            "out_of_fuel" => return Err(PluginError::OutOfFuel),
                                            "timeout" => return Err(PluginError::Timeout),
                                            "memory_limit" => {
                                                return Err(PluginError::Trap(
                                                    "Vượt quá giới hạn bộ nhớ 16 MB.".into(),
                                                ));
                                            }
                                            "invalid_output" => {
                                                return Err(PluginError::InvalidOutput(err_msg.into()));
                                            }
                                            "guest_failed" => {
                                                return Err(PluginError::GuestFailed(
                                                    msg.get("code")
                                                        .and_then(|v| v.as_i64())
                                                        .unwrap_or(1)
                                                        as i32,
                                                ));
                                            }
                                            _ => return Err(PluginError::Trap(err_msg.into())),
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(PluginError::Trap("Worker process kết thúc bất ngờ".into()))
                };

                let res = run();
                let _ = tx.send(res);
            })
            .map_err(|e| PluginError::Internal(format!("Không thể tạo worker thread: {e}")))?;

        if let Ok(mut lock) = worker_thread_ref.lock() {
            *lock = Some(thread_handle);
        }

        let (output_val, staged_writes, staged_content) = match tokio::time::timeout(remaining, rx).await {
            Ok(Ok(res)) => match res {
                Ok((out, writes, content_cmd)) => {
                    // Disarm child kill in guard on normal completion
                    if let Ok(mut lock) = guard.child.lock() {
                        let _ = lock.take();
                    }
                    (out, writes, content_cmd)
                }
                Err(err) => {
                    guard.cleanup();
                    return Err(err);
                }
            },
            Ok(Err(_)) => {
                guard.cleanup();
                return Err(PluginError::Trap(
                    "Worker process hoặc thread kết thúc bất ngờ".into(),
                ));
            }
            Err(_) => {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }
        };

        if !staged_writes.is_empty() && staged_content.is_some() {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Không cho phép trộn ghi KV và ghi nội dung trong cùng một action (V1).".into(),
            ));
        }

        if !staged_writes.is_empty() {
            if execution_mode != opendash_core::hooks::HookExecutionMode::Standard {
                guard.cleanup();
                return Err(PluginError::InvalidOutput(
                    "Ghi KV bị cấm trong save hook.".into(),
                ));
            }
            if cancellation_token.load(Ordering::Relaxed) || Instant::now() >= deadline {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let remaining_commit = deadline.saturating_duration_since(Instant::now());
            if remaining_commit < Duration::from_millis(50) {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let pool_clone = db_pool.clone();
            let mut conn = match tokio::time::timeout(remaining_commit, pool_clone.acquire()).await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    guard.cleanup();
                    return Err(PluginError::Internal(format!(
                        "Không lấy được kết nối database: {e}"
                    )));
                }
                Err(_) => {
                    guard.cleanup();
                    return Err(PluginError::Timeout);
                }
            };

            let initial_busy_timeout: i64 =
                plugin_kv::busy_timeout(&mut conn).await.unwrap_or(5000);

            let mut commit_guard = StagedCommitGuard {
                conn: Some(conn),
                _initial_busy_timeout: initial_busy_timeout,
                committed: false,
            };
            let conn = commit_guard.conn.as_mut().unwrap();

            let plugin_id = manifest.id;

            let cur_rem = deadline.saturating_duration_since(Instant::now());
            if cur_rem.is_zero() || cancellation_token.load(Ordering::Relaxed) {
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let begin_budget_ms = cur_rem.as_millis().max(1);
            if let Err(e) = plugin_kv::set_busy_timeout(conn, begin_budget_ms as u128).await {
                guard.cleanup();
                return Err(PluginError::Internal(format!("Lỗi set busy_timeout: {e}")));
            }

            if let Err(e) = plugin_kv::begin_immediate(conn).await {
                let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                guard.cleanup();
                let err_str = e.to_string();
                if err_str.contains("database is locked")
                    || err_str.contains("busy")
                    || Instant::now() >= deadline
                {
                    return Err(PluginError::Timeout);
                }
                return Err(PluginError::Internal(format!(
                    "Không mở được transaction: {e}"
                )));
            }

            if let Some(expected_gen) = expected_generation {
                match crate::storage::plugin::is_plugin_generation_active(
                    conn,
                    &plugin_id,
                    expected_gen,
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Disabled);
                    }
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi kiểm tra generation plugin: {e}"
                        )));
                    }
                }
            }

            if let Some(ref fence) = actor_fence {
                match crate::storage::plugin::is_actor_role_active(
                    conn,
                    fence.user_id,
                    &fence.required_role,
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(
                            "rbac_revoked: Quyền của tài khoản đã bị thu hồi.".into(),
                        ));
                    }
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi kiểm tra RBAC actor: {e}"
                        )));
                    }
                }
            }

            for (key, val) in staged_writes {
                let cur_rem = deadline.saturating_duration_since(Instant::now());
                if cur_rem.is_zero() || cancellation_token.load(Ordering::Relaxed) {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    return Err(PluginError::Timeout);
                }
                let insert_budget_ms = cur_rem.as_millis().max(1);
                let _ = plugin_kv::set_busy_timeout(conn, insert_budget_ms as u128).await;

                let val_str = match serde_json::to_string(&val) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi serialize staged KV: {e}"
                        )));
                    }
                };

                if let Err(e) = plugin_kv::set(conn, &plugin_id, &key, &val_str).await {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    let err_str = e.to_string();
                    if err_str.contains("database is locked")
                        || err_str.contains("busy")
                        || Instant::now() >= deadline
                    {
                        return Err(PluginError::Timeout);
                    }
                    return Err(PluginError::Internal(format!("Lỗi ghi database: {e}")));
                }
            }

            #[cfg(test)]
            {
                if TEST_STAGE_WRITE_PAUSE.load(Ordering::SeqCst) {
                    TEST_STAGE_WRITE_BARRIER_HIT.store(true, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                }
            }

            // --- COMMIT HANDOVER POINT ---
            // If deadline expired or cancelled before sending COMMIT, we must rollback and confirm cleanup.
            if Instant::now() >= deadline || cancellation_token.load(Ordering::Relaxed) {
                let _ = plugin_kv::rollback(conn).await;
                let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                guard.cleanup();
                return Err(PluginError::Timeout);
            }

            let cur_rem = deadline.saturating_duration_since(Instant::now());
            let commit_budget_ms = cur_rem.as_millis().max(1);
            let _ = plugin_kv::set_busy_timeout(conn, commit_budget_ms as u128).await;

            let commit_res = plugin_kv::commit(conn).await;
            // Always restore initial busy_timeout so it never leaks to other callers from the pool
            let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;

            match commit_res {
                Ok(_) => {
                    // Transaction successfully committed to disk!
                    // Invariant: Do NOT report Timeout for a committed transaction.
                    commit_guard.committed = true;
                }
                Err(e) => {
                    let _ = plugin_kv::rollback(conn).await;
                    guard.cleanup();
                    let err_str = e.to_string();
                    if err_str.contains("database is locked")
                        || err_str.contains("busy")
                        || Instant::now() >= deadline
                    {
                        return Err(PluginError::Timeout);
                    }
                    return Err(PluginError::Internal(format!(
                        "Lỗi commit transaction: {e}"
                    )));
                }
            }
        }

        guard.cleanup();
        Ok(ActionExecutionOutput {
            output: output_val,
            staged_content_command: staged_content,
        })
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn execute_wasm_sync(
    engine: &Engine,
    wasm_bytes: &[u8],
    manifest: PluginManifest,
    input: Value,
    db_pool: SqlitePool,
    correlation_id: String,
    allow_loopback: bool,
    cancellation_token: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<(Value, BTreeMap<String, Value>), PluginError> {
    if cancellation_token.load(Ordering::Relaxed) || Instant::now() >= deadline {
        return Err(PluginError::Timeout);
    }

    let input_bytes = serde_json::to_vec(&input)
        .map_err(|e| PluginError::Internal(format!("Lỗi serialize input: {e}")))?;

    if input_bytes.len() > 1024 * 1024 {
        return Err(PluginError::Internal(
            "Input vượt quá giới hạn 1 MB.".into(),
        ));
    }

    let limits = StoreLimitsBuilder::new()
        .memory_size(MAX_LINEAR_MEMORY_BYTES)
        .instances(1)
        .tables(10)
        .build();

    let host_ctx = HostContext {
        plugin_id: manifest.id.clone(),
        manifest,
        input_bytes,
        output_bytes: None,
        host_calls_count: 0,
        http_calls_count: 0,
        db_pool: Some(db_pool.clone()),
        kv_ipc: None,
        _correlation_id: correlation_id,
        allow_loopback,
        limits,
        staged_kv_writes: Default::default(),
        cancellation_token,
        deadline,
        execution_mode: opendash_core::hooks::HookExecutionMode::Standard,
        staged_content_command: None,
        action_capabilities: None,
    };

    let mut store = Store::new(engine, host_ctx);
    store.limiter(|ctx: &mut HostContext| &mut ctx.limits);
    store
        .set_fuel(INVOCATION_FUEL)
        .map_err(|e| PluginError::Internal(format!("Không thể cấp fuel: {e}")))?;

    if store.data().is_cancelled() {
        return Err(PluginError::Timeout);
    }

    let module = Module::new(engine, wasm_bytes)
        .map_err(|e| PluginError::ModuleCompilation(e.to_string()))?;

    let mut linker = <Linker<HostContext>>::new(engine);
    register_host_imports(&mut linker)
        .map_err(|e| PluginError::Internal(format!("Lỗi linker: {e}")))?;

    if store.data().is_cancelled() {
        return Err(PluginError::Timeout);
    }

    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|e| match e.as_trap_code() {
            Some(TrapCode::OutOfFuel) => PluginError::OutOfFuel,
            _ => {
                let msg = e.to_string();
                if msg.contains("deadline exceeded") || msg.contains("cancelled") {
                    PluginError::Timeout
                } else {
                    PluginError::Trap(msg)
                }
            }
        })?;

    if store.data().is_cancelled() {
        return Err(PluginError::Timeout);
    }

    let run_fn = instance
        .get_typed_func::<(), i32>(&store, "opendash_run")
        .map_err(|e| PluginError::ModuleCompilation(format!("Không tìm thấy opendash_run: {e}")))?;

    let result_code = match run_fn.call(&mut store, ()) {
        Ok(code) => code,
        Err(e) => {
            return match e.as_trap_code() {
                Some(TrapCode::OutOfFuel) => Err(PluginError::OutOfFuel),
                _ => {
                    let msg = e.to_string();
                    if msg.contains("deadline exceeded") || msg.contains("cancelled") {
                        Err(PluginError::Timeout)
                    } else {
                        Err(PluginError::Trap(msg))
                    }
                }
            };
        }
    };

    if store.data().is_cancelled() {
        return Err(PluginError::Timeout);
    }

    if result_code != 0 {
        return Err(PluginError::GuestFailed(result_code));
    }

    let ctx = store.into_data();
    if ctx.is_cancelled() {
        return Err(PluginError::Timeout);
    }

    let out_bytes = ctx
        .output_bytes
        .as_ref()
        .ok_or_else(|| PluginError::InvalidOutput("Plugin không gọi host_write_output.".into()))?;

    let output_val: Value = serde_json::from_slice(out_bytes)
        .map_err(|e| PluginError::InvalidOutput(format!("Output không phải JSON hợp lệ: {e}")))?;

    if ctx.is_cancelled() {
        return Err(PluginError::Timeout);
    }

    Ok((output_val, ctx.staged_kv_writes))
}

pub(crate) fn register_host_imports(linker: &mut Linker<HostContext>) -> Result<(), WasmiError> {
    // 1. host_get_input_len() -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_get_input_len",
        |mut caller: Caller<'_, HostContext>| -> Result<i32, wasmi::Error> {
            if caller.data().is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            Ok(caller.data().input_bytes.len() as i32)
        },
    )?;

    // 2. host_read_input(dest_ptr: i32, max_len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_read_input",
        |mut caller: Caller<'_, HostContext>,
         dest_ptr: i32,
         max_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if dest_ptr < 0 || max_len < 0 {
                return Ok(-1);
            }
            let dest = dest_ptr as usize;
            let max = max_len as usize;
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };

            let input = caller.data().input_bytes.clone();
            let to_copy = std::cmp::min(max, input.len());
            let Some(end) = dest.checked_add(to_copy) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }

            if memory.write(&mut caller, dest, &input[..to_copy]).is_err() {
                return Ok(-1);
            }
            Ok(to_copy as i32)
        },
    )?;

    // 3. host_write_output(src_ptr: i32, len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_write_output",
        |mut caller: Caller<'_, HostContext>,
         src_ptr: i32,
         len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if src_ptr < 0 || len < 0 {
                return Ok(-1);
            }
            let src = src_ptr as usize;
            let length = len as usize;
            if length > 1024 * 1024 {
                return Ok(-2); // Exceeds 1 MB
            }
            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let Some(end) = src.checked_add(length) else {
                return Ok(-1);
            };
            if end > memory.data_size(&caller) {
                return Ok(-1);
            }

            let mut buf = vec![0u8; length];
            if memory.read(&caller, src, &mut buf).is_err() {
                return Ok(-1);
            }

            let Ok(text) = std::str::from_utf8(&buf) else {
                return Ok(-3); // Invalid UTF-8
            };
            if serde_json::from_str::<Value>(text).is_err() {
                return Ok(-4); // Invalid JSON
            }

            caller.data_mut().output_bytes = Some(buf);
            Ok(0)
        },
    )?;

    // 4. host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32
    linker.func_wrap(
        "opendash_v1",
        "host_call",
        |mut caller: Caller<'_, HostContext>,
         op_ptr: i32,
         op_len: i32,
         resp_ptr: i32,
         max_resp_len: i32|
         -> Result<i32, wasmi::Error> {
            if caller.data().is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }
            if op_ptr < 0 || op_len < 0 || resp_ptr < 0 || max_resp_len < 0 {
                return Ok(-1);
            }
            let op_src = op_ptr as usize;
            let op_length = op_len as usize;
            let resp_dest = resp_ptr as usize;
            let resp_max = max_resp_len as usize;

            let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let Some(op_end) = op_src.checked_add(op_length) else {
                return Ok(-1);
            };
            if op_end > memory.data_size(&caller) {
                return Ok(-1);
            }

            let mut op_buf = vec![0u8; op_length];
            if memory.read(&caller, op_src, &mut op_buf).is_err() {
                return Ok(-1);
            }

            let Ok(op_str) = std::str::from_utf8(&op_buf) else {
                return Ok(-1);
            };

            let resp = host_ops::handle_host_call(caller.data_mut(), op_str);

            if caller.data().is_cancelled() {
                let _ = caller.as_context_mut().set_fuel(0);
                return Err(wasmi::Error::new(
                    "Invocation deadline exceeded or cancelled",
                ));
            }

            let resp_bytes = serde_json::to_vec(&resp)
                .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"Internal JSON error\"}".to_vec());

            if resp_bytes.len() > resp_max {
                return Ok(-2); // Buffer too small
            }

            let Some(resp_end) = resp_dest.checked_add(resp_bytes.len()) else {
                return Ok(-1);
            };
            if resp_end > memory.data_size(&caller) {
                return Ok(-1);
            }

            if memory.write(&mut caller, resp_dest, &resp_bytes).is_err() {
                return Ok(-1);
            }
            Ok(resp_bytes.len() as i32)
        },
    )?;

    linker.func_wrap(
        "env",
        "abort",
        |mut caller: Caller<'_, HostContext>,
         _msg: i32,
         _file: i32,
         _line: i32,
         _col: i32|
         -> Result<(), wasmi::Error> {
            let _ = caller.as_context_mut().set_fuel(0);
            Err(wasmi::Error::new("Guest aborted via env.abort"))
        },
    )?;

    Ok(())
}
