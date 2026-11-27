//! Admin API error messages backed by `CatalogRegistry` + `l10n::error_catalog`.
//! Opaque plugin messages and interpolated user data stay untouched.

use std::sync::{Arc, RwLock};

use crate::l10n::{CatalogRegistry, error_catalog::translate_from_error_catalog};

#[derive(Debug, PartialEq, Eq)]
pub struct LocalizedError {
    pub code: &'static str,
    pub message: String,
}

/// Unknown messages must remain byte-for-byte unchanged; the middleware can
/// distinguish them from a recognized system error using this sentinel.
pub const UNKNOWN_ERROR_CODE: &str = "server.unknown";

pub fn encode_ui_locale_str(configured: &str) -> Option<String> {
    opendash_core::l10n::canonicalize_locale_code(configured).ok()
}

pub fn translate_api_message(raw_msg: &str, effective_ui_locale: &str) -> LocalizedError {
    translate_api_message_with_registry(raw_msg, effective_ui_locale, None)
}

pub fn translate_api_message_with_registry(
    raw_msg: &str,
    effective_ui_locale: &str,
    registry: Option<&CatalogRegistry>,
) -> LocalizedError {
    if let Some((code, message)) =
        translate_from_error_catalog(raw_msg, effective_ui_locale, registry)
    {
        return LocalizedError { code, message };
    }
    LocalizedError {
        code: UNKNOWN_ERROR_CODE,
        message: raw_msg.to_owned(),
    }
}

const MAX_LOCALIZED_ERROR_BYTES: usize = 1_048_576;

fn is_localized_admin_path(path: &str) -> bool {
    path == "/admin"
        || path.starts_with("/admin/")
        || path == "/builder"
        || path.starts_with("/builder/")
        || path.starts_with("/auth/")
        || path.starts_with("/api/admin/")
        || path.starts_with("/api/themes/")
}

pub(crate) async fn effective_locale_for_error(
    db: &sqlx::SqlitePool,
    ui_locale: &Arc<RwLock<Option<String>>>,
) -> String {
    if let Ok(guard) = ui_locale.try_read()
        && let Some(cached) = guard.as_ref()
    {
        return cached.clone();
    }
    if let Some(mut conn) = db.try_acquire()
        && let Ok(configured) = crate::storage::settings::get_cms_locale_in_conn(&mut conn).await
        && let Some(canon) = encode_ui_locale_str(&configured)
    {
        if let Ok(mut guard) = ui_locale.try_write()
            && guard.is_none()
        {
            *guard = Some(canon.clone());
        }
        return canon;
    }
    "en".to_owned()
}

fn status_error_code(status: axum::http::StatusCode) -> &'static str {
    use axum::http::StatusCode as S;
    match status {
        S::UNAUTHORIZED => "auth.unauthorized",
        S::FORBIDDEN => "auth.forbidden",
        S::METHOD_NOT_ALLOWED => "request.method_not_allowed",
        S::PAYLOAD_TOO_LARGE => "request.body_too_large",
        S::UNSUPPORTED_MEDIA_TYPE => "request.unsupported_media_type",
        S::PRECONDITION_REQUIRED => "request.if_match_required",
        S::PRECONDITION_FAILED => "request.precondition_failed",
        S::SERVICE_UNAVAILABLE => "storage.busy",
        _ => "server.error",
    }
}

fn empty_error_message(status: axum::http::StatusCode, guard: bool) -> &'static str {
    use axum::http::StatusCode as S;
    match (status, guard) {
        (S::BAD_REQUEST, true) => "Invalid Host header.",
        (S::FORBIDDEN, true) => "Origin không hợp lệ.",
        (S::METHOD_NOT_ALLOWED, _) => "Method Not Allowed",
        (S::PAYLOAD_TOO_LARGE, _) => "length limit exceeded",
        (S::UNSUPPORTED_MEDIA_TYPE, _) => "Unsupported Media Type",
        _ => status.canonical_reason().unwrap_or("Server error"),
    }
}

