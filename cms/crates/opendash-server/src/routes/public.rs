use axum::{http::StatusCode, response::Html};

pub async fn home() -> Result<Html<String>, (StatusCode, &'static str)> {
    crate::theme::render_home().map(Html).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Template rendering failed",
        )
    })
}
