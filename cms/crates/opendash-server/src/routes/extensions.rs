//! Scoped custom routes for Themes (`functions.js`) and Plugins (`plugin.js` / `plugin.wasm`).
//!
//! Enforces three distinct URL families:
//! - `GET /p/{pkg}` and `GET /p/{pkg}/{*subpath}` (`RouteGet`, `Public`, read-only, 1000ms timeout)
//! - `GET` & `POST /api/p/{pkg}` and `/api/p/{pkg}/{*subpath}` (`Session | Admin`, CSRF + `AdmittedMutation`)
//! - `POST /api/public/p/{pkg}` and `/api/public/p/{pkg}/{*subpath}` (`Public` Origin + RateLimit OR
//!   `Webhook` RateLimit + raw_body/HMAC headers, holding `MutationLease` with `ExecutionPrincipal::Public`)

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{ConnectInfo, OriginalUri, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::{
    hooks::{ExecutionPrincipal, HookExecutionMode},
    plugin::{RouteAccess, RouteMethod},
    theme::ResolvedEnqueuedAssets,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{
    auth::{Role, require_session, require_session_with_csrf},
    contact::normalize_peer_ip,
    content_render, document,
    error::ApiResult,
    hooks::HookDispatcher,
    operations::SiteCoordinator,
    routes::contact::{ExpectedListenerOrigin, check_strict_contact_origin},
    seo::SeoView,
    storage::settings,
};

pub const MAX_EXTENSION_ROUTE_RESPONSE_BYTES: usize = 1024 * 1024;
pub const ROUTE_GET_TIMEOUT: Duration = Duration::from_millis(1000);
pub const ROUTE_POST_TIMEOUT: Duration = Duration::from_secs(5);
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT_MAX_REQUESTS: usize = 30;
const RATE_LIMIT_MAX_BUCKETS: usize = 4096;

#[derive(Clone, Default)]
pub struct ExtensionRouteRateLimiter {
    buckets: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl ExtensionRouteRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_and_record(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let mut guard = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() >= RATE_LIMIT_MAX_BUCKETS && !guard.contains_key(key) {
            guard.retain(|_, q| {
                while let Some(&front) = q.front() {
                    if now.duration_since(front) >= RATE_LIMIT_WINDOW {
                        q.pop_front();
                    } else {
                        break;
                    }
                }
                !q.is_empty()
            });
            if guard.len() >= RATE_LIMIT_MAX_BUCKETS {
                return Err(RATE_LIMIT_WINDOW.as_secs());
            }
        }
        let queue = guard.entry(key.to_owned()).or_default();
        while let Some(&front) = queue.front() {
            if now.duration_since(front) >= RATE_LIMIT_WINDOW {
                queue.pop_front();
            } else {
                break;
            }
        }
        if queue.len() >= RATE_LIMIT_MAX_REQUESTS {
            let retry_after = queue
                .front()
                .map(|&oldest| {
                    RATE_LIMIT_WINDOW
                        .saturating_sub(now.duration_since(oldest))
                        .as_secs()
                        .max(1)
                })
                .unwrap_or(1);
            return Err(retry_after);
        }
        queue.push_back(now);
        Ok(())
    }
}

fn parse_query_map(raw_query: Option<&str>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(q) = raw_query else {
        return map;
    };
    for (k, v) in url::form_urlencoded::parse(q.as_bytes()) {
        if map.len() >= 64 {
            break;
        }
        map.insert(k.into_owned(), v.into_owned());
    }
    map
}

fn extract_signature_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (name, val) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        let allowed = lower.starts_with("x-signature")
            || lower.starts_with("x-hub-signature")
            || lower == "stripe-signature"
            || lower.starts_with("x-webhook-");
        if allowed && let Ok(v) = val.to_str() {
            if out.len() >= 16 || v.len() > 2048 {
                continue;
            }
            out.insert(lower, v.to_owned());
        }
    }
    out
}

fn no_store_nosniff_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h
}

fn json_error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        no_store_nosniff_headers(),
        Json(json!({ "ok": false, "code": code, "error": message })),
    )
        .into_response()
}

fn principal_for_role(role: Role) -> ExecutionPrincipal {
    match role {
        Role::Admin => ExecutionPrincipal::Admin,
        Role::Editor => ExecutionPrincipal::Editor,
    }
}

