use std::{path::PathBuf, sync::Arc};

use axum::{
    Extension, Router,
    extract::{DefaultBodyLimit, Request},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
};
use opendash_core::builder::MAX_BUILDER_JSON_BYTES;
use rust_embed::RustEmbed;
use sqlx::SqlitePool;

use crate::{
    hooks::HookDispatcher,
    media::{MAX_MEDIA_BYTES, MediaStore},
    media_video::MAX_VIDEO_BYTES,
    package::V2_MAX_ZIP_BYTES,
    plugin::PluginManager,
    routes::{
        auth::{auth_login, auth_logout, auth_me, auth_setup},
        builder::{
            builder, homepage_get, homepage_preview, homepage_put, install, override_get,
            override_preview, override_put, override_reset, overrides_list, page, preview,
            registry, save, serve_asset,
        },
        content::{MAX_CONTENT_TYPE_JSON_BYTES, content_type_create, content_type_list},
        dashboard::dashboard_get,
        entries::{
            MAX_CONTENT_ENTRY_JSON_BYTES, entry_create, entry_delete, entry_get, entry_list,
            entry_preview, entry_publication, entry_update,
        },
        media::{media_list, media_original, media_thumbnail, media_upload, media_video_upload},
        menu::{
            MAX_MENU_JSON_BYTES, menu_slots_get, menu_slots_put, menus_create, menus_delete,
            menus_get, menus_list, menus_preview, menus_put,
        },
        import::{import_dry_run, import_execute},
        plugin::{
            MAX_PLUGIN_JSON_BYTES, MAX_PLUGIN_ZIP_BYTES, plugin_disable, plugin_enable,
            plugin_action, plugin_install, plugin_list, plugin_run, plugin_ui_schema,
        },
        public::public_page,
        public_content::{archive, category_archive, media_file, media_thumb, single, tag_archive},
        settings::{
            general_get, general_put, public_get, public_put, theme_options_get,
            theme_options_put, timezones_get,
        },
        taxonomy::{
            MAX_TAXONOMY_JSON_BYTES, taxonomy_create, taxonomy_delete, taxonomy_list,
            taxonomy_patch, taxonomy_eligibility_list,
        },
        themes::{ThemeStoreRoot, theme_activate, theme_asset, theme_install, theme_list},
        users::{
            max_users_json_bytes, user_create, user_delete, user_list, user_patch,
            user_reset_password,
        },
    },
    theme::MAX_ZIP_BYTES,
};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub plugin_manager: Arc<PluginManager>,
    pub hooks: Arc<HookDispatcher>,
    pub media_store: Arc<MediaStore>,
    pub content_registry: Arc<tokio::sync::RwLock<opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>>>,
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