fn localize_json_error(
    value: &mut serde_json::Value,
    locale: &str,
    registry: Option<&CatalogRegistry>,
) -> Option<&'static str> {
    let obj = value.as_object_mut()?;
    let is_veto = obj
        .get("field_errors")
        .is_some_and(serde_json::Value::is_object)
        && obj.get("code").is_some_and(serde_json::Value::is_string);
    let is_plugin = obj.contains_key("correlation_id") && obj.contains_key("success");
    let mut code = None;
    for key in ["error", "message"] {
        if let Some(raw) = obj.get(key).and_then(serde_json::Value::as_str) {
            let localized = translate_api_message_with_registry(raw, locale, registry);
            if localized.code != UNKNOWN_ERROR_CODE {
                code.get_or_insert(localized.code);
                obj.insert(key.to_owned(), serde_json::Value::String(localized.message));
            }
        }
    }
    if let Some(fields) = obj
        .get_mut("field_errors")
        .and_then(serde_json::Value::as_object_mut)
    {
        for value in fields.values_mut() {
            if let Some(raw) = value.as_str() {
                let localized = translate_api_message_with_registry(raw, locale, registry);
                if localized.code != UNKNOWN_ERROR_CODE {
                    *value = serde_json::Value::String(localized.message);
                }
            }
        }
    }
    if is_plugin
        && let Some(data) = obj.get_mut("data")
        && data
            .get("field_errors")
            .is_some_and(serde_json::Value::is_object)
        && data.get("code").is_some_and(serde_json::Value::is_string)
    {
        let _ = localize_json_error(data, locale, registry);
    }
    if is_veto {
        Some("hook.before_save_veto")
    } else if is_plugin
        && obj.get("code").and_then(serde_json::Value::as_str) == Some("confirmation_required")
        && obj
            .get("confirmation")
            .is_some_and(serde_json::Value::is_object)
    {
        Some("confirmation_required")
    } else if is_plugin
        && obj.get("code").and_then(serde_json::Value::as_str) == Some("confirmation_invalid")
    {
        Some("confirmation_invalid")
    } else if is_plugin {
        code.or(Some("plugin.error"))
    } else {
        code
    }
}

struct ErrorBodyReplay {
    frames: std::collections::VecDeque<Result<http_body::Frame<axum::body::Bytes>, axum::Error>>,
    rest: axum::body::Body,
}

impl http_body::Body for ErrorBodyReplay {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if let Some(frame) = self.frames.pop_front() {
            return std::task::Poll::Ready(Some(frame));
        }
        std::pin::Pin::new(&mut self.rest).poll_frame(cx)
    }
}

async fn read_bounded_error_body(
    mut body: axum::body::Body,
) -> Result<axum::body::Bytes, axum::body::Body> {
    use http_body::Body as _;
    if body
        .size_hint()
        .upper()
        .is_some_and(|n| n > MAX_LOCALIZED_ERROR_BYTES as u64)
    {
        return Err(body);
    }
    let mut frames = std::collections::VecDeque::new();
    let mut total = 0usize;
    while let Some(frame) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await
    {
        let data_len = frame
            .as_ref()
            .ok()
            .and_then(http_body::Frame::data_ref)
            .map(|b| b.len());
        frames.push_back(frame);
        if let Some(len) = data_len
            && len <= MAX_LOCALIZED_ERROR_BYTES.saturating_sub(total)
        {
            total += len;
        } else {
            return Err(axum::body::Body::new(ErrorBodyReplay {
                frames,
                rest: body,
            }));
        }
    }
    let mut bytes = Vec::with_capacity(total);
    for frame in frames {
        if let Ok(frame) = frame
            && let Ok(data) = frame.into_data()
        {
            bytes.extend_from_slice(&data);
        }
    }
    Ok(bytes.into())
}

#[derive(Clone, Copy)]
pub(crate) struct PrelocalizedErrorResponse;