async fn format_route_output(
    db: &SqlitePool,
    output: Value,
    render_trusted_html: bool,
    allow_html_document: bool,
) -> Response {
    let Some(obj) = output.as_object() else {
        let bytes = serde_json::to_vec(&output).unwrap_or_else(|_| b"null".to_vec());
        if bytes.len() > MAX_EXTENSION_ROUTE_RESPONSE_BYTES {
            return json_error_response(
                StatusCode::BAD_GATEWAY,
                "response_too_large",
                "Phản hồi từ route mở rộng vượt quá giới hạn 1 MiB.",
            );
        }
        let mut headers = no_store_nosniff_headers();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        return (StatusCode::OK, headers, bytes).into_response();
    };

    let status_u16 = obj
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|&code| (200..=599).contains(&code))
        .unwrap_or(200);
    let status = StatusCode::from_u16(status_u16).unwrap_or(StatusCode::OK);

    let has_json = obj.contains_key("json");
    let content_type = obj
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or(if has_json {
            "application/json"
        } else if obj.get("body").is_some() {
            "text/html"
        } else {
            "application/json"
        });

    match content_type {
        "application/json" => {
            let body_val = obj.get("json").cloned().unwrap_or(output);
            let bytes = match serde_json::to_vec(&body_val) {
                Ok(b) if b.len() <= MAX_EXTENSION_ROUTE_RESPONSE_BYTES => b,
                _ => {
                    return json_error_response(
                        StatusCode::BAD_GATEWAY,
                        "response_too_large",
                        "Phản hồi JSON từ route mở rộng vượt quá giới hạn 1 MiB.",
                    );
                }
            };
            let mut headers = no_store_nosniff_headers();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            (status, headers, bytes).into_response()
        }
        "text/plain" => {
            let body = obj
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if body.len() > MAX_EXTENSION_ROUTE_RESPONSE_BYTES {
                return json_error_response(
                    StatusCode::BAD_GATEWAY,
                    "response_too_large",
                    "Phản hồi văn bản từ route mở rộng vượt quá giới hạn 1 MiB.",
                );
            }
            let mut headers = no_store_nosniff_headers();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            (status, headers, body).into_response()
        }
        "text/html" if allow_html_document => {
            let raw_body = obj.get("body").and_then(Value::as_str).unwrap_or_default();
            if raw_body.len() > MAX_EXTENSION_ROUTE_RESPONSE_BYTES {
                return json_error_response(
                    StatusCode::BAD_GATEWAY,
                    "response_too_large",
                    "Phản hồi HTML từ route mở rộng vượt quá giới hạn 1 MiB.",
                );
            }
            if !render_trusted_html
                && crate::template_sec::validate_untrusted_extension_html_fragment(raw_body)
                    .is_err()
            {
                return json_error_response(
                    StatusCode::BAD_GATEWAY,
                    "unsafe_html_output",
                    "HTML trả về từ route không tin cậy bị bộ lọc bảo mật từ chối.",
                );
            }
            let normalized_body = match document::normalize_public_document_with_options(
                raw_body,
                render_trusted_html,
            ) {
                Ok(h) => h,
                Err(_) => {
                    return json_error_response(
                        StatusCode::BAD_GATEWAY,
                        "invalid_html_output",
                        "Không thể chuẩn hóa HTML trả về từ route mở rộng.",
                    );
                }
            };
            let raw_title = obj
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("OpenDash");
            let safe_title = opendash_core::hooks::sanitize_plain_text(raw_title, 160);
            let (_locale_code, bcp47, dir) = settings::get_locale_bcp47_and_dir(db, None)
                .await
                .unwrap_or_else(|_| ("vi".into(), "vi".into(), "ltr".into()));
            let seo_view = SeoView::for_noindex_page_with_dir(&bcp47, &dir, &safe_title);
            let composed = match content_render::compose_document_with_seo_with_assets(
                None,
                &normalized_body.body_fragment,
                None,
                &seo_view,
                &["od-route-extension"],
                &ResolvedEnqueuedAssets::default(),
                render_trusted_html,
            ) {
                Ok(doc) => {
                    let doc = doc.replace(content_render::HEAD_END_MARKER, "");
                    if doc.len() <= MAX_EXTENSION_ROUTE_RESPONSE_BYTES {
                        doc
                    } else {
                        return json_error_response(
                            StatusCode::BAD_GATEWAY,
                            "invalid_html_output",
                            "Không thể dựng tài liệu HTML cho route mở rộng.",
                        );
                    }
                }
                Err(_) => {
                    return json_error_response(
                        StatusCode::BAD_GATEWAY,
                        "invalid_html_output",
                        "Không thể dựng tài liệu HTML cho route mở rộng.",
                    );
                }
            };
            let mut resp = crate::routes::public::rendered_layout_with_form_csp(composed, false);
            *resp.status_mut() = status;
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            resp.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            resp
        }
        _ => json_error_response(
            StatusCode::BAD_GATEWAY,
            "unsupported_content_type",
            "content_type không được hỗ trợ cho route mở rộng.",
        ),
    }
}

