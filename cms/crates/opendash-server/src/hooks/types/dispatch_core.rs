use super::*;

impl HookDispatcher {
    pub fn new(plugin_manager: Arc<PluginManager>) -> Self {
        let mut dispatcher = Self {
            plugin_manager,
            handlers: Vec::new(),
            recent_events: Mutex::new(Vec::new()),
            async_queue: None,
        };
        dispatcher
            .register_handler("core/page_saved", HookEvent::PageAfterSave, 0, |event| {
                Box::pin(async move {
                    eprintln!("{} {}", event.event.as_str(), event.correlation_id);
                    Ok(())
                })
            })
            .expect("unique built-in hook");
        dispatcher
    }

    pub fn with_async_queue(
        plugin_manager: Arc<PluginManager>,
        maintenance_cancel_rx: tokio::sync::watch::Receiver<bool>,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        let mut dispatcher = Self::new(Arc::clone(&plugin_manager));
        dispatcher.async_queue = Some(AsyncHookQueue::spawn(
            plugin_manager,
            maintenance_cancel_rx,
            shutdown_rx,
        ));
        dispatcher
    }

    pub fn enqueue_async(&self, job: AsyncHookJob) -> bool {
        if let Some(ref queue) = self.async_queue {
            queue.enqueue(job)
        } else if job.dispatch_depth < 2 && tokio::runtime::Handle::try_current().is_ok() {
            let pm = Arc::clone(&self.plugin_manager);
            tokio::spawn(async move {
                run_async_hook_job(&pm, job).await;
            });
            true
        } else {
            false
        }
    }

    /// Trusted cooperative callbacks registered before AppState is shared.
    pub fn register_handler<F>(
        &mut self,
        id: &str,
        event: HookEvent,
        priority: i32,
        callback: F,
    ) -> Result<(), &'static str>
    where
        F: Fn(HookEnvelope) -> HookFuture + Send + Sync + 'static,
    {
        if event != HookEvent::PageAfterSave {
            return Err("Only page/after_save is implemented");
        }
        if !opendash_core::id::valid_namespaced_snake_key(id) || !id.starts_with("core/") {
            return Err("Core handler ID must be core/snake_key");
        }
        if self.handlers.iter().any(|h| h.id == id) {
            return Err("Duplicate handler ID");
        }
        if self.handlers.len() >= MAX_HANDLERS {
            return Err("Too many handlers");
        }
        self.handlers.push(Handler {
            id: id.into(),
            event,
            priority,
            callback: Arc::new(callback),
        });
        Ok(())
    }

    pub(crate) fn envelope(event: HookEvent, payload: Value) -> HookEnvelope {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        HookEnvelope::new(
            event,
            millis,
            format!("evt_{}", generate_random_token()),
            payload,
        )
    }

    /// Records content lifecycle events without invoking plugin handlers.
    pub fn emit(&self, event: HookEvent, payload: Value) {
        let envelope = Self::envelope(event, payload);
        if let Ok(mut log) = self.recent_events.lock() {
            if log.len() >= 64 {
                log.remove(0);
            }
            log.push(envelope);
        }
    }

    pub async fn dispatch(&self, event: HookEvent, payload: Value) -> DispatchReport {
        let envelope = Self::envelope(event, payload);
        if event != HookEvent::PageAfterSave || IN_DISPATCH.try_with(|v| *v).unwrap_or(false) {
            return DispatchReport {
                envelope,
                outcomes: vec![HookOutcome {
                    handler_id: "dispatcher".into(),
                    error: Some("Unsupported or nested lifecycle dispatch".into()),
                }],
            };
        }
        IN_DISPATCH
            .scope(true, self.dispatch_envelope(envelope))
            .await
    }

