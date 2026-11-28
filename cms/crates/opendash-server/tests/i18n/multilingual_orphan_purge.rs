//! S02: HTTP locale lifecycle and atomic orphan purge with isolated SQLite fixtures.
use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Method, Response, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
struct SiteRoot(PathBuf);

impl SiteRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "opendash-admin-i18n-s02-{}-{stamp}",
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
}

const LOCALES: &str = "/api/admin/locales";
const TOGGLE: &str = "/api/admin/locales/multilingual";
const ORPHANS: &str = "/api/admin/locales/orphans";
const PUBLIC: &str = "/api/admin/settings/public";

impl Admin {
    async fn mutate(
        &self,
        client: &Client,
        server: &Server,
        method: Method,
        path: &str,
        etag: Option<&str>,
        body: Option<Value>,
    ) -> Response {
        let mut request = client
            .request(method, server.url(path))
            .header(header::COOKIE, &self.cookie)
            .header(header::ORIGIN, &server.base)
            .header("X-CSRF-Token", &self.csrf);
        if let Some(etag) = etag {
            request = request.header(header::IF_MATCH, etag);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.unwrap()
    }
}

async fn snapshot(response: Response, status: StatusCode) -> (String, Value) {
    let actual = response.status();
    let headers = response.headers().clone();
    let body = response.text().await.unwrap();
    assert_eq!(actual, status, "{body}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let etag = headers[header::ETAG].to_str().unwrap().to_owned();
    let value: Value = serde_json::from_str(&body).unwrap();
    let version = value["snapshot"]["version"]
        .as_i64()
        .or_else(|| value["version"].as_i64())
        .unwrap();
    assert_eq!(etag, format!("\"v{version}\""));
    (etag, value)
}

async fn error(response: Response, status: StatusCode, code: &str) {
    let actual = response.status();
    let actual_code = response
        .headers()
        .get("x-opendash-error-code")
        .map(|v| v.to_str().unwrap().to_owned());
    let body = response.text().await.unwrap();
    assert_eq!(actual, status, "{body}");
    assert_eq!(actual_code.as_deref(), Some(code), "{body}");
}

async fn add_locale(
    admin: &Admin,
    client: &Client,
    server: &Server,
    etag: &str,
    code: &str,
) -> (String, Value) {
    snapshot(
        admin
            .mutate(
                client,
                server,
                Method::POST,
                LOCALES,
                Some(etag),
                Some(json!({"code":code,"label":code,"direction":"ltr"})),
            )
            .await,
        StatusCode::CREATED,
    )
    .await
}

async fn toggle(
    admin: &Admin,
    client: &Client,
    server: &Server,
    etag: &str,
    enabled: bool,
) -> (String, Value) {
    snapshot(
        admin
            .mutate(
                client,
                server,
                Method::PUT,
                TOGGLE,
                Some(etag),
                Some(json!({"enabled":enabled})),
            )
            .await,
        StatusCode::OK,
    )
    .await
}

async fn delete(
    admin: &Admin,
    client: &Client,
    server: &Server,
    etag: &str,
    code: &str,
) -> (String, Value) {
    snapshot(
        admin
            .mutate(
                client,
                server,
                Method::DELETE,
                &format!("{LOCALES}/{code}"),
                Some(etag),
                None,
            )
            .await,
        StatusCode::OK,
    )
    .await
}

async fn seed_entry(db: &SqlitePool, kind: &str, locale: &str, slug: &str, group: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key,locale,translation_group_id,slug,title,values_json,publication_status)
         VALUES(?,?,?,?,?,'{}','draft') RETURNING id",
    ).bind(kind).bind(locale).bind(group).bind(slug).bind(slug).fetch_one(db).await.unwrap()
}

