use axum::{Router, routing::get};
use sqlx::SqlitePool;

use crate::routes;

pub fn router(database: SqlitePool) -> Router {
    Router::new()
        .route("/", get(routes::public::home))
        .route("/api/health", get(routes::health::status))
        .route("/admin", get(routes::admin::index))
        .route("/admin/", get(routes::admin::index))
        .route("/builder", get(routes::admin::index))
        .route("/admin/assets/{*path}", get(routes::admin::asset))
        .with_state(database)
}
