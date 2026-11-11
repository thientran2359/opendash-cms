use serde::Serialize;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::{self, AuthUser, Role, SessionContext},
    error::{ApiError, ApiResult},
};

#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub version: i64,
}

#[derive(Serialize)]
pub struct UsersPage {
    pub items: Vec<UserSummary>,
    pub total: i64,
}

type UserRow = (i64, String, String, i64);

fn summary(row: UserRow) -> ApiResult<UserSummary> {
    let (id, username, role, version) = row;
    Ok(UserSummary {
        id,
        username,
        role: Role::from_str(&role)
            .ok_or_else(|| ApiError::internal("Vai trò người dùng không hợp lệ."))?,
        version,
    })
}

fn mutation_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_error) = error {
        if db_error.is_unique_violation() {
            return ApiError::conflict("Tên đăng nhập đã tồn tại.");
        }
        // SQLite BUSY/LOCKED may happen when another connection reserves the writer.
        if db_error
            .code()
            .as_deref()
            .is_some_and(sqlite_is_write_lock_conflict)
        {
            return ApiError::service_unavailable("Dữ liệu đang được cập nhật, vui lòng thử lại.");
        }
    }
    if matches!(error, sqlx::Error::PoolTimedOut) {
        return ApiError::service_unavailable("Dữ liệu đang được cập nhật, vui lòng thử lại.");
    }
    ApiError::from(error)
}

fn sqlite_is_write_lock_conflict(code: &str) -> bool {
    // SQLite returns numeric extended result codes (primary code in low 8
    // bits), or symbolic names depending on the driver. Both must map to 503.
    code.parse::<u32>()
        .is_ok_and(|extended| matches!(extended & 0xff, 5 | 6))
        || code.starts_with("SQLITE_BUSY")
        || code.starts_with("SQLITE_LOCKED")
}

#[cfg(test)]
mod sqlite_error_tests {
    use super::insert_session;
    use super::sqlite_is_write_lock_conflict;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn extended_busy_and_locked_codes_remain_retryable() {
        for code in [
            "5",
            "6",
            "261",
            "262",
            "517",
            "SQLITE_BUSY",
            "SQLITE_BUSY_SNAPSHOT",
            "SQLITE_LOCKED_SHAREDCACHE",
        ] {
            assert!(sqlite_is_write_lock_conflict(code), "retryable: {code}");
        }
        for code in ["19", "2067", "SQLITE_CONSTRAINT", "unknown"] {
            assert!(
                !sqlite_is_write_lock_conflict(code),
                "non-retryable: {code}"
            );
        }
    }

    #[tokio::test]
    async fn login_verified_before_reset_cannot_create_session_after_commit() {
        // Deterministically reproduce the order: login reads version 1 and
        // verifies the old hash, reset commits version 2 + revocation, then
        // the in-flight login attempts its conditional session INSERT.
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, version INTEGER NOT NULL, password_hash TEXT NOT NULL)"
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE sessions (token_hash TEXT PRIMARY KEY, user_id INTEGER NOT NULL, csrf_token TEXT, expires_at INTEGER)"
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO users VALUES (1, 1, 'old_hash')")
            .execute(&db)
            .await
            .unwrap();
        let observed_version: i64 = sqlx::query_scalar("SELECT version FROM users WHERE id=1")
            .fetch_one(&db)
            .await
            .unwrap();
        let mut reset = db.begin().await.unwrap();
        sqlx::query("UPDATE users SET password_hash='new_hash', version=version+1 WHERE id=1")
            .execute(&mut *reset)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id=1")
            .execute(&mut *reset)
            .await
            .unwrap();
        reset.commit().await.unwrap();
        assert!(
            !insert_session(&db, "stale_token", 1, "csrf", 9999999999, observed_version)
                .await
                .unwrap()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert!(
            insert_session(&db, "new_token", 1, "csrf", 9999999999, 2)
                .await
                .unwrap()
        );
    }
}

/// Hold SQLite's single writer reservation before inspecting the actor and
/// last-Admin invariant. This also prevents CAS and session revocation races.
async fn begin_admin_mutation<'a>(
    db: &'a SqlitePool,
    actor: &SessionContext,
) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await.map_err(mutation_error)?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.role FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(auth::current_time_secs())
    .fetch_optional(&mut *tx)
    .await
    .map_err(mutation_error)?;
    let (id, role) = row.ok_or_else(|| ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi."))?;
    if id != actor.user.id {
        return Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi."));
    }
    if role != "admin" {
        return Err(ApiError::forbidden(
            "Chỉ quản trị viên được quản lý tài khoản.",
        ));
    }
    Ok(tx)
}

