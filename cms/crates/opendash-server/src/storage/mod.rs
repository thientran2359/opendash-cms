pub mod auth;
pub mod builder;
pub mod builder_binding;
pub mod content;
pub mod dashboard;
pub mod entries;
pub mod import;
pub mod media;
pub mod menu;
pub mod overrides;
pub mod page;
pub mod plugin;
pub mod plugin_kv;
pub mod public_content;
pub mod seo;
pub mod settings;
pub mod sitemap;
pub mod smtp;
pub mod taxonomy;
pub mod theme;

use std::{path::Path, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

pub async fn init_db(db_path: &Path) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    if let Some(parent) = db_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Must run before SQLx migration 0008: never overwrite an existing
    // user/theme/plugin post/page descriptor during bootstrap.
    content::preflight_builtin_key_collisions(&db).await?;
    sqlx::migrate!("../../migrations").run(&db).await?;
    Ok(db)
}

pub async fn check_db_ready(db: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(db)
        .await?;
    Ok(())
}
pub mod contact;
