use super::*;

/// Shared HostPolicy enforcement for both Worker pre-checks and Host IPC handlers.
pub fn check_read_policy(
    manifest: &PluginManifest,
    execution_mode: HookExecutionMode,
    principal: ExecutionPrincipal,
    action_caps: Option<&[String]>,
    req: &HostReadRequest,
) -> Result<(), &'static str> {
    if execution_mode == HookExecutionMode::FilterContactBeforeSubmit
        && !matches!(
            req,
            HostReadRequest::PluginSecretGet { .. }
                | HostReadRequest::L10nTranslate { .. }
                | HostReadRequest::L10nTranslateBatch { .. }
        )
    {
        return Err("Thao tác đọc dữ liệu bị cấm trong filter:contact/before_submit.");
    }
    match req {
        HostReadRequest::KvGet { .. } => {
            if !manifest.has_sqlite_perm("kv_read") {
                return Err("permission_denied: sqlite:kv_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "kv_read")
            {
                return Err("permission_denied: action:kv_read");
            }
        }
        HostReadRequest::KvList { .. } => {
            if !matches!(
                execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::FilterRender
                    | HookExecutionMode::RouteGet
                    | HookExecutionMode::RoutePost
            ) {
                return Err("kv_list chỉ được phép trong action hoặc async post-commit hook.");
            }
            if !manifest.has_sqlite_perm("kv_read") {
                return Err("permission_denied: sqlite:kv_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "kv_list")
            {
                return Err("permission_denied: action:kv_list");
            }
        }
        HostReadRequest::ContentGet { type_key, .. }
        | HostReadRequest::ContentList { type_key, .. } => {
            if !manifest.can_read_content_type(type_key) {
                return Err("permission_denied: content_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "content_read")
            {
                return Err("permission_denied: action:content_read");
            }
        }
        HostReadRequest::ContentTrashGet { type_key, .. }
        | HostReadRequest::ContentTrashList { type_key, .. } => {
            if execution_mode != HookExecutionMode::Standard {
                return Err("content_trash_read chỉ được phép trong action mode.");
            }
            if !matches!(
                principal,
                ExecutionPrincipal::Admin | ExecutionPrincipal::Editor
            ) {
                return Err("permission_denied: authenticated_actor_required");
            }
            if !action_caps.is_some_and(|caps| caps.iter().any(|c| c == "content_trash_read")) {
                return Err("permission_denied: action:content_trash_read");
            }
            if !manifest.can_read_trash_content_type(type_key) {
                return Err("permission_denied: content_trash_read");
            }
        }
        HostReadRequest::TaxonomyList { .. } => {
            if !manifest.permissions.taxonomy_read {
                return Err("permission_denied: taxonomy_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "taxonomy_read")
            {
                return Err("permission_denied: action:taxonomy_read");
            }
        }
        HostReadRequest::MediaGet { .. } => {
            if !manifest.permissions.media_read {
                return Err("permission_denied: media_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "media_read")
            {
                return Err("permission_denied: action:media_read");
            }
        }
        HostReadRequest::SiteOptionsGet { .. } => {
            if !manifest.permissions.site_options_read {
                return Err("permission_denied: site_options_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "site_options_read")
            {
                return Err("permission_denied: action:site_options_read");
            }
        }
        HostReadRequest::PluginSettingsGet => {
            if !manifest.permissions.plugin_settings_read {
                return Err("permission_denied: plugin_settings_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "plugin_settings_read")
            {
                return Err("permission_denied: action:plugin_settings_read");
            }
        }
        HostReadRequest::PluginSecretGet { key } => {
            if !matches!(
                execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::FilterContactBeforeSubmit
                    | HookExecutionMode::RoutePost
            ) {
                return Err("plugin_secret_get bị cấm trong hook/filter hiện tại.");
            }
            if !manifest
                .permissions
                .plugin_secrets_read
                .iter()
                .any(|k| k == key)
            {
                return Err("permission_denied: plugin_secrets_read");
            }
            if let Some(caps) = action_caps
                && !caps.iter().any(|c| c == "plugin_secrets_read")
            {
                return Err("permission_denied: action:plugin_secrets_read");
            }
        }
        HostReadRequest::L10nTranslate {
            text_domain, msgid, ..
        } => {
            check_l10n_item_policy(manifest, text_domain.as_deref(), msgid)?;
        }
        HostReadRequest::L10nTranslateBatch { items } => {
            if items.is_empty() || items.len() > MAX_L10N_CALLS {
                return Err("Số lượng mục l10n_translate_batch không hợp lệ.");
            }
            for item in items {
                check_l10n_item_policy(manifest, item.text_domain.as_deref(), &item.msgid)?;
            }
        }
    }
    Ok(())
}

fn check_l10n_item_policy(
    manifest: &PluginManifest,
    text_domain: Option<&str>,
    msgid: &str,
) -> Result<(), &'static str> {
    if msgid.len() > 2048 {
        return Err("msgid vượt quá giới hạn 2 KiB.");
    }
    if let Some(dom) = text_domain {
        let own_domain = manifest.text_domain.as_deref().unwrap_or(&manifest.id);
        if dom != own_domain && dom != "opendash" {
            return Err("permission_denied: l10n_foreign_domain");
        }
    }
    Ok(())
}

pub fn handle_host_call(ctx: &mut HostContext, req_json: &str) -> HostOperationResponse {
    ctx.last_resp_bytes = None;
    if ctx.is_cancelled() {
        return HostOperationResponse::err("Quá thời gian thực thi (deadline exceeded).");
    }

    let req: HostOperationRequest = match serde_json::from_str(req_json) {
        Ok(r) => r,
        Err(_) => return HostOperationResponse::err("Thao tác host không đúng định dạng JSON."),
    };

    if matches!(
        req,
        HostOperationRequest::L10nTranslate { .. }
            | HostOperationRequest::L10nTranslateBatch { .. }
    ) {
        let charge = match &req {
            HostOperationRequest::L10nTranslateBatch { items } => items.len().max(1),
            _ => 1,
        };
        if ctx.l10n_calls_count.saturating_add(charge) > MAX_L10N_CALLS {
            return HostOperationResponse::err(format!(
                "Vượt quá số lần gọi l10n tối đa ({MAX_L10N_CALLS})."
            ));
        }
        ctx.l10n_calls_count += charge;
    } else {
        ctx.host_calls_count += 1;
        let max_calls = if matches!(
            ctx.execution_mode,
            HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
        ) {
            MAX_RENDER_HOST_CALLS
        } else {
            MAX_HOST_CALLS
        };
        if ctx.host_calls_count > max_calls {
            return HostOperationResponse::err(format!(
                "Vượt quá số lần gọi host tối đa ({max_calls})."
            ));
        }
    }

    match req {
        HostOperationRequest::KvGet { key } => handle_kv_get(ctx, &key),
        HostOperationRequest::KvSet { key, value } => handle_kv_set(ctx, &key, value),
        HostOperationRequest::KvDelete { key } => handle_kv_delete(ctx, &key),
        HostOperationRequest::KvList { prefix, limit } => handle_kv_list(ctx, prefix, limit),
        HostOperationRequest::HttpRequest {
            method,
            url,
            headers,
            body,
            timeout_ms,
        } => {
            if ctx.execution_mode == HookExecutionMode::FilterRender {
                if ctx.manifest.render_cache == opendash_core::plugin::RenderCachePolicy::None {
                    return HostOperationResponse::err(
                        "Gọi HTTP ngoài trong filter:render/* yêu cầu bật render_cache khác none.",
                    );
                }
            } else if !matches!(
                ctx.execution_mode,
                HookExecutionMode::Standard
                    | HookExecutionMode::AsyncPostCommit
                    | HookExecutionMode::FilterContactBeforeSubmit
                    | HookExecutionMode::RouteGet
                    | HookExecutionMode::RoutePost
            ) {
                return HostOperationResponse::err(
                    "Gọi HTTP ngoài bị cấm trong chế độ thực thi này.",
                );
            }
            if let Some(ref caps) = ctx.action_capabilities {
                if caps
                    .iter()
                    .any(|c| c == "content_restore" || c == "content_purge")
                {
                    return HostOperationResponse::err(
                        "permission_denied: http_forbidden_with_lifecycle_write",
                    );
                }
                if !caps.iter().any(|c| c == "http") {
                    return HostOperationResponse::err("permission_denied: action:http");
                }
            }
            if let Ok(parsed) = url::Url::parse(&url)
                && crate::plugin::egress::is_forbidden_internal_path(parsed.path())
            {
                return HostOperationResponse::err(
                    "Cấm gọi HTTP loopback tới Admin/CMS nội bộ (/api/admin, /save, /preview, /theme, /p/, /api/p/, /api/public/p/).",
                );
            }
            handle_http_request(ctx, &method, &url, headers, body, timeout_ms)
        }
        HostOperationRequest::ContentSave {
            type_key,
            entry_id,
            expected_version,
            locale,
            slug,
            title,
            status: _,
            values,
            term_ids,
        } => {
            if !matches!(
                ctx.execution_mode,
                HookExecutionMode::Standard | HookExecutionMode::RoutePost
            ) {
                return HostOperationResponse::err("Ghi nội dung bị cấm ngoài action mode.");
            }
            if ctx.principal == ExecutionPrincipal::Public {
                return HostOperationResponse::err(
                    "permission_denied: content_save bị cấm ở public route.",
                );
            }
            if let Some(ref caps) = ctx.action_capabilities
                && !caps.iter().any(|c| c == "content_write")
            {
                return HostOperationResponse::err("permission_denied: action:content_write");
            }
            if ctx.has_any_kv_stage() || ctx.staged_lifecycle_command.is_some() {
                return HostOperationResponse::err(
                    "ERR_MIXED_WRITE_SCOPE: Không cho phép trộn ghi nội dung với KV hoặc xóa nội dung trong cùng action.",
                );
            }
            if ctx.staged_content_command.is_some() {
                return HostOperationResponse::err(
                    "Mỗi action chỉ được thực thi tối đa một lệnh ghi nội dung.",
                );
            }
            if !ctx.manifest.can_write_content_type(&type_key) {
                return HostOperationResponse::err("permission_denied: content_write");
            }
            ctx.staged_content_command = Some(StagedContentCommand {
                type_key,
                entry_id,
                expected_version,
                locale,
                slug: slug.unwrap_or_default(),
                title: title.unwrap_or_default(),
                values,
                term_ids,
            });
            HostOperationResponse::ok_empty()
        }
        HostOperationRequest::ContentDelete {
            type_key,
            entry_id,
            expected_version,
        } => stage_lifecycle_command(
            ctx,
            StagedLifecycleOperation::Trash,
            type_key,
            entry_id,
            expected_version,
        ),
        HostOperationRequest::ContentRestore {
            type_key,
            entry_id,
            expected_version,
        } => stage_lifecycle_command(
            ctx,
            StagedLifecycleOperation::Restore,
            type_key,
            entry_id,
            expected_version,
        ),
        HostOperationRequest::ContentPurge {
            type_key,
            entry_id,
            expected_version,
        } => stage_lifecycle_command(
            ctx,
            StagedLifecycleOperation::Purge,
            type_key,
            entry_id,
            expected_version,
        ),
        HostOperationRequest::ContentGet {
            type_key,
            entry_id,
            locale,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentGet {
                type_key,
                entry_id,
                locale,
            },
        ),
        HostOperationRequest::ContentList {
            type_key,
            locale,
            limit,
            offset,
            published_only,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentList {
                type_key,
                locale,
                limit,
                offset,
                published_only,
            },
        ),
        HostOperationRequest::ContentTrashGet {
            type_key,
            entry_id,
            locale,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentTrashGet {
                type_key,
                entry_id,
                locale,
            },
        ),
        HostOperationRequest::ContentTrashList {
            type_key,
            locale,
            limit,
            offset,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::ContentTrashList {
                type_key,
                locale,
                limit,
                offset,
            },
        ),
        HostOperationRequest::TaxonomyList {
            kind,
            locale,
            limit,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::TaxonomyList {
                kind,
                locale,
                limit,
            },
        ),
        HostOperationRequest::MediaGet { media_id } => {
            dispatch_host_read(ctx, HostReadRequest::MediaGet { media_id })
        }
        HostOperationRequest::SiteOptionsGet { locale } => {
            dispatch_host_read(ctx, HostReadRequest::SiteOptionsGet { locale })
        }
        HostOperationRequest::PluginSettingsGet => {
            dispatch_host_read(ctx, HostReadRequest::PluginSettingsGet)
        }
        HostOperationRequest::PluginSecretGet { key } => {
            if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
                return HostOperationResponse::err("Khóa secret không hợp lệ.");
            }
            dispatch_host_read(ctx, HostReadRequest::PluginSecretGet { key })
        }
        HostOperationRequest::L10nTranslate {
            text_domain,
            msgid,
            msgid_plural,
            count,
            context,
            locale,
        } => dispatch_host_read(
            ctx,
            HostReadRequest::L10nTranslate {
                text_domain,
                msgid,
                msgid_plural,
                count,
                context,
                locale: locale.or_else(|| ctx.default_locale.clone()),
            },
        ),
        HostOperationRequest::L10nTranslateBatch { mut items } => {
            for item in &mut items {
                if item.locale.is_none() {
                    item.locale = ctx.default_locale.clone();
                }
            }
            dispatch_host_read(ctx, HostReadRequest::L10nTranslateBatch { items })
        }
        HostOperationRequest::Log { level, message } => {
            if ctx.execution_mode == HookExecutionMode::FilterContactBeforeSubmit {
                return HostOperationResponse::err(
                    "Ghi log bị cấm trong filter:contact/before_submit.",
                );
            }
            if !matches!(level.as_str(), "info" | "warn" | "error") {
                return HostOperationResponse::err("level log chỉ hỗ trợ info, warn hoặc error.");
            }
            if ctx.staged_logs.len() >= MAX_STAGED_LOGS {
                return HostOperationResponse::err(
                    "quota_exceeded: Vượt quá giới hạn 20 log mỗi lần chạy.",
                );
            }
            let clean = sanitize_plain_text(&message, MAX_LOG_MESSAGE_CHARS);
            ctx.staged_logs.push(StagedPluginLog {
                level,
                message: clean,
            });
            HostOperationResponse::ok_data(json!({ "staged": true }))
        }
    }
}
