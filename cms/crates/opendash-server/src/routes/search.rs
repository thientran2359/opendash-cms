use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Extension, Json,
    extract::{ConnectInfo, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::{content::valid_content_type_key, search::sanitize_search_query};
use sqlx::SqlitePool;

use crate::{
    app::AppState,
    auth::{require_admin, require_admin_with_csrf},
    content_render::PaginationView,
    error::{ApiError, ApiResult},
    operations::MAX_MAINTENANCE_DRAIN,
    public_context::{MAX_SEARCH_PAGE, validate_percent_encoding},
    routes::operations::map_maintenance_error,
    storage::{
        operations::{
            JobCreateInput, JobTransitionInput, OperationKind, OperationState, create_job,
            latest_job_by_kind, transition_job_cas,
        },
        search::{
            PublicSearchFacet, PublicSearchQueryInput, get_search_index_status,
            query_public_search, rebuild_search_index,
        },
    },
};

pub const SEARCH_RATE_LIMIT_MAX_REQUESTS: usize = 60;
pub const SEARCH_RATE_LIMIT_MAX_BUCKETS: usize = 4096;
pub const SEARCH_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
pub const PUBLIC_SEARCH_API_DEFAULT_LIMIT: i64 = 10;
pub const PUBLIC_SEARCH_API_MAX_LIMIT: i64 = 20;

#[derive(Debug)]
struct LimiterState {
    buckets: HashMap<String, VecDeque<Instant>>,
    max_requests: usize,
    window: Duration,
}

#[derive(Clone, Debug)]
pub struct SearchRateLimiter {
    inner: Arc<Mutex<LimiterState>>,
}

impl Default for SearchRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchRateLimiter {
    pub fn new() -> Self {
        Self::with_limits(SEARCH_RATE_LIMIT_MAX_REQUESTS, SEARCH_RATE_LIMIT_WINDOW)
    }

    pub fn with_limits(max_requests: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LimiterState {
                buckets: HashMap::new(),
                max_requests: max_requests.max(1),
                window: if window.is_zero() {
                    Duration::from_secs(1)
                } else {
                    window
                },
            })),
        }
    }

    pub fn check(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let window = guard.window;
        let max_requests = guard.max_requests;

        if guard.buckets.len() >= SEARCH_RATE_LIMIT_MAX_BUCKETS && !guard.buckets.contains_key(key) {
            guard.buckets.retain(|_, q| {
                while let Some(&front) = q.front() {
                    if now.duration_since(front) >= window {
                        q.pop_front();
                    } else {
                        break;
                    }
                }
                !q.is_empty()
            });
            if guard.buckets.len() >= SEARCH_RATE_LIMIT_MAX_BUCKETS {
                return Err(window.as_secs().max(1));
            }
        }

        let queue = guard.buckets.entry(key.to_owned()).or_default();
        while let Some(&front) = queue.front() {
            if now.duration_since(front) >= window {
                queue.pop_front();
            } else {
                break;
            }
        }
        if queue.len() >= max_requests {
            let retry_after = queue
                .front()
                .map(|&oldest| window.saturating_sub(now.duration_since(oldest)).as_secs().max(1))
                .unwrap_or_else(|| window.as_secs().max(1));
            return Err(retry_after);
        }
        queue.push_back(now);
        Ok(())
    }
}

pub fn peer_rate_limit_key(peer: Option<&Extension<ConnectInfo<SocketAddr>>>) -> String {
    peer.map(|Extension(ConnectInfo(addr))| crate::contact::normalize_peer_ip(addr.ip()))
        .unwrap_or_else(|| "local".to_owned())
}

pub fn rate_limited_ssr_response(retry_after_secs: u64, message: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(val) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        headers.insert(header::RETRY_AFTER, val);
    }
    (StatusCode::TOO_MANY_REQUESTS, headers, message.to_owned()).into_response()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSearchApiQuery {
    pub s: String,
    pub locale: Option<String>,
    pub content_type: Option<String>,
    pub page: i64,
    pub limit: i64,
}

