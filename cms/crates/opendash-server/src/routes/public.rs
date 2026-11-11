use axum::{
    extract::State,
    http::header,
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    error::ApiResult,
    page::{render_builder_resolved, render_legacy_state},
    storage::{builder, builder_binding},
};

pub async fn public_page(State(db): State<SqlitePool>) -> ApiResult<Response> {
    // One SQLite read snapshot for source, document, active theme and block
    // templates. Do not split them across old / and new Builder reads.
    let state = builder::read(&db).await?;
    if let Some(document) = &state.saved {
        let bindings = builder_binding::resolve(&db, document, false).await?;
        return Ok(rendered_layout(
            render_builder_resolved(&state, document, false, &bindings)?.html,
        ));
    }
    let html = match render_legacy_state(&state)? {
        Some(snapshot) => snapshot.html,
        None => "<p>Chưa lưu block.</p>".into(),
    };
    Ok(rendered(html))
}

pub fn rendered_layout(html: String) -> Response {
    (
        [
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(html),
    )
        .into_response()
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
