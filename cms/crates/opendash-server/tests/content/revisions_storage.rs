use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let base_tmp = fs::canonicalize(std::env::temp_dir()).expect("canonical temp dir");
        let dir = base_tmp.join(format!(
            "opendash-rev-s01-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningServer {
    child: Option<Child>,
    root: PathBuf,
    base_url: String,
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let bin = env!("CARGO_BIN_EXE_opendash");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..140 {
            if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
                if resp.status() == StatusCode::OK {
                    return Self {
                        child: Some(child),
                        root: site_root.to_path_buf(),
                        base_url,
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server failed to become ready at {base_url}");
    }

    async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new().filename(self.root.join("database/opendash.sqlite")),
        )
        .await
        .expect("connect test site db")
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

async fn setup_and_login_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, base, "admin", "Password123!").await
}

async fn schema_token(client: &Client, base: &str, cookie: &str, type_key: &str) -> String {
    let resp = client
        .get(format!(
            "{base}/api/admin/content-types/{type_key}/effective-editor-config"
        ))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    body["schema_token"].as_str().unwrap().to_string()
}

async fn seed_autosave(
    db: &SqlitePool,
    entry_id: i64,
    user_id: i64,
    type_key: &str,
    surface: &str,
    base_version: i64,
) {
    sqlx::query(
        "INSERT OR REPLACE INTO entry_autosaves
            (entry_id, user_id, content_type_key, source_surface, base_version, title, content_mode, values_json)
         VALUES (?, ?, ?, ?, ?, 'Autosave WIP', 'editor', '{\"body\":\"<p>WIP</p>\"}')",
    )
    .bind(entry_id)
    .bind(user_id)
    .bind(type_key)
    .bind(surface)
    .bind(base_version)
    .execute(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn matrix_hooks_capture_dedup_autosaves_and_lifecycle_via_http() {
    let dir = TempDirGuard::new("http-matrix");
    let srv = RunningServer::start(dir.path()).await;
    let db = srv.db().await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    let ed_resp = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_resp.status(), StatusCode::CREATED);
    let editor_id = ed_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let admin_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&db)
        .await
        .unwrap();

    let post_token = schema_token(&client, &srv.base_url, &admin_cookie, "post").await;
    let page_token = schema_token(&client, &srv.base_url, &admin_cookie, "page").await;

    // 1. Composite create + update + dedup + per-user autosave isolation.
    let create_post = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/composite",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": &post_token,
            "title": "Post V1",
            "slug": "rev-post",
            "values": { "body": "<p>First body</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_post.status(), StatusCode::CREATED);
    let post_id = create_post.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();

    seed_autosave(&db, post_id, admin_id, "post", "editor", 1).await;
    seed_autosave(&db, post_id, editor_id, "post", "editor", 1).await;

    let save_post = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/composite",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_version": 1,
            "expected_schema_token": &post_token,
            "title": "Post V2",
            "slug": "rev-post",
            "values": { "body": "<p>Second body</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_post.status(), StatusCode::OK);

    // Identical content save must deduplicate (still only 2 revisions).
    let noop_post = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/composite",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_version": 2,
            "expected_schema_token": &post_token,
            "title": "Post V2",
            "slug": "rev-post",
            "values": { "body": "<p>Second body</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(noop_post.status(), StatusCode::OK);

    let post_revs: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT revision_number, source, author_username FROM entry_revisions WHERE entry_id = ? ORDER BY revision_number",
    )
    .bind(post_id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        post_revs,
        vec![
            (1, "create".to_owned(), "admin".to_owned()),
            (2, "save".to_owned(), "admin".to_owned()),
        ]
    );
    let autosave_uids: Vec<i64> =
        sqlx::query_scalar("SELECT user_id FROM entry_autosaves WHERE entry_id = ?")
            .bind(post_id)
            .fetch_all(&db)
            .await
            .unwrap();
    assert_eq!(autosave_uids, vec![editor_id]);

    // 2. Quick Edit & SEO save keep autosaves and record attribution.
    seed_autosave(&db, post_id, admin_id, "post", "editor", 3).await;

    let quick_edit = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/metadata",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .json(&json!({
            "expected_schema_token": &post_token,
            "title": "Post Quick Edited"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(quick_edit.status(), StatusCode::OK);

    let seo_put = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v4\"")
        .json(&json!({
            "title": "SEO Title",
            "description": "SEO Desc",
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(seo_put.status(), StatusCode::OK);

    let latest_sources: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT revision_number, source, author_username FROM entry_revisions WHERE entry_id = ? AND revision_number >= 3 ORDER BY revision_number",
    )
    .bind(post_id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        latest_sources,
        vec![
            (3, "quick_edit".to_owned(), "admin".to_owned()),
            (4, "seo_save".to_owned(), "admin".to_owned()),
        ]
    );
    let preserved_autosaves: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves WHERE entry_id = ?")
            .bind(post_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(preserved_autosaves, 2);

    // 3. Studio Mode 2 Builder save & reset on page.
    let create_page = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/composite",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": &page_token,
            "title": "Studio Page",
            "slug": "studio-page",
            "values": { "body": "<p>Init</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_page.status(), StatusCode::CREATED);
    let page_id = create_page.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();
    seed_autosave(&db, page_id, admin_id, "page", "studio", 1).await;

    let builder_put = client
        .put(format!(
            "{}/api/admin/entries/page/{page_id}/builder",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "schema_version": 1,
            "blocks": [{
                "id": "sec-1",
                "block_id": "core/section",
                "props": {},
                "children": [{
                    "id": "row-1",
                    "block_id": "core/row",
                    "props": {},
                    "children": [{
                        "id": "col-1",
                        "block_id": "core/column",
                        "props": {},
                        "children": [{
                            "id": "hd-1",
                            "block_id": "core/heading",
                            "props": { "text": "Landing Hero", "level": "h2" }
                        }]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(builder_put.status(), StatusCode::OK);

    let builder_reset = client
        .post(format!(
            "{}/api/admin/entries/page/{page_id}/builder/reset",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({ "confirm": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(builder_reset.status(), StatusCode::OK);

    let page_revs: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT revision_number, source, content_mode FROM entry_revisions WHERE entry_id = ? ORDER BY revision_number",
    )
    .bind(page_id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        page_revs,
        vec![
            (1, "create".to_owned(), "editor".to_owned()),
            (2, "builder_save".to_owned(), "builder".to_owned()),
            (3, "builder_reset".to_owned(), "editor".to_owned()),
        ]
    );
    let page_autosaves: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves WHERE entry_id = ?")
            .bind(page_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(page_autosaves, 0);

    // 4. Pattern CRUD, autosave preservation & pattern SEO/route 404 barrier.
    let pat_create = client
        .post(format!("{}/api/admin/patterns", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "title": "Hero Pattern", "slug": "hero-pattern" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pat_create.status(), StatusCode::CREATED);
    let pat_id = pat_create.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    seed_autosave(&db, pat_id, admin_id, "pattern", "studio", 1).await;

    let pat_update = client
        .put(format!("{}/api/admin/patterns/{pat_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "title": "Hero Pattern V2", "slug": "hero-pattern" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pat_update.status(), StatusCode::OK);

    let pat_revs: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT revision_number, source, author_username FROM entry_revisions WHERE entry_id = ? ORDER BY revision_number",
    )
    .bind(pat_id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        pat_revs,
        vec![
            (1, "pattern_save".to_owned(), "admin".to_owned()),
            (2, "pattern_save".to_owned(), "admin".to_owned()),
        ]
    );
    let pat_autosaves: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves WHERE entry_id = ?")
            .bind(pat_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(pat_autosaves, 1);

    let pat_route = client
        .get(format!(
            "{}/api/admin/content-types/pattern/entries/{pat_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(pat_route.status(), StatusCode::OK);
    assert!(pat_route.json::<Value>().await.unwrap()["public_url"].is_null());
    let pat_seo = client
        .get(format!(
            "{}/api/admin/content-types/pattern/entries/{pat_id}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(pat_seo.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        client
            .put(format!(
                "{}/api/admin/content-types/pattern/entries/{pat_id}/seo",
                srv.base_url
            ))
            .header(header::COOKIE, &admin_cookie)
            .header(header::ORIGIN, &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({ "title": "Nope", "noindex": false }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .put(format!(
                "{}/api/admin/content-types/pattern/entries/{pat_id}/route",
                srv.base_url
            ))
            .header(header::COOKIE, &admin_cookie)
            .header(header::ORIGIN, &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({ "custom_path": "/nope" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // 5. Trash clears all autosaves while keeping revisions; Purge cascades revisions.
    let trash_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/trash",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v5\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(trash_resp.status(), StatusCode::OK);

    let post_autosaves_after_trash: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves WHERE entry_id = ?")
            .bind(post_id)
            .fetch_one(&db)
            .await
            .unwrap();
    let post_revs_after_trash: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_revisions WHERE entry_id = ?")
            .bind(post_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(post_autosaves_after_trash, 0);
    assert_eq!(post_revs_after_trash, 4);

    let purge_resp = client
        .delete(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/purge",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v6\"")
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert!(purge_resp.status().is_success());

    let post_revs_after_purge: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_revisions WHERE entry_id = ?")
            .bind(post_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(post_revs_after_purge, 0);
}
