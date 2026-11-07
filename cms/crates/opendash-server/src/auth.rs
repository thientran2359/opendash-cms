use std::{
    sync::{Arc, LazyLock},
    time::{SystemTime, UNIX_EPOCH},
};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::http::{HeaderMap, header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

use crate::{
    error::{ApiError, ApiResult},
    storage::auth::{delete_session_by_token_hash, load_session_by_token_hash},
};

pub const SESSION_COOKIE_NAME: &str = "opendash_session";
pub const SESSION_DURATION_SECS: i64 = 8 * 3600; // 8 hours

const ARGON2_CONCURRENCY_LIMIT: usize = 2;
static ARGON2_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(ARGON2_CONCURRENCY_LIMIT)));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Editor,
}

impl Role {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Editor => "editor",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "editor" => Some(Role::Editor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub user: AuthUser,
    pub csrf_token: String,
    pub token_hash: String,
}

#[derive(Deserialize)]
pub struct AuthInput {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: AuthUser,
    pub csrf_token: String,
}

pub fn validate_username(username: &str) -> Result<(), &'static str> {
    if username.len() < 3 || username.len() > 32 {
        return Err("Tên đăng nhập phải từ 3 đến 32 ký tự.");
    }
    let bytes = username.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() {
        return Err("Tên đăng nhập phải bắt đầu bằng chữ cái hoặc chữ số.");
    }
    for &b in bytes {
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
            return Err("Tên đăng nhập chỉ được chứa chữ cái, số, gạch dưới hoặc gạch ngang.");
        }
    }
    Ok(())
}

pub fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < 8 || password.len() > 128 {
        return Err("Mật khẩu phải từ 8 đến 128 ký tự.");
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, &'static str> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| "Không thể băm mật khẩu.")
}

pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(password_hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub async fn hash_password_blocking(password: String) -> ApiResult<String> {
    let permit = ARGON2_SEMAPHORE
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal("Không thể điều phối tác vụ băm mật khẩu."))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        hash_password(&password)
    })
    .await
    .map_err(|_| ApiError::internal("Lỗi luồng xử lý mật khẩu."))?
    .map_err(ApiError::bad_request)
}

pub async fn verify_password_blocking(password: String, password_hash: String) -> ApiResult<bool> {
    let permit = ARGON2_SEMAPHORE
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal("Không thể điều phối tác vụ băm mật khẩu."))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        verify_password(&password, &password_hash)
    })
    .await
    .map_err(|_| ApiError::internal("Lỗi luồng xử lý mật khẩu."))
}

pub fn generate_random_token() -> String {
    use argon2::password_hash::rand_core::RngCore;
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn verify_csrf_token(expected: &str, provided: &str) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    expected.as_bytes().ct_eq(provided.as_bytes()).into()
}

pub fn current_time_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn session_cookie_header(token: &str) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_DURATION_SECS}"
    )
}

pub fn clear_session_cookie_header() -> String {
    format!("{SESSION_COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
}

pub fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let (k, v) = part.trim().split_once('=')?;
        if k == SESSION_COOKIE_NAME {
            return Some(v.to_string());
        }
    }
    None
}

pub async fn load_session(
    db: &SqlitePool,
    token: &str,
) -> Result<Option<SessionContext>, sqlx::Error> {
    let token_hash = hash_token(token);
    let now = current_time_secs();
    load_session_by_token_hash(db, &token_hash, now).await
}

pub async fn delete_session(db: &SqlitePool, token_hash: &str) -> Result<(), sqlx::Error> {
    delete_session_by_token_hash(db, token_hash).await
}

pub async fn require_session(headers: &HeaderMap, db: &SqlitePool) -> ApiResult<SessionContext> {
    let token =
        extract_session_token(headers).ok_or_else(|| ApiError::unauthorized("Cần đăng nhập."))?;
    let session = load_session(db, &token)
        .await?
        .ok_or_else(|| ApiError::unauthorized("Phiên đăng nhập không hợp lệ hoặc đã hết hạn."))?;
    Ok(session)
}

pub fn require_csrf(headers: &HeaderMap, session: &SessionContext) -> ApiResult<()> {
    let csrf_header = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("CSRF token không hợp lệ hoặc bị thiếu."))?;
    if !verify_csrf_token(&session.csrf_token, csrf_header) {
        return Err(ApiError::forbidden(
            "CSRF token không hợp lệ hoặc bị thiếu.",
        ));
    }
    Ok(())
}

pub async fn require_admin(headers: &HeaderMap, db: &SqlitePool) -> ApiResult<SessionContext> {
    let session = require_session(headers, db).await?;
    if session.user.role != Role::Admin {
        return Err(ApiError::forbidden(
            "Chỉ quản trị viên mới có quyền thực hiện thao tác này.",
        ));
    }
    Ok(session)
}

/// Combined session + CSRF guard for authenticated mutations (`Admin` or `Editor`).
pub async fn require_session_with_csrf(
    headers: &HeaderMap,
    db: &SqlitePool,
) -> ApiResult<SessionContext> {
    let session = require_session(headers, db).await?;
    require_csrf(headers, &session)?;
    Ok(session)
}

/// Combined Admin role + CSRF guard for administrative mutations (`Admin` only).
pub async fn require_admin_with_csrf(
    headers: &HeaderMap,
    db: &SqlitePool,
) -> ApiResult<SessionContext> {
    let session = require_admin(headers, db).await?;
    require_csrf(headers, &session)?;
    Ok(session)
}
