//! M7-S03-T01 admin entry route handlers and public custom-path / redirect fallback.
use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::content::valid_content_type_key;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{Role, require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    routes::{
        public_content::render_single_entry_response,
        settings::{no_store_headers, parse_if_match},
        themes::ThemeStoreRoot,
    },
    storage::seo::{self, EntryRouteSnapshot, PublicRouteResolution},
};

pub const MAX_ROUTE_JSON_BYTES: usize = 8192;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryRouteUpdateInput {
    pub custom_path: Option<String>,
}

fn require_editor_or_admin(role: Role) -> ApiResult<()> {
    if matches!(role, Role::Admin | Role::Editor) {
        Ok(())
    } else {
        Err(ApiError::forbidden("Cần quyền Editor hoặc Admin."))
    }
}

pub async fn get_entry_route(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session(&headers, &db).await?;
    require_editor_or_admin(session.user.role)?;
    if !valid_content_type_key(&type_key) {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let snapshot: EntryRouteSnapshot = seo::get_entry_route(&db, &type_key, id).await?;
    Ok((no_store_headers(Some(snapshot.entry_version)), Json(snapshot)))
}

pub async fn put_entry_route(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<EntryRouteUpdateInput>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session_with_csrf(&headers, &db).await?;
    require_editor_or_admin(session.user.role)?;
    if !valid_content_type_key(&type_key) {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let expected_version = parse_if_match(&headers)?;
    let snapshot = seo::put_entry_route(
        &db,
        &type_key,
        id,
        expected_version,
        input.custom_path.as_deref(),
    )
    .await?;
    Ok((no_store_headers(Some(snapshot.entry_version)), Json(snapshot)))
}

pub async fn public_fallback(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    method: Method,
    OriginalUri(uri): OriginalUri,
) -> ApiResult<Response> {
    if method != Method::GET && method != Method::HEAD {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let path = uri.path();
    match seo::resolve_custom_or_redirect_path(&db, path).await? {
        PublicRouteResolution::Entry {
            record,
            canonical_path,
        } => render_single_entry_response(&db, themes_root, record, Some(canonical_path)).await,
        PublicRouteResolution::Redirect { location } => Ok((
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, location)],
            "",
        )
            .into_response()),
        PublicRouteResolution::NotFound => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}
