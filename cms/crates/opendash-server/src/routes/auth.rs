use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    auth::{
        self, AuthInput, AuthResponse, AuthUser, Role, hash_password_blocking, require_admin,
        require_csrf, require_session, verify_password_blocking,
    },
    error::{ApiError, ApiResult},
};

pub async fn auth_setup(
    State(db): State<SqlitePool>,
    Json(input): Json<AuthInput>,
) -> ApiResult<StatusCode> {
    auth::validate_username(&input.username).map_err(ApiError::bad_request)?;
    auth::validate_password(&input.password).map_err(ApiError::bad_request)?;

    let hash = hash_password_blocking(input.password).await?;

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
        .bind(&input.username)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::CREATED)
}

pub async fn auth_login(
    State(db): State<SqlitePool>,
    Json(input): Json<AuthInput>,
) -> ApiResult<Response> {
    let invalid_creds = ApiError::unauthorized("Tên đăng nhập hoặc mật khẩu không chính xác.");
    if input.username.is_empty()
        || input.username.len() > 32
        || input.password.is_empty()
        || input.password.len() > 128
    {
        return Err(invalid_creds);
    }
    let row: Option<(i64, String, String, String)> =
        sqlx::query_as("SELECT id, username, password_hash, role FROM users WHERE username = ?")
            .bind(&input.username)
            .fetch_optional(&db)
            .await?;

    let (id, username, password_hash, role_str) = row.ok_or(invalid_creds)?;
    if !verify_password_blocking(input.password, password_hash).await? {
        return Err(invalid_creds);
    }
    let role = auth::Role::from_str(&role_str).ok_or(invalid_creds)?;

    let session_token = auth::generate_random_token();
    let token_hash = auth::hash_token(&session_token);
    let csrf_token = auth::generate_random_token();
    let expires_at = auth::current_time_secs() + auth::SESSION_DURATION_SECS;

    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&token_hash)
    .bind(id)
    .bind(&csrf_token)
    .bind(expires_at)
    .execute(&db)
    .await?;

    let cookie_header = auth::session_cookie_header(&session_token);
    let body = Json(AuthResponse {
        user: AuthUser { id, username, role },
        csrf_token,
    });

    Ok(([(header::SET_COOKIE, cookie_header)], body).into_response())
}

pub async fn auth_me(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<AuthResponse>> {
    let session = require_session(&headers, &db).await?;
    Ok(Json(AuthResponse {
        user: session.user,
        csrf_token: session.csrf_token,
    }))
}

pub async fn auth_logout(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    let session = require_session(&headers, &db).await?;
    require_csrf(&headers, &session)?;
    auth::delete_session(&db, &session.token_hash).await?;
    let clear_cookie = auth::clear_session_cookie_header();
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, clear_cookie)]).into_response())
}

pub async fn create_editor(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(input): Json<AuthInput>,
) -> ApiResult<(StatusCode, Json<AuthUser>)> {
    let session = require_admin(&headers, &db).await?;
    require_csrf(&headers, &session)?;

    auth::validate_username(&input.username).map_err(ApiError::bad_request)?;
    auth::validate_password(&input.password).map_err(ApiError::bad_request)?;

    let hash = hash_password_blocking(input.password).await?;
    let result =
        sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, ?, 'editor')")
            .bind(&input.username)
            .bind(&hash)
            .execute(&db)
            .await;

    match result {
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            Err(ApiError::conflict("Tên đăng nhập đã tồn tại."))
        }
        res => {
            let r = res?;
            let id = r.last_insert_rowid();
            Ok((
                StatusCode::CREATED,
                Json(AuthUser {
                    id,
                    username: input.username,
                    role: Role::Editor,
                }),
            ))
        }
    }
}
