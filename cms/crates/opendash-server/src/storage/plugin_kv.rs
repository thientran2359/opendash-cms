//! SQL primitives borrowed by the worker supervisor. No task spawning or timeout
//! policy here: the caller retains its deadline, cancellation and connection guard.
use sqlx::{SqliteConnection, SqlitePool, sqlite::SqliteQueryResult};

pub async fn get(
    db: &SqlitePool,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT value_json FROM plugin_kv WHERE plugin_id = ? AND key = ?")
        .bind(plugin_id)
        .bind(key)
        .fetch_optional(db)
        .await
}

pub async fn busy_timeout(conn: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(conn)
        .await
}

pub async fn set_busy_timeout(
    conn: &mut SqliteConnection,
    millis: u128,
) -> Result<SqliteQueryResult, sqlx::Error> {
    // Numeric-only internal value, never user SQL. SQLite busy timeout is a signed int.
    let millis = millis.min(i32::MAX as u128);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "PRAGMA busy_timeout = {millis}"
    )))
    .execute(conn)
    .await
}

pub async fn begin_immediate(
    conn: &mut SqliteConnection,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query("BEGIN IMMEDIATE").execute(conn).await
}

pub async fn rollback(conn: &mut SqliteConnection) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query("ROLLBACK").execute(conn).await
}

pub async fn commit(conn: &mut SqliteConnection) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query("COMMIT").execute(conn).await
}

pub async fn set(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query("INSERT INTO plugin_kv (plugin_id, key, value_json, updated_at)
        VALUES (?, ?, ?, datetime('now'))
        ON CONFLICT(plugin_id, key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at")
        .bind(plugin_id).bind(key).bind(value).execute(conn).await
}