    pub(crate) async fn dispatch_envelope(&self, envelope: HookEnvelope) -> DispatchReport {
        let started = Instant::now();
        let mut outcomes = Vec::new();
        let mut ordered: Vec<(
            i32,
            String,
            Option<Callback>,
            Option<crate::plugin::host_ops::RuntimeOwner>,
        )> = self
            .handlers
            .iter()
            .filter(|h| h.event == envelope.event)
            .map(|h| (h.priority, h.id.clone(), Some(h.callback.clone()), None))
            .collect();
        match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager.owner_hook_subscriptions(envelope.event),
        )
        .await
        {
            Ok(Ok(subs)) => ordered.extend(subs.into_iter().map(|sub| {
                let prefix = sub.owner.kind.as_str();
                (
                    100.max(sub.priority),
                    format!("{prefix}/{}", sub.owner.id),
                    None,
                    Some(sub.owner),
                )
            })),
            failure => {
                let error = format!("Subscription discovery failed: {failure:?}");
                eprintln!("{} {error}", envelope.correlation_id);
                outcomes.push(HookOutcome {
                    handler_id: "dispatcher".into(),
                    error: Some(error),
                });
            }
        }
        ordered.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        for (index, (_, id, callback, owner_opt)) in ordered.into_iter().enumerate() {
            let result = if index >= MAX_HANDLERS || started.elapsed() >= DISPATCH_ADMISSION_BUDGET
            {
                Err("Hook dispatch admission budget exceeded".to_string())
            } else if let Some(callback) = callback {
                match tokio::time::timeout(
                    CORE_HANDLER_TIMEOUT,
                    invoke_callback(callback, envelope.clone()),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err("Core hook timed out".into()),
                }
            } else if let Some(ref owner) = owner_opt {
                // No outer timeout around commit: retain M2 commit handover semantics.
                let legacy_env = serde_json::to_value(&envelope).expect("JSON envelope");
                self.plugin_manager
                    .run_owner_dispatch(
                        owner,
                        EnvelopeKind::Hook,
                        legacy_env,
                        envelope.event.as_str(),
                        envelope.occurred_at_ms,
                        envelope.correlation_id.clone(),
                        envelope.locale.clone(),
                        ExecutionPrincipal::System,
                        0,
                        envelope.payload.clone(),
                        HookExecutionMode::Standard,
                        None,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|(_, response)| {
                        response
                            .error
                            .unwrap_or_else(|| "Extension hook failed".into())
                    })
            } else {
                Ok(())
            };
            if let Err(error) = &result {
                eprintln!("{} {id}: {error}", envelope.correlation_id);
                if let Some(ref owner) = owner_opt {
                    self.plugin_manager
                        .record_owner_hook_failure(owner, &envelope.correlation_id, error)
                        .await;
                }
            }
            outcomes.push(HookOutcome {
                handler_id: id,
                error: result.err(),
            });
        }
        DispatchReport { envelope, outcomes }
    }

    #[allow(dead_code)]
    pub async fn filter_subscriptions(
        &self,
        event: HookEvent,
        type_key: Option<&str>,
    ) -> Result<Vec<(i32, String)>, String> {
        self.plugin_manager
            .filter_subscriptions(event, type_key)
            .await
    }

    #[allow(dead_code)]
    pub async fn dispatch_content_before_save(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> Result<(), ContentBeforeSaveVeto> {
        self.dispatch_content_before_save_for_actor(type_key, actor_role, payload)
            .await
    }

    pub async fn dispatch_content_before_save_for_actor(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> Result<(), ContentBeforeSaveVeto> {
        if IN_DISPATCH.try_with(|v| *v).unwrap_or(false) {
            return Err(ContentBeforeSaveVeto {
                code: "nested_dispatch".into(),
                message: "Không cho phép gọi lồng nhau trong content/before_save.".into(),
                field_errors: Default::default(),
            });
        }
        let envelope = Self::envelope(HookEvent::ContentBeforeSave, payload);
        IN_DISPATCH
            .scope(true, async {
                let started = Instant::now();
                let subs = match tokio::time::timeout(
                    Duration::from_secs(1),
                    self.plugin_manager
                        .owner_content_hook_subscriptions(HookEvent::ContentBeforeSave, type_key),
                )
                .await
                {
                    Ok(Ok(s)) => s,
                    _ => {
                        return Err(ContentBeforeSaveVeto {
                            code: "hook_discovery_failed".into(),
                            message: "Không thể tra cứu gói mở rộng content/before_save.".into(),
                            field_errors: Default::default(),
                        });
                    }
                };

                for (index, sub) in subs.into_iter().enumerate() {
                    if index >= MAX_HANDLERS || started.elapsed() >= CONTENT_HOOK_PHASE_BUDGET {
                        return Err(ContentBeforeSaveVeto {
                            code: "hook_budget_exceeded".into(),
                            message: "Vượt quá giới hạn xử lý content/before_save.".into(),
                            field_errors: Default::default(),
                        });
                    }
                    let remaining = CONTENT_HOOK_PHASE_BUDGET.saturating_sub(started.elapsed());
                    let mut plugin_env = envelope.clone();
                    self.plugin_manager
                        .filter_content_payload_for_owner(
                            &sub.owner,
                            type_key,
                            actor_role,
                            &mut plugin_env.payload,
                        )
                        .await;
                    let env_val = serde_json::to_value(&plugin_env).expect("JSON envelope");
                    let out = match self
                        .plugin_manager
                        .run_owner_dispatch(
                            &sub.owner,
                            EnvelopeKind::Hook,
                            env_val,
                            plugin_env.event.as_str(),
                            plugin_env.occurred_at_ms,
                            envelope.correlation_id.clone(),
                            plugin_env.locale.clone(),
                            principal_for_field_role(actor_role),
                            0,
                            plugin_env.payload,
                            HookExecutionMode::ContentBeforeSave,
                            Some(remaining),
                        )
                        .await
                    {
                        Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                        Err((_status, resp)) => {
                            let err_msg = opendash_core::hooks::sanitize_plain_text(
                                resp.error.as_deref().unwrap_or("Extension hook failed"),
                                256,
                            );
                            self.plugin_manager
                                .record_owner_hook_failure(
                                    &sub.owner,
                                    &envelope.correlation_id,
                                    &err_msg,
                                )
                                .await;
                            return Err(ContentBeforeSaveVeto {
                                code: resp.code.unwrap_or_else(|| "hook_failed".into()),
                                message: err_msg,
                                field_errors: Default::default(),
                            });
                        }
                    };

                    let decision: opendash_core::hooks::ContentBeforeSaveDecision =
                        match serde_json::from_value(out) {
                            Ok(d) => d,
                            Err(_) => {
                                let msg = "Đầu ra content/before_save không hợp lệ.";
                                self.plugin_manager
                                    .record_owner_hook_failure(
                                        &sub.owner,
                                        &envelope.correlation_id,
                                        msg,
                                    )
                                    .await;
                                return Err(ContentBeforeSaveVeto {
                                    code: "invalid_hook_output".into(),
                                    message: msg.into(),
                                    field_errors: Default::default(),
                                });
                            }
                        };

                    match decision {
                        opendash_core::hooks::ContentBeforeSaveDecision::Allow => {}
                        opendash_core::hooks::ContentBeforeSaveDecision::Veto {
                            code,
                            message,
                            field_errors,
                        } => {
                            let clean_code = opendash_core::hooks::sanitize_plain_text(&code, 64);
                            let clean_msg =
                                opendash_core::hooks::sanitize_plain_text(&message, 256);
                            let clean_fields = field_errors
                                .into_iter()
                                .take(32)
                                .map(|(k, v)| {
                                    (
                                        opendash_core::hooks::sanitize_plain_text(&k, 64),
                                        opendash_core::hooks::sanitize_plain_text(&v, 256),
                                    )
                                })
                                .collect();
                            return Err(ContentBeforeSaveVeto {
                                code: if clean_code.is_empty() {
                                    "vetoed".into()
                                } else {
                                    clean_code
                                },
                                message: clean_msg,
                                field_errors: clean_fields,
                            });
                        }
                    }
                }
                Ok(())
            })
            .await
    }

    #[allow(dead_code)]
    pub async fn dispatch_content_after_save(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> DispatchReport {
        self.dispatch_content_after_save_for_actor(type_key, actor_role, payload)
            .await
    }

    pub async fn dispatch_content_after_save_for_actor(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> DispatchReport {
        let envelope = Self::envelope(HookEvent::ContentAfterSave, payload);
        if IN_DISPATCH.try_with(|v| *v).unwrap_or(false) {
            return DispatchReport {
                envelope,
                outcomes: vec![HookOutcome {
                    handler_id: "dispatcher".into(),
                    error: Some("Nested lifecycle dispatch".into()),
                }],
            };
        }
        IN_DISPATCH
            .scope(true, async {
                let started = Instant::now();
                let mut outcomes = Vec::new();
                let subs = match tokio::time::timeout(
                    Duration::from_secs(1),
                    self.plugin_manager
                        .owner_content_hook_subscriptions(HookEvent::ContentAfterSave, type_key),
                )
                .await
                {
                    Ok(Ok(s)) => s,
                    failure => {
                        let error = format!("Subscription discovery failed: {failure:?}");
                        outcomes.push(HookOutcome {
                            handler_id: "dispatcher".into(),
                            error: Some(error),
                        });
                        return DispatchReport { envelope, outcomes };
                    }
                };
                for (index, sub) in subs.into_iter().enumerate() {
                    let result = if index >= MAX_HANDLERS
                        || started.elapsed() >= CONTENT_HOOK_PHASE_BUDGET
                    {
                        Err("ContentAfterSave phase budget exceeded".to_string())
                    } else {
                        let remaining = CONTENT_HOOK_PHASE_BUDGET.saturating_sub(started.elapsed());
                        let mut plugin_env = envelope.clone();
                        self.plugin_manager
                            .filter_content_payload_for_owner(
                                &sub.owner,
                                type_key,
                                actor_role,
                                &mut plugin_env.payload,
                            )
                            .await;
                        let env_val = serde_json::to_value(&plugin_env).expect("JSON envelope");
                        self.plugin_manager
                            .run_owner_dispatch(
                                &sub.owner,
                                EnvelopeKind::Hook,
                                env_val,
                                plugin_env.event.as_str(),
                                plugin_env.occurred_at_ms,
                                envelope.correlation_id.clone(),
                                plugin_env.locale.clone(),
                                principal_for_field_role(actor_role),
                                0,
                                plugin_env.payload,
                                HookExecutionMode::ContentAfterSave,
                                Some(remaining),
                            )
                            .await
                            .map(|_| ())
                            .map_err(|(_, resp)| {
                                resp.error
                                    .unwrap_or_else(|| "ContentAfterSave hook failed".into())
                            })
                    };
                    if let Err(ref err) = result {
                        eprintln!(
                            "{} {}/{}: {err}",
                            envelope.correlation_id,
                            sub.owner.kind.as_str(),
                            sub.owner.id
                        );
                        self.plugin_manager
                            .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, err)
                            .await;
                    }
                    outcomes.push(HookOutcome {
                        handler_id: format!("{}/{}", sub.owner.kind.as_str(), sub.owner.id),
                        error: result.err(),
                    });
                }
                DispatchReport { envelope, outcomes }
            })
            .await
    }

    pub async fn dispatch_plugin_run(
        &self,
        plugin_id: &str,
        input: Value,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let envelope = Self::envelope(HookEvent::PluginManualRun, Value::Null);
        self.plugin_manager
            .run_plugin_dispatch(
                plugin_id,
                EnvelopeKind::Manual,
                input.clone(),
                HookEvent::PluginManualRun.as_str(),
                envelope.occurred_at_ms,
                envelope.correlation_id,
                None,
                ExecutionPrincipal::Admin,
                0,
                input,
                HookExecutionMode::Standard,
                None,
            )
            .await
    }

    pub async fn dispatch_content_prepare_save(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        effective_schema: &opendash_core::content::EffectiveContentSchema,
        payload: Value,
    ) -> crate::error::ApiResult<ContentPrepareSavePatch> {
        self.dispatch_content_prepare_save_with_scope(
            type_key,
            actor_role,
            effective_schema,
            payload,
            PrepareSaveMutationScope::Full,
            None,
        )
        .await
    }

    pub async fn dispatch_content_prepare_save_with_scope(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        effective_schema: &opendash_core::content::EffectiveContentSchema,
        mut payload: Value,
        scope: PrepareSaveMutationScope,
        db: Option<&sqlx::SqlitePool>,
    ) -> crate::error::ApiResult<ContentPrepareSavePatch> {
        use crate::error::ApiError;

        let subs = match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager
                .owner_filter_subscriptions(HookEvent::FilterContentPrepareSave, Some(type_key)),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => {
                return Err(ApiError::unprocessable(
                    "Không thể tra cứu plugin filter:content/prepare_save.",
                ));
            }
        };
        if subs.is_empty() {
            return Ok(ContentPrepareSavePatch::default());
        }

        if let PrepareSaveMutationScope::Metadata {
            requested_title,
            requested_slug,
            ref taxonomy_key,
            ..
        } = scope
            && let Some(obj) = payload.as_object_mut()
        {
            obj.insert(
                "mutation_scope".into(),
                serde_json::json!({
                    "mode": "metadata",
                    "requested_title": requested_title,
                    "requested_slug": requested_slug,
                    "taxonomy_key": taxonomy_key,
                }),
            );
        }

        let started = Instant::now();
        let envelope = Self::envelope(HookEvent::FilterContentPrepareSave, payload);
        let mut running_payload = envelope.payload.clone();
        let mut merged = ContentPrepareSavePatch::default();

        for (index, sub) in subs.into_iter().enumerate() {
            if index >= MAX_PREPARE_SAVE_PLUGINS || started.elapsed() >= PREPARE_SAVE_PHASE_BUDGET {
                return Err(ApiError::unprocessable(
                    "Vượt quá giới hạn xử lý filter:content/prepare_save.",
                ));
            }
            let remaining = PREPARE_SAVE_PHASE_BUDGET.saturating_sub(started.elapsed());
            let mut plugin_payload = running_payload.clone();
            self.plugin_manager
                .filter_content_payload_for_owner(
                    &sub.owner,
                    type_key,
                    actor_role,
                    &mut plugin_payload,
                )
                .await;
            let legacy_env = serde_json::json!({
                "event": HookEvent::FilterContentPrepareSave.as_str(),
                "occurred_at_ms": envelope.occurred_at_ms,
                "correlation_id": &envelope.correlation_id,
                "locale": &envelope.locale,
                "payload": &plugin_payload,
            });
            let resp = match self
                .plugin_manager
                .run_owner_dispatch(
                    &sub.owner,
                    EnvelopeKind::Filter,
                    legacy_env,
                    HookEvent::FilterContentPrepareSave.as_str(),
                    envelope.occurred_at_ms,
                    envelope.correlation_id.clone(),
                    envelope.locale.clone(),
                    principal_for_field_role(actor_role),
                    0,
                    plugin_payload,
                    HookExecutionMode::FilterContentPrepareSave,
                    Some(remaining),
                )
                .await
            {
                Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                Err((_status, resp)) => {
                    let msg = opendash_core::hooks::sanitize_plain_text(
                        resp.error
                            .as_deref()
                            .unwrap_or("filter:content/prepare_save thất bại."),
                        256,
                    );
                    if matches!(
                        resp.code.as_deref(),
                        Some("concurrency_limit" | "safe_mode")
                    ) {
                        return Err(ApiError::service_unavailable(msg));
                    }
                    self.plugin_manager
                        .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, &msg)
                        .await;
                    return Err(ApiError::unprocessable(msg));
                }
            };

            let patch: ContentPrepareSavePatch = if resp.is_null() {
                ContentPrepareSavePatch::default()
            } else {
                match serde_json::from_value(resp) {
                    Ok(p) => p,
                    Err(_) => {
                        let msg = "Đầu ra filter:content/prepare_save không hợp lệ.";
                        self.plugin_manager
                            .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, msg)
                            .await;
                        return Err(ApiError::unprocessable(msg));
                    }
                }
            };

            if patch.is_empty() {
                continue;
            }

            let Some(manifest) = self.plugin_manager.cached_owner_manifest(&sub.owner).await else {
                return Err(ApiError::unprocessable(
                    "Không tìm thấy manifest plugin filter:content/prepare_save.",
                ));
            };
            if !manifest.can_write_content_type(type_key) {
                return Err(ApiError::unprocessable(
                    "Plugin không có quyền ghi loại nội dung trong filter:content/prepare_save.",
                ));
            }
            for field_key in patch.values_patch.keys() {
                let Some(field_entry) = effective_schema.effective_fields.get(field_key) else {
                    return Err(ApiError::unprocessable(format!(
                        "Trường '{field_key}' trong values_patch không tồn tại ở '{type_key}'."
                    )));
                };
                if !opendash_core::plugin::plugin_can_write_field_for_owner(
                    sub.owner.kind,
                    &manifest,
                    None,
                    actor_role,
                    type_key,
                    field_entry,
                ) {
                    return Err(ApiError::unprocessable(format!(
                        "Plugin hoặc vai trò hiện tại không có quyền ghi trường '{field_key}' qua filter:content/prepare_save."
                    )));
                }
            }

            match &scope {
                PrepareSaveMutationScope::Full => {
                    if let Some(t) = patch.title {
                        merged.title = Some(t.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("title".into(), Value::String(t));
                        }
                    }
                    if let Some(s) = patch.slug {
                        merged.slug = Some(s.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("slug".into(), Value::String(s));
                        }
                    }
                    if let Some(term_ids) = patch.term_ids {
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("term_ids".into(), serde_json::json!(term_ids));
                        }
                        merged.term_ids = Some(term_ids);
                    }
                    for (k, v) in patch.values_patch {
                        if let Some(Value::Object(vals)) =
                            running_payload.get_mut("candidate_values")
                        {
                            if v.is_null() {
                                vals.remove(&k);
                            } else {
                                vals.insert(k.clone(), v.clone());
                            }
                        }
                        merged.values_patch.insert(k, v);
                    }
                }
                PrepareSaveMutationScope::Metadata {
                    requested_title,
                    requested_slug,
                    taxonomy_key,
                    entry_locale,
                } => {
                    let prior_title = running_payload
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let prior_slug = running_payload
                        .get("slug")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let prior_values = running_payload
                        .get("candidate_values")
                        .and_then(Value::as_object);
                    let prior_terms: std::collections::BTreeSet<i64> = running_payload
                        .get("term_ids")
                        .and_then(Value::as_array)
                        .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default();

                    for (k, v) in &patch.values_patch {
                        let prior_val = prior_values.and_then(|vals| vals.get(k));
                        let is_noop = if v.is_null() {
                            prior_val.is_none() || prior_val == Some(&Value::Null)
                        } else {
                            prior_val == Some(v)
                        };
                        if !is_noop {
                            return Err(ApiError::unprocessable(
                                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi trường nội dung ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                            ));
                        }
                    }

                    if let Some(ref t) = patch.title
                        && t != &prior_title
                    {
                        if !requested_title {
                            return Err(ApiError::unprocessable(
                                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi tiêu đề ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                            ));
                        }
                        merged.title = Some(t.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("title".into(), Value::String(t.clone()));
                        }
                    }

                    if let Some(ref s) = patch.slug
                        && s != &prior_slug
                    {
                        if !requested_slug {
                            return Err(ApiError::unprocessable(
                                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi slug ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                            ));
                        }
                        merged.slug = Some(s.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("slug".into(), Value::String(s.clone()));
                        }
                    }

                    if let Some(ref term_ids) = patch.term_ids {
                        let mut new_set = std::collections::BTreeSet::new();
                        for &tid in term_ids {
                            if tid <= 0 || !new_set.insert(tid) {
                                return Err(ApiError::unprocessable(
                                    "metadata_hook_scope_conflict: Plugin filter trả về danh sách term_ids không hợp lệ trong phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                ));
                            }
                        }
                        if new_set != prior_terms {
                            let Some(allowed_key) = taxonomy_key.as_deref() else {
                                return Err(ApiError::unprocessable(
                                    "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi phân loại ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                ));
                            };
                            let Some(pool) = db else {
                                return Err(ApiError::unprocessable(
                                    "metadata_hook_scope_conflict: Không thể xác minh phạm vi phân loại của plugin filter; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                ));
                            };
                            let changed: Vec<i64> = prior_terms
                                .symmetric_difference(&new_set)
                                .copied()
                                .collect();
                            let mut conn = pool.acquire().await?;
                            for tid in changed {
                                let info =
                                    crate::storage::entry_lifecycle::lookup_term_scope_in_conn(
                                        &mut conn, tid,
                                    )
                                    .await?;
                                let valid = info.is_some_and(|(kind, loc, active)| {
                                    active && kind == allowed_key && loc == *entry_locale
                                });
                                if !valid {
                                    return Err(ApiError::unprocessable(
                                        "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi phân loại ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                    ));
                                }
                            }
                            let canonical_ids: Vec<i64> = new_set.into_iter().collect();
                            if let Some(obj) = running_payload.as_object_mut() {
                                obj.insert("term_ids".into(), serde_json::json!(canonical_ids));
                            }
                            merged.term_ids = Some(canonical_ids);
                        }
                    }
                }
            }
        }

        Ok(merged)
    }
}
