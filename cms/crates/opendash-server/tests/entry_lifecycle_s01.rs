//! S01: Entry lifecycle foundation, additive migration 0053, full public/writer visibility
//! closure, query-plan verification, backup/restore compatibility, and old-binary refusal proof.
//! Every test owns a disposable site root and an ephemeral loopback port (never touches 18964).

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

struct Site {
    root: PathBuf,
    child: Child,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn temp_root(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("{prefix}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    root
}

impl Site {
    async fn start(prefix: &str) -> Self {
        Self::start_with_root(temp_root(prefix)).await
    }

    async fn start_with_root(root: PathBuf) -> Self {
        let port = free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..400 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self { root, child, base };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("S01 temporary server did not become healthy");
    }

    fn stop_keeping_root(mut self) -> PathBuf {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let root = std::mem::take(&mut self.root);
        std::mem::forget(self);
        root
    }

    async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(self.root.join("database/opendash.sqlite"))
                .foreign_keys(true),
        )
        .await
        .unwrap()
    }
}

async fn setup_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{base}/auth/login"))
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|h| h.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

fn minimal_png(seed: u8) -> Vec<u8> {
    let mut img = image::RgbaImage::new(2, 2);
    for p in img.pixels_mut() {
        *p = image::Rgba([seed, seed.wrapping_mul(3), 255u8.wrapping_sub(seed), 255]);
    }
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    bytes
}

