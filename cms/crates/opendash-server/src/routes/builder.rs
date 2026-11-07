use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::hooks::HookEvent;
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    app::AdminAssets,
    auth::{require_admin_with_csrf, require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    package::is_valid_zip_content_type,
    routes::public::rendered,
    storage::page::{
        insert_theme_block, list_theme_blocks, load_home_page, load_theme_block, save_home_page,
    },
    theme::{self, Block, Fields, Theme},
};

pub async fn builder() -> Response {
    match AdminAssets::get("index.html") {
        Some(file) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            file.data,
        )
            .into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Giao diện quản trị chưa được đóng gói.",
        )
            .into_response(),
    }
}

pub async fn serve_asset(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    let clean_path = path.trim_start_matches('/');
    let asset_path = format!("assets/{clean_path}");
    match AdminAssets::get(&asset_path) {
        Some(file) => {
            let mime = mime_guess::from_path(&asset_path).first_or_octet_stream();
            (
                [
                    (header::CONTENT_TYPE, mime.as_ref()),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                file.data,
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Asset không tồn tại.",
        )
            .into_response(),
    }
}

pub async fn registry(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<BTreeMap<String, Fields>>> {
    require_session(&headers, &db).await?;
    Ok(Json(list_theme_blocks(&db).await?))
}

pub async fn install(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<StatusCode> {
    require_admin_with_csrf(&headers, &db).await?;

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    if !is_valid_zip_content_type(content_type, true) {
        return Err(ApiError::unsupported_media_type("Cần gói application/zip."));
    }
    let theme = Theme::from_zip(&bytes).map_err(ApiError::bad_request)?;
    insert_theme_block(&db, &theme).await?;
    Ok(StatusCode::CREATED)
}

pub async fn page(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<Option<Block>>> {
    require_session(&headers, &db).await?;
    Ok(Json(load_home_page(&db).await?))
}

pub async fn preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(block): Json<Block>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    Ok(rendered(render_block(&db, &block).await?))
}

pub async fn save(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Json(block): Json<Block>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let html = render_block(&db, &block).await?;
    save_home_page(&db, &block).await?;
    hooks.emit(
        HookEvent::PageAfterSave,
        json!({ "block_type": &block.kind }),
    );
    Ok(rendered(html))
}

pub async fn render_block(db: &SqlitePool, block: &Block) -> ApiResult<String> {
    let (template, fields) = load_theme_block(db, &block.kind).await?;
    theme::render(&template, &fields, block).map_err(ApiError::bad_request)
}
