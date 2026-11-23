//! M7-S03-T01 admin entry route handlers and public custom-path / redirect fallback.
use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::content::valid_content_type_key;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{
        Role, require_admin, require_admin_with_csrf, require_session, require_session_with_csrf,
    },
    error::{ApiError, ApiResult},
    public_context::{
        CanonicalUrlBuilder, QueryScope, parse_query, validate_public_request_path,
        validate_single_query,
    },
    routes::{
        public_content::{
            redirect_canonical_taxonomy, render_canonical_archive, render_canonical_taxonomy,
            render_single_entry_response,
        },
        settings::{no_store_headers, parse_if_match},
        themes::ThemeStoreRoot,
    },
    storage::{
        permalinks::{self, PublicRouteTarget},
        public_content,
        seo::{self, EntryRouteSnapshot, ReadingPageState},
    },
};

pub const MAX_SEO_JSON_BYTES: usize = 16 * 1024;

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
    Ok((
        no_store_headers(Some(snapshot.entry_version)),
        Json(snapshot),
    ))
}

pub async fn put_entry_route(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
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
    Ok((
        no_store_headers(Some(snapshot.entry_version)),
        Json(snapshot),
    ))
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
    if !path.starts_with('/') || path.contains("//") || path.contains('\\') {
        return Err(ApiError::bad_request("Đường dẫn công khai không hợp lệ."));
    }
    let first_seg = path[1..].split('/').next().unwrap_or("");
    if !first_seg.is_empty() && crate::seo::is_reserved_system_root(first_seg) {
        return missing_public_page(&db, themes_root).await;
    }
    let had_trailing_slash = path.len() > 1 && path.ends_with('/');
    let lookup_path = if had_trailing_slash {
        &path[..path.len() - 1]
    } else {
        path
    };
    validate_public_request_path(lookup_path).map_err(ApiError::bad_request)?;
    let resolved = permalinks::resolve_public_path(&db, lookup_path).await?;
    let Some(resolved) = resolved else {
        return missing_public_page(&db, themes_root).await;
    };
    let raw = uri.query();
    match resolved.target {
        PublicRouteTarget::Entry(id) => {
            validate_single_query(raw).map_err(ApiError::bad_request)?;
            let Some(record) = public_content::get_published_by_id(&db, id).await? else {
                return missing_public_page(&db, themes_root).await;
            };
            if resolved.redirect || had_trailing_slash {
                return Ok(permanent_public_redirect(resolved.canonical_path));
            }
            render_single_entry_response(&db, themes_root, record, Some(resolved.canonical_path))
                .await
        }
        PublicRouteTarget::Archive(type_key) => {
            let query =
                parse_query(raw, QueryScope::ContentArchive).map_err(ApiError::bad_request)?;
            if type_key == "post"
                && matches!(
                    seo::reading_page_state(&db, false).await?,
                    ReadingPageState::Missing
                )
            {
                return missing_public_page(&db, themes_root).await;
            }
            public_content::require_public_archive(&db, &type_key).await?;
            if resolved.redirect || had_trailing_slash {
                return Ok(permanent_public_redirect(CanonicalUrlBuilder::with_query(
                    resolved.canonical_path,
                    query,
                    QueryScope::ContentArchive,
                )));
            }
            render_canonical_archive(
                db,
                themes_root,
                type_key,
                resolved.canonical_path,
                raw.map(str::to_owned),
            )
            .await
        }
        PublicRouteTarget::Term(term_id) => {
            if resolved.redirect || had_trailing_slash {
                return redirect_canonical_taxonomy(&db, term_id, resolved.canonical_path, raw)
                    .await;
            }
            render_canonical_taxonomy(
                &db,
                themes_root.map(|Extension(root)| root),
                term_id,
                resolved.canonical_path,
                raw,
                None,
            )
            .await
        }
        PublicRouteTarget::Home => missing_public_page(&db, themes_root).await,
    }
}

fn permanent_public_redirect(location: String) -> Response {
    (
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, location)],
        "",
    )
        .into_response()
}

async fn missing_public_page(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
) -> ApiResult<Response> {
    crate::routes::public_content::render_not_found_response(db, themes_root).await
}

pub async fn get_entry_seo(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session(&headers, &db).await?;
    require_editor_or_admin(session.user.role)?;
    if !valid_content_type_key(&type_key) {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let snapshot = seo::get_entry_seo(&db, &type_key, id).await?;
    Ok((
        no_store_headers(Some(snapshot.entry_version)),
        Json(snapshot),
    ))
}

pub async fn put_entry_seo(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<crate::seo::SeoMetadataInput>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session_with_csrf(&headers, &db).await?;
    require_editor_or_admin(session.user.role)?;
    if !valid_content_type_key(&type_key) {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let expected_version = parse_if_match(&headers)?;
    let snapshot = seo::put_entry_seo(&db, &type_key, id, expected_version, input).await?;
    Ok((
        no_store_headers(Some(snapshot.entry_version)),
        Json(snapshot),
    ))
}

pub async fn get_home_seo(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<crate::storage::settings::LocaleQuery>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    let snapshot = seo::get_home_seo_for_locale(&db, query.locale.as_deref()).await?;
    Ok((no_store_headers(Some(snapshot.version)), Json(snapshot)))
}

pub async fn put_home_seo(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<crate::storage::settings::LocaleQuery>,
    Json(input): Json<crate::seo::SeoMetadataInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let snapshot =
        seo::put_home_seo_for_locale(&db, query.locale.as_deref(), expected_version, input).await?;
    Ok((no_store_headers(Some(snapshot.version)), Json(snapshot)))
}
