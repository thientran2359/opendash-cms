//! Admin API for CPT definitions and extension field groups (`M3` / `M7-S09-T01`).

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use opendash_core::{
    content::{
        ContentTypeDefinition, ContentTypeDefinitionInput, EditorLayoutConfigV1, FieldGroupRecord,
        FieldRole, UserFieldGroupInput, valid_content_type_key,
    },
    registry::RegistryCatalog,
};
use sqlx::SqlitePool;

use crate::{
    auth::{
        Role, require_admin, require_admin_with_csrf, require_session, require_session_with_csrf,
    },
    content_render,
    error::{ApiError, ApiResult},
    routes::settings::{no_store_headers, parse_if_match},
    storage::{
        content::{self as content_storage, ContentTypeApiItem},
        entries as entry_storage,
        page::active_content_templates,
    },
};

pub const MAX_CONTENT_TYPE_JSON_BYTES: usize = 512 * 1024;

pub async fn content_type_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<ContentTypeApiItem>>> {
    require_session(&headers, &db).await?;
    Ok(Json(
        content_storage::list_content_types_with_meta(&db).await?,
    ))
}

pub async fn content_type_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(content_registry): State<
        Arc<tokio::sync::RwLock<RegistryCatalog<ContentTypeDefinition>>>,
    >,
    headers: HeaderMap,
    Json(input): Json<ContentTypeDefinitionInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let result = content_storage::insert_user_content_type(&db, &input).await?;
    *content_registry.write().await = content_storage::load_content_type_catalog(&db).await?;
    Ok((
        StatusCode::CREATED,
        no_store_headers(Some(result.version)),
        Json(result),
    ))
}

pub async fn content_type_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(content_registry): State<
        Arc<tokio::sync::RwLock<RegistryCatalog<ContentTypeDefinition>>>,
    >,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(input): Json<ContentTypeDefinitionInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&type_key) {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let expected_version = parse_if_match(&headers)?;
    let result =
        content_storage::update_user_content_type(&db, &type_key, expected_version, &input).await?;
    *content_registry.write().await = content_storage::load_content_type_catalog(&db).await?;
    Ok((no_store_headers(Some(result.version)), Json(result)))
}

/// Publication policy is persisted per CPT; one CAS endpoint handles Post/Page
/// and opted-in user CPTs with the same authorization and transaction semantics.
pub async fn content_entry_publication(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<crate::routes::entries::PublicationInput>,
) -> ApiResult<impl IntoResponse> {
    require_session_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&type_key) || type_key == "pattern" {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let expected_version = parse_if_match(&headers)?;
    let entry =
        entry_storage::set_publication_status(&db, &type_key, id, expected_version, &input.status)
            .await?;
    Ok((no_store_headers(Some(entry.version)), Json(entry)))
}

/// Private authenticated preview never uses the published-content reader.
pub async fn content_entry_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    if !valid_content_type_key(&type_key) || type_key == "pattern" {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let effective = content_storage::load_effective_content_schema(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let mut record = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let role = match actor.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    };
    entry_storage::filter_entry_values_for_role(&mut record, &effective, role);
    let entry_locale = record.locale.clone();
    let entry_title = record.title.clone();
    let (_, selected_template) = active_content_templates(&db).await?;
    let raw = crate::routes::entries::render_admin_single_fallback_html(
        &db,
        &record,
        &effective,
        role,
        selected_template.as_deref(),
    )
    .await?;
    let (_, bcp47_tag, dir) =
        crate::storage::settings::get_locale_bcp47_and_dir(&db, Some(&entry_locale)).await?;
    let seo_view = crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, &entry_title);
    let html = content_render::compose_document_with_seo(None, &raw, None, &seo_view)?;
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

pub async fn field_group_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    let groups: Vec<FieldGroupRecord> = content_storage::list_field_groups(&db).await?;
    Ok((no_store_headers(None), Json(groups)))
}

pub async fn field_group_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(input): Json<UserFieldGroupInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let created = content_storage::create_user_field_group(&db, &input).await?;
    Ok((
        StatusCode::CREATED,
        no_store_headers(Some(created.version)),
        Json(created),
    ))
}

pub async fn field_group_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<UserFieldGroupInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let updated =
        content_storage::update_user_field_group(&db, &group_id, expected_version, &input).await?;
    Ok((no_store_headers(Some(updated.version)), Json(updated)))
}

pub async fn field_group_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let withdrawn =
        content_storage::withdraw_user_field_group(&db, &group_id, expected_version).await?;
    Ok((no_store_headers(Some(withdrawn.version)), Json(withdrawn)))
}

fn parse_if_match_allow_zero(headers: &HeaderMap) -> ApiResult<i64> {
    let raw = headers
        .get(axum::http::header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| {
            ApiError(
                StatusCode::PRECONDITION_REQUIRED,
                "Thiếu header If-Match.".into(),
            )
        })?;
    let trimmed = raw.trim_matches('"');
    let digits = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let version = digits
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("Header If-Match không hợp lệ."))?;
    if version < 0 {
        return Err(ApiError::bad_request("Header If-Match không hợp lệ."));
    }
    Ok(version)
}

pub async fn editor_layout_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    let snapshot = content_storage::get_editor_layout(&db, &type_key).await?;
    Ok((no_store_headers(Some(snapshot.version)), Json(snapshot)))
}

pub async fn editor_layout_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(input): Json<EditorLayoutConfigV1>,
) -> ApiResult<impl IntoResponse> {
    let session = require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match_allow_zero(&headers)?;
    let snapshot = content_storage::put_editor_layout(
        &db,
        &type_key,
        expected_version,
        session.user.id,
        input,
    )
    .await?;
    Ok((no_store_headers(Some(snapshot.version)), Json(snapshot)))
}

pub async fn effective_editor_config_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session(&headers, &db).await?;
    let caller_role = match session.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    };
    let projection =
        content_storage::get_effective_editor_config(&db, &type_key, caller_role).await?;
    Ok((no_store_headers(None), Json(projection)))
}
