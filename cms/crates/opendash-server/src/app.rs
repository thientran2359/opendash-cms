use std::sync::Arc;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Request},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
};
use rust_embed::RustEmbed;
use sqlx::SqlitePool;

use crate::{
    hooks::HookDispatcher,
    plugin::PluginManager,
    routes::{
        auth::{auth_login, auth_logout, auth_me, auth_setup, create_editor},
        builder::{builder, install, page, preview, registry, save, serve_asset},
        plugin::{
            MAX_PLUGIN_JSON_BYTES, MAX_PLUGIN_ZIP_BYTES, plugin_disable, plugin_enable,
            plugin_install, plugin_list, plugin_run,
        },
        public::public_page,
    },
    theme::MAX_ZIP_BYTES,
};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub plugin_manager: Arc<PluginManager>,
    pub hooks: Arc<HookDispatcher>,
}

impl axum::extract::FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<PluginManager> {
    fn from_ref(state: &AppState) -> Self {
        state.plugin_manager.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<HookDispatcher> {
    fn from_ref(state: &AppState) -> Self {
        state.hooks.clone()
    }
}

#[derive(RustEmbed)]
#[folder = "../../admin/dist/"]
pub struct AdminAssets;

pub fn build_router(state: AppState, expected_host: String) -> Router {
    Router::new()
        .route("/", get(public_page))
        .route("/api/health", get(crate::routes::health::status))
        .route("/admin", get(builder))
        .route("/admin/", get(builder))
        .route("/admin/assets/{*path}", get(serve_asset))
        .route("/builder", get(builder))
        .route("/builder/", get(builder))
        .route("/assets/{*path}", get(serve_asset))
        .route("/auth/setup", post(auth_setup))
        .route("/auth/login", post(auth_login))
        .route("/auth/me", get(auth_me))
        .route("/auth/logout", post(auth_logout))
        .route("/api/admin/users", post(create_editor))
        .route(
            "/api/admin/plugins",
            get(plugin_list)
                .post(plugin_install)
                .layer(DefaultBodyLimit::max(MAX_PLUGIN_ZIP_BYTES)),
        )
        .route(
            "/api/admin/plugins/{id}/run",
            post(plugin_run).layer(DefaultBodyLimit::max(MAX_PLUGIN_JSON_BYTES)),
        )
        .route("/api/admin/plugins/{id}/disable", post(plugin_disable))
        .route("/api/admin/plugins/{id}/enable", post(plugin_enable))
        .route("/registry", get(registry))
        .route("/page", get(page))
        .route("/theme", post(install))
        .route("/preview", post(preview))
        .route("/save", post(save))
        .layer(DefaultBodyLimit::max(MAX_ZIP_BYTES))
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            let valid_host = request
                .headers()
                .get(header::HOST)
                .and_then(|h| h.to_str().ok())
                == Some(expected_host.as_str());

            let is_mutation = matches!(
                *request.method(),
                axum::http::Method::POST
                    | axum::http::Method::PUT
                    | axum::http::Method::DELETE
                    | axum::http::Method::PATCH
            );

            let mut origin_valid = true;
            if is_mutation {
                let expected_origin = format!("http://{expected_host}");

                if let Some(origin) = request
                    .headers()
                    .get(header::ORIGIN)
                    .and_then(|h| h.to_str().ok())
                {
                    if origin != expected_origin {
                        origin_valid = false;
                    }
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
                    StatusCode::BAD_REQUEST.into_response()
                } else if !origin_valid {
                    (StatusCode::FORBIDDEN, "Origin không hợp lệ.").into_response()
                } else {
                    next.run(request).await
                }
            }
        }))
        .with_state(state)
}
