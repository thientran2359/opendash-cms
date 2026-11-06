use axum::{
    extract::State,
    http::header,
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{error::ApiResult, routes::builder::render_block, storage::page::load_home_page};

pub async fn public_page(State(db): State<SqlitePool>) -> ApiResult<Response> {
    let html = match load_home_page(&db).await? {
        Some(block) => render_block(&db, &block).await?,
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