fn normalized_subpath(subpath: Option<&str>) -> String {
    match subpath {
        Some(s) if !s.trim_matches('/').is_empty() => format!("/{}", s.trim_matches('/')),
        _ => "/".to_owned(),
    }
}

async fn handle_public_get_inner(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    pkg: String,
    subpath: Option<String>,
    uri: axum::http::Uri,
) -> ApiResult<Response> {
    let norm_sub = normalized_subpath(subpath.as_deref());
    let resolved = match hooks
        .resolve_extension_route(&pkg, RouteMethod::Get, &norm_sub)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Ok(json_error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "Không tìm thấy route mở rộng.",
            ));
        }
        Err((st, msg)) => return Ok(json_error_response(st, "route_unavailable", &msg)),
    };
    if resolved.route.access != RouteAccess::Public {
        return Ok(json_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Route yêu cầu phiên đăng nhập; hãy gọi qua /api/p/.",
        ));
    }
    let payload = json!({
        "route_id": &resolved.route.id,
        "method": "GET",
        "path": uri.path(),
        "subpath": &norm_sub,
        "params": &resolved.params,
        "query": parse_query_map(uri.query()),
    });
    match hooks
        .execute_resolved_route(
            resolved,
            payload,
            HookExecutionMode::RouteGet,
            ExecutionPrincipal::Public,
            ROUTE_GET_TIMEOUT,
            None,
        )
        .await
    {
        Ok(exec) => Ok(format_route_output(&db, exec.output, exec.render_trusted_html, true).await),
        Err((st, err_resp)) => Ok((st, no_store_nosniff_headers(), Json(err_resp)).into_response()),
    }
}

pub async fn extension_public_get_root(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    Path(pkg): Path<String>,
    OriginalUri(uri): OriginalUri,
) -> ApiResult<Response> {
    handle_public_get_inner(db, hooks, pkg, None, uri).await
}

pub async fn extension_public_get(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    Path((pkg, subpath)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
) -> ApiResult<Response> {
    handle_public_get_inner(db, hooks, pkg, Some(subpath), uri).await
}

async fn handle_api_get_inner(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    headers: HeaderMap,
    pkg: String,
    subpath: Option<String>,
    uri: axum::http::Uri,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    let norm_sub = normalized_subpath(subpath.as_deref());
    let resolved = match hooks
        .resolve_extension_route(&pkg, RouteMethod::Get, &norm_sub)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Ok(json_error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "Không tìm thấy route mở rộng.",
            ));
        }
        Err((st, msg)) => return Ok(json_error_response(st, "route_unavailable", &msg)),
    };
    match resolved.route.access {
        RouteAccess::Admin if actor.user.role != Role::Admin => {
            return Ok(json_error_response(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Route chỉ dành cho quản trị viên.",
            ));
        }
        RouteAccess::Session | RouteAccess::Admin => {}
        _ => {
            return Ok(json_error_response(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Route công khai không chạy trên /api/p/.",
            ));
        }
    }
    let payload = json!({
        "route_id": &resolved.route.id,
        "method": "GET",
        "path": uri.path(),
        "subpath": &norm_sub,
        "params": &resolved.params,
        "query": parse_query_map(uri.query()),
    });
    match hooks
        .execute_resolved_route(
            resolved,
            payload,
            HookExecutionMode::RouteGet,
            principal_for_role(actor.user.role),
            ROUTE_GET_TIMEOUT,
            None,
        )
        .await
    {
        Ok(exec) => Ok(format_route_output(&db, exec.output, exec.render_trusted_html, true).await),
        Err((st, err_resp)) => Ok((st, no_store_nosniff_headers(), Json(err_resp)).into_response()),
    }
}

pub async fn extension_api_get_root(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(pkg): Path<String>,
    OriginalUri(uri): OriginalUri,
) -> ApiResult<Response> {
    handle_api_get_inner(db, hooks, headers, pkg, None, uri).await
}

