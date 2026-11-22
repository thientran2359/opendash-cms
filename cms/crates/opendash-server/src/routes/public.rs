use axum::{
    Extension,
    extract::{RawQuery, State},
    http::header,
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    content_render,
    error::{ApiError, ApiResult},
    public_context::validate_single_query,
    routes::themes::ThemeStoreRoot,
    storage::{
        public_content,
        seo::{self, ReadingPageState},
    },
};

pub async fn public_page(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    validate_single_query(raw.as_deref()).map_err(ApiError::bad_request)?;
    match seo::reading_page_state(&db, true).await? {
        ReadingPageState::Published(page_id) => {
            // Re-check publication in the content snapshot. A stale Reading
            // assignment must never expose a draft/private Page.
            let Some(page) = public_content::get_published_by_id(&db, page_id).await? else {
                return super::public_content::render_not_found_response(&db, themes_root).await;
            };
            if page.content_type_key != "page" {
                return super::public_content::render_not_found_response(&db, themes_root).await;
            }
            super::public_content::render_homepage_entry_response(&db, themes_root, page).await
        }
        ReadingPageState::Missing => {
            super::public_content::render_not_found_response(&db, themes_root).await
        }
        ReadingPageState::Unconfigured => {
            // No implicit Page selection or migration of the old Builder layout.
            let mut view = seo::build_home_seo_view(&db).await?;
            view.robots = "noindex,nofollow".to_owned();
            let html = content_render::compose_document_with_seo(
                None,
                "<main><p>Chưa cấu hình trang chủ.</p></main>",
                None,
                &view,
            )?;
            Ok(rendered_layout(html))
        }
    }
}

pub fn rendered_layout_with_form_csp(html: String, allow_form_self: bool) -> Response {
    let has_video = html.contains("<video ");
    let csp = match (allow_form_self, has_video) {
        (true, true) => {
            "default-src 'none'; img-src 'self'; media-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
        }
        (true, false) => {
            "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
        }
        (false, true) => {
            "default-src 'none'; img-src 'self'; media-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        }
        (false, false) => {
            "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
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

pub fn rendered(html: String) -> Response {
    (
        [
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Html(html),
    )
        .into_response()
}
