use axum::{
    Router,
    extract::Request,
    http::{StatusCode, header},
    middleware::{self, Next},
    response::IntoResponse,
};

#[derive(Clone, Copy)]
pub(crate) struct HostOriginRejected;

/// Enforces single trusted `Host` header and verified `Origin`/`Referer` bounds on mutations.
pub(crate) fn apply_host_origin_layer<S>(
    router: Router<S>,
    expected_host: String,
    runtime_config: &crate::config::RuntimeConfigV1,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let trusted_host = runtime_config.trusted_host();
    let trusted_origin = runtime_config.trusted_origin_normalized();
    let trusted_is_https = runtime_config.trusted_origin_is_https();

    router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let host_vals: Vec<&str> = request
            .headers()
            .get_all(header::HOST)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        let valid_host = host_vals.len() == 1
            && request.headers().get_all(header::HOST).iter().count() == 1
            && {
                let raw = host_vals[0];
                let clean = !raw.is_empty()
                    && !raw.chars().any(|c| c.is_control() || c.is_whitespace())
                    && !raw.contains('@')
                    && !raw.contains('/');
                let normalized = if trusted_is_https {
                    raw.strip_suffix(":443").unwrap_or(raw)
                } else {
                    raw.strip_suffix(":80").unwrap_or(raw)
                };
                clean
                    && (raw == expected_host.as_str()
                        || trusted_host.as_deref().is_some_and(|th| {
                            normalized.eq_ignore_ascii_case(th) || raw.eq_ignore_ascii_case(th)
                        }))
            };
        let is_mutation = matches!(
            *request.method(),
            axum::http::Method::POST
                | axum::http::Method::PUT
                | axum::http::Method::DELETE
                | axum::http::Method::PATCH
        );

        let mut origin_valid = request.headers().get_all(header::ORIGIN).iter().count() <= 1;
        let is_public_contact_submit = *request.method() == axum::http::Method::POST
            && (request.uri().path().starts_with("/api/public/contact/")
                || request.uri().path().starts_with("/api/public/p/"));
        if origin_valid && is_mutation && !is_public_contact_submit {
            let expected_origin = format!("http://{expected_host}");
            let origin_present = request.headers().contains_key(header::ORIGIN);
            let origin_hdr = request
                .headers()
                .get(header::ORIGIN)
                .and_then(|h| h.to_str().ok());
            if origin_present && origin_hdr.is_none() {
                origin_valid = false;
            }
            if let Some(origin) = origin_hdr {
                let clean = !origin.is_empty()
                    && !origin.eq_ignore_ascii_case("null")
                    && !origin.chars().any(|c| c.is_control() || c.is_whitespace())
                    && !origin.contains('@');
                let allowed = origin == expected_origin
                    || trusted_origin
                        .as_deref()
                        .is_some_and(|to| origin.eq_ignore_ascii_case(to));
                if !clean || !allowed {
                    origin_valid = false;
                }
            } else if trusted_origin.is_some() {
                // Remote/proxy deployment: mandatory Origin header on mutations (no missing-header or Referer loophole).
                origin_valid = false;
            } else if let Some(referer) = request
                .headers()
                .get(header::REFERER)
                .and_then(|h| h.to_str().ok())
            {
                let ref_http = format!("http://{expected_host}/");
                if !referer.starts_with(&ref_http) && referer != expected_origin {
                    origin_valid = false;
                }
            }
        }
        async move {
            if !valid_host {
                let mut response = StatusCode::BAD_REQUEST.into_response();
                response.extensions_mut().insert(HostOriginRejected);
                response
            } else if !origin_valid {
                let mut response = (StatusCode::FORBIDDEN, "Origin không hợp lệ.").into_response();
                response.extensions_mut().insert(HostOriginRejected);
                response
            } else {
                next.run(request).await
            }
        }
    }))
}

/// Injects `X-Robots-Tag: noindex, nofollow` on non-admin responses while recovery safe mode is active.
pub(crate) fn apply_safe_mode_robots_layer<S>(
    router: Router<S>,
    recovery: crate::config::RecoveryState,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let recovery = recovery.clone();
        let path = request.uri().path().to_string();
        async move {
            let mut resp = next.run(request).await;
            if recovery.is_safe_mode() && !path.starts_with("/api/admin/") {
                resp.headers_mut().insert(
                    header::HeaderName::from_static("x-robots-tag"),
                    header::HeaderValue::from_static("noindex, nofollow"),
                );
            }
            resp
        }
    }))
}