impl axum::extract::FromRef<AppState> for Arc<MediaStore> {
    fn from_ref(state: &AppState) -> Self {
        state.media_store.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<tokio::sync::RwLock<opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>>> {
    fn from_ref(state: &AppState) -> Self {
        state.content_registry.clone()
    }
}

#[derive(RustEmbed)]
#[folder = "../../admin/dist/"]
pub struct AdminAssets;

pub fn build_router(state: AppState, expected_host: String) -> Router {
    let themes_root = crate::paths::SitePaths::from_env()
        .map(|p| p.themes_dir())
        .unwrap_or_else(|_| PathBuf::from("themes"));
    build_router_with_themes_root(state, expected_host, themes_root)
}

pub fn build_router_with_themes_root(
    state: AppState,
    expected_host: String,
    themes_root: PathBuf,
) -> Router {
    let smtp_secrets_dir = crate::paths::SitePaths::from_env()
        .map(|p| p.smtp_secrets_dir())
        .unwrap_or_else(|_| PathBuf::from("database/secrets/smtp"));
    let secret_store = crate::secrets::SecretStore::new(smtp_secrets_dir);
    let smtp_service = crate::smtp::SmtpService::new(state.db.clone(), secret_store.clone());

    Router::new()
        .route("/", get(public_page))
        .route("/content/{type_key}", get(archive))
        .route("/content/{type_key}/{slug}", get(single))
        .route("/taxonomy/category/{slug}", get(category_archive))
        .route("/taxonomy/tag/{slug}", get(tag_archive))
        .route("/media/{id}/file", get(media_file))
        .route("/media/{id}/thumbnail", get(media_thumb))
        .route("/theme/{revision_digest}/{*path}", get(theme_asset))
        .route("/api/health", get(crate::routes::health::status))
        .route("/api/public/contact/received", get(crate::routes::contact::public_received))
        .route(
            "/api/public/contact/{id}",
            post(crate::routes::contact::public_submit)
                .layer(DefaultBodyLimit::max(crate::routes::contact::MAX_PUBLIC_CONTACT_FORM_BYTES)),
        )
        .route(
            "/api/admin/contact/forms",
            get(crate::routes::contact::admin_list_forms)
                .post(crate::routes::contact::admin_create_form)
                .layer(DefaultBodyLimit::max(crate::routes::contact::MAX_ADMIN_FORM_JSON_BYTES)),
        )
        .route(
            "/api/admin/contact/forms/{id}",
            get(crate::routes::contact::admin_get_form)
                .put(crate::routes::contact::admin_update_form)
                .delete(crate::routes::contact::admin_delete_form)
                .layer(DefaultBodyLimit::max(crate::routes::contact::MAX_ADMIN_FORM_JSON_BYTES)),
        )
        .route(
            "/api/admin/contact/submissions",
            get(crate::routes::contact::admin_list_submissions),
        )
        .route(
            "/api/admin/contact/submissions/{id}",
            get(crate::routes::contact::admin_get_submission)
                .delete(crate::routes::contact::admin_delete_submission),
        )
        .route("/robots.txt", get(crate::routes::sitemap::robots_txt))
        .route("/sitemap.xml", get(crate::routes::sitemap::sitemap_index))
        .route("/sitemaps/{part}", get(crate::routes::sitemap::sitemap_part))
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
        .route("/api/admin/dashboard", get(dashboard_get))
        .route(
            "/api/admin/builder/homepage",
            get(homepage_get)
                .put(homepage_put)
                .layer(DefaultBodyLimit::max(MAX_BUILDER_JSON_BYTES)),
        )
        .route(
            "/api/admin/builder/homepage/preview",
            post(homepage_preview).layer(DefaultBodyLimit::max(MAX_BUILDER_JSON_BYTES)),
        )
        .route("/api/admin/builder/overrides", get(overrides_list))
        .route(
            "/api/admin/builder/overrides/{role}",
            get(override_get)
                .put(override_put)
                .layer(DefaultBodyLimit::max(MAX_BUILDER_JSON_BYTES)),
        )
        .route(
            "/api/admin/builder/overrides/{role}/preview",
            post(override_preview).layer(DefaultBodyLimit::max(MAX_BUILDER_JSON_BYTES)),
        )
        .route(
            "/api/admin/builder/overrides/{role}/reset",
            post(override_reset).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/admin/settings/general",
            get(general_get)
                .put(general_put)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/admin/settings/public",
            get(public_get)
                .put(public_put)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route("/api/admin/settings/timezones", get(timezones_get))
        .route(
            "/api/admin/menus",
            get(menus_list)
                .post(menus_create)
                .layer(DefaultBodyLimit::max(MAX_MENU_JSON_BYTES)),
        )
        .route(
            "/api/admin/menus/{id}",
            get(menus_get)
                .put(menus_put)
                .delete(menus_delete)
                .layer(DefaultBodyLimit::max(MAX_MENU_JSON_BYTES)),
        )
        .route(
            "/api/admin/menus/{id}/preview",
            post(menus_preview).layer(DefaultBodyLimit::max(MAX_MENU_JSON_BYTES)),
        )
        .route(
            "/api/admin/menu-slots/{theme_id}",
            get(menu_slots_get)
                .put(menu_slots_put)
                .layer(DefaultBodyLimit::max(MAX_MENU_JSON_BYTES)),
        )
        .route(
            "/api/admin/taxonomy/terms",
            get(taxonomy_list)
                .post(taxonomy_create)
                .layer(DefaultBodyLimit::max(MAX_TAXONOMY_JSON_BYTES)),
        )
        .route(
            "/api/admin/taxonomy/terms/{id}",
            axum::routing::patch(taxonomy_patch)
                .delete(taxonomy_delete)
                .layer(DefaultBodyLimit::max(MAX_TAXONOMY_JSON_BYTES)),
        )
        .route(
            "/api/admin/taxonomy/eligibility/{content_type_key}",
            get(taxonomy_eligibility_list),
        )
        .route(
            "/api/admin/themes",
            get(theme_list)
                .post(theme_install)
                .layer(DefaultBodyLimit::max(V2_MAX_ZIP_BYTES)),
        )
        .route(
            "/api/admin/themes/active",
            axum::routing::put(theme_activate).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/themes/{theme_id}/options",
            get(theme_options_get)
                .put(theme_options_put)
                .layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/themes/{theme_id}/import/dry-run",
            post(import_dry_run).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/themes/{theme_id}/import/execute",
            post(import_execute).layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/admin/users",
            get(user_list)
                .post(user_create)
                .layer(DefaultBodyLimit::max(max_users_json_bytes())),
        )
        .route(
            "/api/admin/users/{id}",
            axum::routing::patch(user_patch)
                .delete(user_delete)
                .layer(DefaultBodyLimit::max(max_users_json_bytes())),
        )
        .route(
            "/api/admin/users/{id}/password",
            post(user_reset_password).layer(DefaultBodyLimit::max(max_users_json_bytes())),
        )
        .route(
            "/api/admin/content-types",
            get(content_type_list)
                .post(content_type_create)
                .layer(DefaultBodyLimit::max(MAX_CONTENT_TYPE_JSON_BYTES)),
        )
        .route(
            "/api/admin/content-types/{type_key}/entries",
            get(entry_list)
                .post(entry_create)
                .layer(DefaultBodyLimit::max(MAX_CONTENT_ENTRY_JSON_BYTES)),
        )
        .route(
            "/api/admin/content-types/{type_key}/entries/{id}",
            get(entry_get)
                .put(entry_update)
                .delete(entry_delete)
                .layer(DefaultBodyLimit::max(MAX_CONTENT_ENTRY_JSON_BYTES)),
        )
        .route(
            "/api/admin/content-types/{type_key}/entries/{id}/publication",
            post(entry_publication).layer(DefaultBodyLimit::max(MAX_CONTENT_ENTRY_JSON_BYTES)),
        )
        .route(
            "/api/admin/content-types/{type_key}/entries/{id}/preview",
            get(entry_preview),
        )
        .route(
            "/api/admin/content-types/{type_key}/entries/{id}/route",
            get(crate::routes::seo::get_entry_route)
                .put(crate::routes::seo::put_entry_route)
                .layer(DefaultBodyLimit::max(crate::routes::seo::MAX_ROUTE_JSON_BYTES)),
        )
        .route(
            "/api/admin/content-types/{type_key}/entries/{id}/seo",
            get(crate::routes::seo::get_entry_seo)
                .put(crate::routes::seo::put_entry_seo)
                .layer(DefaultBodyLimit::max(crate::routes::seo::MAX_SEO_JSON_BYTES)),
        )
        .route(
            "/api/admin/seo/home",
            get(crate::routes::seo::get_home_seo)
                .put(crate::routes::seo::put_home_seo)
                .layer(DefaultBodyLimit::max(crate::routes::seo::MAX_SEO_JSON_BYTES)),
        )
        .route(
            "/api/admin/media/video",
            post(media_video_upload).layer(DefaultBodyLimit::max(MAX_VIDEO_BYTES)),
        )
        .route(
            "/api/admin/media",
            get(media_list)
                .post(media_upload)
                .layer(DefaultBodyLimit::max(MAX_MEDIA_BYTES)),
        )
        .route("/api/admin/media/{id}/file", get(media_original))
        .route("/api/admin/media/{id}/thumbnail", get(media_thumbnail))
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
        .route(
            "/api/admin/plugins/{id}/actions/{action}",
            post(plugin_action).layer(DefaultBodyLimit::max(MAX_PLUGIN_JSON_BYTES)),
        )
        .route("/api/admin/plugins/{id}/ui-schema", get(plugin_ui_schema))
        .route("/api/admin/plugins/{id}/disable", post(plugin_disable))
        .route("/api/admin/plugins/{id}/enable", post(plugin_enable))
        .route(
            "/api/admin/settings/smtp",
            get(crate::routes::smtp::get_smtp_settings)
                .put(crate::routes::smtp::put_smtp_settings)
                .layer(DefaultBodyLimit::max(crate::routes::smtp::MAX_SMTP_SETTINGS_JSON_BYTES)),
        )
        .route(
            "/api/admin/settings/smtp/test",
            post(crate::routes::smtp::test_smtp_settings),
        )
        .route(
            "/api/admin/contact/submissions/{id}/notify",
            post(crate::routes::smtp::notify_submission)
                .layer(DefaultBodyLimit::max(crate::routes::smtp::MAX_SMTP_SETTINGS_JSON_BYTES)),
        )
        .route("/registry", get(registry))
        .route("/page", get(page))
        .route("/theme", post(install))
        .route("/preview", post(preview))
        .route("/save", post(save))
        .layer(DefaultBodyLimit::max(MAX_ZIP_BYTES))
        .layer(axum::Extension(secret_store))
        .layer(axum::Extension(smtp_service))
        .layer(axum::Extension(crate::contact::ContactAdmissionLimiter::new()))
        .layer(axum::Extension(crate::routes::contact::ExpectedListenerOrigin(format!(
            "http://{expected_host}"
        ))))
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
            let is_public_contact_submit = *request.method() == axum::http::Method::POST
                && request.uri().path().starts_with("/api/public/contact/");
            if is_mutation && !is_public_contact_submit {
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
        .fallback(crate::routes::seo::public_fallback)
        .layer(Extension(ThemeStoreRoot(themes_root)))
        .with_state(state)
}
