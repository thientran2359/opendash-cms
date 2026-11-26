//! Scoped custom routes for Themes (`functions.js`) and Plugins (`plugin.js` / `plugin.wasm`).
//!
//! Enforces three distinct URL families:
//! - `GET /p/{pkg}` and `GET /p/{pkg}/{*subpath}` (`RouteGet`, `Public`, read-only, 1000ms timeout)
//! - `GET` & `POST /api/p/{pkg}` and `/api/p/{pkg}/{*subpath}` (`Session | Admin`, CSRF + `AdmittedMutation`)
//! - `POST /api/public/p/{pkg}` and `/api/public/p/{pkg}/{*subpath}` (`Public` Origin + RateLimit OR
//!   `Webhook` RateLimit + raw_body/HMAC headers, holding `MutationLease` with `ExecutionPrincipal::Public`)

use super::*;

pub(super) use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) use axum::{
    Extension, Json,
    body::Bytes,
    extract::{ConnectInfo, OriginalUri, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use opendash_core::{
    hooks::{ExecutionPrincipal, HookExecutionMode},
    plugin::{RouteAccess, RouteMethod},
    theme::ResolvedEnqueuedAssets,
};

pub(super) use serde_json::{Value, json};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
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

pub(super) const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

pub(super) const RATE_LIMIT_MAX_REQUESTS: usize = 30;

pub(super) const RATE_LIMIT_MAX_BUCKETS: usize = 4096;

#[derive(Clone, Default)]
pub struct ExtensionRouteRateLimiter {
    pub(super) buckets: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}
