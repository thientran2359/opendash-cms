use std::{net::SocketAddr, sync::Arc};

use axum::{
    Extension,
    extract::{ConnectInfo, RawQuery, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    cache::ExtensionRenderCache,
    content_render,
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    public_context::{
        PublicFilters, QueryScope, has_search_param, parse_query, validate_percent_encoding,
        validate_single_query,
    },
    routes::{
        public_content::{PublicRenderServices, extract_origin_host},
        search::{SearchRateLimiter, peer_rate_limit_key},
        themes::ThemeStoreRoot,
    },
    storage::{
        public_content,
        seo::{self, ReadingPageState},
        settings,
    },
};

pub(crate) async fn render_home_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    locale: &str,
    canonical_path: String,
    raw_query: Option<&str>,
    services: Option<&PublicRenderServices>,
    search_limiter: Option<&SearchRateLimiter>,
    peer_key: &str,
) -> ApiResult<Response> {
    validate_percent_encoding(raw_query).map_err(ApiError::bad_request)?;
    if has_search_param(raw_query) {
        return super::public_content::render_search_response_for_locale(
            db,
            themes_root,
            locale,
            raw_query,
            services,
            search_limiter,
            peer_key,
        )
        .await;
    }
    match seo::reading_page_state_for_locale(db, Some(locale), true).await? {
        ReadingPageState::Published(page_id) => {
            validate_single_query(raw_query).map_err(ApiError::bad_request)?;
            // Re-check publication in the content snapshot. A stale Reading
            // assignment must never expose a draft/private Page.
            let Some(page) = public_content::get_published_by_id(db, page_id).await? else {
                return super::public_content::render_not_found_response_for_locale(
                    db,
                    themes_root,
                    Some(locale),
                    services,
                )
                .await;
            };
            if page.content_type_key != "page" {
                return super::public_content::render_not_found_response_for_locale(
                    db,
                    themes_root,
                    Some(locale),
                    services,
                )
                .await;
            }
            super::public_content::render_homepage_entry_response_for_locale(
                db,
                themes_root,
                page,
                canonical_path,
                raw_query,
                services,
            )
            .await
        }
        ReadingPageState::Missing => {
            validate_single_query(raw_query).map_err(ApiError::bad_request)?;
            super::public_content::render_not_found_response_for_locale(
                db,
                themes_root,
                Some(locale),
                services,
            )
            .await
        }
        ReadingPageState::Unconfigured => {
            let query = parse_query(raw_query, QueryScope::ContentArchive)
                .map_err(ApiError::bad_request)?;
            if query.filters != PublicFilters::default() {
                return Err(ApiError::bad_request(
                    "Trang chủ không hỗ trợ bộ lọc taxonomy.",
                ));
            }
            super::public_content::render_index_home_response_for_locale(
                db,
                themes_root,
                locale,
                canonical_path,
                query,
                raw_query,
                services,
            )
            .await
        }
    }
}

pub async fn public_page(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    State(render_cache): State<Arc<ExtensionRenderCache>>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    search_limiter: Option<Extension<SearchRateLimiter>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    let default_locale = settings::get_default_locale_code(&db).await?;
    let services = PublicRenderServices {
        hooks,
        render_cache,
        origin_host: extract_origin_host(&headers),
    };
    let peer_key = peer_rate_limit_key(peer.as_ref());
    render_home_for_locale(
        &db,
        themes_root,
        &default_locale,
        "/".to_owned(),
        raw.as_deref(),
        Some(&services),
        search_limiter.as_ref().map(|Extension(l)| l),
        &peer_key,
    )
    .await
}

pub fn rendered_layout_with_form_csp(html: String, allow_form_self: bool) -> Response {
    let allow_form_self = allow_form_self || crate::document::contains_html_form_tag(&html);
    let has_video = html.contains("<video ");
    let csp = match (allow_form_self, has_video) {
        (true, true) => {
            "default-src 'none'; img-src 'self'; media-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
        }
        (true, false) => {
            "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
        }
        (false, true) => {
            "default-src 'none'; img-src 'self'; media-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        }
        (false, false) => {
            "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        }
    };
    let referrer_policy = if allow_form_self {
        "same-origin"
    } else {
        "no-referrer"
    };
    (
        [
            (header::CONTENT_SECURITY_POLICY, csp),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, referrer_policy),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(html),
    )
        .into_response()
}

pub fn rendered_layout(html: String) -> Response {
    rendered_layout_with_form_csp(html, false)
}
