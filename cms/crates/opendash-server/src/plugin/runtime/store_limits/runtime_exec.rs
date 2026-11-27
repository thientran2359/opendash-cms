use super::*;

impl PluginRuntime {
    pub fn new() -> Self {
        let secrets_root = crate::paths::SitePaths::from_env()
            .map(|p| p.secrets_dir())
            .unwrap_or_else(|_| std::env::temp_dir().join("opendash_secrets_default"));
        Self::with_secrets_root(secrets_root)
    }

    pub fn with_secrets_root(secrets_root: PathBuf) -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        Self {
            engine,
            admin_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_INVOCATIONS)),
            background_public_semaphore: Arc::new(Semaphore::new(
                MAX_BACKGROUND_PUBLIC_INVOCATIONS,
            )),
            public_render_semaphore: Arc::new(Semaphore::new(MAX_PUBLIC_RENDER_INVOCATIONS)),
            active_workers: Arc::new(AtomicUsize::new(0)),
            secrets_root,
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn active_workers(&self) -> usize {
        self.active_workers.load(Ordering::SeqCst)
    }

    pub fn semaphore(&self) -> &Arc<Semaphore> {
        &self.admin_semaphore
    }

    pub fn background_public_semaphore(&self) -> &Arc<Semaphore> {
        &self.background_public_semaphore
    }

    pub fn public_render_semaphore(&self) -> &Arc<Semaphore> {
        &self.public_render_semaphore
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

    #[allow(clippy::too_many_arguments)]
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
            HookExecutionMode::Standard,
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
        execution_mode: HookExecutionMode,
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
        execution_mode: HookExecutionMode,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        self.run_plugin_action_with_principal(
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            expected_generation,
            execution_mode,
            ExecutionPrincipal::Admin,
            timeout_override,
            action_capabilities,
            actor_fence,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_action_with_principal(
        &self,
        wasm_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        expected_generation: Option<i64>,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        let runtime_kind = manifest.runtime_kind();
        let owner_fence = expected_generation.map(|expected_generation| OwnerFence::Plugin {
            plugin_id: manifest.id.clone(),
            expected_generation,
        });
        self.run_owner_action_with_principal(
            ExtensionOwnerKind::Plugin,
            runtime_kind,
            wasm_bytes,
            manifest,
            input,
            db_pool,
            correlation_id,
            allow_loopback,
            owner_fence,
            execution_mode,
            principal,
            timeout_override,
            action_capabilities,
            actor_fence,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_owner_action_with_principal(
        &self,
        owner_kind: ExtensionOwnerKind,
        runtime_kind: ExtensionRuntimeKind,
        code_bytes: Vec<u8>,
        manifest: PluginManifest,
        input: Value,
        db_pool: SqlitePool,
        correlation_id: String,
        allow_loopback: bool,
        owner_fence: Option<OwnerFence>,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<Duration>,
        action_capabilities: Option<Vec<String>>,
        actor_fence: Option<ActionActorFence>,
    ) -> Result<ActionExecutionOutput, PluginError> {
        let cancellation_token = Arc::new(AtomicBool::new(false));

        // Respect global 10-worker ceiling: 4 admin + 2 background/public + 4 public_render
        let permit = if matches!(
            execution_mode,
            HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
        ) {
            match tokio::time::timeout(
                PUBLIC_RENDER_SEMAPHORE_WAIT,
                self.public_render_semaphore.clone().acquire_owned(),
            )
            .await
            {
                Ok(Ok(p)) => p,
                _ => return Err(PluginError::ConcurrencyLimit),
            }
        } else {
            let target_sem = match execution_mode {
                HookExecutionMode::AsyncPostCommit
                | HookExecutionMode::FilterContactBeforeSubmit => &self.background_public_semaphore,
                HookExecutionMode::RoutePost if principal == ExecutionPrincipal::Public => {
                    &self.background_public_semaphore
                }
                _ => &self.admin_semaphore,
            };
            match target_sem.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => return Err(PluginError::ConcurrencyLimit),
            }
        };

        let start = Instant::now();
        let effective_budget = timeout_override
            .unwrap_or(INVOCATION_DEADLINE)
            .min(INVOCATION_DEADLINE);
        let deadline = start + effective_budget;
        let remaining = effective_budget;

        let current_exe = std::env::current_exe().map_err(|e| {
            PluginError::Internal(format!("Không thể lấy đường dẫn executable hiện tại: {e}"))
        })?;

        let worker_args: &[&str] = if cfg!(test) {
            &[
                "plugin::tests::suite_01::tests::test_worker_entry",
                "--exact",
                "--nocapture",
            ]
        } else {
            &["--plugin-worker"]
        };

        let mut child = spawn_worker_process(WorkerSpawnConfig {
            exe: &current_exe,
            args: worker_args,
            env_vars: &[("OPENDASH_WORKER_CHILD", "1")],
            memory_limit_bytes: PLUGIN_WORKER_MEMORY_LIMIT_BYTES,
            macos_bootstrap_proof: PLUGIN_BOOTSTRAP_PROOF,
            inherit_stderr: true,
        })
        .map_err(|e| PluginError::Internal(format!("Không khởi động được worker process: {e}")))?;

        let mut child_stdin = child.take_stdin().ok_or_else(|| {
            PluginError::Internal("Không mở được stdin của worker process".into())
        })?;
        let child_stdout = child.take_stdout().ok_or_else(|| {
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
        let secrets_root_clone = self.secrets_root.clone();
        let child_ref_worker = Arc::clone(&child_ref);
        let action_caps_for_host = action_capabilities.clone();
        let correlation_for_log = correlation_id.clone();
        let thread_handle = std::thread::Builder::new()
            .name(format!("plugin-worker-{}", manifest.id))
            .spawn(move || {
                use std::io::{BufReader, Write};

                let cancel_watch = cancel_rx;
                let run = move || -> Result<super::host_ops::WorkerExecutionResult, PluginError> {
                    let code_len = code_bytes.len() as u32;
                    child_stdin
                        .write_all(&code_len.to_le_bytes())
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi code len: {e}")))?;
                    child_stdin
                        .write_all(&code_bytes)
                        .map_err(|e| PluginError::Internal(format!("Lỗi ghi code bytes: {e}")))?;

                    let req = crate::plugin::worker::WorkerInvocationRequest {
                        default_locale: input
                            .get("locale")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned),
                        manifest: manifest_clone.clone(),
                        input,
                        allow_loopback,
                        correlation_id,
                        deadline_ms: remaining.as_millis() as u64,
                        execution_mode,
                        principal,
                        action_capabilities: action_caps_for_host.clone(),
                        runtime_kind,
                        owner_kind,
                    };
                    write_ipc_frame(&mut child_stdin, &req, MAX_IPC_FRAME_BYTES)
                        .map_err(PluginError::Internal)?;
                    let mut reader = BufReader::new(child_stdout);
                    scan_ipc_magic(&mut reader)?;
                    loop {
                        let frame: WorkerToHostFrame =
                            read_ipc_frame(&mut reader, MAX_IPC_FRAME_BYTES).map_err(|_| {
                                PluginError::Trap("Worker process kết thúc bất ngờ".into())
                            })?;
                        match frame {
                            WorkerToHostFrame::ReadRequest(read_req) => {
                                if worker_cancel.load(Ordering::Relaxed)
                                    || Instant::now() >= deadline
                                    || *cancel_watch.borrow()
                                {
                                    return Err(PluginError::Timeout);
                                }
                                let rem = deadline.saturating_duration_since(Instant::now());
                                if rem.is_zero() {
                                    return Err(PluginError::Timeout);
                                }

                                #[cfg(test)]
                                if matches!(read_req, HostReadRequest::KvGet { .. }) {
                                    TEST_IPC_KV_GET_WAITING.store(true, Ordering::SeqCst);
                                }

                                let db = db_pool_clone.clone();
                                let m = manifest_clone.clone();
                                let caps = action_caps_for_host.clone();
                                let sec_root = secrets_root_clone.clone();

                                let read_fut = async move {
                                    execute_host_read_request_for_owner(
                                        &db,
                                        owner_kind,
                                        &m.id,
                                        &m,
                                        execution_mode,
                                        principal,
                                        caps.as_deref(),
                                        Some(&sec_root),
                                        read_req,
                                    )
                                    .await
                                };

                                let mut watch_rx = cancel_watch.clone();
                                let resp = tokio_handle.block_on(async {
                                    tokio::select! {
                                        biased;
                                        _ = watch_rx.changed() => Err(PluginError::Timeout),
                                        res = read_fut => Ok(res),
                                        _ = tokio::time::sleep(rem) => Err(PluginError::Timeout),
                                    }
                                })?;

                                write_ipc_frame(
                                    &mut child_stdin,
                                    &HostToWorkerFrame::ReadResponse(resp),
                                    MAX_IPC_FRAME_BYTES,
                                )
                                .map_err(PluginError::Internal)?;
                            }
                            WorkerToHostFrame::Result(res) => {
                                if let Ok(mut lock) = child_ref_worker.lock()
                                    && let Some(ref mut c) = *lock
                                {
                                    let _ = c.wait();
                                }
                                if res.ok {
                                    return Ok(res);
                                }
                                let error_type = res.error_type.as_deref().unwrap_or("internal");
                                let err_msg =
                                    res.error.unwrap_or_else(|| "Lỗi thực thi plugin".into());
                                return Err(match error_type {
                                    "out_of_fuel" => PluginError::OutOfFuel,
                                    "timeout" => PluginError::Timeout,
                                    "memory_limit" => PluginError::Trap(err_msg),
                                    "invalid_output" => PluginError::InvalidOutput(err_msg),
                                    "guest_failed" => PluginError::GuestFailed(res.code),
                                    _ => PluginError::Trap(err_msg),
                                });
                            }
                        }
                    }
                };

                let res = run();
                let _ = tx.send(res);
            })
            .map_err(|e| PluginError::Internal(format!("Không thể tạo worker thread: {e}")))?;

        if let Ok(mut lock) = worker_thread_ref.lock() {
            *lock = Some(thread_handle);
        }

        let worker_res = match tokio::time::timeout(remaining, rx).await {
            Ok(Ok(res)) => match res {
                Ok(exec_res) => {
                    if let Ok(mut lock) = guard.child.lock() {
                        let _ = lock.take();
                    }
                    exec_res
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

        let output_val = worker_res.output.unwrap_or(Value::Null);
        let staged_kv = worker_res.staged_kv;
        let staged_logs = worker_res.staged_logs;
        let staged_content = worker_res.staged_content_command;
        let staged_lifecycle = worker_res.staged_lifecycle_command;

        let scope_count = (!staged_kv.is_empty() as u8)
            + (staged_content.is_some() as u8)
            + (staged_lifecycle.is_some() as u8);
        if scope_count > 1 {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "ERR_MIXED_WRITE_SCOPE: Không cho phép trộn ghi/xóa KV và ghi/xóa nội dung.".into(),
            ));
        }

        let content_write_allowed = execution_mode == HookExecutionMode::Standard
            || (execution_mode == HookExecutionMode::RoutePost
                && principal != ExecutionPrincipal::Public);
        if (staged_content.is_some() || staged_lifecycle.is_some()) && !content_write_allowed {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Ghi/xóa nội dung bị cấm ngoài action mode.".into(),
            ));
        }
        if let Some(ref cmd) = staged_lifecycle
            && matches!(
                cmd.operation,
                host_ops::StagedLifecycleOperation::Restore
                    | host_ops::StagedLifecycleOperation::Purge
            )
            && action_capabilities.is_none()
        {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "content_restore và content_purge chỉ được phép qua action khai báo.".into(),
            ));
        }

        if !staged_kv.is_empty()
            && !matches!(
                execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::RoutePost
            )
        {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Ghi/xóa KV bị cấm trong hook/filter đồng bộ.".into(),
            ));
        }

        if !staged_logs.is_empty() && execution_mode == HookExecutionMode::FilterContactBeforeSubmit
        {
            guard.cleanup();
            return Err(PluginError::InvalidOutput(
                "Ghi log bị cấm trong filter:contact/before_submit.".into(),
            ));
        }

        if !matches!(
            execution_mode,
            HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
        ) && (!staged_kv.is_empty() || !staged_logs.is_empty())
        {
            let had_kv_writes = !staged_kv.is_empty();
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

            if let Some(ref fence) = owner_fence {
                let fence_check = match fence {
                    OwnerFence::Plugin {
                        plugin_id: fence_pid,
                        expected_generation,
                    } => {
                        plugin_storage::is_plugin_generation_active(
                            conn,
                            fence_pid,
                            *expected_generation,
                        )
                        .await
                    }
                    OwnerFence::Theme {
                        revision_id,
                        selection_version,
                        ..
                    } => {
                        plugin_storage::verify_active_theme_fence_in_tx(
                            conn,
                            revision_id,
                            *selection_version,
                        )
                        .await
                    }
                };
                match fence_check {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Disabled);
                    }
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi kiểm tra fence gói mở rộng: {e}"
                        )));
                    }
                }
            }

            if let Some(ref fence) = actor_fence {
                match plugin_storage::is_actor_role_active(
                    conn,
                    fence.user_id,
                    &fence.required_role,
                )
                .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(
                            "rbac_revoked: Quyền của tài khoản đã bị thu hồi.".into(),
                        ));
                    }
                    Err(e) => {
                        let _ = plugin_kv::rollback(conn).await;
                        let _ =
                            plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                        guard.cleanup();
                        return Err(PluginError::Internal(format!(
                            "Lỗi kiểm tra RBAC actor: {e}"
                        )));
                    }
                }
            }

            for (key, opt_val) in staged_kv {
                let cur_rem = deadline.saturating_duration_since(Instant::now());
                if cur_rem.is_zero() || cancellation_token.load(Ordering::Relaxed) {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    return Err(PluginError::Timeout);
                }
                let op_budget_ms = cur_rem.as_millis().max(1);
                let _ = plugin_kv::set_busy_timeout(conn, op_budget_ms as u128).await;

                let write_res = match opt_val {
                    Some(val) => {
                        let val_str = match serde_json::to_string(&val) {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = plugin_kv::rollback(conn).await;
                                let _ =
                                    plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128)
                                        .await;
                                guard.cleanup();
                                return Err(PluginError::Internal(format!(
                                    "Lỗi serialize staged KV: {e}"
                                )));
                            }
                        };
                        plugin_kv::set_for_owner(conn, owner_kind, &plugin_id, &key, &val_str).await
                    }
                    None => plugin_kv::delete_for_owner(conn, owner_kind, &plugin_id, &key).await,
                };

                if let Err(e) = write_res {
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

            for entry in staged_logs.into_iter().take(MAX_STAGED_LOGS) {
                let level = match entry.level.as_str() {
                    "warn" => "warn",
                    "error" => "error",
                    _ => "info",
                };
                let msg = opendash_core::hooks::sanitize_plain_text(
                    &entry.message,
                    MAX_LOG_MESSAGE_CHARS,
                );
                if let Err(e) = plugin_storage::insert_owner_log_in_conn(
                    conn,
                    owner_kind,
                    &plugin_id,
                    &correlation_for_log,
                    level,
                    &msg,
                )
                .await
                {
                    let _ = plugin_kv::rollback(conn).await;
                    let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;
                    guard.cleanup();
                    return Err(PluginError::Internal(format!("Lỗi ghi plugin_logs: {e}")));
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
            let _ = plugin_kv::set_busy_timeout(conn, initial_busy_timeout as u128).await;

            match commit_res {
                Ok(_) => {
                    commit_guard.committed = true;
                    if had_kv_writes {
                        crate::cache::SiteCacheEpoch::global().bump();
                    }
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
            staged_lifecycle_command: staged_lifecycle,
        })
    }
}
