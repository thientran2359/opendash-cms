//! S01: Migration 0051, CMS locale settings, bootstrap and HTTP localization.
use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

const ERROR_CODE: &str = "x-opendash-error-code";
const GENERAL: &str = "/api/admin/settings/general";
const BOOTSTRAP: &str = "/api/i18n/admin";
const AUTH_VI: &str = "Cần đăng nhập.";

async fn schema_count(db: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(db)
        .await
        .unwrap()
}

async fn check_migration(db: &SqlitePool, multilingual: bool, count: i64) {
    let settings: (String, i64, i64) = sqlx::query_as(
        "SELECT cms_locale, multilingual_enabled,
                (SELECT COUNT(*) FROM site_locales WHERE deleted_at IS NOT NULL)
           FROM site_general_settings CROSS JOIN site_locales_state",
    )
    .fetch_one(db)
    .await
    .unwrap();
    assert_eq!(settings, ("en".into(), i64::from(multilingual), 0));
    let after = schema_count(db).await;
    assert_eq!(after, count, "migration 0051 must add zero schema objects");
    assert!(after <= 256, "schema budget exceeded: {after}");
    let fk = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(db)
        .await
        .unwrap();
    assert!(fk.is_empty(), "migration left foreign-key violations");
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[tokio::test]
async fn migration_0051_fresh_and_upgrade_preserve_schema_budget_and_content() {
    let migrator = sqlx::migrate!("../../migrations");
    let fresh = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let upgrade = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    migrator.run_to(50, &upgrade).await.unwrap();
    let before = schema_count(&upgrade).await;
    sqlx::query(
        "INSERT INTO site_locales (code,bcp47_tag,label,direction,is_default,enabled,position)
         VALUES ('en','en','English','ltr',0,1,1)",
    )
    .execute(&upgrade)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO site_general_settings_locales (locale,site_title,tagline,version)
         VALUES ('en','English title','English tagline',7)",
    )
    .execute(&upgrade)
    .await
    .unwrap();

    migrator.run(&fresh).await.unwrap();
    check_migration(&fresh, false, before).await;
    let default_locale: String =
        sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1")
            .fetch_one(&fresh)
            .await
            .unwrap();
    assert_eq!(default_locale, "vi");

    migrator.run(&upgrade).await.unwrap();
    check_migration(&upgrade, true, before).await;
    let preserved: (String, String, i64) = sqlx::query_as(
        "SELECT site_title,tagline,version FROM site_general_settings_locales WHERE locale='en'",
    )
    .fetch_one(&upgrade)
    .await
    .unwrap();
    assert_eq!(
        preserved,
        ("English title".into(), "English tagline".into(), 7)
    );
    let locale_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_locales")
        .fetch_one(&upgrade)
        .await
        .unwrap();
    assert_eq!(locale_count, 2);
}

struct SiteRoot(PathBuf);

