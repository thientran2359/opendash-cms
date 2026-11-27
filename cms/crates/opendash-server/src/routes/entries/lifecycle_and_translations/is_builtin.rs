use super::*;

pub(crate) fn is_builtin(key: &str) -> bool {
    matches!(key, "post" | "page")
}

pub(crate) fn actor_field_role(
    actor: &crate::auth::SessionContext,
) -> opendash_core::content::FieldRole {
    match actor.user.role {
        crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
        crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
    }
}

pub(crate) fn execution_principal_for_actor(
    actor: &crate::auth::SessionContext,
) -> opendash_core::hooks::ExecutionPrincipal {
    match actor.user.role {
        crate::auth::Role::Admin => opendash_core::hooks::ExecutionPrincipal::Admin,
        crate::auth::Role::Editor => opendash_core::hooks::ExecutionPrincipal::Editor,
    }
}

pub(crate) fn apply_prepare_save_patch_to_entry_input(
    input: &mut ContentEntryInput,
    patch: crate::hooks::ContentPrepareSavePatch,
) -> bool {
    if patch.is_empty() {
        return false;
    }
    if let Some(title) = patch.title {
        input.title = title;
    }
    if let Some(slug) = patch.slug {
        input.slug = slug;
    }
    for (k, v) in patch.values_patch {
        input.values.insert(k, v);
    }
    if let Some(term_ids) = patch.term_ids {
        input.term_ids = Some(term_ids);
    }
    true
}

pub(crate) fn apply_prepare_save_patch_to_composite_input(
    input: &mut entry_storage::CompositeEntrySaveInput,
    patch: crate::hooks::ContentPrepareSavePatch,
) -> bool {
    if patch.is_empty() {
        return false;
    }
    if let Some(title) = patch.title {
        input.title = title;
    }
    if let Some(slug) = patch.slug {
        input.slug = slug;
    }
    for (k, v) in patch.values_patch {
        input.values.insert(k, v);
    }
    if let Some(term_ids) = patch.term_ids {
        input.term_ids = Some(term_ids);
    }
    true
}

