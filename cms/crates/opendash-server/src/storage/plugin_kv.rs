//! SQL primitives borrowed by the worker supervisor. No task spawning or timeout
//! policy here: the caller retains its deadline, cancellation and connection guard.
use opendash_core::hooks::ExtensionOwnerKind;
use sqlx::{SqliteConnection, SqlitePool, sqlite::SqliteQueryResult};

pub async fn get(
    db: &SqlitePool,
    plugin_id: &str,
    key: &str,
) -> Result<Option<String>, sqlx::Error> {
    get_for_owner(db, ExtensionOwnerKind::Plugin, plugin_id, key).await
}

pub async fn get_for_owner(
    db: &SqlitePool,
    owner_kind: ExtensionOwnerKind,
    owner_id: &str,
    key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE owner_kind = ? AND plugin_id = ? AND key = ?",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
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

#[allow(dead_code)]
pub async fn set(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    key: &str,
    value: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    set_for_owner(conn, ExtensionOwnerKind::Plugin, plugin_id, key, value).await
}

pub async fn set_for_owner(
    conn: &mut SqliteConnection,
    owner_kind: ExtensionOwnerKind,
    owner_id: &str,
    key: &str,
    value: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO plugin_kv (owner_kind, plugin_id, key, value_json, updated_at)
         VALUES (?, ?, ?, ?, datetime('now'))
         ON CONFLICT(owner_kind, plugin_id, key) DO UPDATE SET
             value_json = excluded.value_json,
             updated_at = excluded.updated_at",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
    .bind(key)
    .bind(value)
    .execute(conn)
    .await
}

#[allow(dead_code)]
pub async fn delete(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    key: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    delete_for_owner(conn, ExtensionOwnerKind::Plugin, plugin_id, key).await
}

pub async fn delete_for_owner(
    conn: &mut SqliteConnection,
    owner_kind: ExtensionOwnerKind,
    owner_id: &str,
    key: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query("DELETE FROM plugin_kv WHERE owner_kind = ? AND plugin_id = ? AND key = ?")
        .bind(owner_kind.as_str())
        .bind(owner_id)
        .bind(key)
        .execute(conn)
        .await
}

#[allow(dead_code)]
pub async fn list(
    db: &SqlitePool,
    plugin_id: &str,
    prefix: &str,
    limit: i64,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    list_for_owner(db, ExtensionOwnerKind::Plugin, plugin_id, prefix, limit).await
}

pub async fn list_for_owner(
    db: &SqlitePool,
    owner_kind: ExtensionOwnerKind,
    owner_id: &str,
    prefix: &str,
    limit: i64,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let clamped = limit.clamp(1, 200);
    if prefix.is_empty() {
        sqlx::query_as(
            "SELECT key, value_json FROM plugin_kv
              WHERE owner_kind = ? AND plugin_id = ?
              ORDER BY key COLLATE BINARY ASC LIMIT ?",
        )
        .bind(owner_kind.as_str())
        .bind(owner_id)
        .bind(clamped)
        .fetch_all(db)
        .await
    } else {
        sqlx::query_as(
            "SELECT key, value_json FROM plugin_kv
              WHERE owner_kind = ? AND plugin_id = ? AND substr(key, 1, length(?)) = ?
              ORDER BY key COLLATE BINARY ASC LIMIT ?",
        )
        .bind(owner_kind.as_str())
        .bind(owner_id)
        .bind(prefix)
        .bind(prefix)
        .bind(clamped)
        .fetch_all(db)
        .await
    }
}
