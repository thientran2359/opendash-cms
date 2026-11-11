//! M3-S24: Admin-only user management. All writes and session revocations
//! are serialized inside storage/auth's SQLite transaction.
use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, de::DeserializeOwned};
use sqlx::SqlitePool;

use crate::{
    auth::{
        Role, hash_password_blocking, require_admin, require_admin_with_csrf, validate_password,
        validate_username,
    },
    error::{ApiError, ApiResult},
    storage::auth as users,
};

const MAX_USERS_JSON_BYTES: usize = 8 * 1024;

pub fn max_users_json_bytes() -> usize {
    MAX_USERS_JSON_BYTES
}

#[derive(Default, Deserialize)]
pub struct UserPagination {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewUser {
    username: String,
    password: String,
    role: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchUser {
    username: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordReset {
    password: String,
}

fn parse_body<T: DeserializeOwned>(result: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    result
        .map(|Json(body)| body)
        .map_err(|_| ApiError::bad_request("Dữ liệu JSON không hợp lệ."))
}

fn parse_role(role: &str) -> ApiResult<Role> {
    Role::from_str(role).ok_or_else(|| ApiError::bad_request("Vai trò phải là admin hoặc editor."))
}

fn valid_user_id(id: i64) -> ApiResult<()> {
    if id > 0 {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "ID người dùng phải là số nguyên dương.",
        ))
    }
}

fn parse_version(headers: &HeaderMap) -> ApiResult<i64> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match phiên bản người dùng."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let digits = value
        .strip_prefix("\"v")
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"v1\"."))?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    let version = digits
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    if version < 1 {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    Ok(version)
}

fn response(status: StatusCode, summary: users::UserSummary) -> Response {
    (
        status,
        [
            (header::ETAG, format!("\"v{}\"", summary.version)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(summary),
    )
        .into_response()
}

pub async fn user_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(pagination): Query<UserPagination>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let limit = pagination.limit.unwrap_or(50);
    let offset = pagination.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || offset < 0 {
        return Err(ApiError::bad_request("Phân trang không hợp lệ."));
    }
    let page = users::list_users(&db, limit, offset).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}

pub async fn user_create(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    payload: Result<Json<NewUser>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    validate_username(&input.username).map_err(ApiError::bad_request)?;
    validate_password(&input.password).map_err(ApiError::bad_request)?;
    let role = match input.role {
        Some(s) => parse_role(&s)?,
        None => Role::Editor,
    };
    let hash = hash_password_blocking(input.password).await?;
    let summary = users::create_user(&db, &actor, &input.username, &hash, role).await?;
    Ok(response(StatusCode::CREATED, summary))
}

pub async fn user_patch(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<PatchUser>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    valid_user_id(id)?;
    let version = parse_version(&headers)?;
    let input = parse_body(payload)?;
    if input.username.is_none() && input.role.is_none() {
        return Err(ApiError::bad_request("Cần username hoặc role."));
    }
    if let Some(username) = &input.username {
        validate_username(username).map_err(ApiError::bad_request)?;
    }
    let role = input.role.as_deref().map(parse_role).transpose()?;
    let summary =
        users::patch_user(&db, &actor, id, version, input.username.as_deref(), role).await?;
    Ok(response(StatusCode::OK, summary))
}

pub async fn user_reset_password(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<PasswordReset>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    valid_user_id(id)?;
    let version = parse_version(&headers)?;
    let input = parse_body(payload)?;
    validate_password(&input.password).map_err(ApiError::bad_request)?;
    let hash = hash_password_blocking(input.password).await?;
    let summary = users::reset_password(&db, &actor, id, version, &hash).await?;
    Ok(response(StatusCode::OK, summary))
}

pub async fn user_delete(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    valid_user_id(id)?;
    let version = parse_version(&headers)?;
    users::delete_user(&db, &actor, id, version).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}