/// Unlike the legacy list extractor this intentionally rejects duplicate and
/// unknown query keys, including invalid scalar syntax.
pub fn parse_table_query(raw: Option<&str>) -> ApiResult<entry_table::EntryTableFilter> {
    let raw = raw.unwrap_or("");
    if raw.len() > 4096 {
        return Err(ApiError::bad_request("Truy vấn bảng nội dung quá dài."));
    }
    let mut filter = entry_table::EntryTableFilter::default();
    let mut keys = BTreeSet::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if !keys.insert(key.to_string()) {
            return Err(ApiError::bad_request("Tham số truy vấn bảng bị trùng."));
        }
        let value = value.as_ref();
        match key.as_ref() {
            "q" => {
                filter.q = value.trim().to_string();
                if filter.q.chars().count() > 200 || filter.q.chars().any(char::is_control) {
                    return Err(ApiError::bad_request("Tìm kiếm quá dài hoặc không hợp lệ."));
                }
            }
            "status" if matches!(value, "all" | "draft" | "published" | "trash") => {
                filter.status = value.into();
            }
            "retained_trash" if matches!(value, "true" | "false") => {
                filter.retained_trash = value == "true";
            }
            "locale" => {
                let norm = value.trim().to_ascii_lowercase();
                if norm.is_empty() || norm.len() > 35 || norm.chars().any(char::is_control) {
                    return Err(ApiError::bad_request("Tham số locale không hợp lệ."));
                }
                if norm != "all" {
                    filter.locale = Some(norm);
                }
            }
            "category_id" => {
                filter.category_id = Some(
                    value
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| ApiError::bad_request("category_id không hợp lệ."))?,
                );
            }
            "tag_id" => {
                filter.tag_id = Some(
                    value
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| ApiError::bad_request("tag_id không hợp lệ."))?,
                );
            }
            "taxonomy_key" if opendash_core::content::valid_content_type_key(value) => {
                filter.taxonomy_key = Some(value.to_owned());
            }
            "term_id" => {
                filter.term_id = Some(
                    value
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0 && id.to_string() == value)
                        .ok_or_else(|| ApiError::bad_request("term_id không hợp lệ."))?,
                );
            }
            "include_descendants" if matches!(value, "true" | "false") => {
                filter.include_descendants = value == "true";
            }
            "updated_month" => {
                let bytes = value.as_bytes();
                if bytes.len() != 7
                    || bytes[4] != b'-'
                    || !bytes[..4].iter().chain(&bytes[5..]).all(u8::is_ascii_digit)
                    || value[..4].parse::<u16>().unwrap_or(0) == 0
                    || !(1..=12).contains(&value[5..].parse::<u8>().unwrap_or(0))
                {
                    return Err(ApiError::bad_request("updated_month phải là YYYY-MM."));
                }
                filter.updated_month = Some(value.into());
            }
            "sort"
                if matches!(
                    value,
                    "title" | "created_at" | "updated_at" | "id" | "trashed_at"
                ) =>
            {
                filter.sort = value.into();
            }
            "order" if matches!(value, "asc" | "desc") => {
                filter.order = value.into();
            }
            "limit" => {
                filter.limit = value
                    .parse::<i64>()
                    .ok()
                    .filter(|n| (1..=100).contains(n))
                    .ok_or_else(|| ApiError::bad_request("limit phải là 1..100."))?;
            }
            "offset" => {
                filter.offset = value
                    .parse::<i64>()
                    .ok()
                    .filter(|n| (0..=1_000_000).contains(n))
                    .ok_or_else(|| ApiError::bad_request("offset phải là 0..1000000."))?;
            }
            _ => return Err(ApiError::bad_request("Tham số bảng nội dung không hợp lệ.")),
        }
    }
    if filter.taxonomy_key.is_some() != filter.term_id.is_some()
        || (filter.taxonomy_key.is_some()
            && (filter.category_id.is_some() || filter.tag_id.is_some()))
    {
        return Err(ApiError::bad_request(
            "taxonomy_key/term_id thiếu cặp hoặc trộn aliases.",
        ));
    }
    if filter.include_descendants && filter.category_id.is_none() && filter.taxonomy_key.is_none() {
        return Err(ApiError::bad_request(
            "include_descendants cần taxonomy filter.",
        ));
    }
    if filter.retained_trash {
        if filter.status != "trash" {
            return Err(ApiError::bad_request(
                "retained_trash=true chỉ hỗ trợ cùng status=trash.",
            ));
        }
        let allowed_keys = ["status", "retained_trash", "locale", "limit", "offset"];
        if keys.iter().any(|k| !allowed_keys.contains(&k.as_str())) {
            return Err(ApiError::bad_request(
                "Chế độ bảo trì retained_trash không hỗ trợ bộ lọc bổ sung.",
            ));
        }
    }
    Ok(filter)
}

pub async fn entry_table_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    RawQuery(raw): RawQuery,
) -> ApiResult<Json<entry_table::EntryTableResult>> {
    let actor = require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let filter = parse_table_query(raw.as_deref())?;
    let role = match actor.user.role {
        crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
        crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
    };
    Ok(Json(
        entry_table::query(&db, &type_key, role, &filter).await?,
    ))
}

/// Declarative provider metadata for the authenticated list. The table's
/// legacy row DTO and /entries bare array are unchanged.
pub async fn entry_table_effective_config(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let role = if actor.user.role == crate::auth::Role::Admin {
        opendash_core::content::FieldRole::Admin
    } else {
        opendash_core::content::FieldRole::Editor
    };
    let config = crate::storage::admin_extensions::effective_list(&db, &type_key, role).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(config)).into_response())
}

pub(crate) fn valid_type_key(key: &str) -> ApiResult<()> {
    if valid_content_type_key(key) {
        Ok(())
    } else {
        Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."))
    }
}

pub(crate) async fn content_schema(
    db: &SqlitePool,
    type_key: &str,
) -> ApiResult<opendash_core::content::ContentTypeDefinition> {
    valid_type_key(type_key)?;
    load_content_type_definition(db, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))
}

pub(crate) fn expected_version(headers: &HeaderMap) -> ApiResult<i64> {
    let raw = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match với phiên bản hiện tại."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let digits = raw
        .strip_prefix("\"v")
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"v1\"."))?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    let parsed = digits
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    if parsed <= 0 {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    Ok(parsed)
}

pub(crate) fn etag(version: i64) -> String {
    format!("\"v{version}\"")
}

