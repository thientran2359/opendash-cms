use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use opendash_core::contact::FormSchema;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    contact::{ContactAdmissionLimiter, SubmitInput, SubmitOutcome, submit},
    error::{ApiError, ApiResult},
    routes::settings::{no_store_headers, parse_if_match},
    storage::{contact as store, settings},
};

pub const MAX_ADMIN_FORM_JSON_BYTES: usize = 64 * 1024;
pub const MAX_PUBLIC_CONTACT_FORM_BYTES: usize = 32 * 1024;
pub const MAX_PUBLIC_CONTACT_KEYS: usize = 23;

#[derive(Clone)]
pub struct ExpectedListenerOrigin(pub String);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormUpsertBody {
    pub slug: String,
    pub title: String,
    #[serde(default = "default_active_status")]
    pub status: String,
    pub schema: FormSchema,
}

fn default_active_status() -> String {
    "active".to_string()
}

#[derive(Debug, Deserialize)]
pub struct SubmissionListQuery {
    pub form_id: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn form_response(status: StatusCode, record: store::ContactFormRecord) -> Response {
    (status, no_store_headers(Some(record.version)), Json(record)).into_response()
}

fn parse_json_body<T: serde::de::DeserializeOwned>(
    payload: Result<Json<T>, JsonRejection>,
) -> ApiResult<T> {
    payload
        .map(|Json(v)| v)
        .map_err(|err| ApiError::bad_request(format!("Dữ liệu JSON không hợp lệ: {err}")))
}

pub async fn admin_list_forms(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let forms = store::list_forms(&db).await?;
    Ok((no_store_headers(None), Json(forms)).into_response())
}

pub async fn admin_get_form(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let form = store::get_form(&db, id).await?;
    Ok(form_response(StatusCode::OK, form))
}

pub async fn admin_create_form(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    payload: Result<Json<FormUpsertBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let body = parse_json_body(payload)?;
    let created =
        store::create_form(&db, &body.slug, &body.title, &body.status, &body.schema).await?;
    Ok(form_response(StatusCode::CREATED, created))
}

pub async fn admin_update_form(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<FormUpsertBody>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let body = parse_json_body(payload)?;
    let updated = store::update_form(
        &db,
        id,
        version,
        &body.slug,
        &body.title,
        &body.status,
        &body.schema,
    )
    .await?;
    Ok(form_response(StatusCode::OK, updated))
}

pub async fn admin_delete_form(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    store::delete_form(&db, id, version).await?;
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)).into_response())
}

pub async fn admin_list_submissions(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<SubmissionListQuery>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let limit = query.limit.unwrap_or(20);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || offset < 0 {
        return Err(ApiError::bad_request("limit phải trong 1..=100 và offset >= 0."));
    }
    let list = store::list_submissions(&db, query.form_id, limit, offset).await?;
    Ok((no_store_headers(None), Json(list)).into_response())
}

pub async fn admin_get_submission(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let detail = store::get_submission(&db, id).await?;
    Ok((no_store_headers(None), Json(detail)).into_response())
}

pub async fn admin_delete_submission(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    store::delete_submission(&db, id).await?;
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)).into_response())
}

fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

fn check_urlencoded_content_type(headers: &HeaderMap) -> bool {
    let Some(ct) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let mime = ct.split(';').next().unwrap_or("").trim();
    mime.eq_ignore_ascii_case("application/x-www-form-urlencoded")
}

async fn check_strict_contact_origin(
    headers: &HeaderMap,
    db: &SqlitePool,
    listener_origin: &str,
) -> ApiResult<bool> {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return Ok(false);
    };
    if origin.is_empty() || origin.eq_ignore_ascii_case("null") {
        return Ok(false);
    }
    if origin == listener_origin {
        return Ok(true);
    }
    let pub_settings = settings::get_public(db).await?;
    Ok(pub_settings
        .public_origin
        .as_deref()
        .is_some_and(|configured| origin == configured))
}