async fn upload_png(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    name: &str,
    seed: u8,
) -> String {
    let png = minimal_png(seed);
    let resp = client
        .post(format!("{base}/api/admin/media"))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .header(header::ORIGIN, base)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", name)
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn s01_public_and_writer_closure_across_routes_seo_media_studio_and_query_plan() {
    let site = Site::start("opendash-s01-closure").await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = setup_admin(&client, &site.base).await;

    // Upload distinct exclusive, shared, and OG images.
    let exclusive_img = upload_png(&client, &site.base, &cookie, &csrf, "exclusive.png", 11).await;
    let shared_img = upload_png(&client, &site.base, &cookie, &csrf, "shared.png", 47).await;
    let og_img = upload_png(&client, &site.base, &cookie, &csrf, "og.png", 93).await;
    assert_ne!(exclusive_img, shared_img);
    assert_ne!(exclusive_img, og_img);

    // Enable multilingual first, then add English locale for hreflang & sibling translation closure.
    let locales_resp = client
        .get(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(locales_resp.status(), StatusCode::OK);
    let loc_etag = locales_resp
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let enable_ml = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enable_ml.status(), StatusCode::OK);
    let loc_etag2 = enable_ml
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let add_en = client
        .post(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, &loc_etag2)
        .json(&json!({"code":"en","label":"English","direction":"ltr"}))
        .send()
        .await
        .unwrap();
    assert_eq!(add_en.status(), StatusCode::CREATED);

    // Configure public_origin so sitemap & hreflang emit alternates.
    let pub_get = client
        .get(format!("{}/api/admin/settings/public", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let pub_etag = pub_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let pub_put = client
        .put(format!("{}/api/admin/settings/public", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, &pub_etag)
        .json(&json!({"public_origin": site.base, "content_language": "vi", "indexing_enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_put.status(), StatusCode::OK);

    // Create category term for taxonomy archive & related-post closure.
    let cat = client
        .post(format!("{}/api/admin/taxonomy/terms", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"kind":"category","name":"News","slug":"news","locale":"vi"}))
        .send()
        .await
        .unwrap();
    assert_eq!(cat.status(), StatusCode::CREATED);
    let cat_id = cat.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    // Create two published posts: target (with exclusive_img + shared_img + og_img + custom path) and survivor (with shared_img).
    let create_post = |slug: &'static str, title: &'static str, img: String, cat_id: i64| {
        let client = client.clone();
        let base = site.base.clone();
        let cookie = cookie.clone();
        let csrf = csrf.clone();
        async move {
            let r = client
                .post(format!("{base}/api/admin/content-types/post/entries"))
                .header(header::COOKIE, &cookie)
                .header("X-CSRF-Token", &csrf)
                .header(header::ORIGIN, &base)
                .json(&json!({
                    "title": title,
                    "slug": slug,
                    "locale": "vi",
                    "values": {"body": "<p>Hello world</p>", "featured_image": img},
                    "term_ids": [cat_id]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::CREATED);
            let body: Value = r.json().await.unwrap();
            let id = body["id"].as_i64().unwrap();
            let ver = body["version"].as_i64().unwrap();
            let pub_r = client
                .post(format!(
                    "{base}/api/admin/content-types/post/entries/{id}/publication"
                ))
                .header(header::COOKIE, &cookie)
                .header("X-CSRF-Token", &csrf)
                .header(header::ORIGIN, &base)
                .header(header::IF_MATCH, format!("\"v{ver}\""))
                .json(&json!({"status": "published"}))
                .send()
                .await
                .unwrap();
            assert_eq!(pub_r.status(), StatusCode::OK);
            let pub_body: Value = pub_r.json().await.unwrap();
            (id, pub_body["version"].as_i64().unwrap())
        }
    };

    let (target_id, mut target_ver) =
        create_post("target-post", "Target Post", exclusive_img.clone(), cat_id).await;
    let (_survivor_id, _) =
        create_post("survivor-post", "Survivor Post", shared_img.clone(), cat_id).await;

    // Set SEO OG image + custom route on target, then change custom route to create a historical redirect.
    let seo_put = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{target_id}/seo",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{target_ver}\""))
        .json(&json!({"title":"Target SEO","description":"Desc","og_image_id":og_img,"noindex":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(seo_put.status(), StatusCode::OK);
    target_ver = seo_put.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let route_put_1 = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{target_id}/route",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{target_ver}\""))
        .json(&json!({"custom_path": "/old-target-url"}))
        .send()
        .await
        .unwrap();
    assert_eq!(route_put_1.status(), StatusCode::OK);
    target_ver = route_put_1.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let route_put_2 = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{target_id}/route",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{target_ver}\""))
        .json(&json!({"custom_path": "/custom-target-url"}))
        .send()
        .await
        .unwrap();
    assert_eq!(route_put_2.status(), StatusCode::OK);
    target_ver = route_put_2.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    // Create English translation sibling of target and publish it.
    let tr = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{target_id}/translations",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{target_ver}\""))
        .json(&json!({"target_locale":"en","slug":"target-post-en","title":"Target Post EN"}))
        .send()
        .await
        .unwrap();
    assert_eq!(tr.status(), StatusCode::CREATED);
    let tr_body: Value = tr.json().await.unwrap();
    let tr_id = tr_body["id"].as_i64().unwrap();
    let tr_ver_0 = tr_body["version"].as_i64().unwrap();
    // Point EN sibling's featured_image at shared_img so exclusive_img stays exclusive to target_id.
    let tr_upd = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{tr_id}",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{tr_ver_0}\""))
        .json(&json!({
            "title": "Target Post EN",
            "slug": "target-post-en",
            "locale": "en",
            "values": {"body": "<p>Hello EN</p>", "featured_image": shared_img}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_upd.status(), StatusCode::OK);
    let tr_ver_1 = tr_upd.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let tr_seo_clear = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{tr_id}/seo",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{tr_ver_1}\""))
        .json(&json!({"title":"Target Post EN","description":"Desc EN","og_image_id":null,"noindex":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_seo_clear.status(), StatusCode::OK);
    let tr_ver = tr_seo_clear.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();
    let tr_pub = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{tr_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{tr_ver}\""))
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_pub.status(), StatusCode::OK);

    // Pre-trash: target URL, historical redirect, exclusive image, OG image, sitemap, and EN sibling hreflang work.
    assert_eq!(
        client
            .get(format!("{}/custom-target-url", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{}/old-target-url", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::MOVED_PERMANENTLY
    );
    assert_eq!(
        client
            .get(format!("{}/media/{exclusive_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{}/media/{og_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let pre_sitemap = client
        .get(format!("{}/sitemaps/1.xml", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(pre_sitemap.contains("/custom-target-url"), "{pre_sitemap}");
    assert!(pre_sitemap.contains("/en/target-post-en"), "{pre_sitemap}");

    // Post-S02 activation: POST .../trash is live and enforces If-Match (428 when missing).
    let s02_trash_missing_if_match = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{target_id}/trash",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        s02_trash_missing_if_match.status(),
        StatusCode::PRECONDITION_REQUIRED
    );

    // Retired Phase 2 legacy endpoints remain 404.
    for retired in ["/api/admin/builder/page", "/api/admin/builder/registry"] {
        assert_eq!(
            client
                .get(format!("{}{retired}", site.base))
                .header(header::COOKIE, &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    let pre_table_total = client
        .get(format!(
            "{}/api/admin/content-types/post/entry-table?locale=vi",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["total"]
        .as_i64()
        .unwrap();
    let pre_dash_published = client
        .get(format!("{}/api/admin/dashboard", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["post"]["published"]
        .as_i64()
        .unwrap();

    // Move target_id to Trash at the DB boundary (simulating S01 storage-only transition:
    // trashed_at set, version bumped, derived seo_public_media_refs cleared).
    let db = site.db().await;
    let trashed_ver = target_ver + 1;
    sqlx::query(
        "UPDATE content_entries
            SET trashed_at = datetime('now'), version = ?
          WHERE id = ?",
    )
    .bind(trashed_ver)
    .bind(target_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
    )
    .bind(target_id)
    .execute(&db)
    .await
    .unwrap();

    // Verify EXPLAIN QUERY PLAN uses idx_content_entries_lifecycle_list!
    let plan_rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN
         SELECT id FROM content_entries
          WHERE content_type_key = 'post' AND locale = 'vi' AND trashed_at IS NULL AND publication_status = 'published'
          ORDER BY id DESC LIMIT 20",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let plan_text = plan_rows
        .iter()
        .map(|(_, _, _, detail)| detail.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        plan_text.contains("idx_content_entries_lifecycle_list"),
        "expected lifecycle index in query plan, got: {plan_text}"
    );

    // PUBLIC CLOSURE: canonical URL, historical redirect, exclusive media, exclusive OG media all 404!
    assert_eq!(
        client
            .get(format!("{}/custom-target-url", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{}/old-target-url", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{}/media/{exclusive_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{}/media/{og_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    // Shared media referenced by active survivor-post MUST stay 200!
    assert_eq!(
        client
            .get(format!("{}/media/{shared_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    // Public archive & taxonomy archive exclude trashed target, keep survivor!
    let archive_html = client
        .get(format!("{}/posts", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!archive_html.contains("Target Post"));
    assert!(archive_html.contains("Survivor Post"));
    let cat_html = client
        .get(format!("{}/category/news", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!cat_html.contains("Target Post"));
    assert!(cat_html.contains("Survivor Post"));

    // Sitemap excludes trashed target, keeps EN sibling and survivor; EN sibling HTML must NOT include trashed VI hreflang!
    let post_sitemap = client
        .get(format!("{}/sitemaps/1.xml", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !post_sitemap.contains("/custom-target-url"),
        "{post_sitemap}"
    );
    assert!(
        post_sitemap.contains("/en/target-post-en"),
        "{post_sitemap}"
    );
    let en_sibling_html = client
        .get(format!("{}/en/target-post-en", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!en_sibling_html.contains("/custom-target-url"));

    // ORDINARY MANAGEMENT CLOSURE: GET entry, entry-table, dashboard, preview, Builder open all exclude/block trashed row!
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/post/entries/{target_id}",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let table_json: Value = client
        .get(format!(
            "{}/api/admin/content-types/post/entry-table?locale=vi",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(table_json["total"].as_i64(), Some(pre_table_total - 1));
    let dash_json: Value = client
        .get(format!("{}/api/admin/dashboard", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Trashed target excluded from active dashboard published count.
    assert_eq!(
        dash_json["post"]["published"].as_i64(),
        Some(pre_dash_published - 1)
    );
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/post/entries/{target_id}/preview",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    // Studio Mode 2 Builder closure on a Builder-capable Page that is in Trash: GET /builder returns 409!
    let page_create = client
        .post(format!("{}/api/admin/content-types/page/entries", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"title":"Trashed Page","slug":"trashed-builder-page","locale":"vi","values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(page_create.status(), StatusCode::CREATED);
    let page_id = page_create.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    sqlx::query(
        "UPDATE content_entries SET trashed_at = datetime('now'), version = version + 1 WHERE id = ?",
    )
    .bind(page_id)
    .execute(&db)
    .await
    .unwrap();
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/entries/page/{page_id}/builder",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // ORDINARY WRITER CLOSURE: stale version -> 412 FIRST; fresh version -> 409 entry_trashed!
    for (ver, expected_status) in [
        (target_ver, StatusCode::PRECONDITION_FAILED),
        (trashed_ver, StatusCode::CONFLICT),
    ] {
        let put_r = client
            .put(format!(
                "{}/api/admin/content-types/post/entries/{target_id}",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::ORIGIN, &site.base)
            .header(header::IF_MATCH, format!("\"v{ver}\""))
            .json(&json!({"title":"Edited","slug":"target-post","locale":"vi","values":{}}))
            .send()
            .await
            .unwrap();
        assert_eq!(put_r.status(), expected_status, "PUT ver={ver}");

        let pub_r = client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{target_id}/publication",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::ORIGIN, &site.base)
            .header(header::IF_MATCH, format!("\"v{ver}\""))
            .json(&json!({"status":"draft"}))
            .send()
            .await
            .unwrap();
        assert_eq!(pub_r.status(), expected_status, "PUB ver={ver}");

        let seo_r = client
            .put(format!(
                "{}/api/admin/content-types/post/entries/{target_id}/seo",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::ORIGIN, &site.base)
            .header(header::IF_MATCH, format!("\"v{ver}\""))
            .json(&json!({"title":"Hacked SEO","noindex":false}))
            .send()
            .await
            .unwrap();
        assert_eq!(seo_r.status(), expected_status, "SEO ver={ver}");

        let route_r = client
            .put(format!(
                "{}/api/admin/content-types/post/entries/{target_id}/route",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::ORIGIN, &site.base)
            .header(header::IF_MATCH, format!("\"v{ver}\""))
            .json(&json!({"custom_path":"/hijack"}))
            .send()
            .await
            .unwrap();
        assert_eq!(route_r.status(), expected_status, "ROUTE ver={ver}");
    }

    // Verify canonical DB row was completely untouched by all rejected mutations.
    let (still_title, still_ver): (String, i64) =
        sqlx::query_as("SELECT title, version FROM content_entries WHERE id = ?")
            .bind(target_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(still_title, "Target Post");
    assert_eq!(still_ver, trashed_ver);

    // Media deletion guard: exclusive_img is STILL pinned by trashed target_id -> cannot delete media!
    let pins_check = client
        .post(format!("{}/api/admin/operations/reconcile/plan", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pins_check.status(), StatusCode::OK);
    let plan_body: Value = pins_check.json().await.unwrap();
    let plan_str = plan_body.to_string();
    assert!(
        !plan_str.contains(&exclusive_img),
        "trashed entry must retain media pin for {exclusive_img}: {plan_str}"
    );
}

#[tokio::test]
async fn s01_sqlx_compiled_migration_refuses_unknown_newer_version_and_optional_real_baseline_refusal()
 {
    // 1. Always-run deterministic proof that compiled sqlx::migrate! refuses unknown applied migration.
    let mem_pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../../migrations")
        .run(&mem_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (9999, 'future_unknown_migration', 1, X'01020304', 1)",
    )
    .execute(&mem_pool)
    .await
    .unwrap();
    let err = sqlx::migrate!("../../migrations")
        .run(&mem_pool)
        .await
        .expect_err("compiled migrator must fail closed on unknown newer migration");
    assert!(
        matches!(err, sqlx::migrate::MigrateError::VersionMissing(9999)),
        "unexpected migration error: {err:?}"
    );

    // 2. Real baseline executable refusal proof when OPENDASH_BASELINE_BINARY is provided
    // (used during native S01 verification with the exact pre-S01 efdcbd4 release binary).
    if let Ok(baseline_bin) = std::env::var("OPENDASH_BASELINE_BINARY") {
        let baseline_path = Path::new(&baseline_bin);
        assert!(
            baseline_path.is_file(),
            "OPENDASH_BASELINE_BINARY does not exist: {baseline_bin}"
        );
        let site = Site::start("opendash-s01-upgrade-refusal").await;
        let db = site.db().await;
        let applied_53: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 53")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(applied_53, 1);
        db.close().await;
        let root = site.stop_keeping_root();

        let port = free_port();
        let output = Command::new(baseline_path)
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .output()
            .unwrap();
        let _ = fs::remove_dir_all(&root);
        assert!(
            !output.status.success(),
            "baseline binary MUST refuse to open 0053-migrated database"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{stdout}\n{stderr}");
        assert!(
            combined.contains("53") || combined.to_lowercase().contains("migration"),
            "expected migration refusal diagnostics from old binary, got: {combined}"
        );
    }
}