pub async fn extension_api_get(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((pkg, subpath)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
) -> ApiResult<Response> {
    handle_api_get_inner(db, hooks, headers, pkg, Some(subpath), uri).await
}

async fn execute_api_post_after_auth(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    actor: crate::auth::SessionContext,
    pkg: String,
    subpath: Option<String>,
    uri: axum::http::Uri,
    body: Bytes,
) -> ApiResult<Response> {
    let norm_sub = normalized_subpath(subpath.as_deref());
    let resolved = match hooks
        .resolve_extension_route(&pkg, RouteMethod::Post, &norm_sub)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Ok(json_error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "Không tìm thấy route mở rộng.",
            ));
        }
        Err((st, msg)) => return Ok(json_error_response(st, "route_unavailable", &msg)),
    };
    match resolved.route.access {
        RouteAccess::Admin if actor.user.role != Role::Admin => {
            return Ok(json_error_response(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Route chỉ dành cho quản trị viên.",
            ));
        }
        RouteAccess::Session | RouteAccess::Admin => {}
        RouteAccess::Public | RouteAccess::Webhook => {
            return Ok(json_error_response(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Route công khai/webhook phải gọi qua /api/public/p/.",
            ));
        }
    }
    let parsed_body: Value = if body.is_empty() {
        Value::Null
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => {
                return Ok(json_error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_json",
                    "Dữ liệu JSON không hợp lệ.",
                ));
            }
        }
    };
    let payload = json!({
        "route_id": &resolved.route.id,
        "method": "POST",
        "path": uri.path(),
        "subpath": &norm_sub,
        "params": &resolved.params,
        "query": parse_query_map(uri.query()),
        "body": parsed_body,
    });
    match hooks
        .execute_resolved_route(
            resolved,
            payload,
            HookExecutionMode::RoutePost,
            principal_for_role(actor.user.role),
            ROUTE_POST_TIMEOUT,
            Some(&actor),
        )
        .await
    {
        Ok(exec) => {
            Ok(format_route_output(&db, exec.output, exec.render_trusted_html, false).await)
        }
        Err((st, err_resp)) => Ok((st, no_store_nosniff_headers(), Json(err_resp)).into_response()),
    }
}

pub async fn extension_api_post_root(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(pkg): Path<String>,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    execute_api_post_after_auth(db, hooks, actor, pkg, None, uri, body).await
}