pub async fn public_submit(
    State(db): State<SqlitePool>,
    Extension(limiter): Extension<ContactAdmissionLimiter>,
    Extension(ExpectedListenerOrigin(listener_origin)): Extension<ExpectedListenerOrigin>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(form_id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    if !check_strict_contact_origin(&headers, &db, &listener_origin).await? {
        return Ok((
            StatusCode::FORBIDDEN,
            [(header::CACHE_CONTROL, "no-store")],
            "Origin không hợp lệ.",
        )
            .into_response());
    }
    if !check_urlencoded_content_type(&headers) {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Chỉ chấp nhận application/x-www-form-urlencoded.".into(),
        ));
    }

    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(&body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.len() > MAX_PUBLIC_CONTACT_KEYS {
        return Err(ApiError::unprocessable(
            "Số lượng trường gửi lên vượt quá giới hạn cho phép.",
        ));
    }

    let mut form_version: Option<i64> = None;
    let mut nonce: Option<String> = None;
    let mut honeypot: Option<String> = None;
    let mut field_values: Vec<(String, String)> = Vec::new();

    for (k, v) in pairs {
        match k.as_str() {
            "form_version" => {
                if form_version.is_some() {
                    return Err(ApiError::unprocessable("form_version bị gửi lặp lại."));
                }
                let parsed = v
                    .parse::<i64>()
                    .map_err(|_| ApiError::unprocessable("form_version không hợp lệ."))?;
                form_version = Some(parsed);
            }
            "nonce" => {
                if nonce.is_some() {
                    return Err(ApiError::unprocessable("nonce bị gửi lặp lại."));
                }
                nonce = Some(v);
            }
            "honeypot" => {
                if honeypot.is_some() {
                    return Err(ApiError::unprocessable("honeypot bị gửi lặp lại."));
                }
                honeypot = Some(v);
            }
            _ => field_values.push((k, v)),
        }
    }

    let form_version =
        form_version.ok_or_else(|| ApiError::unprocessable("Thiếu form_version."))?;
    let nonce = nonce.ok_or_else(|| ApiError::unprocessable("Thiếu nonce."))?;
    let honeypot = honeypot.unwrap_or_default();

    let peer_ip = connect_info
        .map(|Extension(ConnectInfo(addr))| addr.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

    let outcome = submit(
        &db,
        &limiter,
        peer_ip,
        SubmitInput {
            form_id,
            form_version,
            nonce,
            honeypot,
            values: field_values,
        },
    )
    .await?;

    match outcome {
        SubmitOutcome::Accepted(_) => Ok((
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, "/api/public/contact/received"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            "",
        )
            .into_response()),
        SubmitOutcome::RateLimited { retry_after_secs } => Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [
                (header::RETRY_AFTER, retry_after_secs.to_string()),
                (header::CACHE_CONTROL, "no-store".to_string()),
            ],
            "Bạn đã gửi quá nhiều yêu cầu. Vui lòng thử lại sau.",
        )
            .into_response()),
        SubmitOutcome::ValidationFailed(errs) => {
            let mut items = String::new();
            for err in &errs {
                items.push_str(&format!(
                    "<li><strong>{}</strong>: {}</li>",
                    escape_html(&err.key),
                    escape_html(&err.message)
                ));
            }
            let html = format!(
                "<!DOCTYPE html><html lang=\"vi\"><head><meta charset=\"utf-8\"><meta name=\"robots\" content=\"noindex\"><title>Lỗi gửi biểu mẫu</title></head><body><main role=\"alert\"><h1>Không thể gửi biểu mẫu</h1><ul>{items}</ul></main></body></html>"
            );
            Ok((
                StatusCode::UNPROCESSABLE_ENTITY,
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-store"),
                    (header::HeaderName::from_static("x-robots-tag"), "noindex"),
                ],
                Html(html),
            )
                .into_response())
        }
    }
}

pub async fn public_received() -> Response {
    let html = "<!DOCTYPE html><html lang=\"vi\"><head><meta charset=\"utf-8\"><meta name=\"robots\" content=\"noindex\"><title>Đã nhận liên hệ</title></head><body><main><h1>Cảm ơn bạn đã liên hệ</h1><p>Thông tin của bạn đã được tiếp nhận.</p></main></body></html>";
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::HeaderName::from_static("x-robots-tag"), "noindex"),
        ],
        Html(html),
    )
        .into_response()
}
