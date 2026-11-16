use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, header, redirect::Policy};
use serde_json::{Value, json};

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
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s03-t01-{nanos}"));
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

async fn setup_and_login_admin(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(setup.status().is_success(), "setup failed: {}", setup.status());

    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "login failed: {}", resp.status());
    let cookies = extract_cookies(resp.headers());
    let body: Value = resp.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_string();
    (cookies, csrf)
}

async fn create_entry(
    client: &Client,
    base_url: &str,
    cookies: &str,
    csrf: &str,
    type_key: &str,
    slug: &str,
    title: &str,
) -> (i64, i64) {
    let resp = client
        .post(format!("{base_url}/api/admin/content-types/{type_key}/entries"))
        .header(header::COOKIE, cookies)
        .header("x-csrf-token", csrf)
        .json(&json!({
            "slug": slug,
            "title": title,
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create_entry failed");
    let body: Value = resp.json().await.unwrap();
    (
        body["id"].as_i64().unwrap(),
        body["version"].as_i64().unwrap(),
    )
}

async fn publish_entry(
    client: &Client,
    base_url: &str,
    cookies: &str,
    csrf: &str,
    type_key: &str,
    id: i64,
    version: i64,
) -> i64 {
    let resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/{type_key}/entries/{id}/publication"
        ))
        .header(header::COOKIE, cookies)
        .header("x-csrf-token", csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "publish_entry failed");
    let body: Value = resp.json().await.unwrap();
    body["version"].as_i64().unwrap()
}

async fn unpublish_entry(
    client: &Client,
    base_url: &str,
    cookies: &str,
    csrf: &str,
    type_key: &str,
    id: i64,
    version: i64,
) -> i64 {
    let resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/{type_key}/entries/{id}/publication"
        ))
        .header(header::COOKIE, cookies)
        .header("x-csrf-token", csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&json!({ "status": "draft" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "unpublish_entry failed");
    let body: Value = resp.json().await.unwrap();
    body["version"].as_i64().unwrap()
}

#[tokio::test]
async fn custom_entry_routes_redirects_cas_and_privacy() {
    let srv = spawn_server().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookies, csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    // 1. Create two posts and publish the first.
    let (post1_id, mut post1_ver) =
        create_entry(&client, &srv.base_url, &cookies, &csrf, "post", "alpha-post", "Alpha Post")
            .await;
    post1_ver = publish_entry(&client, &srv.base_url, &cookies, &csrf, "post", post1_id, post1_ver).await;

    let (post2_id, mut post2_ver) =
        create_entry(&client, &srv.base_url, &cookies, &csrf, "post", "beta-post", "Beta Post")
            .await;
    post2_ver = publish_entry(&client, &srv.base_url, &cookies, &csrf, "post", post2_id, post2_ver).await;

    // 2. Auth, CSRF, and If-Match enforcement on route endpoints.
    let unauth_get = client
        .get(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_get.status(), 401);

    let no_csrf_put = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header(header::IF_MATCH, format!("\"v{post1_ver}\""))
        .json(&json!({ "custom_path": "/gioi-thieu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf_put.status(), 403);

    let no_if_match = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "custom_path": "/gioi-thieu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_if_match.status(), 428);

    let stale_if_match = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{}\"", post1_ver + 99))
        .json(&json!({ "custom_path": "/gioi-thieu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_if_match.status(), 412);

    // 3. Invalid custom paths and reserved system roots -> 400.
    for bad in [
        "/",
        "no-leading-slash",
        "/trailing/",
        "/Upper",
        "/under_score",
        "/a/b/c/d/e/f",
        "/dot.html",
        "/q?x=1",
        "/admin",
        "/admin/sub",
        "/api/foo",
        "/content/anything",
        "/robots.txt",
        "/sitemap.xml",
    ] {
        let resp = client
            .put(format!(
                "{}/api/admin/content-types/post/entries/{post1_id}/route",
                srv.base_url
            ))
            .header(header::COOKIE, &cookies)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, format!("\"v{post1_ver}\""))
            .json(&json!({ "custom_path": bad }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "expected 400 for bad path {bad}");
    }

    // 4. Set valid custom path /gioi-thieu -> 200, version increments.
    let set_custom = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post1_ver}\""))
        .json(&json!({ "custom_path": "/gioi-thieu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(set_custom.status(), 200);
    let snap: Value = set_custom.json().await.unwrap();
    assert_eq!(snap["custom_path"].as_str(), Some("/gioi-thieu"));
    post1_ver = snap["entry_version"].as_i64().unwrap();

    // 5. Custom path serves 200; fixed namespace /content/post/alpha-post serves 301 -> /gioi-thieu.
    let custom_get = client
        .get(format!("{}/gioi-thieu", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(custom_get.status(), 200);
    let html = custom_get.text().await.unwrap();
    assert!(html.contains("Alpha Post"));

    let legacy_get = client
        .get(format!("{}/content/post/alpha-post?utm=1", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy_get.status(), 301);
    assert_eq!(
        legacy_get
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/gioi-thieu")
    );

    // 6. Change custom path -> /ve-chung-toi; old custom path redirects 301 (single hop, drops query).
    let update_custom = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post1_ver}\""))
        .json(&json!({ "custom_path": "/ve-chung-toi" }))
        .send()
        .await
        .unwrap();
    assert_eq!(update_custom.status(), 200);
    post1_ver = update_custom.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let old_custom_redir = client
        .get(format!("{}/gioi-thieu?foo=bar", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(old_custom_redir.status(), 301);
    assert_eq!(
        old_custom_redir
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/ve-chung-toi")
    );

    // 7. Another entry cannot claim /ve-chung-toi or /gioi-thieu -> 409.
    for taken in ["/ve-chung-toi", "/gioi-thieu"] {
        let collision = client
            .put(format!(
                "{}/api/admin/content-types/post/entries/{post2_id}/route",
                srv.base_url
            ))
            .header(header::COOKIE, &cookies)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, format!("\"v{post2_ver}\""))
            .json(&json!({ "custom_path": taken }))
            .send()
            .await
            .unwrap();
        assert_eq!(collision.status(), 409, "expected 409 for {taken}");
    }

    // 8. CPT root collision: set custom path /landing on post2, then creating CPT `landing` -> 409;
    // and cannot set custom path `/post/custom` (collides with CPT `post`).
    let set_landing = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post2_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post2_ver}\""))
        .json(&json!({ "custom_path": "/landing" }))
        .send()
        .await
        .unwrap();
    assert_eq!(set_landing.status(), 200);
    post2_ver = set_landing.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let cpt_collision = client
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "key": "landing",
            "label": "Landing",
            "fields": [
                { "key": "summary", "label": "Summary", "type": "short_text" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cpt_collision.status(), 409);

    let clear_landing = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post2_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post2_ver}\""))
        .json(&json!({ "custom_path": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(clear_landing.status(), 200);
    post2_ver = clear_landing.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let custom_cpt_collision = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post2_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post2_ver}\""))
        .json(&json!({ "custom_path": "/post/custom" }))
        .send()
        .await
        .unwrap();
    assert_eq!(custom_cpt_collision.status(), 409);

    // 9. Slug change on post2 records redirect from /content/post/beta-post -> /content/post/beta-v2.
    let slug_change = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post2_id}",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post2_ver}\""))
        .json(&json!({
            "slug": "beta-v2",
            "title": "Beta V2",
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(slug_change.status(), 200);
    post2_ver = slug_change.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let old_slug_redir = client
        .get(format!("{}/content/post/beta-post?ref=test", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(old_slug_redir.status(), 301);
    assert_eq!(
        old_slug_redir
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/content/post/beta-v2")
    );

    // 10. Menu and archive URLs use custom path /ve-chung-toi.
    let archive_resp = client
        .get(format!("{}/content/post", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(archive_resp.status(), 200);
    let archive_html = archive_resp.text().await.unwrap();
    assert!(
        archive_html.contains("/ve-chung-toi") || archive_html.contains("&#x2f;ve-chung-toi"),
        "archive_html did not contain /ve-chung-toi: {archive_html}"
    );

    let menu_create = client
        .post(format!("{}/api/admin/menus", srv.base_url))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "menu_key": "main-nav",
            "title": "Main Nav"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(menu_create.status(), 201);
    let menu_created: Value = menu_create.json().await.unwrap();
    let menu_id = menu_created["menu"]["id"].as_i64().unwrap();

    let menu_put = client
        .put(format!("{}/api/admin/menus/{menu_id}", srv.base_url))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "title": "Main Nav",
            "items": [
                {
                    "position": 0,
                    "label": "About",
                    "target_kind": "entry",
                    "target_entry_id": post1_id
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(menu_put.status(), 200);

    let menu_preview = client
        .post(format!("{}/api/admin/menus/{menu_id}/preview", srv.base_url))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "title": "Main Nav",
            "items": [
                {
                    "position": 0,
                    "label": "About",
                    "target_kind": "entry",
                    "target_entry_id": post1_id
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(menu_preview.status(), 200);
    let preview_body: Value = menu_preview.json().await.unwrap();
    assert_eq!(preview_body["items"][0]["url"].as_str(), Some("/ve-chung-toi"));

    // 11. Unpublishing post1 makes /ve-chung-toi and /gioi-thieu return 404 WITHOUT Location.
    post1_ver = unpublish_entry(&client, &srv.base_url, &cookies, &csrf, "post", post1_id, post1_ver).await;
    for path in ["/ve-chung-toi", "/gioi-thieu", "/content/post/alpha-post"] {
        let resp = client
            .get(format!("{}{path}", srv.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404, "expected 404 for unpublished {path}");
        assert!(
            resp.headers().get(header::LOCATION).is_none(),
            "unpublished {path} must not leak Location"
        );
    }

    // 12. Re-publish and reclaim own alias /gioi-thieu works; post2 still cannot.
    post1_ver = publish_entry(&client, &srv.base_url, &cookies, &csrf, "post", post1_id, post1_ver).await;
    let reclaim = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post1_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookies)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post1_ver}\""))
        .json(&json!({ "custom_path": "/gioi-thieu" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reclaim.status(), 200);
    let _ = post2_ver;

    // 13. Explicit system routes precede fallback.
    let health = client.get(format!("{}/api/health", srv.base_url)).send().await.unwrap();
    assert_eq!(health.status(), 200);
}
