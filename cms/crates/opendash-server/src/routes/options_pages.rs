//! Admin REST endpoints for Custom Options Pages and their multilingual values.

use std::collections::BTreeMap;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use opendash_core::content::{CustomOptionsPageDefinition, CustomOptionsPageInput, FieldRole};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::{
    auth::{Role, require_admin_with_csrf, require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    routes::settings::{no_store_headers, parse_if_match},
    storage::options_pages::{self, OptionsPageValuesSnapshot},
};

pub const MAX_OPTIONS_PAGE_CONFIG_JSON_BYTES: usize = 16 * 1024;
pub const MAX_OPTIONS_PAGE_VALUES_JSON_BYTES: usize = 256 * 1024;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DeleteOptionsPageQuery {
    #[serde(default)]
    pub purge: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct OptionsPageLocaleQuery {
    pub locale: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutOptionsPageValuesInput {
    pub values: BTreeMap<String, Value>,
    #[serde(default, alias = "schema_token")]
    pub expected_schema_token: Option<String>,
}

#[derive(Serialize)]
struct OptionsPageValuesResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<CustomOptionsPageDefinition>,
    is_default_locale: bool,
    #[serde(flatten)]
    snapshot: OptionsPageValuesSnapshot,
}

fn actor_role(role: Role) -> FieldRole {
    match role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    }
}

fn resolve_page_ref(raw: &str) -> String {
    if raw == "site_global" || raw.starts_with("theme:") || raw.starts_with("custom:") {
        raw.to_owned()
    } else {
        format!("custom:{raw}")
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(
    payload: Result<Json<T>, JsonRejection>,
) -> ApiResult<T> {
    payload
        .map(|Json(v)| v)
        .map_err(|_| ApiError::bad_request("Dữ liệu JSON trang tùy chọn không hợp lệ."))
}

pub async fn options_pages_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let session = require_session(&headers, &db).await?;
    let items =
        options_pages::list_visible_options_pages(&db, actor_role(session.user.role)).await?;
    Ok((no_store_headers(None), Json(items)))
}

pub async fn options_page_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    payload: Result<Json<CustomOptionsPageInput>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let created = options_pages::create_user_options_page(&db, &input).await?;
    Ok((
        StatusCode::CREATED,
        no_store_headers(Some(created.version)),
        Json(created),
    ))
}

pub async fn options_page_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(page_key): Path<String>,
    payload: Result<Json<CustomOptionsPageInput>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let input = parse_body(payload)?;
    let updated =
        options_pages::update_user_options_page(&db, &page_key, expected_version, &input).await?;
    Ok((no_store_headers(Some(updated.version)), Json(updated)))
}

pub async fn options_page_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(page_key): Path<String>,
    Query(query): Query<DeleteOptionsPageQuery>,
    body: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    if !body.is_empty() {
        return Err(ApiError::bad_request(
            "Body của DELETE options-page phải rỗng.",
        ));
    }
    let expected_version = parse_if_match(&headers)?;
    options_pages::delete_user_options_page(&db, &page_key, expected_version, query.purge).await?;
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)).into_response())
}

async fn build_values_response(
    db: &SqlitePool,
    page_ref: &str,
    snapshot: OptionsPageValuesSnapshot,
) -> ApiResult<OptionsPageValuesResponse> {
    let page = match page_ref.strip_prefix("custom:") {
        Some(key) => options_pages::get_options_page(db, key).await?,
        None => None,
    };
    let is_default_locale = snapshot.locale == snapshot.default_locale;
    Ok(OptionsPageValuesResponse {
        page,
        is_default_locale,
        snapshot,
    })
}

pub async fn options_page_values_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(query): Query<OptionsPageLocaleQuery>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session(&headers, &db).await?;
    let page_ref = resolve_page_ref(&key);
    let snapshot = options_pages::get_options_page_values(
        &db,
        &page_ref,
        query.locale.as_deref(),
        actor_role(session.user.role),
    )
    .await?;
    let version = snapshot.version;
    let body = build_values_response(&db, &page_ref, snapshot).await?;
    Ok((no_store_headers(Some(version)), Json(body)))
}

pub async fn options_page_values_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(query): Query<OptionsPageLocaleQuery>,
    payload: Result<Json<PutOptionsPageValuesInput>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let input = parse_body(payload)?;
    let page_ref = resolve_page_ref(&key);
    let snapshot = options_pages::put_options_page_values(
        &db,
        &page_ref,
        query.locale.as_deref(),
        expected_version,
        input.expected_schema_token.as_deref(),
        input.values,
        actor_role(session.user.role),
    )
    .await?;
    let version = snapshot.version;
    let body = build_values_response(&db, &page_ref, snapshot).await?;
    Ok((no_store_headers(Some(version)), Json(body)))
}