pub(crate) async fn localize_error_response(
    axum::extract::State(state): axum::extract::State<crate::app::AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{Method, StatusCode, header};
    let method = request.method().clone();
    let admin = is_localized_admin_path(request.uri().path());
    let mut response = next.run(request).await;
    if response
        .extensions()
        .get::<PrelocalizedErrorResponse>()
        .is_some()
    {
        return response;
    }
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }
    let guard = response
        .extensions()
        .get::<crate::app::HostOriginRejected>()
        .is_some();
    if !guard && !admin {
        return response;
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if content_type == "text/html"
        || response
            .headers()
            .get(header::CONTENT_ENCODING)
            .is_some_and(|v| v.as_bytes() != b"identity")
    {
        return response;
    }
    let error_header = header::HeaderName::from_static("x-opendash-error-code");
    let fallback_code = if guard {
        if status == StatusCode::BAD_REQUEST {
            "request.bad_host"
        } else {
            "request.bad_origin"
        }
    } else {
        status_error_code(status)
    };
    response.headers_mut().insert(
        error_header.clone(),
        header::HeaderValue::from_static(fallback_code),
    );
    if method == Method::HEAD {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let original = match read_bounded_error_body(body).await {
        Ok(bytes) => bytes,
        Err(body) => return axum::response::Response::from_parts(parts, body),
    };
    let locale = effective_locale_for_error(&state.db, &state.ui_locale).await;
    let reg = Some(state.catalog_registry.as_ref());
    let (replacement, code) = if original.is_empty() {
        let translated =
            translate_api_message_with_registry(empty_error_message(status, guard), &locale, reg);
        parts.headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        (Some(translated.message.into_bytes()), translated.code)
    } else if content_type == "application/json" {
        match serde_json::from_slice::<serde_json::Value>(&original) {
            Ok(mut value) => {
                let before = value.clone();
                let code = localize_json_error(&mut value, &locale, reg);
                let replacement = if value != before {
                    serde_json::to_vec(&value).ok()
                } else {
                    None
                };
                (replacement, code.unwrap_or(fallback_code))
            }
            Err(_) => (None, fallback_code),
        }
    } else if let Ok(raw) = std::str::from_utf8(&original) {
        let translated = translate_api_message_with_registry(raw, &locale, reg);
        (Some(translated.message.into_bytes()), translated.code)
    } else {
        (None, fallback_code)
    };
    let code = if guard || code == UNKNOWN_ERROR_CODE {
        fallback_code
    } else if code == "entry.confirmation_required" {
        "confirmation_required"
    } else {
        code
    };
    parts
        .headers
        .insert(error_header, header::HeaderValue::from_static(code));
    if let Some(bytes) = replacement
        && bytes.as_slice() != original.as_ref()
    {
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.remove(header::TRANSFER_ENCODING);
        parts.headers.insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from(bytes.len()),
        );
        return axum::response::Response::from_parts(parts, axum::body::Body::from(bytes));
    }
    axum::response::Response::from_parts(parts, axum::body::Body::from(original))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use serde_json::json;

    #[test]
    fn exact_table_has_valid_codes_and_bidirectional_coverage() {
        for (code, vi, en) in crate::l10n::error_catalog::all_exact_entries() {
            assert!(!code.is_empty() && code.contains('.'));
            for raw in [vi.as_str(), en.as_str()] {
                for locale in ["en", "vi"] {
                    assert_ne!(
                        translate_api_message(raw, locale).code,
                        UNKNOWN_ERROR_CODE,
                        "{raw}"
                    );
                }
            }
        }
    }

    #[test]
    fn json_errors_preserve_veto_metadata_and_unrecognized_plugin_text() {
        let mut veto = json!({
            "code": "plugin.validation",
            "message": "Cần đăng nhập.",
            "field_errors": {"known": "Cần đăng nhập.", "custom": "My custom error #47"}
        });
        let original = veto.clone();
        assert_eq!(
            localize_json_error(&mut veto, "en", None),
            Some("hook.before_save_veto")
        );
        assert_ne!(veto["message"], original["message"]);
        assert_eq!(veto["field_errors"]["known"], veto["message"]);
        assert_eq!(
            veto["field_errors"]["custom"],
            original["field_errors"]["custom"]
        );
        assert_eq!(
            localize_json_error(&mut veto, "vi", None),
            Some("hook.before_save_veto")
        );
        assert_eq!(veto, original);
    }

    #[tokio::test]
    async fn error_body_limit_preserves_oversized_content() {
        let original = vec![b'x'; MAX_LOCALIZED_ERROR_BYTES + 1];
        let body = read_bounded_error_body(Body::from(original.clone()))
            .await
            .expect_err("oversized body must not be buffered");
        let actual = to_bytes(body, original.len()).await.unwrap();
        assert_eq!(actual.as_ref(), original.as_slice());
    }

    #[tokio::test]
    async fn unset_locale_does_not_wait_for_exhausted_pool() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let _held = db.acquire().await.unwrap();
        let cache = Arc::new(RwLock::new(None));
        let locale = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            effective_locale_for_error(&db, &cache),
        )
        .await
        .expect("503 must never wait for a pooled connection");
        assert_eq!(locale, "en");
        assert!(cache.read().unwrap().is_none());
        *cache.write().unwrap() = Some("vi".to_owned());
        assert_eq!(effective_locale_for_error(&db, &cache).await, "vi");
    }
}
