//! M7-S03-T01 admin entry route handlers and public custom-path / redirect fallback.
use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, Extension, OriginalUri, Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::{
    content::valid_content_type_key,
    redirects::{
        MAX_NOT_FOUND_PATH_BYTES, MAX_REDIRECT_SOURCE_PATH_BYTES, build_redirect_location,
        decode_safe_legacy_request_path,
    },
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{
        Role, require_admin, require_admin_with_csrf, require_session, require_session_with_csrf,
    },
    cache::ExtensionRenderCache,
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    public_context::{
        CanonicalUrlBuilder, QueryScope, parse_query, validate_public_request_path,
        validate_single_query,
    },
    redirects::RedirectTelemetryBuffer,
    routes::{
        public_content::{
            PublicRenderServices, extract_origin_host, redirect_canonical_taxonomy,
            render_canonical_archive, render_canonical_taxonomy,
            render_single_entry_response_with_query,
        },
        settings::{no_store_headers, parse_if_match},
        themes::ThemeStoreRoot,
    },
    storage::{
        permalinks::{self, PublicRouteTarget},
        public_content,
        redirects::find_active_manual_redirect,
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
    if !valid_content_type_key(&type_key) || type_key == "pattern" {
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
    State(hooks): State<Arc<HookDispatcher>>,
    State(render_cache): State<Arc<ExtensionRenderCache>>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    search_limiter: Option<Extension<crate::routes::search::SearchRateLimiter>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
) -> ApiResult<Response> {
    if method != Method::GET && method != Method::HEAD {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let services = PublicRenderServices {
        hooks,
        render_cache,
        origin_host: extract_origin_host(&headers),
    };
    let path = uri.path();
    if !path.starts_with('/') || path.contains("//") || path.contains('\\') {
        return Err(ApiError::bad_request("Đường dẫn công khai không hợp lệ."));
    }
    let first_seg = path[1..].split('/').next().unwrap_or("");
    if !first_seg.is_empty() && crate::seo::is_reserved_system_root(&first_seg.to_ascii_lowercase())
    {
        return missing_public_page(&db, themes_root, None, Some(&services)).await;
    }
    let had_trailing_slash = path.len() > 1 && path.ends_with('/');
    let lookup_path = if had_trailing_slash {
        &path[..path.len() - 1]
    } else {
        path
    };
    let raw = uri.query();
    let referer = headers.get(header::REFERER).and_then(|v| v.to_str().ok());
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    if lookup_path.contains('%') {
        let decoded_path = decode_safe_legacy_request_path(lookup_path)
            .map_err(|_| ApiError::bad_request("Đường dẫn công khai không hợp lệ."))?;
        let decoded_first_seg = decoded_path[1..].split('/').next().unwrap_or("");
        if !decoded_first_seg.is_empty()
            && crate::seo::is_reserved_system_root(&decoded_first_seg.to_ascii_lowercase())
        {
            return missing_public_page(&db, themes_root, None, Some(&services)).await;
        }
        crate::public_context::validate_percent_encoding(raw).map_err(ApiError::bad_request)?;
        if decoded_path != "/" && decoded_path.len() <= MAX_REDIRECT_SOURCE_PATH_BYTES {
            if let Some(hit) = find_active_manual_redirect(&db, &decoded_path)
                .await
                .map_err(|err| {
                    eprintln!("manual redirect lookup failed: {err:?}");
                    ApiError::internal("Không truy cập được dữ liệu.")
                })?
            {
                if method == Method::GET
                    && let Some(Extension(buf)) = &telemetry
                {
                    buf.record_redirect_hit(hit.id);
                }
                let location = build_redirect_location(&hit.target_url, raw);
                return Ok(manual_public_redirect(hit.status_code, location));
            }
        }
        let inferred_locale =
            crate::storage::settings::enabled_secondary_locale_for_segment(&db, decoded_first_seg)
                .await?;
        let response = missing_public_page(
            &db,
            themes_root,
            inferred_locale.as_deref(),
            Some(&services),
        )
        .await?;
        if method == Method::GET
            && decoded_path != "/"
            && decoded_path.len() <= MAX_NOT_FOUND_PATH_BYTES
            && let Some(Extension(buf)) = &telemetry
        {
            buf.record_404(&decoded_path, referer, user_agent);
        }
        return Ok(response);
    }

    validate_public_request_path(lookup_path).map_err(ApiError::bad_request)?;
    crate::public_context::validate_percent_encoding(raw).map_err(ApiError::bad_request)?;
    let resolved = permalinks::resolve_public_path(&db, lookup_path).await?;
    let peer_key = crate::routes::search::peer_rate_limit_key(peer.as_ref());
    if resolved.is_none()
        && !first_seg.is_empty()
        && !lookup_path[1..].contains('/')
        && crate::public_context::has_search_param(raw)
        && let Some(locale) =
            crate::storage::settings::enabled_secondary_locale_for_segment(&db, first_seg).await?
    {
        let sq = crate::public_context::parse_search_query(raw).map_err(ApiError::bad_request)?;
        if had_trailing_slash {
            let target = CanonicalUrlBuilder::search_url(
                &format!("/{locale}"),
                Some(&sq.query),
                sq.content_type.as_deref(),
                sq.page,
            );
            return Ok(permanent_public_redirect(target));
        }
        return crate::routes::public_content::render_search_response_for_locale(
            &db,
            themes_root,
            &locale,
            raw,
            Some(&services),
            search_limiter.as_ref().map(|Extension(l)| l),
            &peer_key,
        )
        .await;
    }
    if resolved.as_ref().is_none_or(|r| r.redirect)
        && lookup_path != "/"
        && lookup_path.len() <= MAX_REDIRECT_SOURCE_PATH_BYTES
    {
        if let Some(hit) = find_active_manual_redirect(&db, lookup_path)
            .await
            .map_err(|err| {
                eprintln!("manual redirect lookup failed: {err:?}");
                ApiError::internal("Không truy cập được dữ liệu.")
            })?
        {
            if method == Method::GET
                && let Some(Extension(buf)) = &telemetry
            {
                buf.record_redirect_hit(hit.id);
            }
            let location = build_redirect_location(&hit.target_url, raw);
            return Ok(manual_public_redirect(hit.status_code, location));
        }
    }

    let Some(resolved) = resolved else {
        let inferred_locale =
            crate::storage::settings::enabled_secondary_locale_for_segment(&db, first_seg).await?;
        let response = missing_public_page(
            &db,
            themes_root,
            inferred_locale.as_deref(),
            Some(&services),
        )
        .await?;
        let is_secondary_root = inferred_locale
            .as_deref()
            .is_some_and(|loc| lookup_path == format!("/{loc}"));
        if method == Method::GET
            && !is_secondary_root
            && lookup_path != "/"
            && lookup_path.len() <= MAX_NOT_FOUND_PATH_BYTES
            && let Some(Extension(buf)) = &telemetry
        {
            buf.record_404(lookup_path, referer, user_agent);
        }
        return Ok(response);
    };
    match resolved.target {
        PublicRouteTarget::Entry {
            id,
            locale,
            is_home,
        } => {
            if is_home && crate::public_context::has_search_param(raw) {
                let sq = crate::public_context::parse_search_query(raw)
                    .map_err(ApiError::bad_request)?;
                if resolved.redirect || had_trailing_slash {
                    let target = CanonicalUrlBuilder::search_url(
                        &resolved.canonical_path,
                        Some(&sq.query),
                        sq.content_type.as_deref(),
                        sq.page,
                    );
                    return Ok(permanent_public_redirect(target));
                }
                return crate::routes::public_content::render_search_response_for_locale(
                    &db,
                    themes_root,
                    &locale,
                    raw,
                    Some(&services),
                    search_limiter.as_ref().map(|Extension(l)| l),
                    &peer_key,
                )
                .await;
            }
            validate_single_query(raw).map_err(ApiError::bad_request)?;
            let Some(record) = public_content::get_published_by_id(&db, id).await? else {
                let response =
                    missing_public_page(&db, themes_root, Some(&locale), Some(&services)).await?;
                if !is_home
                    && method == Method::GET
                    && lookup_path != "/"
                    && lookup_path.len() <= MAX_NOT_FOUND_PATH_BYTES
                    && let Some(Extension(buf)) = &telemetry
                {
                    buf.record_404(lookup_path, referer, user_agent);
                }
                return Ok(response);
            };
            if resolved.redirect || had_trailing_slash {
                return Ok(permanent_public_redirect(resolved.canonical_path));
            }
            if is_home {
                crate::routes::public::render_home_for_locale(
                    &db,
                    themes_root,
                    &locale,
                    resolved.canonical_path,
                    raw,
                    Some(&services),
                    search_limiter.as_ref().map(|Extension(l)| l),
                    &peer_key,
                )
                .await
            } else {
                render_single_entry_response_with_query(
                    &db,
                    themes_root,
                    record,
                    Some(resolved.canonical_path),
                    raw,
                    Some(&services),
                )
                .await
            }
        }
        PublicRouteTarget::Archive { type_key, locale } => {
            let query =
                parse_query(raw, QueryScope::ContentArchive).map_err(ApiError::bad_request)?;
            if type_key == "post"
                && matches!(
                    seo::reading_page_state_for_locale(&db, Some(&locale), false).await?,
                    ReadingPageState::Missing
                )
            {
                let response =
                    missing_public_page(&db, themes_root, Some(&locale), Some(&services)).await?;
                if method == Method::GET
                    && lookup_path != "/"
                    && lookup_path.len() <= MAX_NOT_FOUND_PATH_BYTES
                    && let Some(Extension(buf)) = &telemetry
                {
                    buf.record_404(lookup_path, referer, user_agent);
                }
                return Ok(response);
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
                locale,
                resolved.canonical_path,
                raw.map(str::to_owned),
                Some(services),
            )
            .await
        }
        PublicRouteTarget::Term {
            id: term_id,
            locale,
        } => {
            if resolved.redirect || had_trailing_slash {
                return redirect_canonical_taxonomy(&db, term_id, resolved.canonical_path, raw)
                    .await;
            }
            render_canonical_taxonomy(
                &db,
                themes_root.map(|Extension(root)| root),
                term_id,
                locale,
                resolved.canonical_path,
                raw,
                None,
                Some(&services),
            )
            .await
        }
        PublicRouteTarget::Home { locale } => {
            if crate::public_context::has_search_param(raw) {
                let sq = crate::public_context::parse_search_query(raw)
                    .map_err(ApiError::bad_request)?;
                if resolved.redirect || had_trailing_slash {
                    let target = CanonicalUrlBuilder::search_url(
                        &resolved.canonical_path,
                        Some(&sq.query),
                        sq.content_type.as_deref(),
                        sq.page,
                    );
                    return Ok(permanent_public_redirect(target));
                }
                return crate::routes::public_content::render_search_response_for_locale(
                    &db,
                    themes_root,
                    &locale,
                    raw,
                    Some(&services),
                    search_limiter.as_ref().map(|Extension(l)| l),
                    &peer_key,
                )
                .await;
            }
            if resolved.redirect || had_trailing_slash {
                if matches!(
                    crate::storage::seo::reading_page_state_for_locale(&db, Some(&locale), true)
                        .await?,
                    crate::storage::seo::ReadingPageState::Unconfigured
                ) {
                    let query = parse_query(raw, QueryScope::ContentArchive)
                        .map_err(ApiError::bad_request)?;
                    if query.filters != crate::public_context::PublicFilters::default() {
                        return Err(ApiError::bad_request(
                            "Trang chủ không hỗ trợ bộ lọc taxonomy.",
                        ));
                    }
                    let target = CanonicalUrlBuilder::with_query(
                        resolved.canonical_path,
                        query,
                        QueryScope::ContentArchive,
                    );
                    return Ok(permanent_public_redirect(target));
                }
                validate_single_query(raw).map_err(ApiError::bad_request)?;
                return Ok(permanent_public_redirect(resolved.canonical_path));
            }
            crate::routes::public::render_home_for_locale(
                &db,
                themes_root,
                &locale,
                resolved.canonical_path,
                raw,
                Some(&services),
                search_limiter.as_ref().map(|Extension(l)| l),
                &peer_key,
            )
            .await
        }
    }
}

fn manual_public_redirect(status_code: u16, location: String) -> Response {
    let status = if status_code == 302 {
        StatusCode::FOUND
    } else {
        StatusCode::MOVED_PERMANENTLY
    };
    (
        status,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        "",
    )
        .into_response()
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
    locale: Option<&str>,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    crate::routes::public_content::render_not_found_response_for_locale(
        db,
        themes_root,
        locale,
        services,
    )
    .await
}

pub async fn get_entry_seo(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<impl IntoResponse> {
    let session = require_session(&headers, &db).await?;
    require_editor_or_admin(session.user.role)?;
    if !valid_content_type_key(&type_key) || type_key == "pattern" {
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
    if !valid_content_type_key(&type_key) || type_key == "pattern" {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let expected_version = parse_if_match(&headers)?;
    let snapshot =
        seo::put_entry_seo_for_actor(&db, &type_key, id, expected_version, input, Some(&session))
            .await?;
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
