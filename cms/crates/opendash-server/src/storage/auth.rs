use sqlx::SqlitePool;

use crate::{
    auth::{AuthUser, Role, SessionContext},
    error::{ApiError, ApiResult},
};

pub async fn setup_initial_admin(
    db: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> ApiResult<()> {
    let mut tx = db.begin().await?;
    let rows = sqlx::query("UPDATE bootstrap SET initialized = 1 WHERE id = 1 AND initialized = 0")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(ApiError::conflict("Hệ thống đã được thiết lập."));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *tx)
        .await?;
    if count > 0 {
        return Err(ApiError::conflict("Hệ thống đã được thiết lập."));
    }
    sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, 'admin')")
        .bind(username)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn find_user_by_username(
    db: &SqlitePool,
    username: &str,
) -> Result<Option<(i64, String, String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT id, username, password_hash, role FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(db)
        .await
}

pub async fn insert_session(
    db: &SqlitePool,
    token_hash: &str,
    user_id: i64,
    csrf_token: &str,
    expires_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(user_id)
    .bind(csrf_token)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn load_session_by_token_hash(
    db: &SqlitePool,
    token_hash: &str,
    now: i64,
) -> Result<Option<SessionContext>, sqlx::Error> {
    let row: Option<(i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT u.id, u.username, u.role, s.csrf_token, s.expires_at
         FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(db)
    .await?;

    Ok(
        row.and_then(|(user_id, username, role_str, csrf_token, _expires_at)| {
            let role = Role::from_str(&role_str)?;
            Some(SessionContext {
                user: AuthUser {
                    id: user_id,
                    username,
                    role,
                },
                csrf_token,
                token_hash: token_hash.to_string(),
            })
        }),
    )
}

pub async fn delete_session_by_token_hash(
    db: &SqlitePool,
    token_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(token_hash)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn insert_editor_user(
    db: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> ApiResult<i64> {
    let result =
        sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, 'editor')")
            .bind(username)
            .bind(password_hash)
            .execute(db)
            .await;

    match result {
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            Err(ApiError::conflict("Tên đăng nhập đã tồn tại."))
        }
        res => {
            let r = res?;
            Ok(r.last_insert_rowid())
        }
    }
}
