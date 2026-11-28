//! S02: Multilingual Permalinks, Public Routing, Default-Locale Swap, Media ACL & SEO/Sitemap.
use std::{
    fs,
    io::Cursor,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct IsolatedRoot(PathBuf);

impl IsolatedRoot {
    fn new(label: &str) -> Self {
        let temp = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = temp.join(format!(
            "opendash-s02-{label}-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create isolated root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for IsolatedRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningServer {
    child: Child,
    base_url: String,
    site_root: PathBuf,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .env_remove("OPENDASH_PUBLIC_ORIGIN")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash");
        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..120 {
            if client
                .get(format!("{base_url}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self {
                    child,
                    base_url,
                    site_root: site_root.to_path_buf(),
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server did not become healthy at {base_url}");
    }

    async fn db_rw(&self) -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(self.site_root.join("database/opendash.sqlite"))
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await
            .unwrap()
    }
}

fn no_redirect_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .build()
        .expect("client")
}

fn tiny_png(seed: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let img = RgbaImage::from_pixel(4, 4, Rgba([seed, 90, 150, 255]));
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

fn etag_version(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(header::ETAG)
        .expect("ETag header")
        .to_str()
        .unwrap()
        .to_owned()
}

async fn setup_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "StrongPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": "admin", "password": "StrongPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn add_locale(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    code: &str,
    label: &str,
    direction: &str,
) -> reqwest::Response {
    let get = client
        .get(format!("{base}/api/admin/locales"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let etag = etag_version(&get);
    let enabled = client
        .put(format!("{base}/api/admin/locales/multilingual"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, &etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let etag = etag_version(&enabled);
    client
        .post(format!("{base}/api/admin/locales"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({ "code": code, "label": label, "direction": direction }))
        .send()
        .await
        .unwrap()
}

async fn create_entry_http(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    type_key: &str,
    slug: &str,
    title: &str,
    publish: bool,
) -> (i64, i64) {
    let created = client
        .post(format!("{base}/api/admin/content-types/{type_key}/entries"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .json(&json!({ "slug": slug, "title": title, "values": {} }))
        .send()
        .await
        .unwrap();
    let status = created.status();
    let raw_text = created.text().await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create_entry_http({type_key}, {slug}) failed: {raw_text}"
    );
    let body: Value = serde_json::from_str(&raw_text).unwrap();
    let id = body["id"].as_i64().unwrap();
    let mut ver = body["version"].as_i64().unwrap();
    if publish {
        let pub_resp = client
            .post(format!(
                "{base}/api/admin/content-types/{type_key}/entries/{id}/publication"
            ))
            .header(header::COOKIE, cookie)
            .header("Origin", base)
            .header("X-CSRF-Token", csrf)
            .header(header::IF_MATCH, format!("\"v{ver}\""))
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap();
        assert_eq!(pub_resp.status(), StatusCode::OK);
        ver = pub_resp.json::<Value>().await.unwrap()["version"]
            .as_i64()
            .unwrap();
    }
    (id, ver)
}

async fn sync_permalinks_http(client: &Client, base: &str, cookie: &str, csrf: &str) {
    let get = client
        .get(format!("{base}/api/admin/settings/permalinks"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let cur: Value = get.json().await.unwrap();
    let ver = cur["version"].as_i64().unwrap();
    let put = client
        .put(format!("{base}/api/admin/settings/permalinks"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({
            "post_structure": cur["post_structure"],
            "content_type_bases": cur["content_type_bases"],
            "taxonomy_bases": cur["taxonomy_bases"],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
}

async fn seed_entry_sql(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
    title: &str,
    locale: &str,
    translation_group_id: &str,
) -> i64 {
    let id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES (?, ?, ?, '{}', 'draft', ?, ?)",
    )
    .bind(type_key)
    .bind(slug)
    .bind(title)
    .bind(locale)
    .bind(translation_group_id)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .unwrap();
    id
}

async fn seed_term_sql(
    db: &SqlitePool,
    kind: &str,
    name: &str,
    slug: &str,
    locale: &str,
    translation_group_id: &str,
) -> i64 {
    sqlx::query(
        "INSERT INTO taxonomy_terms(kind, name, slug, locale, translation_group_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(kind)
    .bind(name)
    .bind(slug)
    .bind(locale)
    .bind(translation_group_id)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid()
}

async fn set_reading_http(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    locale: &str,
    homepage_page_id: Option<i64>,
    posts_page_id: Option<i64>,
) {
    let get = client
        .get(format!("{base}/api/admin/settings/reading?locale={locale}"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let etag = etag_version(&get);
    let put = client
        .put(format!("{base}/api/admin/settings/reading?locale={locale}"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({
            "homepage_page_id": homepage_page_id,
            "posts_page_id": posts_page_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), StatusCode::OK);
}

async fn upload_png(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    name: &str,
    seed: u8,
) -> String {
    let resp = client
        .post(format!("{base}/api/admin/media"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", name)
        .body(tiny_png(seed))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn s02_secondary_routing_home_chain_builder_list_pagination_and_archive_isolation() {
    let root = IsolatedRoot::new("routing");
    let srv = RunningServer::start(root.path()).await;
    let client = no_redirect_client();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    assert_eq!(
        add_locale(
            &client,
            &srv.base_url,
            &cookie,
            &csrf,
            "en",
            "English",
            "ltr"
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // Customize General Settings per locale to prove renderers load locale overlays.
    for (loc, site_title) in [("vi", "Trang Chủ Việt"), ("en", "English Home Title")] {
        let g = client
            .get(format!(
                "{}/api/admin/settings/general?locale={loc}",
                srv.base_url
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        let etag = etag_version(&g);
        let tz = g.json::<Value>().await.unwrap()["timezone"]
            .as_str()
            .unwrap()
            .to_owned();
        let p = client
            .put(format!(
                "{}/api/admin/settings/general?locale={loc}",
                srv.base_url
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, etag)
            .json(&json!({ "site_title": site_title, "tagline": "Tag", "timezone": tz }))
            .send()
            .await
            .unwrap();
        assert_eq!(p.status(), StatusCode::OK);
    }

    // Create Home override template to verify secondary /en home runs Home candidate chain.
    let home_override = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "page" },
            "document": {
                "schema_version": 1,
                "blocks": [{
                    "id": "home-sec",
                    "block_id": "core/section",
                    "children": [{
                        "id": "home-row",
                        "block_id": "core/row",
                        "children": [{
                            "id": "home-col",
                            "block_id": "core/column",
                            "children": [
                                { "id": "home-badge", "block_id": "core/heading", "props": { "level": "h1", "text": "Shared Home Candidate Chain" } },
                                { "id": "home-slot", "block_id": "core/content-slot", "props": {} }
                            ]
                        }]
                    }]
                }]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(home_override.status(), StatusCode::CREATED);

    let (vi_home, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "trang-chu",
        "Trang Chủ",
        true,
    )
    .await;
    let (vi_about, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "gioi-thieu",
        "Giới Thiệu",
        true,
    )
    .await;
    let (vi_post_1, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "post",
        "bai-vi-1",
        "Bai Viet VI 1",
        true,
    )
    .await;
    let _ = vi_about;

    // Seed EN homepage, EN about, EN posts, plus matching-slug category terms ("news") in both vi and en.
    let db = srv.db_rw().await;
    let en_home = seed_entry_sql(&db, "page", "home", "EN Home", "en", "tg-home").await;
    let _en_about = seed_entry_sql(&db, "page", "about", "EN About", "en", "tg-about").await;
    let en_post_1 = seed_entry_sql(&db, "post", "en-post-1", "EN Post 1", "en", "tg-p1").await;
    let _en_post_2 = seed_entry_sql(&db, "post", "en-post-2", "EN Post 2", "en", "tg-p2").await;
    let cat_vi = seed_term_sql(&db, "category", "Tin Tuc VI", "news", "vi", "tg-news").await;
    let cat_en = seed_term_sql(&db, "category", "News EN", "news", "en", "tg-news").await;
    sqlx::query("INSERT INTO entry_terms(entry_id, term_id) VALUES (?, ?), (?, ?)")
        .bind(vi_post_1)
        .bind(cat_vi)
        .bind(en_post_1)
        .bind(cat_en)
        .execute(&db)
        .await
        .unwrap();
    drop(db);

    sync_permalinks_http(&client, &srv.base_url, &cookie, &csrf).await;

    // Put Builder core/list (page_size=1) on en_home BEFORE assigning en_home to /en.
    let en_builder_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec-1",
            "block_id": "core/section",
            "children": [{
                "id": "row-1",
                "block_id": "core/row",
                "children": [{
                    "id": "col-1",
                    "block_id": "core/column",
                    "children": [{
                        "id": "list-1",
                        "block_id": "core/list",
                        "props": { "type_key": "post", "page_size": 1, "sort": "id_desc" }
                    }]
                }]
            }]
        }]
    });
    let b_put = client
        .put(format!(
            "{}/api/admin/entries/page/{en_home}/builder",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&en_builder_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(b_put.status(), StatusCode::OK);

    set_reading_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "vi",
        Some(vi_home),
        None,
    )
    .await;
    set_reading_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "en",
        Some(en_home),
        None,
    )
    .await;

    // 1. Default & Secondary Home + Home Candidate Chain + list_page href on /en
    let r_root = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(r_root.status(), StatusCode::OK);
    let root_html = r_root.text().await.unwrap();
    assert!(root_html.contains("<html lang=\"vi\" dir=\"ltr\">"));
    assert!(root_html.contains("Trang Chủ Việt"));
    assert!(root_html.contains("Shared Home Candidate Chain"));

    let r_en_home = client
        .get(format!("{}/en", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(r_en_home.status(), StatusCode::OK);
    let en_html = r_en_home.text().await.unwrap();
    assert!(en_html.contains("<html lang=\"en\" dir=\"ltr\">"));
    assert!(en_html.contains("English Home Title"));
    assert!(en_html.contains("Shared Home Candidate Chain"));
    assert!(
        en_html.contains("href=\"/en?list_page_list-1=2\""),
        "CanonicalUrlBuilder::list_pages must preserve /en base path: {en_html}"
    );
    let r_en_p2 = client
        .get(format!("{}/en?list_page_list-1=2", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(r_en_p2.status(), StatusCode::OK);
    let en_p2_html = r_en_p2.text().await.unwrap();
    assert!(en_p2_html.contains("href=\"/en\""));

    // 2. Trailing slash -> 301 canonical without slash
    let r_en_slash = client
        .get(format!("{}/en/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(r_en_slash.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(r_en_slash.headers()[header::LOCATION], "/en");

    let r_en_about_slash = client
        .get(format!("{}/en/about/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(r_en_about_slash.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(r_en_about_slash.headers()[header::LOCATION], "/en/about");

    // 3. Archive & taxonomy locale isolation (including same slug 'news' in category)
    let vi_posts = client
        .get(format!("{}/posts", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_posts.status(), StatusCode::OK);
    let vi_posts_html = vi_posts.text().await.unwrap();
    assert!(vi_posts_html.contains("Bai Viet VI 1"));
    assert!(!vi_posts_html.contains("EN Post 1"));

    let en_posts = client
        .get(format!("{}/en/posts", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(en_posts.status(), StatusCode::OK);
    let en_posts_html = en_posts.text().await.unwrap();
    assert!(en_posts_html.contains("EN Post 1"));
    assert!(en_posts_html.contains("EN Post 2"));
    assert!(!en_posts_html.contains("Bai Viet VI 1"));

    let vi_cat = client
        .get(format!("{}/category/news", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_cat.status(), StatusCode::OK);
    let vi_cat_html = vi_cat.text().await.unwrap();
    assert!(vi_cat_html.contains("Bai Viet VI 1"));
    assert!(!vi_cat_html.contains("EN Post 1"));

    let en_cat = client
        .get(format!("{}/en/category/news", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(en_cat.status(), StatusCode::OK);
    let en_cat_html = en_cat.text().await.unwrap();
    assert!(en_cat_html.contains("EN Post 1"));
    assert!(!en_cat_html.contains("Bai Viet VI 1"));
}

#[tokio::test]
async fn s02_dynamic_reserved_root_guard_blocks_collisions_and_auto_unique_slug() {
    let root = IsolatedRoot::new("reserved_root");
    let srv = RunningServer::start(root.path()).await;
    let client = no_redirect_client();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    assert_eq!(
        add_locale(
            &client,
            &srv.base_url,
            &cookie,
            &csrf,
            "en",
            "English",
            "ltr"
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let (vi_page, vi_ver) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "fr",
        "French Root Page",
        true,
    )
    .await;

    // Adding locale 'fr' collides with existing default-locale page /fr -> 409 PermalinkConflictError
    let conflict_locale = add_locale(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "fr",
        "French",
        "ltr",
    )
    .await;
    assert_eq!(conflict_locale.status(), StatusCode::CONFLICT);
    let conflict_body: Value = conflict_locale.json().await.unwrap();
    assert!(conflict_body["total_conflicts"].as_i64().unwrap() >= 1);

    // Custom path on default-locale page cannot claim /en or /en/foo
    for bad_path in ["/en", "/en/foo"] {
        let r = client
            .put(format!(
                "{}/api/admin/content-types/page/entries/{vi_page}/route",
                srv.base_url
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, format!("\"v{vi_ver}\""))
            .json(&json!({ "custom_path": bad_path }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            StatusCode::CONFLICT,
            "expected 409 for {bad_path}"
        );
    }

    // auto_unique_slug on default locale avoids enabled secondary root 'en' -> 'en-2'
    let cts: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let page_schema_token = cts
        .iter()
        .find(|item| item["descriptor"]["key"] == "page")
        .unwrap()["schema_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let auto = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/composite",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": page_schema_token,
            "title": "EN Conflict Test",
            "slug": "en",
            "auto_unique_slug": true,
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    let auto_status = auto.status();
    let auto_body = auto.text().await.unwrap();
    assert_eq!(
        auto_status,
        StatusCode::CREATED,
        "composite save failed: {auto_body}"
    );
    let auto_json: Value = serde_json::from_str(&auto_body).unwrap();
    assert_eq!(auto_json["entry"]["slug"], "en-2");

    // Permalink post_structure starting with active secondary locale root /en/%postname% -> 409
    let p_get = client
        .get(format!("{}/api/admin/settings/permalinks", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let p_ver = p_get.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let p_bad = client
        .put(format!("{}/api/admin/settings/permalinks", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{p_ver}\""))
        .json(&json!({
            "post_structure": "/en/%postname%",
            "content_type_bases": {},
            "taxonomy_bases": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(p_bad.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn s02_default_locale_swap_and_locale_swap_vs_canonical_history() {
    let root = IsolatedRoot::new("swap");
    let srv = RunningServer::start(root.path()).await;
    let client = no_redirect_client();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    assert_eq!(
        add_locale(
            &client,
            &srv.base_url,
            &cookie,
            &csrf,
            "en",
            "English",
            "ltr"
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    let (vi_home, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "home",
        "VI Home",
        true,
    )
    .await;
    let (_, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "gioi-thieu",
        "Giới Thiệu",
        true,
    )
    .await;
    let (_, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "post",
        "chi-co-vi",
        "Chỉ Có Tiếng Việt",
        true,
    )
    .await;

    // Create a pre-swap canonical redirect: /promo-cu -> /promo-moi
    let (promo_id, promo_ver) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "post",
        "promo-post",
        "Promo",
        true,
    )
    .await;
    let r1 = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{promo_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{promo_ver}\""))
        .json(&json!({ "custom_path": "/promo-cu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    let v2 = r1.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();
    let r2 = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{promo_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{v2}\""))
        .json(&json!({ "custom_path": "/promo-moi" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);

    // Seed EN entries with matching slug "home" and "about" (note vi also has "home" -> tests collision-free swap!)
    let db = srv.db_rw().await;
    let en_home = seed_entry_sql(&db, "page", "home", "EN Home Page", "en", "tg-home").await;
    let _en_about = seed_entry_sql(&db, "page", "about", "EN About Page", "en", "tg-about").await;
    drop(db);
    sync_permalinks_http(&client, &srv.base_url, &cookie, &csrf).await;

    set_reading_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "vi",
        Some(vi_home),
        None,
    )
    .await;
    set_reading_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "en",
        Some(en_home),
        None,
    )
    .await;

    // Swap default locale vi -> en
    let loc_get = client
        .get(format!("{}/api/admin/locales", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let loc_etag = etag_version(&loc_get);
    let swap = client
        .post(format!("{}/api/admin/locales/en/set-default", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, loc_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(swap.status(), StatusCode::OK);
    let swap_etag = etag_version(&swap);

    // Verify canonical 200 targets after swap
    for (path, expected_lang) in [
        ("/", "en"),
        ("/about", "en"),
        ("/vi", "vi"),
        ("/vi/gioi-thieu", "vi"),
        ("/vi/chi-co-vi", "vi"),
        ("/vi/promo-moi", "vi"),
    ] {
        let resp = client
            .get(format!("{}{path}", srv.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "path {path}");
        let html = resp.text().await.unwrap();
        assert!(
            html.contains(&format!("<html lang=\"{expected_lang}\"")),
            "wrong lang for {path}"
        );
    }

    // Verify locale_swap 301 redirects
    for (old_path, expected_loc) in [
        ("/en", "/"),
        ("/en/about", "/about"),
        ("/gioi-thieu", "/vi/gioi-thieu"),
        ("/chi-co-vi", "/vi/chi-co-vi"),
        // Pre-swap canonical redirect stays single-hop 301 -> new canonical URL
        ("/promo-cu", "/vi/promo-moi"),
    ] {
        let resp = client
            .get(format!("{}{old_path}", srv.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::MOVED_PERMANENTLY,
            "path {old_path}"
        );
        assert_eq!(
            resp.headers()[header::LOCATION],
            expected_loc,
            "path {old_path}"
        );
    }

    // Protected: pre-swap canonical redirect /promo-cu still blocks new EN page
    let (en_test_id, en_test_ver) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "temp-test",
        "Temp",
        true,
    )
    .await;
    let blocked_canonical = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{en_test_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{en_test_ver}\""))
        .json(&json!({ "custom_path": "/promo-cu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked_canonical.status(), StatusCode::CONFLICT);

    // Reclaimable: locale_swap redirect /chi-co-vi CAN be claimed by a new EN page!
    let reclaim_ok = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{en_test_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{en_test_ver}\""))
        .json(&json!({ "custom_path": "/chi-co-vi" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reclaim_ok.status(), StatusCode::OK);
    let served_reclaimed = client
        .get(format!("{}/chi-co-vi", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(served_reclaimed.status(), StatusCode::OK);
    assert!(served_reclaimed.text().await.unwrap().contains("Temp"));

    // Round-trip swap back en -> vi succeeds without 409!
    let swap_back = client
        .post(format!("{}/api/admin/locales/vi/set-default", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, swap_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(swap_back.status(), StatusCode::OK);
    let promo_back = client
        .get(format!("{}/promo-cu", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(promo_back.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(promo_back.headers()[header::LOCATION], "/promo-moi");
}

#[tokio::test]
async fn s02_disable_locale_hides_routes_redirects_and_media_acl_without_data_loss() {
    let root = IsolatedRoot::new("disable_acl");
    let srv = RunningServer::start(root.path()).await;
    let client = no_redirect_client();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    assert_eq!(
        add_locale(
            &client,
            &srv.base_url,
            &cookie,
            &csrf,
            "en",
            "English",
            "ltr"
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // Upload 4 PNGs referenced only by VI content / SEO / template overrides
    let m_entry_seo = upload_png(&client, &srv.base_url, &cookie, &csrf, "vi-seo.png", 10).await;
    let m_home_seo = upload_png(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "vi-home-seo.png",
        20,
    )
    .await;
    let m_builder = upload_png(&client, &srv.base_url, &cookie, &csrf, "vi-builder.png", 30).await;
    let m_override = upload_png(&client, &srv.base_url, &cookie, &csrf, "vi-ov.png", 40).await;

    let (vi_page_id, mut vi_page_ver) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "page",
        "vi-showcase",
        "VI Showcase",
        true,
    )
    .await;
    let (vi_post_id, _) = create_entry_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "post",
        "vi-article",
        "VI Article",
        true,
    )
    .await;

    // Add Builder image ref to vi_page_id
    let b_put = client
        .put(format!(
            "{}/api/admin/entries/page/{vi_page_id}/builder",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{vi_page_ver}\""))
        .json(&json!({
            "schema_version": 1,
            "blocks": [{
                "id": "b-sec",
                "block_id": "core/section",
                "children": [{
                    "id": "b-row",
                    "block_id": "core/row",
                    "children": [{
                        "id": "b-col",
                        "block_id": "core/column",
                        "children": [{
                            "id": "b-img",
                            "block_id": "core/image",
                            "props": { "image": m_builder, "alt": "Builder" }
                        }]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(b_put.status(), StatusCode::OK);
    vi_page_ver = b_put.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    // Add entry_seo image ref + canonical redirect on vi_page_id
    let eseo_put = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{vi_page_id}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{vi_page_ver}\""))
        .json(&json!({
            "title": "VI SEO",
            "description": null,
            "og_image_id": m_entry_seo,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(eseo_put.status(), StatusCode::OK);
    vi_page_ver = eseo_put.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let route_1 = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{vi_page_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{vi_page_ver}\""))
        .json(&json!({ "custom_path": "/old-vi-showcase" }))
        .send()
        .await
        .unwrap();
    assert_eq!(route_1.status(), StatusCode::OK);
    let v_next = route_1.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();
    let route_2 = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{vi_page_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{v_next}\""))
        .json(&json!({ "custom_path": "/new-vi-showcase" }))
        .send()
        .await
        .unwrap();
    assert_eq!(route_2.status(), StatusCode::OK);

    // Pin m_home_seo via site_home_seo (vi)
    let hseo_get = client
        .get(format!("{}/api/admin/seo/home?locale=vi", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let hseo_etag = etag_version(&hseo_get);
    let hseo_put = client
        .put(format!("{}/api/admin/seo/home?locale=vi", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, hseo_etag)
        .json(&json!({
            "title": "VI Home",
            "description": null,
            "og_image_id": m_home_seo,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(hseo_put.status(), StatusCode::OK);

    // Pin m_override via template_override targeting single entry_id = vi_post_id
    let ov_put = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "single", "content_type": "post", "entry_id": vi_post_id },
            "document": {
                "schema_version": 1,
                "blocks": [{
                    "id": "ov-sec",
                    "block_id": "core/section",
                    "children": [{
                        "id": "ov-row",
                        "block_id": "core/row",
                        "children": [{
                            "id": "ov-col",
                            "block_id": "core/column",
                            "children": [
                                { "id": "ov-img", "block_id": "core/image", "props": { "image": m_override, "alt": "OV" } },
                                { "id": "ov-slot", "block_id": "core/content-slot", "props": {} }
                            ]
                        }]
                    }]
                }]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ov_put.status(), StatusCode::CREATED);

    // Confirm all 4 media assets are publicly accessible before disabling VI
    for mid in [&m_entry_seo, &m_home_seo, &m_builder, &m_override] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    // Switch default to 'en' so we can disable 'vi'
    let loc_get = client
        .get(format!("{}/api/admin/locales", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let loc_etag = etag_version(&loc_get);
    let set_en = client
        .post(format!("{}/api/admin/locales/en/set-default", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, loc_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(set_en.status(), StatusCode::OK);
    let etag_after_swap = etag_version(&set_en);

    let db = srv.db_rw().await;
    let redir_count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE source_path LIKE '/vi/%' OR source_path = '/vi'",
    )
    .fetch_one(&db)
    .await
    .unwrap();

    // Disable VI
    let disable_vi = client
        .put(format!("{}/api/admin/locales/vi", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag_after_swap)
        .json(
            &json!({ "label": "Tiếng Việt", "direction": "ltr", "enabled": false, "position": 0 }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(disable_vi.status(), StatusCode::OK);
    let etag_disabled = etag_version(&disable_vi);

    // Disabled locale routes & historical redirects targeting disabled locale -> 404
    for path in [
        "/vi",
        "/vi/new-vi-showcase",
        "/vi/posts",
        "/old-vi-showcase",
    ] {
        assert_eq!(
            client
                .get(format!("{}{path}", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "disabled locale route {path} must return 404"
        );
    }

    // Disabling locale must NOT dump /vi/* into permalink_route_redirects
    let redir_count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE source_path LIKE '/vi/%' OR source_path = '/vi'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(redir_count_after, redir_count_before);
    drop(db);

    // All 4 media assets referenced only by disabled locale -> 404 publicly!
    for mid in [&m_entry_seo, &m_home_seo, &m_builder, &m_override] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "media {mid} only used in disabled locale must return 404"
        );
    }

    // Re-enable VI -> routes, historical redirect, and media access are cleanly restored!
    let enable_vi = client
        .put(format!("{}/api/admin/locales/vi", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag_disabled)
        .json(&json!({ "label": "Tiếng Việt", "direction": "ltr", "enabled": true, "position": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(enable_vi.status(), StatusCode::OK);
    let mut toggle_etag = etag_version(&enable_vi);
    assert_eq!(
        client
            .get(format!("{}/vi/new-vi-showcase", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let restored_redir = client
        .get(format!("{}/old-vi-showcase", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(restored_redir.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        restored_redir.headers()[header::LOCATION],
        "/vi/new-vi-showcase"
    );
    for mid in [&m_entry_seo, &m_home_seo, &m_builder, &m_override] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    // Global toggle must also hide the enabled secondary locale, then restore
    // its route, history and media without rewriting the per-locale flag.
    for enabled in [false, true] {
        let toggle = client
            .put(format!("{}/api/admin/locales/multilingual", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, &toggle_etag)
            .json(&json!({"enabled": enabled}))
            .send()
            .await
            .unwrap();
        assert_eq!(toggle.status(), StatusCode::OK);
        toggle_etag = etag_version(&toggle);
        let state: Value = toggle.json().await.unwrap();
        assert_eq!(state["multilingual_enabled"], enabled);
        assert_eq!(
            state["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["code"] == "vi")
                .unwrap()["enabled"],
            true
        );
        let visible_status = if enabled {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        assert_eq!(
            client
                .get(format!("{}/vi/new-vi-showcase", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            visible_status
        );
        assert_eq!(
            client
                .get(format!("{}/old-vi-showcase", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            if enabled {
                StatusCode::MOVED_PERMANENTLY
            } else {
                StatusCode::NOT_FOUND
            }
        );
        for mid in [&m_entry_seo, &m_home_seo, &m_builder, &m_override] {
            assert_eq!(
                client
                    .get(format!("{}/media/{mid}/file", srv.base_url))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                visible_status,
                "toggle {enabled}, media {mid}"
            );
        }
    }
}

#[tokio::test]
async fn s02_seo_dir_hreflang_x_default_and_sitemap_xhtml() {
    let root = IsolatedRoot::new("seo_sitemap");
    let srv = RunningServer::start(root.path()).await;
    let client = no_redirect_client();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    assert_eq!(
        add_locale(
            &client,
            &srv.base_url,
            &cookie,
            &csrf,
            "en",
            "English",
            "ltr"
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        add_locale(
            &client,
            &srv.base_url,
            &cookie,
            &csrf,
            "ar",
            "Arabic",
            "rtl"
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // Set public_origin + indexing_enabled
    let pub_get = client
        .get(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let pub_ver = pub_get.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let pub_put = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{pub_ver}\""))
        .json(&json!({
            "public_origin": "https://cms.example.com",
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_put.status(), StatusCode::OK);

    // Seed VI & EN homepages (leave AR homepage unconfigured), plus 3 translated about pages & posts
    let db = srv.db_rw().await;
    let vi_home = seed_entry_sql(&db, "page", "home-vi", "Home VI", "vi", "tg-home").await;
    let en_home = seed_entry_sql(&db, "page", "home-en", "Home EN", "en", "tg-home").await;
    let vi_about = seed_entry_sql(&db, "page", "gioi-thieu", "Giới Thiệu", "vi", "tg-about").await;
    let en_about = seed_entry_sql(&db, "page", "about", "About", "en", "tg-about").await;
    let _ar_about = seed_entry_sql(&db, "page", "about-ar", "حول", "ar", "tg-about").await;
    let _vi_post = seed_entry_sql(&db, "post", "tin-1", "Tin 1", "vi", "tg-p1").await;
    let _en_post = seed_entry_sql(&db, "post", "news-1", "News 1", "en", "tg-p1").await;
    drop(db);
    sync_permalinks_http(&client, &srv.base_url, &cookie, &csrf).await;

    set_reading_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "vi",
        Some(vi_home),
        None,
    )
    .await;
    set_reading_http(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "en",
        Some(en_home),
        None,
    )
    .await;

    // 1. RTL attribute & symmetrical hreflang + x-default across Home and About clusters
    let ar_about_html = client
        .get(format!("{}/ar/about-ar", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(ar_about_html.contains("<html lang=\"ar\" dir=\"rtl\">"));
    assert!(ar_about_html.contains(
        "<link rel=\"alternate\" hreflang=\"vi\" href=\"https://cms.example.com/gioi-thieu\">"
    ));
    assert!(ar_about_html.contains(
        "<link rel=\"alternate\" hreflang=\"en\" href=\"https://cms.example.com/en/about\">"
    ));
    assert!(ar_about_html.contains(
        "<link rel=\"alternate\" hreflang=\"ar\" href=\"https://cms.example.com/ar/about-ar\">"
    ));
    assert!(ar_about_html.contains("<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://cms.example.com/gioi-thieu\">"));

    let vi_home_html = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        vi_home_html
            .contains("<link rel=\"alternate\" hreflang=\"vi\" href=\"https://cms.example.com/\">")
    );
    assert!(
        vi_home_html.contains(
            "<link rel=\"alternate\" hreflang=\"en\" href=\"https://cms.example.com/en\">"
        )
    );
    assert!(vi_home_html.contains(
        "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://cms.example.com/\">"
    ));
    assert!(!vi_home_html.contains("hreflang=\"ar\""));

    // 2. Archive hreflang on page 1 vs suppressed on page 2
    let arch_p1 = client
        .get(format!("{}/posts", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(arch_p1.contains(
        "<link rel=\"alternate\" hreflang=\"en\" href=\"https://cms.example.com/en/posts\">"
    ));
    assert!(arch_p1.contains(
        "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://cms.example.com/posts\">"
    ));
    let arch_p2 = client
        .get(format!("{}/posts?page=2", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!arch_p2.contains("rel=\"alternate\" hreflang"));

    // 3. Mark default-locale vi_about noindex=true -> omitted from cluster + no x-default, while en/ar retain pair!
    let vi_seo = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{vi_about}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "noindex": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_seo.status(), StatusCode::OK);

    let vi_about_html = client
        .get(format!("{}/gioi-thieu", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!vi_about_html.contains("rel=\"alternate\" hreflang"));

    let en_about_html = client
        .get(format!("{}/en/about", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(en_about_html.contains("hreflang=\"en\""));
    assert!(en_about_html.contains("hreflang=\"ar\""));
    assert!(!en_about_html.contains("hreflang=\"vi\""));
    assert!(!en_about_html.contains("hreflang=\"x-default\""));

    // 4. Non-self canonical on en_about drops it from sitemap & siblings
    let en_seo = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{en_about}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "canonical_url": "https://cms.example.com/other-canonical", "noindex": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(en_seo.status(), StatusCode::OK);
    let en_about_non_self = client
        .get(format!("{}/en/about", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!en_about_non_self.contains("rel=\"alternate\" hreflang"));

    // Restore vi_about & en_about to indexable/self-canonical for sitemap check
    let _ = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{vi_about}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({ "noindex": false }))
        .send()
        .await
        .unwrap();
    let _ = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{en_about}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({ "canonical_url": null, "noindex": true }))
        .send()
        .await
        .unwrap();

    // 5. Verify /sitemaps/1.xml emits xhtml namespace & alternates, omitting noindex en_about and unconfigured AR home
    let sm_resp = client
        .get(format!("{}/sitemaps/1.xml", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(sm_resp.status(), StatusCode::OK);
    let sm_xml = sm_resp.text().await.unwrap();
    assert!(sm_xml.contains("xmlns:xhtml=\"http://www.w3.org/1999/xhtml\""));
    assert!(sm_xml.contains("<loc>https://cms.example.com/</loc>"));
    assert!(sm_xml.contains("<loc>https://cms.example.com/en</loc>"));
    assert!(!sm_xml.contains("<loc>https://cms.example.com/ar</loc>"));
    assert!(!sm_xml.contains("<loc>https://cms.example.com/en/about</loc>"));
    assert!(sm_xml.contains("<xhtml:link rel=\"alternate\" hreflang=\"ar\" href=\"https://cms.example.com/ar/about-ar\"/>"));
    assert!(!sm_xml.contains(
        "<xhtml:link rel=\"alternate\" hreflang=\"en\" href=\"https://cms.example.com/en/about\"/>"
    ));

    // 6. Setting public_origin = null disables hreflang links while retaining dir="rtl"
    let pub_clear = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{}\"", pub_ver + 1))
        .json(&json!({
            "public_origin": null,
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_clear.status(), StatusCode::OK);
    let ar_no_origin = client
        .get(format!("{}/ar/about-ar", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(ar_no_origin.contains("<html lang=\"ar\" dir=\"rtl\">"));
    assert!(!ar_no_origin.contains("rel=\"alternate\" hreflang"));
}
