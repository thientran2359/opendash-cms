mod handlers;
mod types;

pub use handlers::*;
pub use types::*;

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
