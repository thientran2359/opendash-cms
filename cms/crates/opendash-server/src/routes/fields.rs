//! HTTP endpoints for unified dynamic select sources and entry resolution.
//! SQL queries remain isolated in `storage/fields.rs`.

use std::collections::BTreeSet;

use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use opendash_core::{
    content::FieldRole,
    field_schema::SelectOptionsSource,
    id::{valid_lowercase_locale_code, valid_snake_key},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{Role, require_session},
    error::{ApiError, ApiResult},
    routes::themes::ThemeStoreRoot,
    storage::fields::{
        MAX_ENTRY_RESOLVE_IDS, OptionsSourceResponse, list_dynamic_options, resolve_entries_by_ids,
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsSourceQuery {
    pub source: SelectOptionsSource,
    pub type_key: Option<String>,
    pub locale: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntriesResolveQuery {
    pub ids: String,
}

fn security_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers
}

fn parse_entry_resolve_ids(raw: &str) -> ApiResult<Vec<i64>> {
    if raw.is_empty() || raw.len() > 2000 {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    let mut ids = Vec::new();
    let mut unique = BTreeSet::new();
    for segment in raw.split(',') {
        if ids.len() == MAX_ENTRY_RESOLVE_IDS
            || segment.is_empty()
            || segment.starts_with('0')
            || !segment.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
        }
        let id = segment
            .parse::<i64>()
            .map_err(|_| ApiError::bad_request("ID nội dung không hợp lệ."))?;
        if id <= 0 || !unique.insert(id) {
            return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
        }
        ids.push(id);
    }
    Ok(ids)
}

pub async fn options_source_get(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    headers: HeaderMap,
    Query(query): Query<OptionsSourceQuery>,
) -> ApiResult<Response> {
    let session = require_session(&headers, &db).await?;
    let type_key = query
        .type_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(tk) = type_key
        && !valid_snake_key(tk)
    {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    let locale = query
        .locale
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(loc) = locale
        && !valid_lowercase_locale_code(loc)
    {
        return Err(ApiError::bad_request("Tham số locale không hợp lệ."));
    }
    let actor_role = match session.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    };
    let items = list_dynamic_options(
        &db,
        &themes_root.0,
        query.source,
        type_key,
        locale,
        actor_role,
    )
    .await?;
    Ok((
        security_headers(),
        Json(OptionsSourceResponse {
            source: query.source,
            items,
        }),
    )
        .into_response())
}

pub async fn entries_resolve_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<EntriesResolveQuery>,
) -> ApiResult<Response> {
    let _session = require_session(&headers, &db).await?;
    let ids = parse_entry_resolve_ids(&query.ids)?;
    let resolved = resolve_entries_by_ids(&db, &ids).await?;
    Ok((security_headers(), Json(resolved)).into_response())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(1);
            let base =
                fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
            let dir = base.join(format!(
                "opendash-route-fields-{prefix}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn fields_endpoints_require_session_validate_inputs_and_set_headers() {
        let tmp = TempDir::new("http");
        fs::create_dir_all(tmp.0.join("themes")).unwrap();
        fs::create_dir_all(tmp.0.join("plugins")).unwrap();
        fs::create_dir_all(tmp.0.join("uploads")).unwrap();
        let paths = crate::paths::SitePaths::from_parts(
            tmp.0.clone(),
            tmp.0.join("database/opendash.sqlite"),
        )
        .canonicalize_for_runtime()
        .unwrap();
        let coordinator =
            Arc::new(crate::operations::SiteCoordinator::acquire(paths.clone()).unwrap());
        let db = crate::storage::init_db(paths.db_path()).await.unwrap();
        let plugin_manager = Arc::new(crate::plugin::PluginManager::with_roots(
            db.clone(),
            false,
            paths.plugins_dir(),
            paths.secrets_dir(),
        ));
        let hooks = Arc::new(crate::hooks::HookDispatcher::new(plugin_manager.clone()));
        let content_registry = Arc::new(tokio::sync::RwLock::new(
            crate::storage::content::load_content_type_catalog(&db)
                .await
                .unwrap(),
        ));
        let runtime_config = crate::config::RuntimeConfigV1::load(&paths).unwrap();
        let state = crate::app::AppState {
            db: db.clone(),
            ui_locale: Arc::new(std::sync::RwLock::new(None)),
            plugin_manager,
            hooks,
            media_store: Arc::new(crate::media::MediaStore::new(paths.uploads_dir())),
            content_registry,
            coordinator,
            backup_registry: Arc::new(crate::backup::BackupRuntimeRegistry::new()),
            block_template_cache: crate::page::BlockTemplateCache::global().clone(),
            site_cache_epoch: Arc::clone(crate::cache::SiteCacheEpoch::global()),
            render_cache: Arc::new(crate::cache::ExtensionRenderCache::new()),
            catalog_registry: Arc::new(crate::l10n::CatalogRegistry::new_empty()),
            recovery: crate::config::RecoveryState::new(false),
            runtime_config,
        };

        let default_locale: String =
            sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1 LIMIT 1")
                .fetch_one(&db)
                .await
                .unwrap();
        let entry_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
             VALUES ('post', ?1, 'tg', 'first-post', 'First Post', '{}', 'draft') RETURNING id",
        )
        .bind(&default_locale)
        .fetch_one(&db)
        .await
        .unwrap();
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
            .bind(entry_id)
            .execute(&db)
            .await
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected_host = format!("127.0.0.1:{port}");
        let origin = format!("http://{expected_host}");
        let router = crate::app::build_router_with_themes_root(
            state,
            expected_host.clone(),
            paths.themes_dir(),
        );
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let client = reqwest::Client::new();

        // Unauthenticated requests -> 401
        let unauth1 = client
            .get(format!(
                "{origin}/api/admin/fields/options-source?source=content_types"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(unauth1.status(), reqwest::StatusCode::UNAUTHORIZED);
        let unauth2 = client
            .get(format!(
                "{origin}/api/admin/fields/entries-resolve?ids={entry_id}"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(unauth2.status(), reqwest::StatusCode::UNAUTHORIZED);

        // Setup & login as admin
        client
            .post(format!("{origin}/auth/setup"))
            .header("Origin", &origin)
            .json(&serde_json::json!({ "username": "admin", "password": "AdminPassword123!" }))
            .send()
            .await
            .unwrap();
        let login = client
            .post(format!("{origin}/auth/login"))
            .header("Origin", &origin)
            .json(&serde_json::json!({ "username": "admin", "password": "AdminPassword123!" }))
            .send()
            .await
            .unwrap();
        let cookie = login
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        // Bad query parameters -> 400
        let bad_type = client
            .get(format!(
                "{origin}/api/admin/fields/options-source?source=context_fields&type_key=BAD!"
            ))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(bad_type.status(), reqwest::StatusCode::BAD_REQUEST);

        let too_many_ids: Vec<String> = (1..=51).map(|i| i.to_string()).collect();
        let bad_ids = client
            .get(format!(
                "{origin}/api/admin/fields/entries-resolve?ids={}",
                too_many_ids.join(",")
            ))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(bad_ids.status(), reqwest::StatusCode::BAD_REQUEST);

        // Valid requests -> 200 + Cache-Control: no-store + X-Content-Type-Options: nosniff
        let ok_opts = client
            .get(format!(
                "{origin}/api/admin/fields/options-source?source=content_types"
            ))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(ok_opts.status(), reqwest::StatusCode::OK);
        assert_eq!(
            ok_opts
                .headers()
                .get(reqwest::header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "no-store"
        );
        assert_eq!(
            ok_opts
                .headers()
                .get(reqwest::header::X_CONTENT_TYPE_OPTIONS)
                .unwrap()
                .to_str()
                .unwrap(),
            "nosniff"
        );
        let opts_json: serde_json::Value = ok_opts.json().await.unwrap();
        assert_eq!(opts_json["source"], "content_types");
        assert!(
            opts_json["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["value"] == "post")
        );

        let ok_res = client
            .get(format!(
                "{origin}/api/admin/fields/entries-resolve?ids={entry_id}"
            ))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(ok_res.status(), reqwest::StatusCode::OK);
        assert_eq!(
            ok_res
                .headers()
                .get(reqwest::header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "no-store"
        );
        assert_eq!(
            ok_res
                .headers()
                .get(reqwest::header::X_CONTENT_TYPE_OPTIONS)
                .unwrap()
                .to_str()
                .unwrap(),
            "nosniff"
        );
        let resolved_json: serde_json::Value = ok_res.json().await.unwrap();
        assert_eq!(resolved_json.as_array().unwrap().len(), 1);
        assert_eq!(resolved_json[0]["id"], entry_id);
        assert_eq!(resolved_json[0]["title"], "First Post");

        server_task.abort();
    }
}
