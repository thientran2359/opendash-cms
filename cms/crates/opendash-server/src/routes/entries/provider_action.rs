use super::*;

/// An Admin contribution action is never invoked from a table read. On
/// explicit POST the server reads the CURRENT entry, checks the registered
/// owner/action and actor-visible column, then creates the input itself.
/// Client row JSON or arbitrary action payloads are not accepted.
pub async fn entry_table_provider_action(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id, plugin_id, action_id)): Path<(String, i64, String, String)>,
    body: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    if !body.is_empty() {
        return Err(ApiError::bad_request(
            "Row action tự đọc dữ liệu mới nhất; không chấp nhận JSON hàng do client gửi.",
        ));
    }
    valid_type_key(&type_key)?;
    if id < 1 {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    let version = expected_version(&headers)?;
    let role = if actor.user.role == crate::auth::Role::Admin {
        opendash_core::content::FieldRole::Admin
    } else {
        opendash_core::content::FieldRole::Editor
    };
    let config = crate::storage::admin_extensions::effective_list(&db, &type_key, role).await?;
    if !config.columns.iter().any(|col| {
        col.owner_kind == "plugin"
            && col.owner_id == plugin_id
            && col.action_id.as_deref() == Some(&action_id)
    }) {
        return Err(ApiError::forbidden(
            "Action không thuộc cột hiệu lực hoặc không đủ quyền.",
        ));
    }
    crate::storage::entry_lifecycle::ensure_active_entry_version(&db, &type_key, id, version)
        .await?;
    let record = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    if record.version != version {
        return Err(ApiError::precondition_failed(
            "Nội dung đã thay đổi trước khi chạy action; hãy tải lại hàng.",
        ));
    }
    // The action manager additionally applies its manifest scoped grants to
    // top-level values and performs the role-generation fence during Wasm.
    let input = json!({
        "type_key": type_key,
        "entry_id": record.id,
        "entry_version": record.version,
        "title": record.title,
        "slug": record.slug,
        "term_ids": record.term_ids,
        "values": record.values,
    });
    let correlation_id = format!("row-{}", crate::auth::generate_random_token());
    let origin = crate::plugin::lifecycle_confirmation::LifecycleActionOrigin::ProviderRow {
        type_key: type_key.clone(),
        entry_id: record.id,
    };
    let maint_epoch = lease.maintenance_epoch();
    match plugins
        .execute_plugin_action(
            &plugin_id,
            &action_id,
            &actor,
            input,
            &content_registry,
            &hooks,
            correlation_id,
            origin,
            maint_epoch,
        )
        .await
    {
        Ok(outcome) => {
            hooks.enqueue_plugin_action_hooks(
                execution_principal_for_actor(&actor),
                outcome.committed.as_ref(),
                lease,
            );
            Ok((
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-store")],
                Json(outcome.response),
            )
                .into_response())
        }
        Err((status, response)) => Ok((
            status,
            [(header::CACHE_CONTROL, "no-store")],
            Json(response),
        )
            .into_response()),
    }
}
