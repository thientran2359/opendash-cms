use axum::{Extension, Json, extract::State, http::StatusCode};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::storage::check_db_ready;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    database: &'static str,
    scaffold: bool,
    safe_mode: bool,
}

pub async fn status(
    State(database): State<SqlitePool>,
    recovery: Option<Extension<crate::config::RecoveryState>>,
) -> Result<Json<Health>, (StatusCode, &'static str)> {
    check_db_ready(&database)
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"))?;

    Ok(Json(Health {
        status: "ok",
        database: "ready",
        scaffold: false,
        safe_mode: recovery.is_some_and(|Extension(r)| r.is_safe_mode()),
    }))
}
