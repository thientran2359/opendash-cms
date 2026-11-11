use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{
    auth::{
        self, AuthInput, AuthResponse, AuthUser, hash_password_blocking, require_session,
        require_session_with_csrf, verify_password_blocking,
    },
    error::{ApiError, ApiResult},
    storage::auth as auth_storage,
};

pub async fn auth_setup(
    State(db): State<SqlitePool>,
    Json(input): Json<AuthInput>,
) -> ApiResult<StatusCode> {
    auth::validate_username(&input.username).map_err(ApiError::bad_request)?;
    auth::validate_password(&input.password).map_err(ApiError::bad_request)?;

    let hash = hash_password_blocking(input.password).await?;
    auth_storage::setup_initial_admin(&db, &input.username, &hash).await?;
    Ok(StatusCode::CREATED)
}

pub async fn auth_login(
    State(db): State<SqlitePool>,
    Json(input): Json<AuthInput>,
) -> ApiResult<Response> {
    let invalid_creds = || ApiError::unauthorized("Tên đăng nhập hoặc mật khẩu không chính xác.");
    if input.username.is_empty()
        || input.username.len() > 32
        || input.password.is_empty()
        || input.password.len() > 128
    {
        return Err(invalid_creds());
    }
    let row = auth_storage::find_user_by_username(&db, &input.username).await?;

    let (id, username, password_hash, role_str, version) = row.ok_or_else(invalid_creds)?;
    if !verify_password_blocking(input.password, password_hash).await? {
        return Err(invalid_creds());
    }
    let role = auth::Role::from_str(&role_str).ok_or_else(invalid_creds)?;

    let session_token = auth::generate_random_token();
    let token_hash = auth::hash_token(&session_token);
    let csrf_token = auth::generate_random_token();
    let expires_at = auth::current_time_secs() + auth::SESSION_DURATION_SECS;

    if !auth_storage::insert_session(&db, &token_hash, id, &csrf_token, expires_at, version).await?
    {
        return Err(invalid_creds());
    }

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
    let session = require_session_with_csrf(&headers, &db).await?;
    auth::delete_session(&db, &session.token_hash).await?;
    let clear_cookie = auth::clear_session_cookie_header();
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, clear_cookie)]).into_response())
}