impl SiteRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "opendash-admin-i18n-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for SiteRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Server {
    child: Child,
    base: String,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Server {
    async fn start(root: &Path, client: &Client) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", root)
            .env("OPENDASH_PORT", port.to_string())
            .env_remove("OPENDASH_PUBLIC_ORIGIN")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut server = Self {
            child,
            base: format!("http://127.0.0.1:{port}"),
        };
        for _ in 0..120 {
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "server exited before becoming ready"
            );
            if client
                .get(server.url("/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return server;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server did not become healthy: {}", server.base);
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

struct Admin {
    cookie: String,
    csrf: String,
}
impl Admin {
    async fn setup(client: &Client, server: &Server) -> Self {
        let credentials = json!({"username":"admin","password":"correct-horse-battery-staple"});
        let setup = client
            .post(server.url("/auth/setup"))
            .header(header::ORIGIN, &server.base)
            .json(&credentials)
            .send()
            .await
            .unwrap();
        assert_eq!(setup.status(), StatusCode::CREATED);
        let login = client
            .post(server.url("/auth/login"))
            .header(header::ORIGIN, &server.base)
            .json(&credentials)
            .send()
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let body: Value = login.json().await.unwrap();
        Self {
            cookie,
            csrf: body["csrf_token"].as_str().unwrap().to_owned(),
        }
    }

    async fn get(&self, client: &Client, server: &Server, path: &str) -> (String, Value) {
        let response = client
            .get(server.url(path))
            .header(header::COOKIE, &self.cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let etag = response.headers()[header::ETAG]
            .to_str()
            .unwrap()
            .to_owned();
        (etag, response.json().await.unwrap())
    }

    async fn put(
        &self,
        client: &Client,
        server: &Server,
        path: &str,
        etag: &str,
        body: &Value,
    ) -> Response {
        client
            .put(server.url(path))
            .header(header::COOKIE, &self.cookie)
            .header(header::ORIGIN, &server.base)
            .header("X-CSRF-Token", &self.csrf)
            .header(header::IF_MATCH, etag)
            .json(body)
            .send()
            .await
            .unwrap()
    }
}

async fn bootstrap(client: &Client, server: &Server, configured: &str, effective: &str) -> Value {
    let response = client.get(server.url(BOOTSTRAP)).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["configured_locale"], configured);
    assert_eq!(value["effective_locale"], effective);
    assert_eq!(
        value["effective_bcp47"],
        if effective == "vi" { "vi-VN" } else { "en-US" }
    );
    assert_eq!(value["effective_direction"], "ltr");
    assert_eq!(
        value["configured_installed"],
        matches!(configured, "vi" | "en")
    );
    assert_eq!(value["catalog_version"], "v1");
    value
}

async fn localized_error(response: Response, status: StatusCode, code: &str) -> String {
    assert_eq!(response.status(), status, "URL: {}", response.url());
    assert_eq!(
        response
            .headers()
            .get(ERROR_CODE)
            .and_then(|h| h.to_str().ok()),
        Some(code),
        "URL: {}",
        response.url()
    );
    let length = response.headers()[header::CONTENT_LENGTH]
        .to_str()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(!response.headers().contains_key(header::TRANSFER_ENCODING));
    let bytes = response.bytes().await.unwrap();
    assert_eq!(
        length,
        bytes.len(),
        "translated Content-Length must count UTF-8 bytes"
    );
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn auth_scope(client: &Client, server: &Server, expected_vi: bool) -> String {
    let mut messages = Vec::new();
    for path in [
        "/auth/me",
        GENERAL,
        "/api/themes/starter/options",
        "/registry",
        "/page",
    ] {
        let body = localized_error(
            client.get(server.url(path)).send().await.unwrap(),
            StatusCode::UNAUTHORIZED,
            "auth.unauthorized",
        )
        .await;
        if expected_vi {
            assert_eq!(body, AUTH_VI, "{path}");
        } else {
            assert!(
                !body.is_empty() && body.is_ascii() && body != AUTH_VI,
                "{path}: {body}"
            );
        }
        messages.push(body);
    }
    assert!(messages.windows(2).all(|pair| pair[0] == pair[1]));
    messages.remove(0)
}

fn general_input(current: &Value, cms_locale: Option<&str>) -> Value {
    let mut body = json!({
        "site_title":current["site_title"], "tagline":current["tagline"], "timezone":current["timezone"],
    });
    if let Some(code) = cms_locale {
        body["cms_locale"] = json!(code);
    }
    body
}

async fn save_locale(
    client: &Client,
    server: &Server,
    admin: &Admin,
    raw: Option<&str>,
    expected: &str,
) {
    let (etag, current) = admin.get(client, server, GENERAL).await;
    let response = admin
        .put(
            client,
            server,
            GENERAL,
            &etag,
            &general_input(&current, raw),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_ne!(response.headers()[header::ETAG].to_str().unwrap(), etag);
    let saved: Value = response.json().await.unwrap();
    assert_eq!(saved["cms_locale"], expected);
    assert_eq!(
        admin.get(client, server, GENERAL).await.1["cms_locale"],
        expected
    );
}

#[tokio::test]
async fn admin_i18n_bootstrap_settings_http_scope_and_restart() {
    let root = SiteRoot::new();
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let server = Server::start(&root.0, &client).await;
    let fresh = bootstrap(&client, &server, "en", "en").await;
    let locales = fresh["available_locales"].as_array().unwrap();
    assert_eq!(locales.len(), 32);
    assert_eq!(locales[0]["code"], "en");
    assert_eq!(locales[1]["code"], "vi");
    assert!(
        locales[2..]
            .windows(2)
            .all(|pair| pair[0]["code"].as_str() < pair[1]["code"].as_str())
    );
    for item in locales {
        let installed = matches!(item["code"].as_str(), Some("en" | "vi"));
        assert_eq!(item["installed"], installed);
        assert_eq!(
            item["translation_coverage"],
            if installed { 100 } else { 0 }
        );
    }

    let en_message = auth_scope(&client, &server, false).await;
    for path in ["/admin-guide", "/builder-info"] {
        let response = client.get(server.url(path)).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert!(!response.headers().contains_key(ERROR_CODE), "{path}");
    }
    let admin = Admin::setup(&client, &server).await;
    let (original_etag, original) = admin.get(&client, &server, GENERAL).await;
    assert_eq!(original["cms_locale"], "en");

    localized_error(
        admin
            .put(
                &client,
                &server,
                GENERAL,
                &original_etag,
                &general_input(&original, Some("unknown")),
            )
            .await,
        StatusCode::BAD_REQUEST,
        "settings.invalid_cms_locale",
    )
    .await;
    save_locale(&client, &server, &admin, Some(" Vi-VN "), "vi").await;
    bootstrap(&client, &server, "vi", "vi").await;
    auth_scope(&client, &server, true).await;
    localized_error(
        admin
            .put(
                &client,
                &server,
                GENERAL,
                &original_etag,
                &general_input(&original, Some("en")),
            )
            .await,
        StatusCode::PRECONDITION_FAILED,
        "request.precondition_failed",
    )
    .await;
    bootstrap(&client, &server, "vi", "vi").await;
    save_locale(&client, &server, &admin, None, "vi").await;

    let (locale_etag, _) = admin.get(&client, &server, "/api/admin/locales").await;
    let created = client
        .post(server.url("/api/admin/locales"))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &server.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::IF_MATCH, &locale_etag)
        .json(&json!({"code":"en","label":"English","direction":"ltr"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let secondary_path = "/api/admin/settings/general?locale=en";
    let (secondary_etag, secondary) = admin.get(&client, &server, secondary_path).await;
    assert_eq!(secondary["cms_locale"], "vi");
    localized_error(
        admin
            .put(
                &client,
                &server,
                secondary_path,
                &secondary_etag,
                &general_input(&secondary, Some("en")),
            )
            .await,
        StatusCode::BAD_REQUEST,
        "settings.secondary_cms_locale_locked",
    )
    .await;
    let secondary_saved = admin
        .put(
            &client,
            &server,
            secondary_path,
            &secondary_etag,
            &general_input(&secondary, Some("VI")),
        )
        .await;
    assert_eq!(secondary_saved.status(), StatusCode::OK);
    assert_eq!(
        secondary_saved.json::<Value>().await.unwrap()["cms_locale"],
        "vi"
    );
    bootstrap(&client, &server, "vi", "vi").await;

    for configured in ["ja", "ar", "en"] {
        save_locale(&client, &server, &admin, Some(configured), configured).await;
        bootstrap(&client, &server, configured, "en").await;
        assert_eq!(auth_scope(&client, &server, false).await, en_message);
    }

    // Public contact rejects an Origin itself, with no middleware marker; its
    // original Vietnamese body must survive even when the CMS is English.
    let public = client
        .post(server.url("/api/public/contact/1"))
        .header(header::ORIGIN, "https://foreign.example")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("form_version=1&nonce=0123456789abcdef0123456789abcdef")
        .send()
        .await
        .unwrap();
    assert_eq!(public.status(), StatusCode::FORBIDDEN);
    assert!(!public.headers().contains_key(ERROR_CODE));
    assert_eq!(public.text().await.unwrap(), "Origin không hợp lệ.");

    let bad_host = localized_error(
        client
            .get(server.url("/api/health"))
            .header(header::HOST, "foreign.example")
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "request.bad_host",
    )
    .await;
    assert_eq!(bad_host, "Invalid Host header.");
    let bad_origin = localized_error(
        client
            .post(server.url("/auth/login"))
            .header(header::ORIGIN, "https://foreign.example")
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "request.bad_origin",
    )
    .await;
    assert!(bad_origin.is_ascii() && !bad_origin.is_empty());

    let head = client.head(server.url("/auth/me")).send().await.unwrap();
    assert_eq!(head.status(), StatusCode::UNAUTHORIZED);
    assert!(head.headers().contains_key(ERROR_CODE));
    assert!(head.bytes().await.unwrap().is_empty());

    let unsupported = localized_error(
        client
            .post(server.url("/auth/login"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "request.unsupported_media_type",
    )
    .await;
    assert!(unsupported.is_ascii() && !unsupported.is_empty());
    let invalid_json = localized_error(
        client
            .post(server.url("/auth/login"))
            .header(header::CONTENT_TYPE, "application/json")
            .body("{")
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "request.invalid_json",
    )
    .await;
    assert!(invalid_json.is_ascii() && !invalid_json.is_empty());
    let wrong_method = localized_error(
        client.post(server.url("/auth/me")).send().await.unwrap(),
        StatusCode::METHOD_NOT_ALLOWED,
        "request.method_not_allowed",
    )
    .await;
    assert!(!wrong_method.is_empty() && wrong_method.is_ascii());

    save_locale(&client, &server, &admin, Some("vi"), "vi").await;
    let vi_origin = localized_error(
        client
            .post(server.url("/auth/login"))
            .header(header::ORIGIN, "https://foreign.example")
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "request.bad_origin",
    )
    .await;
    assert_eq!(vi_origin, "Origin không hợp lệ.");
    let vi_unsupported = localized_error(
        client
            .post(server.url("/auth/login"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "request.unsupported_media_type",
    )
    .await;
    assert_ne!(vi_unsupported, unsupported);
    assert!(!vi_unsupported.is_ascii());
    let vi_wrong_method = localized_error(
        client.post(server.url("/auth/me")).send().await.unwrap(),
        StatusCode::METHOD_NOT_ALLOWED,
        "request.method_not_allowed",
    )
    .await;
    assert_ne!(vi_wrong_method, wrong_method);
    assert!(!vi_wrong_method.is_ascii());

    drop(server);
    let restarted = Server::start(&root.0, &client).await;
    bootstrap(&client, &restarted, "vi", "vi").await;
    auth_scope(&client, &restarted, true).await;
}
