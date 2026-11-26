use super::*;

impl PluginManager {
    pub async fn run_plugin(
        &self,
        id: &str,
        input: Value,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let token = crate::auth::generate_random_token();
        let correlation_id = format!("run_{}", &token[..16.min(token.len())]);

        self.run_plugin_correlated(id, input, correlation_id).await
    }
}

impl PluginManager {
    pub async fn run_plugin_correlated(
        &self,
        id: &str,
        input: Value,
        correlation_id: String,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        self.run_plugin_with_mode(
            id,
            input,
            correlation_id,
            opendash_core::hooks::HookExecutionMode::Standard,
            None,
        )
        .await
    }
}

impl PluginManager {
    pub async fn run_plugin_with_mode(
        &self,
        id: &str,
        input: Value,
        correlation_id: String,
        execution_mode: HookExecutionMode,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        self.run_plugin_with_principal(
            id,
            None,
            input,
            correlation_id,
            execution_mode,
            ExecutionPrincipal::Admin,
            timeout_override,
        )
        .await
    }
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub async fn run_plugin_dispatch(
        &self,
        id: &str,
        kind: EnvelopeKind,
        legacy_input: Value,
        event_str: &str,
        occurred_at_ms: u64,
        correlation_id: String,
        locale: Option<String>,
        principal: ExecutionPrincipal,
        dispatch_depth: u8,
        payload: Value,
        execution_mode: HookExecutionMode,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        self.run_owner_dispatch(
            &RuntimeOwner::plugin(id),
            kind,
            legacy_input,
            event_str,
            occurred_at_ms,
            correlation_id,
            locale,
            principal,
            dispatch_depth,
            payload,
            execution_mode,
            timeout_override,
        )
        .await
    }
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub async fn run_owner_dispatch(
        &self,
        owner: &RuntimeOwner,
        kind: EnvelopeKind,
        legacy_input: Value,
        event_str: &str,
        occurred_at_ms: u64,
        correlation_id: String,
        locale: Option<String>,
        principal: ExecutionPrincipal,
        dispatch_depth: u8,
        payload: Value,
        execution_mode: HookExecutionMode,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        if matches!(kind, EnvelopeKind::Cron) {
            return Err((
                StatusCode::BAD_REQUEST,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some("Envelope kind cron chưa mở ở V1.".into()),
                    code: Some("unsupported_envelope_kind".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }
        let v2_envelope = serde_json::to_value(PluginEnvelopeV2::new(
            kind,
            event_str,
            occurred_at_ms,
            correlation_id.clone(),
            locale,
            principal,
            dispatch_depth,
            payload,
        ))
        .unwrap_or(Value::Null);
        match owner.kind {
            ExtensionOwnerKind::Plugin => {
                self.run_plugin_with_principal(
                    &owner.id,
                    Some(v2_envelope),
                    legacy_input,
                    correlation_id,
                    execution_mode,
                    principal,
                    timeout_override,
                )
                .await
            }
            ExtensionOwnerKind::Theme => {
                self.run_theme_with_principal(
                    &owner.id,
                    v2_envelope,
                    correlation_id,
                    execution_mode,
                    principal,
                    timeout_override,
                )
                .await
            }
        }
    }
}
