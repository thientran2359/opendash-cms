use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    database: &'static str,
    scaffold: bool,
}

pub async fn status(
    State(database): State<SqlitePool>,
) -> Result<Json<Health>, (StatusCode, &'static str)> {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&database)
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"))?;

    Ok(Json(Health {
        status: "ok",
        database: "ready",
        scaffold: false,
    }))
}
