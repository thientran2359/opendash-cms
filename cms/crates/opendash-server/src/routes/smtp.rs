use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::{ApiError, ApiResult},
    routes::settings::{no_store_headers, parse_if_match},
    secrets::SecretStore,
    smtp::{SmtpService, validate_mailbox, validate_smtp_host},
    storage::smtp::{self as smtp_store, SmtpSettingsUpdate, TlsMode},
};

pub const MAX_SMTP_SETTINGS_JSON_BYTES: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasswordUpdateMode {
    Keep,
    Replace,
    Clear,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordUpdate {
    pub mode: PasswordUpdateMode,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmtpSettingsPutBody {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    #[serde(default)]
    pub username: String,
    pub from_address: String,
    pub recipient_address: String,
    pub password_update: PasswordUpdate,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotifySubmissionBody {
    #[serde(default)]
    pub acknowledge_duplicate_risk: bool,
}

fn parse_json<T: serde::de::DeserializeOwned>(
    payload: Result<Json<T>, JsonRejection>,
) -> ApiResult<T> {
    payload
        .map(|Json(v)| v)
        .map_err(|err| ApiError::bad_request(format!("Dữ liệu JSON không hợp lệ: {err}")))
}

pub async fn get_smtp_settings(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let settings = smtp_store::get_smtp_settings(&db).await?;
    let ver = settings.version;
    Ok((no_store_headers(Some(ver)), Json(settings.into_public())).into_response())
}

pub async fn put_smtp_settings(
    State(db): State<SqlitePool>,
    Extension(secrets): Extension<SecretStore>,
    headers: HeaderMap,
    payload: Result<Json<SmtpSettingsPutBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let body = parse_json(payload)?;

    if body.port == 0 {
        return Err(ApiError::bad_request("Cổng SMTP phải trong khoảng 1..=65535."));
    }
    if body.username.len() > 256
        || body
            .username
            .chars()
            .any(|c| c == '\r' || c == '\n' || c.is_control())
    {
        return Err(ApiError::bad_request("Tên đăng nhập SMTP không hợp lệ."));
    }

    let host = if body.enabled || !body.host.trim().is_empty() {
        validate_smtp_host(&body.host)?
    } else {
        String::new()
    };
    let from_address = if body.enabled || !body.from_address.trim().is_empty() {
        validate_mailbox(&body.from_address, "from_address")?.to_string()
    } else {
        String::new()
    };
    let recipient_address = if body.enabled || !body.recipient_address.trim().is_empty() {
        validate_mailbox(&body.recipient_address, "recipient_address")?.to_string()
    } else {
        String::new()
    };

    if body.enabled {
        secrets.verify_native_acl()?;
    }

    let current = smtp_store::get_smtp_settings(&db).await?;
    let mut newly_written_ref: Option<String> = None;
    let next_secret_ref = match body.password_update.mode {
        PasswordUpdateMode::Keep => current.secret_ref,
        PasswordUpdateMode::Clear => None,
        PasswordUpdateMode::Replace => {
            let raw_pw = body
                .password_update
                .value
                .as_deref()
                .ok_or_else(|| ApiError::bad_request("Thiếu mật khẩu mới khi mode=replace."))?;
            let new_ref = secrets.write_smtp_secret(raw_pw)?;
            let ref_str = new_ref.as_str().to_string();
            newly_written_ref = Some(ref_str.clone());
            Some(ref_str)
        }
    };

    let update_res = smtp_store::put_smtp_settings_cas(
        &db,
        expected_version,
        SmtpSettingsUpdate {
            enabled: body.enabled,
            host,
            port: body.port,
            tls_mode: body.tls_mode,
            username: body.username.trim().to_string(),
            secret_ref: next_secret_ref,
            from_address,
            recipient_address,
        },
    )
    .await;

    match update_res {
        Ok(updated) => {
            let ver = updated.version;
            Ok((
                StatusCode::OK,
                no_store_headers(Some(ver)),
                Json(updated.into_public()),
            )
                .into_response())
        }
        Err(err) => {
            if let Some(ref_str) = newly_written_ref {
                secrets.remove_secret_blob(&ref_str);
            }
            Err(err)
        }
    }
}

pub async fn test_smtp_settings(
    State(db): State<SqlitePool>,
    Extension(smtp_service): Extension<SmtpService>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    smtp_service.send_test_email().await?;
    Ok((no_store_headers(None), Json(serde_json::json!({ "ok": true }))).into_response())
}

pub async fn notify_submission(
    State(db): State<SqlitePool>,
    Extension(smtp_service): Extension<SmtpService>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<NotifySubmissionBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_attempt_version = parse_if_match(&headers)?;
    let body = parse_json(payload)?;

    let claimed = smtp_store::claim_or_create_manual_attempt(
        &db,
        id,
        expected_attempt_version,
        body.acknowledge_duplicate_risk,
    )
    .await?;
    let updated = smtp_service.execute_claimed_attempt(claimed).await?;
    let ver = updated.version;
    Ok((no_store_headers(Some(ver)), Json(updated)).into_response())
}
