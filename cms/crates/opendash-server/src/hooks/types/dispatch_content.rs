use super::*;

impl HookDispatcher {
    pub async fn dispatch_contact_before_submit(
        &self,
        _form_id: &str,
        submission_payload: Value,
    ) -> crate::error::ApiResult<ContactBeforeSubmitDecision> {
        use crate::error::ApiError;

        let subs = match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager
                .owner_filter_subscriptions(HookEvent::FilterContactBeforeSubmit, None),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => {
                return Err(ApiError::service_unavailable(
                    "Không thể tra cứu plugin kiểm tra biểu mẫu liên hệ.",
                ));
            }
        };
        if subs.is_empty() {
            return Ok(ContactBeforeSubmitDecision::Allow);
        }

        let started = Instant::now();
        let envelope = Self::envelope(HookEvent::FilterContactBeforeSubmit, submission_payload);

        for (index, sub) in subs.into_iter().enumerate() {
            if index >= MAX_CONTACT_FILTER_PLUGINS
                || started.elapsed() >= CONTACT_FILTER_PHASE_BUDGET
            {
                return Err(ApiError::unprocessable(
                    "Vượt quá giới hạn kiểm tra filter:contact/before_submit.",
                ));
            }
            let remaining = CONTACT_FILTER_PHASE_BUDGET.saturating_sub(started.elapsed());
            let legacy_env = serde_json::to_value(&envelope).unwrap_or(Value::Null);
            let out = match self
                .plugin_manager
                .run_owner_dispatch(
                    &sub.owner,
                    EnvelopeKind::Filter,
                    legacy_env,
                    HookEvent::FilterContactBeforeSubmit.as_str(),
                    envelope.occurred_at_ms,
                    envelope.correlation_id.clone(),
                    envelope.locale.clone(),
                    ExecutionPrincipal::Public,
                    0,
                    envelope.payload.clone(),
                    HookExecutionMode::FilterContactBeforeSubmit,
                    Some(remaining),
                )
                .await
            {
                Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                Err((_status, resp)) => {
                    let msg = opendash_core::hooks::sanitize_plain_text(
                        resp.error
                            .as_deref()
                            .unwrap_or("filter:contact/before_submit thất bại."),
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

            let decision: ContactBeforeSubmitRaw = if out.is_null() {
                ContactBeforeSubmitRaw::Allow
            } else {
                match serde_json::from_value(out) {
                    Ok(d) => d,
                    Err(_) => {
                        let msg = "Đầu ra filter:contact/before_submit không hợp lệ.";
                        self.plugin_manager
                            .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, msg)
                            .await;
                        return Err(ApiError::unprocessable(msg));
                    }
                }
            };

            match decision {
                ContactBeforeSubmitRaw::Allow => {}
                ContactBeforeSubmitRaw::Reject { code, message } => {
                    let clean_code = opendash_core::hooks::sanitize_plain_text(&code, 64);
                    let clean_msg = opendash_core::hooks::sanitize_plain_text(&message, 256);
                    return Ok(ContactBeforeSubmitDecision::Reject {
                        code: if clean_code.is_empty() {
                            "rejected".into()
                        } else {
                            clean_code
                        },
                        message: if clean_msg.is_empty() {
                            "Biểu mẫu liên hệ bị từ chối.".into()
                        } else {
                            clean_msg
                        },
                    });
                }
            }
        }

        Ok(ContactBeforeSubmitDecision::Allow)
    }

    #[allow(dead_code)]
    pub async fn dispatch_projection_filters(
        &self,
        event: HookEvent,
        plugin_id: Option<&str>,
        entry_payload: Value,
    ) -> crate::error::ApiResult<std::collections::BTreeMap<String, Value>> {
        let owner_filter =
            plugin_id.map(|id| (opendash_core::hooks::ExtensionOwnerKind::Plugin, id));
        let raw = self
            .dispatch_projection_filters_for_owners(event, owner_filter, entry_payload)
            .await?;
        let mut results = std::collections::BTreeMap::new();
        for (owner_kind, id, _fence, val) in raw {
            if owner_kind == opendash_core::hooks::ExtensionOwnerKind::Plugin {
                results.insert(id, val);
            }
        }
        Ok(results)
    }

    #[allow(clippy::type_complexity)]
    pub async fn dispatch_projection_filters_for_owners(
        &self,
        event: HookEvent,
        owner_filter: Option<(opendash_core::hooks::ExtensionOwnerKind, &str)>,
        entry_payload: Value,
    ) -> crate::error::ApiResult<
        Vec<(
            opendash_core::hooks::ExtensionOwnerKind,
            String,
            Option<(String, i64)>,
            Value,
        )>,
    > {
        let mut results = Vec::new();
        let type_key = entry_payload
            .get("content_type_key")
            .and_then(Value::as_str)
            .or_else(|| entry_payload.get("type_key").and_then(Value::as_str))
            .map(str::to_owned);

        let subs = match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager
                .owner_filter_subscriptions(event, type_key.as_deref()),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => return Ok(results),
        };
        if subs.is_empty() {
            return Ok(results);
        }

        let started = Instant::now();
        let envelope = Self::envelope(event, entry_payload);

        let mut dispatched = 0usize;
        for sub in subs {
            if let Some((target_kind, target_id)) = owner_filter
                && (sub.owner.kind != target_kind || sub.owner.id != target_id)
            {
                continue;
            }
            if dispatched >= MAX_PROJECTION_FILTER_PLUGINS
                || started.elapsed() >= PROJECTION_FILTER_PHASE_BUDGET
            {
                break;
            }
            dispatched += 1;
            let remaining = PROJECTION_FILTER_PHASE_BUDGET.saturating_sub(started.elapsed());
            let mut plugin_payload = envelope.payload.clone();
            if let Some(tk) = type_key.as_deref() {
                self.plugin_manager
                    .filter_public_content_payload_for_owner(&sub.owner, tk, &mut plugin_payload)
                    .await;
            }
            let legacy_env = serde_json::json!({
                "event": event.as_str(),
                "occurred_at_ms": envelope.occurred_at_ms,
                "correlation_id": &envelope.correlation_id,
                "locale": &envelope.locale,
                "payload": &plugin_payload,
            });
            match self
                .plugin_manager
                .run_owner_dispatch(
                    &sub.owner,
                    EnvelopeKind::Filter,
                    legacy_env,
                    event.as_str(),
                    envelope.occurred_at_ms,
                    envelope.correlation_id.clone(),
                    envelope.locale.clone(),
                    ExecutionPrincipal::Public,
                    0,
                    plugin_payload,
                    HookExecutionMode::FilterProjection,
                    Some(remaining),
                )
                .await
            {
                Ok(run_resp) => {
                    if let Some(val) = run_resp.data
                        && !val.is_null()
                    {
                        results.push((sub.owner.kind, sub.owner.id, sub.theme_fence, val));
                    }
                }
                Err((_status, resp)) => {
                    let msg = opendash_core::hooks::sanitize_plain_text(
                        resp.error.as_deref().unwrap_or("Projection filter failed"),
                        256,
                    );
                    self.plugin_manager
                        .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, &msg)
                        .await;
                }
            }
        }

        Ok(results)
    }
}
