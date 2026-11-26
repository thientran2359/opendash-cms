//! Authenticated content entry CRUD for CPTs (M3.3 backend).

use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::content::{ContentEntryInput, valid_content_type_key};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    auth::{require_session, require_session_with_csrf},
    content_render,
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    storage::{
        content::load_content_type_definition, entries as entry_storage, entry_table,
        media as media_storage, permalinks, public_content, settings,
    },
};

pub const MAX_CONTENT_ENTRY_JSON_BYTES: usize = 768 * 1024;
pub const MAX_METADATA_PATCH_JSON_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationInput {
    pub status: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyLifecycleInput {}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RestoreEntryInput {
    #[serde(default)]
    pub confirm_public_restore: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PurgeEntryQuery {
    #[serde(default)]
    pub retained_trash: bool,
}

fn is_builtin(key: &str) -> bool {
    matches!(key, "post" | "page")
}

fn actor_field_role(actor: &crate::auth::SessionContext) -> opendash_core::content::FieldRole {
    match actor.user.role {
        crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
        crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
    }
}

fn execution_principal_for_actor(
    actor: &crate::auth::SessionContext,
) -> opendash_core::hooks::ExecutionPrincipal {
    match actor.user.role {
        crate::auth::Role::Admin => opendash_core::hooks::ExecutionPrincipal::Admin,
        crate::auth::Role::Editor => opendash_core::hooks::ExecutionPrincipal::Editor,
    }
}

fn apply_prepare_save_patch_to_entry_input(
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

fn apply_prepare_save_patch_to_composite_input(
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

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EntryPagination {
    limit: Option<i64>,
    offset: Option<i64>,
    category_id: Option<i64>,
    tag_id: Option<i64>,
    include_descendants: Option<bool>,
    locale: Option<String>,
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

fn valid_type_key(key: &str) -> ApiResult<()> {
    if valid_content_type_key(key) {
        Ok(())
    } else {
        Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."))
    }
}

async fn content_schema(
    db: &SqlitePool,
    type_key: &str,
) -> ApiResult<opendash_core::content::ContentTypeDefinition> {
    valid_type_key(type_key)?;
    load_content_type_definition(db, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))
}

fn expected_version(headers: &HeaderMap) -> ApiResult<i64> {
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

fn etag(version: i64) -> String {
    format!("\"v{version}\"")
}

pub async fn entry_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Query(pagination): Query<EntryPagination>,
) -> ApiResult<Json<Vec<entry_storage::ContentEntryRecord>>> {
    let actor = require_session(&headers, &db).await?;
    content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    let limit = pagination.limit.unwrap_or(50);
    let offset = pagination.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=1_000_000).contains(&offset) {
        return Err(ApiError::bad_request("Phân trang không hợp lệ."));
    }
    if pagination
        .category_id
        .into_iter()
        .chain(pagination.tag_id)
        .any(|id| id <= 0)
    {
        return Err(ApiError::bad_request("ID bộ lọc taxonomy phải dương."));
    }
    if pagination.include_descendants.unwrap_or(false) && pagination.category_id.is_none() {
        return Err(ApiError::bad_request(
            "include_descendants chỉ được dùng cùng category_id.",
        ));
    }
    let locale_filter = match pagination.locale.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim().to_ascii_lowercase();
            if trimmed == "all" {
                None
            } else {
                Some(crate::storage::settings::resolve_requested_locale(&db, Some(&trimmed)).await?)
            }
        }
        None => None,
    };
    let filters = entry_storage::EntryFilters {
        category_id: pagination.category_id,
        tag_id: pagination.tag_id,
        include_descendants: pagination.include_descendants.unwrap_or(false),
        locale: locale_filter,
        ..entry_storage::EntryFilters::default()
    };
    let mut items = entry_storage::list_entries(&db, &type_key, limit, offset, filters).await?;
    if let Some(eff) = effective {
        let role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        for item in &mut items {
            entry_storage::filter_entry_values_for_role(item, &eff, role);
        }
    }
    Ok(Json(items))
}

pub async fn entry_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let mut entry = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    if let Some(eff) =
        crate::storage::content::load_effective_content_schema(&db, &type_key).await?
    {
        let role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        entry_storage::filter_entry_values_for_role(&mut entry, &eff, role);
    }
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_create(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(mut input): Json<ContentEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let mut schema = content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    let preflight_schema_token = effective.as_ref().map(|e| e.schema_token.clone());
    if let Some(eff) = &effective
        && !schema.fields.iter().any(|f| f.key == "body")
        && let Some(body_fe) = eff.effective_fields.get("body")
    {
        schema.fields.push(body_fe.definition.clone());
    }
    input
        .validate_against(&schema)
        .map_err(ApiError::bad_request)?;
    media_storage::require_existing_references(&db, &schema, &input).await?;
    let candidate_locale =
        crate::storage::settings::resolve_requested_locale(&db, input.locale.as_deref()).await?;
    let actor_role = actor_field_role(&actor);
    if let Some(eff) = &effective {
        let prepare_payload = json!({
            "operation": "create",
            "type_key": &type_key,
            "entry_id": serde_json::Value::Null,
            "locale": &candidate_locale,
            "title": &input.title,
            "slug": &input.slug,
            "candidate_values": &input.values,
            "term_ids": &input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_entry_input(&mut input, patch) {
            input
                .validate_against(&schema)
                .map_err(ApiError::unprocessable)?;
            input
                .validate_against_effective(eff)
                .map_err(ApiError::unprocessable)?;
            media_storage::require_existing_references(&db, &schema, &input).await?;
            media_storage::require_existing_effective_references(&db, eff, &input.values).await?;
        }
    }
    let candidate = json!({
        "operation": "create",
        "type_key": &type_key,
        "entry_id": serde_json::Value::Null,
        "expected_version": serde_json::Value::Null,
        "locale": &candidate_locale,
        "translation_group_id": serde_json::Value::Null,
        "title": &input.title,
        "slug": &input.slug,
        "candidate_values": &input.values,
        "term_ids": &input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let entry = match preflight_schema_token.as_deref() {
        Some(tok) => {
            entry_storage::create_entry_with_schema_fence(
                &db, &actor, &type_key, &schema, &input, tok,
            )
            .await?
        }
        None => entry_storage::create_entry(&db, &actor, &type_key, &schema, &input).await?,
    };
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, entry.id).await?;
    let principal = execution_principal_for_actor(&actor);
    if entry.publication_status == "published" {
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::ContentAfterPublish,
            principal,
            locale: Some(entry.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": entry.id,
                "entry_id": entry.id,
                "version": entry.version,
                "locale": &entry.locale,
                "publication_status": &entry.publication_status,
                "translation_group_id": &entry.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": entry.id,
            "entry_id": entry.id,
            "operation": "create",
            "version": entry.version,
            "locale": &entry.locale,
            "translation_group_id": &entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(entry.version)),
            (
                header::LOCATION,
                format!("/api/admin/content-types/{type_key}/entries/{}", entry.id),
            ),
        ],
        Json(entry),
    )
        .into_response())
}

pub async fn entry_update(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(mut input): Json<ContentEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    let mut schema = content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    let preflight_schema_token = effective.as_ref().map(|e| e.schema_token.clone());
    if let Some(eff) = &effective
        && !schema.fields.iter().any(|f| f.key == "body")
        && let Some(body_fe) = eff.effective_fields.get("body")
    {
        schema.fields.push(body_fe.definition.clone());
    }
    input
        .validate_against(&schema)
        .map_err(ApiError::bad_request)?;
    media_storage::require_existing_references(&db, &schema, &input).await?;
    let preflight =
        crate::storage::entry_lifecycle::ensure_active_entry_version(&db, &type_key, id, version)
            .await?;
    let existing_meta = Some((preflight.locale, preflight.translation_group_id));
    if let (Some(req_loc), Some((cur_loc, _))) = (&input.locale, &existing_meta)
        && req_loc.trim().to_ascii_lowercase() != *cur_loc
    {
        return Err(ApiError::bad_request(
            "Không thể đổi ngôn ngữ của nội dung đã tạo.",
        ));
    }
    let actor_role = actor_field_role(&actor);
    if let Some(eff) = &effective {
        let prepare_payload = json!({
            "operation": "update",
            "type_key": &type_key,
            "entry_id": id,
            "locale": existing_meta.as_ref().map(|(l, _)| l.as_str()),
            "title": &input.title,
            "slug": &input.slug,
            "candidate_values": &input.values,
            "term_ids": &input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_entry_input(&mut input, patch) {
            input
                .validate_against(&schema)
                .map_err(ApiError::unprocessable)?;
            input
                .validate_against_effective(eff)
                .map_err(ApiError::unprocessable)?;
            media_storage::require_existing_references(&db, &schema, &input).await?;
            media_storage::require_existing_effective_references(&db, eff, &input.values).await?;
        }
    }
    let candidate = json!({
        "operation": "update",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": version,
        "locale": existing_meta.as_ref().map(|(l, _)| l.as_str()),
        "translation_group_id": existing_meta.as_ref().map(|(_, g)| g.as_str()),
        "title": &input.title,
        "slug": &input.slug,
        "candidate_values": &input.values,
        "term_ids": &input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let entry = match preflight_schema_token.as_deref() {
        Some(tok) => {
            entry_storage::update_entry_with_schema_fence(
                &db, &actor, &type_key, &schema, id, version, &input, tok,
            )
            .await?
        }
        None => {
            entry_storage::update_entry(&db, &actor, &type_key, &schema, id, version, &input)
                .await?
        }
    };
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, entry.id).await?;
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal: execution_principal_for_actor(&actor),
        locale: Some(entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": entry.id,
            "entry_id": entry.id,
            "operation": "update",
            "version": entry.version,
            "locale": &entry.locale,
            "translation_group_id": &entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_delete(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    let receipt = entry_storage::delete_entry(&db, &actor, &type_key, id, version).await?;
    hooks.enqueue_entry_lifecycle_hooks(execution_principal_for_actor(&actor), &receipt, lease);
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
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

/// Narrow sparse metadata & single-taxonomy mutation shared by Quick Edit and Bulk Taxonomy.
pub async fn entry_metadata_patch(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(patch): Json<crate::storage::entry_lifecycle::MetadataPatch>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let expected_ver = expected_version(&headers)?;
    valid_type_key(&type_key)?;

    let mut plan = crate::storage::entry_lifecycle::prepare_metadata_patch(
        &db,
        &actor,
        &type_key,
        id,
        expected_ver,
        &patch,
    )
    .await?;
    let actor_role = actor_field_role(&actor);

    let prepare_payload = json!({
        "operation": "metadata",
        "type_key": &type_key,
        "entry_id": id,
        "locale": &plan.entry_locale,
        "title": &plan.candidate_title,
        "slug": &plan.candidate_slug,
        "candidate_values": &plan.candidate_values,
        "term_ids": &plan.candidate_term_ids,
    });
    let hook_patch = hooks
        .dispatch_content_prepare_save_with_scope(
            &type_key,
            actor_role,
            &plan.effective,
            prepare_payload,
            crate::hooks::PrepareSaveMutationScope::Metadata {
                requested_title: plan.requested_title,
                requested_slug: plan.requested_slug,
                taxonomy_key: plan.requested_taxonomy_key.clone(),
                entry_locale: plan.entry_locale.clone(),
            },
            Some(&db),
        )
        .await?;
    crate::storage::entry_lifecycle::apply_prepare_save_to_metadata_plan(&mut plan, hook_patch)?;

    let candidate = json!({
        "operation": "metadata",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": expected_ver,
        "locale": &plan.entry_locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.candidate_title,
        "slug": &plan.candidate_slug,
        "publication_status": &plan.candidate_status,
        "candidate_values": &plan.candidate_values,
        "term_ids": &plan.candidate_term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }

    let (row, schema_token, receipt) = crate::storage::entry_lifecycle::commit_metadata_patch(
        &db,
        &actor,
        &type_key,
        id,
        expected_ver,
        &patch,
        &plan,
    )
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

    let response = crate::storage::entry_lifecycle::MetadataPatchResponse {
        schema_token,
        secondary: crate::storage::entry_lifecycle::SecondaryOutcome {
            projection,
            hook_enqueued: hook_out.enqueued,
            hook_dropped: hook_out.dropped,
        },
        row,
    };

    Ok((
        StatusCode::OK,
        [
            (header::ETAG, etag(response.row.version)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Json(response),
    )
        .into_response())
}

pub(crate) async fn render_admin_single_fallback_html(
    db: &SqlitePool,
    record: &entry_storage::ContentEntryRecord,
    effective: &opendash_core::content::EffectiveContentSchema,
    actor_role: opendash_core::content::FieldRole,
    selected_template: Option<&str>,
) -> ApiResult<String> {
    let type_key = &record.content_type_key;
    let eligible =
        public_content::batch_load_eligible_public_taxonomies(db, std::slice::from_ref(type_key))
            .await?
            .remove(type_key)
            .unwrap_or_default();
    let terms = public_content::batch_load_public_entry_terms(db, &[(record.id, type_key.clone())])
        .await?
        .remove(&record.id)
        .unwrap_or_default();
    let view = content_render::authenticated_preview_view_with_context(
        record, effective, None, &terms, &eligible, actor_role,
    );
    let archive_url = {
        let mut conn = db.acquire().await?;
        permalinks::canonical_archive_path_in_tx(&mut conn, type_key, &record.locale).await?
    };
    let general = settings::get_general_for_locale(db, Some(&record.locale)).await?;
    let pub_settings = settings::get_public(db).await?;
    let site_ctx = content_render::SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
        timezone: &general.timezone,
        origin: pub_settings
            .public_origin
            .as_deref()
            .map(|s| s.trim().trim_end_matches('/'))
            .unwrap_or(""),
        current_year: content_render::current_year_in_timezone(&general.timezone),
    };
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: general.timezone.clone(),
    };
    let definition = &effective.base.descriptor;
    let breadcrumbs = content_render::single_breadcrumbs(
        content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        type_key,
        &definition.label,
        definition.settings.has_archive,
        archive_url.clone(),
        view.primary_category.as_ref(),
        &view.title,
        view.url.clone(),
    );
    content_render::render_single_with_context_for_locale(
        definition,
        &view,
        selected_template,
        &site_ctx,
        archive_url.as_deref(),
        &breadcrumbs,
        &fmt_cfg,
        &record.locale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_match_requires_canonical_positive_version() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            expected_version(&headers).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for invalid in ["*", "\"v0\"", "\"v01\"", "v1", "\"v-1\"", "W/\"v1\""] {
            headers.insert(header::IF_MATCH, invalid.parse().unwrap());
            assert_eq!(
                expected_version(&headers).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(header::IF_MATCH, "\"v42\"".parse().unwrap());
        assert_eq!(expected_version(&headers).unwrap(), 42);
    }
}

pub async fn entry_composite_create(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(mut input): Json<entry_storage::CompositeEntrySaveInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let mut plan =
        entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, None, &input).await?;
    let actor_role = actor_field_role(&actor);
    if let Some(eff) =
        crate::storage::content::load_effective_content_schema(&db, &type_key).await?
    {
        let prepare_payload = json!({
            "operation": "create",
            "type_key": &type_key,
            "entry_id": serde_json::Value::Null,
            "locale": &plan.locale,
            "title": &plan.merged_input.title,
            "slug": &plan.merged_input.slug,
            "candidate_values": &plan.merged_input.values,
            "term_ids": &plan.merged_input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, &eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_composite_input(&mut input, patch) {
            plan =
                entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, None, &input)
                    .await?;
        }
    }
    let candidate = json!({
        "operation": "create",
        "type_key": &type_key,
        "entry_id": serde_json::Value::Null,
        "expected_version": serde_json::Value::Null,
        "locale": &plan.locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.merged_input.title,
        "slug": &plan.merged_input.slug,
        "candidate_values": &plan.merged_input.values,
        "term_ids": &plan.merged_input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let result =
        entry_storage::commit_composite_entry_save(&db, &actor, &type_key, None, &input, &plan)
            .await?;
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, result.entry.id)
        .await?;
    let principal = execution_principal_for_actor(&actor);
    if result.entry.publication_status == "published" {
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::ContentAfterPublish,
            principal,
            locale: Some(result.entry.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": result.entry.id,
                "entry_id": result.entry.id,
                "version": result.entry.version,
                "locale": &result.entry.locale,
                "publication_status": &result.entry.publication_status,
                "translation_group_id": &result.entry.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(result.entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": result.entry.id,
            "entry_id": result.entry.id,
            "operation": "create",
            "version": result.entry.version,
            "locale": &result.entry.locale,
            "translation_group_id": &result.entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(result.entry.version)),
            (
                header::LOCATION,
                format!(
                    "/api/admin/content-types/{type_key}/entries/{}",
                    result.entry.id
                ),
            ),
        ],
        Json(result),
    )
        .into_response())
}

pub async fn entry_composite_update(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(mut input): Json<entry_storage::CompositeEntrySaveInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let before_status = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .map(|e| e.publication_status);
    let mut plan =
        entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, Some(id), &input)
            .await?;
    let actor_role = actor_field_role(&actor);
    if let Some(eff) =
        crate::storage::content::load_effective_content_schema(&db, &type_key).await?
    {
        let prepare_payload = json!({
            "operation": "update",
            "type_key": &type_key,
            "entry_id": id,
            "locale": &plan.locale,
            "title": &plan.merged_input.title,
            "slug": &plan.merged_input.slug,
            "candidate_values": &plan.merged_input.values,
            "term_ids": &plan.merged_input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, &eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_composite_input(&mut input, patch) {
            plan = entry_storage::prepare_composite_entry_save(
                &db,
                &actor,
                &type_key,
                Some(id),
                &input,
            )
            .await?;
        }
    }
    let candidate = json!({
        "operation": "update",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": input.expected_version,
        "locale": &plan.locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.merged_input.title,
        "slug": &plan.merged_input.slug,
        "candidate_values": &plan.merged_input.values,
        "term_ids": &plan.merged_input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let result =
        entry_storage::commit_composite_entry_save(&db, &actor, &type_key, Some(id), &input, &plan)
            .await?;
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, result.entry.id)
        .await?;
    let principal = execution_principal_for_actor(&actor);
    if before_status.as_deref() != Some(result.entry.publication_status.as_str()) {
        let pub_event = if result.entry.publication_status == "published" {
            opendash_core::hooks::HookEvent::ContentAfterPublish
        } else {
            opendash_core::hooks::HookEvent::ContentAfterUnpublish
        };
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: pub_event,
            principal,
            locale: Some(result.entry.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": result.entry.id,
                "entry_id": result.entry.id,
                "version": result.entry.version,
                "locale": &result.entry.locale,
                "publication_status": &result.entry.publication_status,
                "translation_group_id": &result.entry.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(result.entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": result.entry.id,
            "entry_id": result.entry.id,
            "operation": "update",
            "version": result.entry.version,
            "locale": &result.entry.locale,
            "translation_group_id": &result.entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok(([(header::ETAG, etag(result.entry.version))], Json(result)).into_response())
}

pub async fn entry_translation_create(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<entry_storage::CreateEntryTranslationInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let expected_source_version = expected_version(&headers)?;
    let plan = entry_storage::prepare_entry_translation(
        &db,
        &actor,
        &type_key,
        id,
        expected_source_version,
        &input,
    )
    .await?;
    let candidate = json!({
        "operation": "create_translation",
        "type_key": &type_key,
        "source_entry_id": id,
        "entry_id": serde_json::Value::Null,
        "expected_source_version": expected_source_version,
        "locale": &plan.target_locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.title,
        "slug": &plan.slug,
        "candidate_values": &plan.values,
        "term_ids": &plan.term_ids,
    });
    let actor_role = actor_field_role(&actor);
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let created = entry_storage::commit_entry_translation(
        &db,
        &actor,
        &type_key,
        id,
        expected_source_version,
        &input,
        &plan,
    )
    .await?;
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, created.id)
        .await?;
    let principal = execution_principal_for_actor(&actor);
    if created.publication_status == "published" {
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::ContentAfterPublish,
            principal,
            locale: Some(created.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": created.id,
                "entry_id": created.id,
                "version": created.version,
                "locale": &created.locale,
                "publication_status": &created.publication_status,
                "translation_group_id": &created.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(created.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": created.id,
            "entry_id": created.id,
            "operation": "create_translation",
            "version": created.version,
            "locale": &created.locale,
            "translation_group_id": &created.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(created.version)),
            (
                header::LOCATION,
                format!("/api/admin/content-types/{type_key}/entries/{}", created.id),
            ),
        ],
        Json(created),
    )
        .into_response())
}