async fn assert_pattern(db: &SqlitePool, id: i64, locale: &str, slug: &str) {
    let actual: (String, String) = sqlx::query_as(
        "SELECT locale,slug FROM content_entries WHERE id=? AND content_type_key='pattern'",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap();
    assert_eq!(actual, (locale.into(), slug.into()));
}

#[tokio::test]
async fn multilingual_toggle_delete_revive_and_atomic_orphan_purge() {
    let root = SiteRoot::new();
    let client = Client::new();
    let server = Server::start(&root.0, &client).await;
    let admin = Admin::setup(&client, &server).await;
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(root.0.join("database/opendash.sqlite"))
                .foreign_keys(true)
                .busy_timeout(Duration::from_secs(5)),
        )
        .await
        .unwrap();
    let (initial, state) = admin.get(&client, &server, LOCALES).await;
    assert_eq!(state["multilingual_enabled"], false);
    assert_eq!(state["default_code"], "vi");

    let create = json!({"code":"en","label":"English"});
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                LOCALES,
                Some(&initial),
                Some(create.clone()),
            )
            .await,
        StatusCode::CONFLICT,
        "multilingual.disabled",
    )
    .await;
    let (enabled, state) = toggle(&admin, &client, &server, &initial, true).await;
    assert_ne!(enabled, initial);
    assert_eq!(state["multilingual_enabled"], true);
    let (same, _) = toggle(&admin, &client, &server, &enabled, true).await;
    assert_eq!(same, enabled, "idempotent toggle bumped CAS");
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::PUT,
                TOGGLE,
                Some(&initial),
                Some(json!({"enabled":false})),
            )
            .await,
        StatusCode::PRECONDITION_FAILED,
        "request.precondition_failed",
    )
    .await;
    for reserved in ["multilingual", "orphans"] {
        let response = admin
            .mutate(
                &client,
                &server,
                Method::POST,
                LOCALES,
                Some(&enabled),
                Some(json!({"code":reserved,"label":reserved})),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let (etag, _) = add_locale(&admin, &client, &server, &enabled, "en").await;
    let (etag, _) = add_locale(&admin, &client, &server, &etag, "de").await;
    let (etag, _) = add_locale(&admin, &client, &server, &etag, "fr").await;

    // A default-language caller of a Pattern must retain the original target ID
    // even if deleting the Pattern's former locale requires a slug change.
    let long_slug = "a".repeat(64);
    let vi_pattern = seed_entry(&db, "pattern", "vi", &long_slug, "pattern-vi").await;
    let de_pattern = seed_entry(&db, "pattern", "de", &long_slug, "pattern-de").await;
    let caller = seed_entry(&db, "post", "vi", "caller", "caller-vi").await;
    sqlx::query("INSERT INTO entry_pattern_refs(source_entry_id,target_pattern_id) VALUES(?,?)")
        .bind(caller)
        .bind(de_pattern)
        .execute(&db)
        .await
        .unwrap();

    let (etag, deleted) = delete(&admin, &client, &server, &etag, "fr").await;
    assert_eq!(deleted["soft_deleted"], false);
    assert_eq!(deleted["orphan_total"], 0);
    let (etag, deleted) = delete(&admin, &client, &server, &etag, "DE").await;
    assert_eq!(deleted["deleted_code"], "de");
    assert_eq!(deleted["soft_deleted"], false);
    assert_eq!(deleted["orphan_total"], 0);
    assert_pattern(&db, vi_pattern, "vi", &long_slug).await;
    let suffix = format!("-{de_pattern}");
    let reassigned = format!("{}{suffix}", &long_slug[..64 - suffix.len()]);
    assert_pattern(&db, de_pattern, "vi", &reassigned).await;
    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_pattern_refs WHERE source_entry_id=? AND target_pattern_id=?",
    )
    .bind(caller)
    .bind(de_pattern)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(ref_count, 1);
    let removed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM site_locales WHERE code IN ('de','fr')")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(removed, 0);

    error(
        admin
            .mutate(
                &client,
                &server,
                Method::DELETE,
                "/api/admin/locales/vi",
                Some(&etag),
                None,
            )
            .await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "locale.cannot_delete_default",
    )
    .await;

    // A populated secondary locale contains every counted bucket, plus
    // cascading rows. Shared media/menu and Patterns survive permanent purge.
    let en_page = seed_entry(&db, "page", "en", "english-page", "page-en").await;
    let en_pattern = seed_entry(&db, "pattern", "en", &long_slug, "pattern-en").await;
    sqlx::query("INSERT INTO entry_pattern_refs(source_entry_id,target_pattern_id) VALUES(?,?)")
        .bind(caller)
        .bind(en_pattern)
        .execute(&db)
        .await
        .unwrap();
    let parent: i64 = sqlx::query_scalar("INSERT INTO taxonomy_terms(kind,locale,translation_group_id,name,slug) VALUES('category','en','parent-en','Parent','parent') RETURNING id")
        .fetch_one(&db).await.unwrap();
    let child: i64 = sqlx::query_scalar("INSERT INTO taxonomy_terms(kind,locale,translation_group_id,name,slug,parent_id) VALUES('category','en','child-en','Child','child',?) RETURNING id")
        .bind(parent).fetch_one(&db).await.unwrap();
    sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
        .bind(en_page)
        .bind(child)
        .execute(&db)
        .await
        .unwrap();
    let media_id = "a".repeat(64);
    sqlx::query("INSERT INTO media_assets(id,kind,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height) VALUES(?,'image','test.png','image/png','png',100,10,10,10,10)")
        .bind(&media_id).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO entry_seo(entry_id,og_image_id) VALUES(?,?)")
        .bind(en_page)
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO seo_public_media_refs(source_kind,source_id,media_id) VALUES('entry_seo',?,?)",
    )
    .bind(en_page)
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO entry_builder_media_refs(entry_id,media_id,ref_role) VALUES(?,?,'image')",
    )
    .bind(en_page)
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    let menu: i64 = sqlx::query_scalar(
        "INSERT INTO site_menus(menu_key,title) VALUES('s02-shared','Shared menu') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO site_menu_slot_assignments(theme_id,locale,slot_key,menu_id) VALUES('s02-test','en','header',?)")
        .bind(menu).execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO site_menu_slot_versions(theme_id,locale,version) VALUES('s02-test','en',4)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO site_general_settings_locales(locale,site_title,tagline,version) VALUES('en','English title','Tagline',7)")
        .execute(&db).await.unwrap();
    sqlx::query("INSERT INTO site_home_seo(locale,og_image_id) VALUES('en',?)")
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO theme_option_locale_values(owner_kind,owner_id,theme_id,locale,option_key,value_json) VALUES('theme','s02-test','s02-test','en','heading','\"Hello\"')")
        .execute(&db).await.unwrap();
    sqlx::query("INSERT INTO theme_option_locale_versions(owner_kind,owner_id,theme_id,locale,version) VALUES('theme','s02-test','s02-test','en',3)")
        .execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO site_reading_settings(locale,homepage_page_id,version) VALUES('en',?,2)",
    )
    .bind(en_page)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?,'en','/english-page-custom')")
        .bind(en_page).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO entry_redirects(entry_id,locale,source_path) VALUES(?,'en','/old-english-page')")
        .bind(en_page).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO plugins(id,name,version,enabled,manifest_json,wasm_blob) VALUES('s02-plugin','Test','1.0.0',0,'{}',X'')")
        .execute(&db).await.unwrap();
    sqlx::query("INSERT INTO plugin_entry_projections(entry_id,plugin_id,locale,seo_json) VALUES(?,'s02-plugin','en','{}')")
        .bind(en_page).execute(&db).await.unwrap();

    let (disabled, state) = toggle(&admin, &client, &server, &etag, false).await;
    assert_eq!(state["multilingual_enabled"], false);
    assert_eq!(state["items"].as_array().unwrap().len(), 2);
    let claims: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM permalink_route_claims WHERE locale='en'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(claims, 0);
    let persisted: (String, i64) = sqlx::query_as(
        "SELECT site_title,version FROM site_general_settings_locales WHERE locale='en'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(persisted, ("English title".into(), 7));
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                LOCALES,
                Some(&disabled),
                Some(json!({"code":"ja","label":"Japanese"})),
            )
            .await,
        StatusCode::CONFLICT,
        "multilingual.disabled",
    )
    .await;
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                &format!("/api/admin/content-types/post/entries/{caller}/translations"),
                Some("\"v1\""),
                Some(json!({"target_locale":"en","slug":"translated-caller"})),
            )
            .await,
        StatusCode::CONFLICT,
        "multilingual.disabled",
    )
    .await;
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                &format!("/api/admin/content-types/page/entries/{en_page}/translations"),
                Some("\"v1\""),
                Some(json!({"target_locale":"vi","slug":"translated-page"})),
            )
            .await,
        StatusCode::CONFLICT,
        "multilingual.disabled",
    )
    .await;

    let (enabled_again, _) = toggle(&admin, &client, &server, &disabled, true).await;
    let claims: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM permalink_route_claims WHERE locale='en'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(claims > 0, "secondary home claims not restored");
    let (disabled, _) = toggle(&admin, &client, &server, &enabled_again, false).await;

    let (deleted_etag, removed) = delete(&admin, &client, &server, &disabled, "en").await;
    assert_eq!(removed["soft_deleted"], true);
    assert_eq!(removed["snapshot"]["items"].as_array().unwrap().len(), 1);
    let (orphan_etag, orphans) = admin.get(&client, &server, ORPHANS).await;
    assert_eq!(orphan_etag, deleted_etag);
    let counts = &orphans["items"][0]["counts"];
    assert_eq!(orphans["items"][0]["code"], "en");
    assert_eq!(counts["entries"], 1);
    assert_eq!(counts["terms"], 2);
    assert_eq!(counts["settings"], 5);
    assert_eq!(counts["menu_slots"], 2);
    assert!(counts["routes"].as_i64().unwrap() >= 2);
    assert_eq!(counts["projections"], 1);
    assert_eq!(counts["media_seo_refs"], 1);
    assert_eq!(counts["total"], removed["orphan_total"]);
    assert_pattern(&db, en_pattern, "en", &long_slug).await;

    let (public_etag, public) = admin.get(&client, &server, PUBLIC).await;
    error(admin.mutate(&client, &server, Method::PUT, PUBLIC, Some(&public_etag), Some(json!({
        "public_origin":public["public_origin"], "content_language":"en", "indexing_enabled":public["indexing_enabled"]
    }))).await, StatusCode::CONFLICT, "locale.orphaned_conflict").await;
    let (current_public_etag, current_public) = admin.get(&client, &server, PUBLIC).await;
    assert_eq!(current_public_etag, public_etag);
    assert_eq!(current_public["content_language"], "vi");

    let (revive_etag, _) = toggle(&admin, &client, &server, &deleted_etag, true).await;
    let (revived_etag, revived) = add_locale(&admin, &client, &server, &revive_etag, "en").await;
    assert_eq!(revived["items"].as_array().unwrap().len(), 2);
    let (_, orphaned) = admin.get(&client, &server, ORPHANS).await;
    assert_eq!(orphaned["items"].as_array().unwrap().len(), 0);
    assert_pattern(&db, en_pattern, "en", &long_slug).await;
    let persisted_parent: i64 =
        sqlx::query_scalar("SELECT parent_id FROM taxonomy_terms WHERE id=?")
            .bind(child)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(persisted_parent, parent);

    let (disabled, _) = toggle(&admin, &client, &server, &revived_etag, false).await;
    let (purge_etag, _) = delete(&admin, &client, &server, &disabled, "en").await;
    let (_, before_purge) = admin.get(&client, &server, ORPHANS).await;
    sqlx::query("UPDATE site_reading_settings SET homepage_page_id=?,version=10 WHERE locale='vi'")
        .bind(en_page)
        .execute(&db)
        .await
        .unwrap();
    let purge_path = "/api/admin/locales/orphans/en/purge";
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                purge_path,
                Some(&disabled),
                Some(json!({"confirm_code":"en"})),
            )
            .await,
        StatusCode::PRECONDITION_FAILED,
        "request.precondition_failed",
    )
    .await;
    error(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                purge_path,
                Some(&purge_etag),
                Some(json!({"confirm_code":"vi"})),
            )
            .await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "locale.purge_confirm_mismatch",
    )
    .await;
    let (after_bad_confirm, unchanged) = admin.get(&client, &server, ORPHANS).await;
    assert_eq!(after_bad_confirm, purge_etag);
    assert_eq!(unchanged, before_purge);
    let (purged_etag, purged) = snapshot(
        admin
            .mutate(
                &client,
                &server,
                Method::POST,
                purge_path,
                Some(&purge_etag),
                Some(json!({"confirm_code":" EN "})),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_ne!(purged_etag, purge_etag);
    assert_eq!(purged["purged_code"], "en");
    assert_eq!(purged["deleted"], before_purge["items"][0]["counts"]);
    assert_eq!(purged["snapshot"]["items"].as_array().unwrap().len(), 0);
    let suffix = format!("-{en_pattern}");
    let reassigned = format!("{}{suffix}", &long_slug[..64 - suffix.len()]);
    assert_pattern(&db, en_pattern, "vi", &reassigned).await;

    let residuals: (i64,i64,i64,i64,i64,i64,i64,i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM site_locales WHERE code='en'),
                (SELECT COUNT(*) FROM content_entries WHERE locale='en'),
                (SELECT COUNT(*) FROM taxonomy_terms WHERE locale='en'),
                (SELECT COUNT(*) FROM entry_terms WHERE term_id=?1),
                (SELECT COUNT(*) FROM seo_public_media_refs WHERE source_id=?2),
                (SELECT COUNT(*) FROM entry_builder_media_refs WHERE entry_id=?2),
                (SELECT COUNT(*) FROM plugin_entry_projections WHERE locale='en'),
                (SELECT COUNT(*) FROM entry_pattern_refs WHERE source_entry_id=?3 AND target_pattern_id=?4)",
    ).bind(child).bind(en_page).bind(caller).bind(en_pattern).fetch_one(&db).await.unwrap();
    assert_eq!(residuals, (0, 0, 0, 0, 0, 0, 0, 1));
    let shared: (i64,i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM media_assets WHERE id=?), (SELECT COUNT(*) FROM site_menus WHERE id=?)")
        .bind(&media_id).bind(menu).fetch_one(&db).await.unwrap();
    assert_eq!(shared, (1, 1));
    let reading: (Option<i64>, i64) = sqlx::query_as(
        "SELECT homepage_page_id,version FROM site_reading_settings WHERE locale='vi'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(reading, (None, 11));
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&db)
            .await
            .unwrap()
            .is_empty()
    );
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
}