impl Default for PublicSearchApiQuery {
    fn default() -> Self {
        Self {
            s: String::new(),
            locale: None,
            content_type: None,
            page: 1,
            limit: PUBLIC_SEARCH_API_DEFAULT_LIMIT,
        }
    }
}

pub fn parse_public_search_api_query(raw: Option<&str>) -> Result<PublicSearchApiQuery, &'static str> {
    validate_percent_encoding(raw)?;
    let raw = raw.unwrap_or("").strip_prefix('?').unwrap_or(raw.unwrap_or(""));
    let mut out = PublicSearchApiQuery::default();
    let mut seen = std::collections::BTreeSet::new();

    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k_raw, v_raw) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_form_utf8(k_raw)?;
        let value = decode_form_utf8(v_raw)?;
        match key.as_str() {
            "s" => {
                if !seen.insert("s") {
                    return Err("Tham số tìm kiếm không được lặp lại.");
                }
                let sanitized = sanitize_search_query(&value)
                    .map_err(|_| "Từ khóa tìm kiếm vượt quá giới hạn.")?;
                out.s = sanitized.display_query;
            }
            "locale" => {
                if !seen.insert("locale") {
                    return Err("Tham số locale không được lặp lại.");
                }
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    out.locale = Some(trimmed.to_owned());
                }
            }
            "type" => {
                if !seen.insert("type") {
                    return Err("Tham số type không được lặp lại.");
                }
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    if !valid_content_type_key(trimmed) {
                        return Err("Loại nội dung tìm kiếm không hợp lệ.");
                    }
                    out.content_type = Some(trimmed.to_owned());
                }
            }
            "page" => {
                if !seen.insert("page") {
                    return Err("Tham số page không được lặp lại.");
                }
                let page: i64 = value.parse().map_err(|_| "Số trang tìm kiếm không hợp lệ.")?;
                if !(1..=MAX_SEARCH_PAGE).contains(&page) {
                    return Err("Số trang tìm kiếm vượt quá giới hạn.");
                }
                out.page = page;
            }
            "limit" => {
                if !seen.insert("limit") {
                    return Err("Tham số limit không được lặp lại.");
                }
                let limit: i64 = value.parse().map_err(|_| "Giới hạn kết quả không hợp lệ.")?;
                if !(1..=PUBLIC_SEARCH_API_MAX_LIMIT).contains(&limit) {
                    return Err("Giới hạn kết quả phải từ 1 đến 20.");
                }
                out.limit = limit;
            }
            "category_id"
            | "tag_id"
            | "taxonomy_key"
            | "term_id"
            | "include_descendants"
            | "descendants" => {
                return Err("Tham số query không dùng được trên API tìm kiếm.");
            }
            _ => {}
        }
    }

    Ok(out)
}

fn decode_form_utf8(raw: &str) -> Result<String, &'static str> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let hi = bytes.get(i + 1).copied().ok_or("Query có mã hóa không hợp lệ.")?;
                let lo = bytes.get(i + 2).copied().ok_or("Query có mã hóa không hợp lệ.")?;
                let hex = [hi, lo];
                let hex_str = std::str::from_utf8(&hex).map_err(|_| "Query có mã hóa không hợp lệ.")?;
                let byte =
                    u8::from_str_radix(hex_str, 16).map_err(|_| "Query có mã hóa không hợp lệ.")?;
                out.push(byte);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "Query chứa chuỗi UTF-8 không hợp lệ.")
}

fn api_no_store_headers(retry_after_secs: Option<u64>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    if let Some(secs) = retry_after_secs
        && let Ok(val) = HeaderValue::from_str(&secs.to_string())
    {
        headers.insert(header::RETRY_AFTER, val);
    }
    headers
}

fn api_error_json(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
    retry_after_secs: Option<u64>,
) -> Response {
    let mut payload = serde_json::json!({
        "error": message.into(),
        "code": code,
    });
    if let Some(secs) = retry_after_secs
        && let Some(obj) = payload.as_object_mut()
    {
        obj.insert("retry_after_secs".to_owned(), serde_json::json!(secs));
    }
    (status, api_no_store_headers(retry_after_secs), Json(payload)).into_response()
}

