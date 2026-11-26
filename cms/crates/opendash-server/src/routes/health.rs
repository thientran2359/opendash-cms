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
    dev_mode: bool,
    dev_theme: Option<String>,
    dev_plugins: Vec<String>,
}

pub async fn status(
    State(database): State<SqlitePool>,
    recovery: Option<Extension<crate::config::RecoveryState>>,
) -> Result<Json<Health>, (StatusCode, &'static str)> {
    check_db_ready(&database)
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"))?;

    let safe_mode = recovery.is_some_and(|Extension(r)| r.is_safe_mode());
    let dev = (!safe_mode)
        .then(crate::dev_mode::DevOverlayConfig::global)
        .flatten();
    let (dev_mode, dev_theme, dev_plugins) = match dev {
        Some(cfg) => (
            true,
            cfg.dev_theme_id().map(str::to_owned),
            cfg.dev_plugin_ids(),
        ),
        None => (false, None, Vec::new()),
    };

    Ok(Json(Health {
        status: "ok",
        database: "ready",
        scaffold: false,
        safe_mode,
        dev_mode,
        dev_theme,
        dev_plugins,
    }))
}
