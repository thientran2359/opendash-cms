use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::storage::check_db_ready;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    database: &'static str,
    scaffold: bool,
}

pub async fn status(
    State(database): State<SqlitePool>,
) -> Result<Json<Health>, (StatusCode, &'static str)> {
    check_db_ready(&database)
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"))?;

    Ok(Json(Health {
        status: "ok",
        database: "ready",
        scaffold: false,
    }))
}