pub async fn public_search(
    State(db): State<SqlitePool>,
    search_limiter: Option<Extension<SearchRateLimiter>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    RawQuery(raw): RawQuery,
) -> Response {
    let parsed = match parse_public_search_api_query(raw.as_deref()) {
        Ok(q) => q,
        Err(msg) => return api_error_json(StatusCode::BAD_REQUEST, "invalid_query", msg, None),
    };

    if let Some(Extension(limiter)) = search_limiter {
        let key = peer_rate_limit_key(peer.as_ref());
        if let Err(retry) = limiter.check(&key) {
            return api_error_json(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Quá nhiều yêu cầu tìm kiếm, vui lòng thử lại sau.",
                Some(retry),
            );
        }
    }

    let output = match query_public_search(
        &db,
        &PublicSearchQueryInput {
            raw_query: Some(&parsed.s),
            locale: parsed.locale.as_deref(),
            content_type: parsed.content_type.as_deref(),
            page: parsed.page,
            per_page: parsed.limit,
        },
    )
    .await
    {
        Ok(out) => out,
        Err(ApiError(status, msg)) if status == StatusCode::BAD_REQUEST => {
            return api_error_json(StatusCode::BAD_REQUEST, "invalid_query", msg, None);
        }
        Err(ApiError(status, msg)) if status == StatusCode::NOT_FOUND => {
            return api_error_json(StatusCode::NOT_FOUND, "locale_not_found", msg, None);
        }
        Err(ApiError(status, msg)) => {
            return api_error_json(status, "search_failed", msg, None);
        }
    };

    let pagination = PaginationView::compute_with_max_pages(
        output.page,
        output.per_page,
        output.total_items,
        MAX_SEARCH_PAGE,
        None,
        None,
        |_| None,
    );
    let items: Vec<serde_json::Value> = output
        .hits
        .into_iter()
        .map(|hit| {
            serde_json::json!({
                "id": hit.view.id,
                "type_key": hit.view.type_key,
                "type_label": hit.type_label,
                "title": hit.view.title,
                "slug": hit.view.slug,
                "url": hit.view.url,
                "published_at": hit.view.published_at,
                "updated_at": hit.view.updated_at,
                "excerpt": hit.view.excerpt,
                "cover_url": hit.view.cover_url,
                "snippet_segments": hit.snippet_segments,
                "rank": hit.rank,
            })
        })
        .collect();
    let facets: Vec<PublicSearchFacet> = output.facets;

    (
        StatusCode::OK,
        api_no_store_headers(None),
        Json(serde_json::json!({
            "locale": output.locale,
            "query": output.query,
            "has_query": output.has_query,
            "type_filter": output.content_type,
            "items": items,
            "facets": facets,
            "pagination": {
                "page": pagination.page,
                "per_page": pagination.per_page,
                "total_items": pagination.total_items,
                "total_pages": pagination.total_pages,
                "has_prev": pagination.has_prev,
                "has_next": pagination.has_next,
            },
        })),
    )
        .into_response()
}

pub async fn admin_search_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &state.db).await?;
    let status = get_search_index_status(&state.db).await?;
    let last_job = latest_job_by_kind(&state.db, OperationKind::SearchReindex).await?;
    let maintenance_active = state.coordinator.is_maintenance_active();
    Ok((
        StatusCode::OK,
        api_no_store_headers(None),
        Json(serde_json::json!({
            "indexed_documents": status.indexed_documents,
            "eligible_published_entries": status.eligible_published_entries,
            "in_sync": status.in_sync,
            "maintenance_active": maintenance_active,
            "last_job": last_job,
        })),
    ))
}

