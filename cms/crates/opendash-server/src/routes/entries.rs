//! Authenticated content entry CRUD for CPTs (M3.3 backend).

use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
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
        media as media_storage, page::active_content_templates,
    },
};

pub const MAX_CONTENT_ENTRY_JSON_BYTES: usize = 768 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationInput {
    pub status: String,
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
            "status" if matches!(value, "all" | "draft" | "published") => {
                filter.status = value.into();
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
            "sort" if matches!(value, "title" | "created_at" | "updated_at" | "id") => {
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
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
    match plugins
        .execute_plugin_action(
            &plugin_id,
            &action_id,
            &actor,
            input,
            &content_registry,
            &hooks,
            correlation_id,
        )
        .await
    {
        Ok(response) => Ok((StatusCode::OK, Json(response)).into_response()),
        Err((status, response)) => Ok((status, Json(response)).into_response()),
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(input): Json<ContentEntryInput>,
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
    let actor_role = actor_field_role(&actor);
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
    let _ = hooks
        .dispatch_content_after_save_for_actor(
            &type_key,
            actor_role,
            json!({
                "type_key": &type_key,
                "id": entry.id,
                "operation": "create",
                "version": entry.version,
                "locale": &entry.locale,
                "translation_group_id": &entry.translation_group_id,
            }),
        )
        .await;
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<ContentEntryInput>,
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
    let existing_meta = entry_storage::get_entry_locale_and_group(&db, &type_key, id).await?;
    if let (Some(req_loc), Some((cur_loc, _))) = (&input.locale, &existing_meta)
        && req_loc.trim().to_ascii_lowercase() != *cur_loc
    {
        return Err(ApiError::bad_request(
            "Không thể đổi ngôn ngữ của nội dung đã tạo.",
        ));
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
    let actor_role = actor_field_role(&actor);
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
    let _ = hooks
        .dispatch_content_after_save_for_actor(
            &type_key,
            actor_role,
            json!({
                "type_key": &type_key,
                "id": entry.id,
                "operation": "update",
                "version": entry.version,
                "locale": &entry.locale,
                "translation_group_id": &entry.translation_group_id,
            }),
        )
        .await;
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<StatusCode> {
    require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    entry_storage::delete_entry(&db, &type_key, id, version).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish/Unpublish is a separate, authenticated optimistic-CAS operation.
/// Ordinary PUT/Save deliberately never alters the current status.
pub async fn entry_publication(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<PublicationInput>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    if !is_builtin(&type_key) || !matches!(input.status.as_str(), "draft" | "published") {
        return Err(ApiError::bad_request(
            "Chỉ Post/Page hỗ trợ draft/published.",
        ));
    }
    let version = expected_version(&headers)?;
    let record =
        entry_storage::set_publication_status(&db, &type_key, id, version, &input.status).await?;
    // No success hooks are emitted until state is durable; on CAS failures
    // storage returns 404/412 without any external side effects.
    Ok(([(header::ETAG, etag(record.version))], Json(record)).into_response())
}

/// Persisted entry preview is private and never shares the public reader.
pub async fn entry_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    if matches!(type_key.as_str(), "pattern" | "section") {
        return Err(ApiError::bad_request(
            "Preview không hỗ trợ Pattern/Section.",
        ));
    }
    let mut schema = content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    if let Some(eff) = &effective
        && !schema.fields.iter().any(|f| f.key == "body")
        && let Some(body_fe) = eff.effective_fields.get("body")
    {
        schema.fields.push(body_fe.definition.clone());
    }
    let supports_builder = schema
        .settings
        .supports(opendash_core::content::ContentSupport::Builder);
    if !is_builtin(&type_key) && !supports_builder {
        return Err(ApiError::bad_request(
            "Preview chỉ hỗ trợ Post/Page hoặc CPT bật Builder.",
        ));
    }
    let mut entry = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let actor_role = match actor.user.role {
        crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
        crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
    };
    if let Some(eff) = &effective {
        entry_storage::filter_entry_values_for_role(&mut entry, eff, actor_role);
    }
    let body_view_allowed = effective.as_ref().is_none_or(|eff| {
        eff.effective_fields
            .get("body")
            .is_none_or(|fe| actor_role >= fe.definition.min_view_role)
    });
    let has_blocks = entry
        .values
        .get("body")
        .and_then(serde_json::Value::as_str)
        .is_some_and(opendash_core::builder_blocks::contains_blocks);
    let (_, bcp47_tag, dir) =
        crate::storage::settings::get_locale_bcp47_and_dir(&db, Some(&entry.locale)).await?;
    let seo_view = crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, &entry.title);
    let raw_html = if supports_builder && body_view_allowed && has_blocks {
        if let Some(rendered) =
            crate::storage::builder::preview_saved_entry_body(&db, &actor, &type_key, id).await?
        {
            rendered.html
        } else {
            let view = content_render::authenticated_preview_view(entry, &schema);
            let (_, selected_template) = active_content_templates(&db).await?;
            content_render::render_single(&schema, &view, selected_template.as_deref())?
        }
    } else {
        let view = content_render::authenticated_preview_view(entry, &schema);
        let (_, selected_template) = active_content_templates(&db).await?;
        content_render::render_single(&schema, &view, selected_template.as_deref())?
    };
    let html = content_render::compose_document_with_seo(None, &raw_html, None, &seo_view)?;
    Ok((
        [
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        Html(html),
    )
        .into_response())
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(input): Json<entry_storage::CompositeEntrySaveInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let plan =
        entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, None, &input).await?;
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
    let actor_role = actor_field_role(&actor);
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let result =
        entry_storage::commit_composite_entry_save(&db, &actor, &type_key, None, &input, &plan)
            .await?;
    let _ = hooks
        .dispatch_content_after_save_for_actor(
            &type_key,
            actor_role,
            json!({
                "type_key": &type_key,
                "id": result.entry.id,
                "operation": "create",
                "version": result.entry.version,
                "locale": &result.entry.locale,
                "translation_group_id": &result.entry.translation_group_id,
            }),
        )
        .await;
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<entry_storage::CompositeEntrySaveInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let plan =
        entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, Some(id), &input)
            .await?;
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
    let actor_role = actor_field_role(&actor);
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let result =
        entry_storage::commit_composite_entry_save(&db, &actor, &type_key, Some(id), &input, &plan)
            .await?;
    let _ = hooks
        .dispatch_content_after_save_for_actor(
            &type_key,
            actor_role,
            json!({
                "type_key": &type_key,
                "id": result.entry.id,
                "operation": "update",
                "version": result.entry.version,
                "locale": &result.entry.locale,
                "translation_group_id": &result.entry.translation_group_id,
            }),
        )
        .await;
    Ok(([(header::ETAG, etag(result.entry.version))], Json(result)).into_response())
}

pub async fn entry_translation_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
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
    let _ = hooks
        .dispatch_content_after_save_for_actor(
            &type_key,
            actor_role,
            json!({
                "type_key": &type_key,
                "id": created.id,
                "operation": "create_translation",
                "version": created.version,
                "locale": &created.locale,
                "translation_group_id": &created.translation_group_id,
            }),
        )
        .await;
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