pub async fn extension_api_post(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((pkg, subpath)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    execute_api_post_after_auth(db, hooks, actor, pkg, Some(subpath), uri, body).await
}

#[allow(clippy::too_many_arguments)]
async fn execute_public_post(
    coordinator: Arc<SiteCoordinator>,
    hooks: Arc<HookDispatcher>,
    db: SqlitePool,
    rate_limiter: ExtensionRouteRateLimiter,
    listener_origin: String,
    runtime_cfg: Option<Extension<crate::config::RuntimeConfigV1>>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    pkg: String,
    subpath: Option<String>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let norm_sub = normalized_subpath(subpath.as_deref());
    let resolved = match hooks
        .resolve_extension_route(&pkg, RouteMethod::Post, &norm_sub)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Ok(json_error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "Không tìm thấy route mở rộng.",
            ));
        }
        Err((st, msg)) => return Ok(json_error_response(st, "route_unavailable", &msg)),
    };

    if matches!(
        resolved.route.access,
        RouteAccess::Session | RouteAccess::Admin
    ) {
        return Ok((
            StatusCode::FORBIDDEN,
            no_store_nosniff_headers(),
            Json(json!({
                "ok": false,
                "code": "forbidden",
                "error": "Route yêu cầu phiên đăng nhập không được phép gọi trên /api/public/p/."
            })),
        )
            .into_response());
    }

    let peer_ip = connect_info
        .map(|Extension(ConnectInfo(addr))| addr.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let rate_key = format!(
        "{}:{}:{}",
        pkg,
        resolved.route.id,
        normalize_peer_ip(peer_ip)
    );
    if let Err(retry_after) = rate_limiter.check_and_record(&rate_key) {
        let mut h = no_store_nosniff_headers();
        if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
            h.insert(header::RETRY_AFTER, v);
        }
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            h,
            Json(json!({
                "ok": false,
                "code": "rate_limited",
                "error": "Quá nhiều yêu cầu; vui lòng thử lại sau."
            })),
        )
            .into_response());
    }

    let mut payload_map = serde_json::Map::new();
    payload_map.insert("route_id".into(), Value::String(resolved.route.id.clone()));
    payload_map.insert("method".into(), Value::String("POST".into()));
    payload_map.insert("path".into(), Value::String(uri.path().to_owned()));
    payload_map.insert("subpath".into(), Value::String(norm_sub));
    payload_map.insert(
        "params".into(),
        serde_json::to_value(&resolved.params).unwrap_or(Value::Null),
    );
    payload_map.insert(
        "query".into(),
        serde_json::to_value(parse_query_map(uri.query())).unwrap_or(Value::Null),
    );

    if resolved.route.access == RouteAccess::Public {
        let trusted_origin_match = runtime_cfg
            .as_ref()
            .and_then(|Extension(cfg)| cfg.trusted_origin_normalized())
            .is_some_and(|trusted| {
                headers
                    .get(header::ORIGIN)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|o| o == trusted)
            });
        if !trusted_origin_match
            && !check_strict_contact_origin(&headers, &db, &listener_origin).await?
        {
            return Ok((
                StatusCode::FORBIDDEN,
                no_store_nosniff_headers(),
                "Origin không hợp lệ.",
            )
                .into_response());
        }
        let parsed_body: Value = if body.is_empty() {
            Value::Null
        } else {
            match serde_json::from_slice(&body) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(json_error_response(
                        StatusCode::BAD_REQUEST,
                        "invalid_json",
                        "Dữ liệu JSON không hợp lệ.",
                    ));
                }
            }
        };
        payload_map.insert("body".into(), parsed_body);
    } else {
        // RouteAccess::Webhook: server-to-server, Origin optional; expose raw_body + allowlisted signature headers.
        let Ok(raw_body) = String::from_utf8(body.to_vec()) else {
            return Ok(json_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_utf8",
                "Payload webhook phải là UTF-8 hợp lệ.",
            ));
        };
        let parsed_body = serde_json::from_str::<Value>(&raw_body).unwrap_or(Value::Null);
        payload_map.insert("body".into(), parsed_body);
        payload_map.insert("raw_body".into(), Value::String(raw_body));
        payload_map.insert(
            "signature_headers".into(),
            serde_json::to_value(extract_signature_headers(&headers)).unwrap_or(Value::Null),
        );
    }

    let _lease = match coordinator.admit_mutation() {
        Ok(lease) => lease,
        Err(err) => return Ok(err.into_response()),
    };

    match hooks
        .execute_resolved_route(
            resolved,
            Value::Object(payload_map),
            HookExecutionMode::RoutePost,
            ExecutionPrincipal::Public,
            ROUTE_POST_TIMEOUT,
            None,
        )
        .await
    {
        Ok(exec) => {
            Ok(format_route_output(&db, exec.output, exec.render_trusted_html, false).await)
        }
        Err((st, err_resp)) => Ok((st, no_store_nosniff_headers(), Json(err_resp)).into_response()),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn extension_public_post_root(
    State(coordinator): State<Arc<SiteCoordinator>>,
    State(hooks): State<Arc<HookDispatcher>>,
    State(db): State<SqlitePool>,
    Extension(rate_limiter): Extension<ExtensionRouteRateLimiter>,
    Extension(ExpectedListenerOrigin(listener_origin)): Extension<ExpectedListenerOrigin>,
    runtime_cfg: Option<Extension<crate::config::RuntimeConfigV1>>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(pkg): Path<String>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    execute_public_post(
        coordinator,
        hooks,
        db,
        rate_limiter,
        listener_origin,
        runtime_cfg,
        connect_info,
        pkg,
        None,
        uri,
        headers,
        body,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn extension_public_post(
    State(coordinator): State<Arc<SiteCoordinator>>,
    State(hooks): State<Arc<HookDispatcher>>,
    State(db): State<SqlitePool>,
    Extension(rate_limiter): Extension<ExtensionRouteRateLimiter>,
    Extension(ExpectedListenerOrigin(listener_origin)): Extension<ExpectedListenerOrigin>,
    runtime_cfg: Option<Extension<crate::config::RuntimeConfigV1>>,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path((pkg, subpath)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    execute_public_post(
        coordinator,
        hooks,
        db,
        rate_limiter,
        listener_origin,
        runtime_cfg,
        connect_info,
        pkg,
        Some(subpath),
        uri,
        headers,
        body,
    )
    .await
}
