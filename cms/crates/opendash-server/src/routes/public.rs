use axum::{
    extract::State,
    http::header,
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{error::ApiResult, page::render_saved, storage::theme::public_home};

pub async fn public_page(State(db): State<SqlitePool>) -> ApiResult<Response> {
    let html = match public_home(&db).await? {
        Some(snapshot) => render_saved(snapshot)?.html,
        None => "<p>Chưa lưu block.</p>".into(),
    };
    Ok(rendered(html))
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
