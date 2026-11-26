use super::*;

pub(super) fn principal_for_field_role(
    role: opendash_core::content::FieldRole,
) -> ExecutionPrincipal {
    match role {
        opendash_core::content::FieldRole::Admin => ExecutionPrincipal::Admin,
        opendash_core::content::FieldRole::Editor => ExecutionPrincipal::Editor,
    }
}

impl ContentPrepareSavePatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.slug.is_none()
            && self.values_patch.is_empty()
            && self.term_ids.is_none()
    }
}

pub(super) fn field_role_for_principal(
    principal: ExecutionPrincipal,
) -> opendash_core::content::FieldRole {
    match principal {
        ExecutionPrincipal::Admin | ExecutionPrincipal::System => {
            opendash_core::content::FieldRole::Admin
        }
        ExecutionPrincipal::Editor | ExecutionPrincipal::Public => {
            opendash_core::content::FieldRole::Editor
        }
    }
}

pub(super) async fn run_async_hook_job(plugin_manager: &PluginManager, job: AsyncHookJob) {
    let AsyncHookJob {
        event,
        principal,
        locale,
        type_key,
        payload,
        dispatch_depth,
        lease,
    } = job;
    let _lease = lease;
    if dispatch_depth >= 2 {
        return;
    }

    let subs = match type_key.as_deref() {
        Some(tk)
            if matches!(
                event,
                HookEvent::ContentAfterSave
                    | HookEvent::ContentAfterPublish
                    | HookEvent::ContentAfterUnpublish
                    | HookEvent::ContentAfterDelete
            ) =>
        {
            match tokio::time::timeout(
                Duration::from_secs(1),
                plugin_manager.owner_content_hook_subscriptions(event, tk),
            )
            .await
            {
                Ok(Ok(items)) => items,
                _ => return,
            }
        }
        _ => match tokio::time::timeout(
            Duration::from_secs(1),
            plugin_manager.owner_hook_subscriptions(event),
        )
        .await
        {
            Ok(Ok(items)) => items,
            _ => return,
        },
    };

    if subs.is_empty() {
        return;
    }

    let actor_role = field_role_for_principal(principal);
    let mut base_env = HookEnvelope::new(
        event,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        format!("evt_{}", generate_random_token()),
        payload,
    );
    if locale.is_some() {
        base_env.locale = locale.clone();
    }

    let started = Instant::now();
    for (idx, sub) in subs.into_iter().enumerate() {
        if idx >= MAX_HANDLERS || started.elapsed() >= DISPATCH_ADMISSION_BUDGET {
            break;
        }
        let mut plugin_env = base_env.clone();
        if let Some(tk) = type_key.as_deref() {
            plugin_manager
                .filter_content_payload_for_owner(
                    &sub.owner,
                    tk,
                    actor_role,
                    &mut plugin_env.payload,
                )
                .await;
        }
        let legacy_env = serde_json::to_value(&plugin_env).unwrap_or(Value::Null);
        let remaining = DISPATCH_ADMISSION_BUDGET.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        if let Err((_status, resp)) = plugin_manager
            .run_owner_dispatch(
                &sub.owner,
                EnvelopeKind::Hook,
                legacy_env,
                event.as_str(),
                plugin_env.occurred_at_ms,
                base_env.correlation_id.clone(),
                plugin_env.locale.clone(),
                principal,
                dispatch_depth.saturating_add(1),
                plugin_env.payload,
                HookExecutionMode::AsyncPostCommit,
                Some(remaining),
            )
            .await
        {
            let err_msg = opendash_core::hooks::sanitize_plain_text(
                resp.error
                    .as_deref()
                    .unwrap_or("Async extension hook failed"),
                256,
            );
            plugin_manager
                .record_owner_hook_failure(&sub.owner, &base_env.correlation_id, &err_msg)
                .await;
        }
    }
}