pub async fn admin_search_reindex(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    let _maint = state
        .coordinator
        .begin_maintenance(OperationKind::SearchReindex, MAX_MAINTENANCE_DRAIN)
        .await
        .map_err(map_maintenance_error)?;
    let job = create_job(
        &state.db,
        JobCreateInput {
            id: crate::auth::generate_random_token(),
            kind: OperationKind::SearchReindex,
            actor_user_id: Some(admin.user.id.to_string()),
            reason: Some("admin_search_reindex".to_owned()),
        },
    )
    .await?;
    let running_job = transition_job_cas(
        &state.db,
        &job.id,
        JobTransitionInput {
            expected_version: job.version,
            next_state: OperationState::Running,
            summary_json: None,
            error_code: None,
            error_message: None,
        },
    )
    .await?;
    let started = Instant::now();
    match rebuild_search_index(&state.db).await {
        Ok(stats) => {
            state.site_cache_epoch.bump();
            let duration_ms = started.elapsed().as_millis() as u64;
            let summary = serde_json::json!({
                "indexed_documents": stats.indexed_documents,
                "eligible_published_entries": stats.eligible_published_entries,
                "duration_ms": duration_ms,
            });
            let done_job = transition_job_cas(
                &state.db,
                &running_job.id,
                JobTransitionInput {
                    expected_version: running_job.version,
                    next_state: OperationState::Succeeded,
                    summary_json: Some(summary.to_string()),
                    error_code: None,
                    error_message: None,
                },
            )
            .await?;
            Ok((
                StatusCode::OK,
                api_no_store_headers(None),
                Json(serde_json::json!({
                    "job": done_job,
                    "indexed_documents": stats.indexed_documents,
                    "eligible_published_entries": stats.eligible_published_entries,
                    "in_sync": true,
                    "duration_ms": duration_ms,
                })),
            ))
        }
        Err(err) => {
            let _ = transition_job_cas(
                &state.db,
                &running_job.id,
                JobTransitionInput {
                    expected_version: running_job.version,
                    next_state: OperationState::Failed,
                    summary_json: None,
                    error_code: Some("search_reindex_failed".to_owned()),
                    error_message: Some(err.1.to_string()),
                },
            )
            .await;
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_fails_closed_at_bucket_cap_and_allows_normal_traffic() {
        let limiter = SearchRateLimiter::with_limits(2, Duration::from_secs(60));
        assert!(limiter.check("203.0.113.10").is_ok());
        assert!(limiter.check("203.0.113.10").is_ok());
        assert!(limiter.check("203.0.113.10").is_err());

        let capped = SearchRateLimiter::with_limits(5, Duration::from_secs(60));
        for i in 0..SEARCH_RATE_LIMIT_MAX_BUCKETS {
            assert!(capped.check(&format!("bucket-{i}")).is_ok());
        }
        assert!(capped.check("new-ip-over-cap").is_err());
    }

    #[test]
    fn peer_key_normalizes_ipv4_and_ipv6_subnets() {
        let v4 = Extension(ConnectInfo("192.0.2.5:4321".parse::<SocketAddr>().unwrap()));
        assert_eq!(peer_rate_limit_key(Some(&v4)), "192.0.2.5");
        let v6 = Extension(ConnectInfo(
            "[2001:db8:abcd:12::99]:9000".parse::<SocketAddr>().unwrap(),
        ));
        assert_eq!(peer_rate_limit_key(Some(&v6)), "2001:db8:abcd:12::/64");
        assert_eq!(peer_rate_limit_key(None), "local");
    }

    #[test]
    fn api_query_parser_handles_defaults_and_bounds() {
        let parsed =
            parse_public_search_api_query(Some("s=nh%C3%A0+%C4%91%E1%BA%B9p&locale=en&limit=15&page=3"))
                .unwrap();
        assert_eq!(parsed.s, "nhà đẹp");
        assert_eq!(parsed.locale.as_deref(), Some("en"));
        assert_eq!(parsed.limit, 15);
        assert_eq!(parsed.page, 3);

        let defaults = parse_public_search_api_query(Some("")).unwrap();
        assert_eq!(defaults.limit, 10);
        assert_eq!(defaults.page, 1);

        assert!(parse_public_search_api_query(Some("limit=21")).is_err());
        assert!(parse_public_search_api_query(Some("limit=0")).is_err());
        assert!(parse_public_search_api_query(Some("locale=en&locale=vi")).is_err());
        assert!(parse_public_search_api_query(Some("category_id=2")).is_err());
    }
}
