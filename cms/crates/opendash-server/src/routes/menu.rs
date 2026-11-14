//! Admin-only M7-S02-T01 menu CRUD, preview, and slot assignment routes.
use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::{ApiError, ApiResult},
    routes::settings::{no_store_headers, parse_if_match},
    storage::menu::{self, MenuItemInput, MenuSlotsSnapshot, MenuWithItems},
};

pub const MAX_MENU_JSON_BYTES: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
pub struct ListMenusQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMenuBody {
    pub menu_key: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMenuBody {
    pub title: String,
    pub items: Vec<MenuItemInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewMenuBody {
    pub title: Option<String>,
    pub items: Vec<MenuItemInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutMenuSlotsBody {
    pub assignments: BTreeMap<String, Option<i64>>,
}

fn map_json_rejection(err: JsonRejection, default_msg: &'static str) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Dữ liệu menu vượt quá giới hạn 256 KiB.".into(),
        )
    } else {
        ApiError::bad_request(default_msg)
    }
}

fn menu_response(status: StatusCode, data: MenuWithItems) -> Response {
    (
        status,
        no_store_headers(Some(data.menu.version)),
        Json(data),
    )
        .into_response()
}

fn slots_response(data: MenuSlotsSnapshot) -> Response {
    (no_store_headers(Some(data.version)), Json(data)).into_response()
}

pub async fn menus_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<ListMenusQuery>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    let page = menu::list_menus(&db, limit, offset).await?;
    Ok((no_store_headers(None), Json(page)).into_response())
}

pub async fn menus_create(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    input: Result<Json<CreateMenuBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let Json(body) = input.map_err(|e| map_json_rejection(e, "Dữ liệu tạo menu không hợp lệ."))?;
    let created = menu::create_menu_admin(&db, &body.menu_key, &body.title).await?;
    Ok(menu_response(StatusCode::CREATED, created))
}

pub async fn menus_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let found = menu::get_menu(&db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy menu."))?;
    Ok(menu_response(StatusCode::OK, found))
}

pub async fn menus_put(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    input: Result<Json<UpdateMenuBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(body) =
        input.map_err(|e| map_json_rejection(e, "Dữ liệu cập nhật menu không hợp lệ."))?;
    let updated = menu::update_menu_with_title(&db, id, version, &body.title, &body.items).await?;
    Ok(menu_response(StatusCode::OK, updated))
}

pub async fn menus_delete(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    menu::delete_menu(&db, id, version).await?;
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)).into_response())
}

pub async fn menus_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    input: Result<Json<PreviewMenuBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let Json(body) =
        input.map_err(|e| map_json_rejection(e, "Dữ liệu preview menu không hợp lệ."))?;
    let view = menu::preview_menu(&db, id, body.title.as_deref(), &body.items)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy menu."))?;
    Ok((no_store_headers(None), Json(view)).into_response())
}

pub async fn menu_slots_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(theme_id): Path<String>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let snapshot = menu::get_menu_slots(&db, &theme_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy theme."))?;
    Ok(slots_response(snapshot))
}

pub async fn menu_slots_put(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(theme_id): Path<String>,
    input: Result<Json<PutMenuSlotsBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(body) =
        input.map_err(|e| map_json_rejection(e, "Dữ liệu gắn slot menu không hợp lệ."))?;
    let updated = menu::put_menu_slots(&db, &theme_id, version, &body.assignments).await?;
    Ok(slots_response(updated))
}