async fn load_target(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    version: i64,
) -> ApiResult<UserSummary> {
    let row: Option<UserRow> =
        sqlx::query_as("SELECT id, username, role, version FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(mutation_error)?;
    let target = summary(row.ok_or_else(|| ApiError::not_found("Không tìm thấy người dùng."))?)?;
    if target.version != version {
        return Err(ApiError::precondition_failed(
            "Tài khoản đã được cập nhật, hãy tải lại.",
        ));
    }
    Ok(target)
}

async fn require_another_admin(tx: &mut Transaction<'_, Sqlite>) -> ApiResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&mut **tx)
        .await
        .map_err(mutation_error)?;
    if count <= 1 {
        return Err(ApiError::conflict(
            "Không thể xóa hoặc hạ quyền Admin cuối cùng.",
        ));
    }
    Ok(())
}

pub async fn list_users(db: &SqlitePool, limit: i64, offset: i64) -> ApiResult<UsersPage> {
    let mut tx = db.begin().await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *tx)
        .await?;
    let rows: Vec<UserRow> = sqlx::query_as(
        "SELECT id, username, role, version FROM users ORDER BY id ASC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(UsersPage {
        items: rows.into_iter().map(summary).collect::<ApiResult<_>>()?,
        total,
    })
}

pub async fn create_user(
    db: &SqlitePool,
    actor: &SessionContext,
    username: &str,
    password_hash: &str,
    role: Role,
) -> ApiResult<UserSummary> {
    let mut tx = begin_admin_mutation(db, actor).await?;
    let result = sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?)")
        .bind(username)
        .bind(password_hash)
        .bind(role.as_str())
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    let created = UserSummary {
        id: result.last_insert_rowid(),
        username: username.to_string(),
        role,
        version: 1,
    };
    tx.commit().await.map_err(mutation_error)?;
    Ok(created)
}

pub async fn patch_user(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected: i64,
    new_username: Option<&str>,
    new_role: Option<Role>,
) -> ApiResult<UserSummary> {
    let mut tx = begin_admin_mutation(db, actor).await?;
    let target = load_target(&mut tx, id, expected).await?;
    let username = new_username.unwrap_or(&target.username);
    let role = new_role.unwrap_or(target.role);
    if target.username == username && target.role == role {
        tx.commit().await.map_err(mutation_error)?;
        return Ok(target);
    }
    if target.role == Role::Admin && role != Role::Admin {
        require_another_admin(&mut tx).await?;
    }
    sqlx::query("UPDATE users SET username = ?, role = ?, version = version + 1 WHERE id = ? AND version = ?")
        .bind(username).bind(role.as_str()).bind(id).bind(expected)
        .execute(&mut *tx).await.map_err(mutation_error)?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(UserSummary {
        id,
        username: username.to_string(),
        role,
        version: expected + 1,
    })
}

pub async fn reset_password(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected: i64,
    password_hash: &str,
) -> ApiResult<UserSummary> {
    let mut tx = begin_admin_mutation(db, actor).await?;
    let target = load_target(&mut tx, id, expected).await?;
    sqlx::query(
        "UPDATE users SET password_hash = ?, version = version + 1 WHERE id = ? AND version = ?",
    )
    .bind(password_hash)
    .bind(id)
    .bind(expected)
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(UserSummary {
        version: expected + 1,
        ..target
    })
}

pub async fn delete_user(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_mutation(db, actor).await?;
    let target = load_target(&mut tx, id, expected).await?;
    if target.role == Role::Admin {
        require_another_admin(&mut tx).await?;
    }
    // Explicitly delete sessions as well as relying on the FK cascade.
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    sqlx::query("DELETE FROM users WHERE id = ? AND version = ?")
        .bind(id)
        .bind(expected)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(())
}

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
) -> Result<Option<(i64, String, String, String, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, username, password_hash, role, version FROM users WHERE username = ?",
    )
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
    expected_version: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at)
         SELECT ?, id, ?, ? FROM users WHERE id = ? AND version = ?",
    )
    .bind(token_hash)
    .bind(csrf_token)
    .bind(expires_at)
    .bind(user_id)
    .bind(expected_version)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
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
