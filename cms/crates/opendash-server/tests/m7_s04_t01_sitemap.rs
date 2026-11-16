use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;

struct TestServer {
    child: Child,
    site_root: PathBuf,
    base_url: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s04-t01-{nanos}"));
    fs::create_dir_all(&site_root).unwrap();

    let bin = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(bin)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..60 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become healthy at {base_url}");
}

fn extract_cookies(headers: &header::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

async fn setup_admin(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert!(setup.status().is_success());
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cookie = extract_cookies(resp.headers());
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn put_public_settings(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    public_origin: Option<&str>,
    indexing_enabled: bool,
) {
    let get_resp = client
        .get(format!("{base_url}/api/admin/settings/public"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    let etag = get_resp
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let cur: Value = get_resp.json().await.unwrap();
    let resp = client
        .put(format!("{base_url}/api/admin/settings/public"))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({
            "public_origin": public_origin,
            "content_language": cur["content_language"].as_str().unwrap_or("vi"),
            "indexing_enabled": indexing_enabled,
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert_eq!(status, 200, "put_public_settings failed: {text}");
}

async fn create_post_entry(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    slug: &str,
    title: &str,
    term_ids: &[i64],
) -> (i64, i64) {
    let resp = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .json(&json!({
            "slug": slug,
            "title": title,
            "values": {},
            "term_ids": term_ids,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await.unwrap();
    (body["id"].as_i64().unwrap(), body["version"].as_i64().unwrap())
}

async fn set_entry_status(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    id: i64,
    version: i64,
    status: &str,
) -> i64 {
    let resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{id}/publication"
        ))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&json!({ "status": status }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    resp.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn dynamic_robots_and_sitemap_respect_privacy_origin_and_pagination() {
    let srv = spawn_server().await;
    let client = Client::new();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    // 1. Without public_origin: robots omits Sitemap even with spoofed forwarded host; sitemap returns 503.
    let robots_no_origin = client
        .get(format!("{}/robots.txt", srv.base_url))
        .header("X-Forwarded-Host", "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(robots_no_origin.status(), 200);
    assert_eq!(
        robots_no_origin.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let robots_text = robots_no_origin.text().await.unwrap();
    assert!(!robots_text.contains("Sitemap:"));
    assert!(!robots_text.contains("evil.example"));

    assert_eq!(
        client
            .get(format!("{}/sitemap.xml", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    assert_eq!(
        client
            .get(format!("{}/sitemaps/1.xml", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        503
    );

    // 2. With public_origin set and indexing_enabled=false: Disallow: /, empty sitemapindex, 404 part.
    put_public_settings(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        Some("https://example.com"),
        false,
    )
    .await;
    let robots_off_text = client
        .get(format!("{}/robots.txt", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(robots_off_text.contains("Disallow: /"));
    assert!(!robots_off_text.contains("Sitemap:"));

    let sm_off = client
        .get(format!("{}/sitemap.xml", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(sm_off.status(), 200);
    assert_eq!(
        sm_off.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let sm_off_xml = sm_off.text().await.unwrap();
    assert!(!sm_off_xml.contains("<sitemap>"));
    assert_eq!(
        client
            .get(format!("{}/sitemaps/1.xml", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    // 3. Enable indexing_enabled=true and create terms, entries, custom path, redirect, noindex, draft, external canonical.
    put_public_settings(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        Some("https://example.com"),
        true,
    )
    .await;

    let robots_on = client
        .get(format!("{}/robots.txt", srv.base_url))
        .header("X-Forwarded-Host", "evil.example")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(robots_on.contains("Sitemap: https://example.com/sitemap.xml"));
    assert!(!robots_on.contains("evil.example"));

    let term_used: Value = client
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "kind": "category", "name": "Tin tuc", "slug": "tin-tuc" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let term_used_id = term_used["id"].as_i64().unwrap();

    client
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "kind": "category", "name": "Empty", "slug": "empty-cat" }))
        .send()
        .await
        .unwrap();

    let (e_pub_id, mut e_pub_ver) = create_post_entry(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "gioi-thieu-slug",
        "Gioi thieu",
        &[term_used_id],
    )
    .await;
    e_pub_ver = set_entry_status(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        e_pub_id,
        e_pub_ver,
        "published",
    )
    .await;

    // Set /old-path then /gioi-thieu so /old-path becomes a redirect alias.
    let r1 = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{e_pub_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{e_pub_ver}\""))
        .json(&json!({ "custom_path": "/old-path" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), 200);
    e_pub_ver = r1.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let r2 = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{e_pub_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{e_pub_ver}\""))
        .json(&json!({ "custom_path": "/gioi-thieu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), 200);
    e_pub_ver = r2.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    // Published entry with noindex=true
    let (e_noindex_id, e_noindex_ver) = create_post_entry(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "secret-noindex",
        "Bi mat",
        &[],
    )
    .await;
    let e_noindex_ver = set_entry_status(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        e_noindex_id,
        e_noindex_ver,
        "published",
    )
    .await;
    let put_noindex = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{e_noindex_id}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{e_noindex_ver}\""))
        .json(&json!({ "noindex": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_noindex.status(), 200);

    // Draft entry
    let _ = create_post_entry(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "draft-only",
        "Ban nhap",
        &[],
    )
    .await;

    // Published entry with external canonical
    let (e_ext_id, e_ext_ver) = create_post_entry(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        "ext-canonical",
        "External",
        &[],
    )
    .await;
    let e_ext_ver = set_entry_status(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        e_ext_id,
        e_ext_ver,
        "published",
    )
    .await;
    let put_ext = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{e_ext_id}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{e_ext_ver}\""))
        .json(&json!({ "canonical_url": "https://other.example/x", "noindex": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_ext.status(), 200);

    // Verify sitemap index and part 1
    let idx_resp = client
        .get(format!("{}/sitemap.xml", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(idx_resp.status(), 200);
    assert_eq!(
        idx_resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let idx_xml = idx_resp.text().await.unwrap();
    assert!(idx_xml.contains("<loc>https://example.com/sitemaps/1.xml</loc>"));
    assert!(!idx_xml.contains("/sitemaps/2.xml"));

    let part1 = client
        .get(format!("{}/sitemaps/1.xml", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(part1.status(), 200);
    assert_eq!(
        part1.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let part1_xml = part1.text().await.unwrap();
    assert!(part1_xml.contains("<loc>https://example.com/</loc>"));
    assert!(part1_xml.contains("<loc>https://example.com/gioi-thieu</loc>"));
    assert!(part1_xml.contains("<loc>https://example.com/content/post</loc>"));
    assert!(part1_xml.contains("<loc>https://example.com/taxonomy/category/tin-tuc</loc>"));
    assert!(!part1_xml.contains("secret-noindex"));
    assert!(!part1_xml.contains("draft-only"));
    assert!(!part1_xml.contains("ext-canonical"));
    assert!(!part1_xml.contains("/old-path"));
    assert!(!part1_xml.contains("empty-cat"));

    // Invalid part paths must return 404
    for bad in ["0.xml", "01.xml", "2.xml", "201.xml", "bad.xml", "1"] {
        let r = client
            .get(format!("{}/sitemaps/{bad}", srv.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404, "expected 404 for /sitemaps/{bad}");
    }

    // Immediate update when unpublishing e_pub
    let _ = set_entry_status(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        e_pub_id,
        e_pub_ver,
        "draft",
    )
    .await;
    let part1_after = client
        .get(format!("{}/sitemaps/1.xml", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!part1_after.contains("/gioi-thieu"));
    assert!(!part1_after.contains("tin-tuc"));

    // 4. Boundary pagination: 5000 routes -> 1 part; 5001 routes -> 2 parts.
    let db_path = srv.site_root.join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect(&format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    // Currently indexable routes: '/' (1) + '/content/post' (1) = 2. Insert 4998 more -> 5000.
    let mut tx = pool.begin().await.unwrap();
    for i in 1..=4998 {
        sqlx::query(
            "INSERT INTO content_entries (content_type_key, slug, title, publication_status, values_json, version)
             VALUES ('post', ?1, ?2, 'draft', '{}', 1)",
        )
        .bind(format!("bulk-item-{i}"))
        .bind(format!("Bulk {i}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published' WHERE slug LIKE 'bulk-item-%'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let idx_5000 = client
        .get(format!("{}/sitemap.xml", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(idx_5000.contains("/sitemaps/1.xml"));
    assert!(!idx_5000.contains("/sitemaps/2.xml"));
    assert_eq!(
        client
            .get(format!("{}/sitemaps/2.xml", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    // Insert 1 more -> 5001 routes -> 2 parts.
    sqlx::query(
        "INSERT INTO content_entries (content_type_key, slug, title, publication_status, values_json, version)
         VALUES ('post', 'bulk-item-4999', 'Bulk 4999', 'draft', '{}', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published' WHERE slug = 'bulk-item-4999'",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let idx_5001 = client
        .get(format!("{}/sitemap.xml", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(idx_5001.contains("/sitemaps/1.xml"));
    assert!(idx_5001.contains("/sitemaps/2.xml"));
    assert!(!idx_5001.contains("/sitemaps/3.xml"));

    let part2 = client
        .get(format!("{}/sitemaps/2.xml", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(part2.status(), 200);
    let part2_xml = part2.text().await.unwrap();
    assert_eq!(part2_xml.matches("<url>").count(), 1);
    assert_eq!(
        client
            .get(format!("{}/sitemaps/3.xml", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}
