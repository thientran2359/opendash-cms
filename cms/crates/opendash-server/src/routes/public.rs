use std::sync::Arc;

use axum::{
    Extension,
    extract::{RawQuery, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    cache::ExtensionRenderCache,
    content_render,
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    public_context::validate_single_query,
    routes::{
        public_content::{PublicRenderServices, extract_origin_host},
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
) -> ApiResult<Response> {
    validate_single_query(raw_query).map_err(ApiError::bad_request)?;
    match seo::reading_page_state_for_locale(db, Some(locale), true).await? {
        ReadingPageState::Published(page_id) => {
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
            super::public_content::render_not_found_response_for_locale(
                db,
                themes_root,
                Some(locale),
                services,
            )
            .await
        }
        ReadingPageState::Unconfigured => {
            // No implicit Page selection or migration of the old Builder layout.
            let mut view =
                seo::build_home_seo_view_for_locale(db, Some(locale), &canonical_path, false)
                    .await?;
            view.robots = "noindex,nofollow".to_owned();
            view.alternates.clear();
            let l10n = content_render::core_l10n(locale);
            let mut html = content_render::compose_document_with_seo(
                None,
                &format!("<main><p>{}</p></main>", l10n.home_unconfigured),
                None,
                &view,
            )?;
            if let Some(svc) = services {
                let ctx = crate::hooks::RenderHookContext {
                    route_kind: "home",
                    route_path: &canonical_path,
                    canonical_query: "",
                    is_not_found: false,
                    locale,
                    origin_host: &svc.origin_host,
                    site: serde_json::json!({}),
                    options: serde_json::json!({}),
                    entry: None,
                    entries: None,
                    principal: opendash_core::hooks::ExecutionPrincipal::Public,
                };
                let rendered = svc
                    .hooks
                    .dispatch_render_hooks(&ctx, &[], &svc.render_cache)
                    .await;
                html = content_render::apply_render_hooks_to_document(
                    html,
                    &rendered.head_html,
                    &rendered.head_styles,
                    &rendered.head_scripts,
                    &rendered.footer_html,
                    &rendered.slot_replacements,
                )?;
            } else {
                html = html.replace(content_render::HEAD_END_MARKER, "");
            }
            Ok(rendered_layout(html))
        }
    }
}

pub async fn public_page(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    State(render_cache): State<Arc<ExtensionRenderCache>>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    let default_locale = settings::get_default_locale_code(&db).await?;
    let services = PublicRenderServices {
        hooks,
        render_cache,
        origin_host: extract_origin_host(&headers),
    };
    render_home_for_locale(
        &db,
        themes_root,
        &default_locale,
        "/".to_owned(),
        raw.as_deref(),
        Some(&services),
    )
    .await
}

pub fn rendered_layout_with_form_csp(html: String, allow_form_self: bool) -> Response {
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