/// Moves an authorized active ordinary entry into recoverable Trash.
pub async fn entry_trash(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(_input): Json<EmptyLifecycleInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    let cmd = crate::storage::entry_lifecycle::LifecycleCommand {
        operation: crate::storage::entry_lifecycle::LifecycleOperation::Trash,
        type_key,
        entry_id: id,
        expected_version: version,
    };
    let authority = crate::storage::entry_lifecycle::LifecycleAuthority {
        session: &actor,
        plugin_fence: None,
        confirmation: crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::None,
        scope: crate::storage::entry_lifecycle::LifecycleScope::Ordinary,
    };
    let receipt =
        crate::storage::entry_lifecycle::execute_lifecycle_transition(&db, &cmd, &authority)
            .await?;
    let hook_out =
        hooks.enqueue_entry_lifecycle_hooks(execution_principal_for_actor(&actor), &receipt, lease);
    let summary = crate::storage::entry_lifecycle::LifecycleSummary::from_receipt(
        &receipt,
        crate::storage::entry_lifecycle::SecondaryOutcome {
            projection: crate::storage::entry_lifecycle::ProjectionOutcome::NotNeeded,
            hook_enqueued: hook_out.enqueued,
            hook_dropped: hook_out.dropped,
        },
    );
    Ok((
        StatusCode::OK,
        [
            (header::ETAG, etag(summary.version)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Json(summary),
    )
        .into_response())
}

/// Restores a trashed entry to draft (draft_publish) or published (immediate, with explicit
/// `confirm_public_restore: true` when public).
pub async fn entry_restore(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<RestoreEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    let confirmation = if input.confirm_public_restore {
        crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::PublicRestore {
            type_key: type_key.clone(),
            entry_id: id,
            expected_version: version,
            session_token_hash: actor.token_hash.clone(),
        }
    } else {
        crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::None
    };
    let cmd = crate::storage::entry_lifecycle::LifecycleCommand {
        operation: crate::storage::entry_lifecycle::LifecycleOperation::Restore,
        type_key,
        entry_id: id,
        expected_version: version,
    };
    let authority = crate::storage::entry_lifecycle::LifecycleAuthority {
        session: &actor,
        plugin_fence: None,
        confirmation,
        scope: crate::storage::entry_lifecycle::LifecycleScope::Ordinary,
    };
    let receipt =
        crate::storage::entry_lifecycle::execute_lifecycle_transition(&db, &cmd, &authority)
            .await?;
    let projection = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
        &db,
        &hooks,
        receipt.id,
        receipt.publication_after.as_deref() == Some("published"),
    )
    .await;
    let hook_out =
        hooks.enqueue_entry_lifecycle_hooks(execution_principal_for_actor(&actor), &receipt, lease);
    let summary = crate::storage::entry_lifecycle::LifecycleSummary::from_receipt(
        &receipt,
        crate::storage::entry_lifecycle::SecondaryOutcome {
            projection,
            hook_enqueued: hook_out.enqueued,
            hook_dropped: hook_out.dropped,
        },
    );
    Ok((
        StatusCode::OK,
        [
            (header::ETAG, etag(summary.version)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Json(summary),
    )
        .into_response())
}

/// Permanently deletes a trashed entry (Admin only, requires `If-Match` and `X-Confirm-Purge: true`).
/// Accepts optional `?retained_trash=true` exclusively for dormant/withdrawn ordinary CPT maintenance.
pub async fn entry_purge(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Query(query): Query<PurgeEntryQuery>,
) -> ApiResult<Response> {
    let actor = crate::auth::require_admin_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    let explicit_confirm = headers
        .get("x-confirm-purge")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "true");
    let confirmation = if explicit_confirm {
        crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::ExplicitPurge {
            type_key: type_key.clone(),
            entry_id: id,
            expected_version: version,
            session_token_hash: actor.token_hash.clone(),
        }
    } else {
        crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::None
    };
    let scope = if query.retained_trash {
        crate::storage::entry_lifecycle::LifecycleScope::RetainedAdmin
    } else {
        crate::storage::entry_lifecycle::LifecycleScope::Ordinary
    };
    let cmd = crate::storage::entry_lifecycle::LifecycleCommand {
        operation: crate::storage::entry_lifecycle::LifecycleOperation::Purge,
        type_key,
        entry_id: id,
        expected_version: version,
    };
    let authority = crate::storage::entry_lifecycle::LifecycleAuthority {
        session: &actor,
        plugin_fence: None,
        confirmation,
        scope,
    };
    let receipt =
        crate::storage::entry_lifecycle::execute_lifecycle_transition(&db, &cmd, &authority)
            .await?;
    hooks.enqueue_entry_lifecycle_hooks(execution_principal_for_actor(&actor), &receipt, lease);
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}
