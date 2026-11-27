use super::*;

impl HookDispatcher {
    /// Dispatches dynamic render filters (`filter:render/head`, `filter:render/slot`,
    /// `filter:render/footer`) under `HookExecutionMode::FilterRender`, enforcing
    /// per-subscriber trust sanitization, `ExtensionRenderCache` (Public only),
    /// `1000ms`/call timeout, `1500ms` total phase budget, and fail-open without caching errors.
    pub async fn dispatch_render_hooks(
        &self,
        ctx: &RenderHookContext<'_>,
        slots: &[crate::template_sec::HookSlotInvocation],
        cache: &crate::cache::ExtensionRenderCache,
    ) -> RenderHooksOutput {
        let mut out = RenderHooksOutput::default();
        let clear_slots =
            |slots: &[crate::template_sec::HookSlotInvocation]| -> Vec<(String, String)> {
                slots
                    .iter()
                    .map(|s| (s.nonce.clone(), String::new()))
                    .collect()
            };

        if self.plugin_manager.is_safe_mode() {
            out.slot_replacements = clear_slots(slots);
            return out;
        }

        let head_subs = self
            .plugin_manager
            .owner_render_filter_subscriptions(HookEvent::FilterRenderHead)
            .await
            .unwrap_or_default();
        let footer_subs = self
            .plugin_manager
            .owner_render_filter_subscriptions(HookEvent::FilterRenderFooter)
            .await
            .unwrap_or_default();
        let slot_subs = if slots.is_empty() {
            Vec::new()
        } else {
            self.plugin_manager
                .owner_render_filter_subscriptions(HookEvent::FilterRenderSlot)
                .await
                .unwrap_or_default()
        };

        if head_subs.is_empty() && footer_subs.is_empty() && slot_subs.is_empty() {
            out.slot_replacements = clear_slots(slots);
            return out;
        }

        let started = Instant::now();
        let epoch = crate::cache::SiteCacheEpoch::global().current();

        // 1. filter:render/head
        for sub in &head_subs {
            if let Some(frag) = self
                .execute_render_subscriber(
                    HookEvent::FilterRenderHead,
                    None,
                    None,
                    sub,
                    ctx,
                    epoch,
                    started,
                    cache,
                )
                .await
            {
                out.head_styles.extend(frag.authorized_styles);
                out.head_scripts.extend(frag.authorized_head_scripts);
                out.head_html.push_str(&frag.html);
            }
        }

        // 2. filter:render/slot for each {{ od_hook(slot, args?) }} invocation
        for inv in slots {
            let mut combined = String::new();
            for sub in &slot_subs {
                if let Some(frag) = self
                    .execute_render_subscriber(
                        HookEvent::FilterRenderSlot,
                        Some(&inv.slot),
                        Some(&inv.args),
                        sub,
                        ctx,
                        epoch,
                        started,
                        cache,
                    )
                    .await
                {
                    out.head_styles.extend(frag.authorized_styles);
                    out.head_scripts.extend(frag.authorized_head_scripts);
                    combined.push_str(&frag.html);
                }
            }
            out.slot_replacements.push((inv.nonce.clone(), combined));
        }

        // 3. filter:render/footer
        for sub in &footer_subs {
            if let Some(frag) = self
                .execute_render_subscriber(
                    HookEvent::FilterRenderFooter,
                    None,
                    None,
                    sub,
                    ctx,
                    epoch,
                    started,
                    cache,
                )
                .await
            {
                out.head_styles.extend(frag.authorized_styles);
                out.head_scripts.extend(frag.authorized_head_scripts);
                out.footer_html.push_str(&frag.html);
            }
        }

        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_render_subscriber(
        &self,
        event: HookEvent,
        slot_name: Option<&str>,
        slot_args: Option<&Value>,
        sub: &crate::plugin::manager::RenderFilterSubscriber,
        ctx: &RenderHookContext<'_>,
        site_data_epoch: u64,
        phase_started: Instant,
        cache: &crate::cache::ExtensionRenderCache,
    ) -> Option<crate::cache::CachedRenderFragment> {
        let cache_key = if ctx.principal == ExecutionPrincipal::Public {
            crate::cache::canonical_route_key(
                sub.render_cache,
                ctx.route_kind,
                ctx.route_path,
                ctx.canonical_query,
                ctx.is_not_found,
            )
            .map(|route_seg| crate::cache::RenderCacheKey {
                owner_kind: sub.owner.kind,
                owner_id: sub.owner.id.clone(),
                revision_digest_or_generation: sub.revision_key.clone(),
                hook_or_slot_name: match slot_name {
                    Some(s) => format!("slot:{s}"),
                    None => event.as_str().to_owned(),
                },
                arg_hash: crate::cache::hash_render_args(slot_args),
                locale: ctx.locale.to_owned(),
                canonical_route_and_query_or_global: route_seg,
                origin_host: ctx.origin_host.to_owned(),
                site_data_epoch,
            })
        } else {
            None
        };

        if let Some(ref key) = cache_key
            && let Some(hit) = cache.get(key)
        {
            return Some(hit);
        }

        let remaining = RENDER_HOOK_PHASE_BUDGET.saturating_sub(phase_started.elapsed());
        if remaining.is_zero() {
            return None;
        }
        let call_timeout = remaining.min(RENDER_HOOK_CALL_TIMEOUT);

        let mut payload_map = serde_json::Map::new();
        if let Some(slot) = slot_name {
            payload_map.insert("slot".into(), Value::String(slot.to_owned()));
        }
        if let Some(args) = slot_args
            && !args.is_null()
        {
            payload_map.insert("args".into(), args.clone());
        }
        payload_map.insert(
            "route_kind".into(),
            Value::String(ctx.route_kind.to_owned()),
        );
        payload_map.insert(
            "route_path".into(),
            Value::String(ctx.route_path.to_owned()),
        );
        payload_map.insert("locale".into(), Value::String(ctx.locale.to_owned()));
        payload_map.insert("site".into(), ctx.site.clone());
        payload_map.insert("options".into(), ctx.options.clone());
        if let Some(ref entry) = ctx.entry {
            payload_map.insert("entry".into(), entry.clone());
        }
        if let Some(ref entries) = ctx.entries {
            payload_map.insert("entries".into(), entries.clone());
        }
        let payload = Value::Object(payload_map);
        let envelope = Self::envelope(event, payload.clone());
        let legacy_env = serde_json::json!({
            "event": event.as_str(),
            "occurred_at_ms": envelope.occurred_at_ms,
            "correlation_id": &envelope.correlation_id,
            "locale": ctx.locale,
            "payload": &payload,
        });

        let run_resp = self
            .plugin_manager
            .run_owner_dispatch(
                &sub.owner,
                EnvelopeKind::Filter,
                legacy_env,
                event.as_str(),
                envelope.occurred_at_ms,
                envelope.correlation_id,
                Some(ctx.locale.to_owned()),
                ctx.principal,
                0,
                payload,
                HookExecutionMode::FilterRender,
                Some(call_timeout),
            )
            .await
            .ok()?;

        let raw_val = run_resp.data.unwrap_or(Value::Null);
        let trusted = sub.render_trusted_html;
        let sanitized = parse_and_sanitize_render_output(event, &raw_val, trusted)?;

        if let Some(key) = cache_key {
            cache.insert(key, sanitized.clone());
        }
        Some(sanitized)
    }

    pub async fn resolve_extension_route(
        &self,
        pkg: &str,
        method: opendash_core::plugin::RouteMethod,
        subpath: &str,
    ) -> Result<
        Option<crate::plugin::manager::ResolvedExtensionRoute>,
        (axum::http::StatusCode, String),
    > {
        self.plugin_manager
            .resolve_extension_route(pkg, method, subpath)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_resolved_route(
        &self,
        resolved: crate::plugin::manager::ResolvedExtensionRoute,
        payload: Value,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout: Duration,
        actor: Option<&crate::auth::SessionContext>,
    ) -> Result<
        crate::plugin::manager::ExtensionRouteExecOutput,
        (
            axum::http::StatusCode,
            crate::plugin::manager::PluginRunResponse,
        ),
    > {
        self.plugin_manager
            .execute_resolved_route(
                resolved,
                payload,
                execution_mode,
                principal,
                timeout,
                actor,
                self,
            )
            .await
    }
}
