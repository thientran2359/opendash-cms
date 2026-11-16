use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, header};
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
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s03-t02-{nanos}"));
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
    login(client, base_url, "admin", "AdminPassword123!").await
}

async fn login(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cookie = extract_cookies(resp.headers());
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

fn minimal_1x1_png() -> Vec<u8> {
    // Valid 1x1 transparent PNG
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}

#[tokio::test]
async fn seo_metadata_head_projection_og_media_acl_and_rbac() {
    let srv = spawn_server().await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &srv.base_url).await;

    // 1. Create an Editor user and verify RBAC: Editor cannot GET/PUT /api/admin/seo/home (403).
    let ed_create = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert!(ed_create.status().is_success());
    let (ed_cookie, ed_csrf) = login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;

    let ed_home_get = client
        .get(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &ed_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ed_home_get.status(), 403);

    let ed_home_put = client
        .put(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &ed_cookie)
        .header("X-CSRF-Token", &ed_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "title": "Nope",
            "description": null,
            "og_image_id": null,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_home_put.status(), 403);

    // 2. Upload an image media asset; initially unreferenced -> /media/{id}/file is 404.
    let up = client
        .post(format!("{}/api/admin/media", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-Media-Filename", "og-cover.png")
        .body(minimal_1x1_png())
        .send()
        .await
        .unwrap();
    assert_eq!(up.status(), 201);
    let media_id = up.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let pub_media_before = client
        .get(format!("{}/media/{media_id}/file", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_media_before.status(), 404);

    // 3. Create a draft post; test 422 on malformed SEO payloads.
    let create_post = client
        .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
        .header(header::COOKIE, &ed_cookie)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({ "slug": "seo-post", "title": "SEO Post", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_post.status(), 201);
    let post_json: Value = create_post.json().await.unwrap();
    let post_id = post_json["id"].as_i64().unwrap();
    let mut post_ver = post_json["version"].as_i64().unwrap();

    for bad_payload in [
        json!({ "title": "Bad", "description": null, "og_image_id": "not-a-sha256", "canonical_url": null, "noindex": false }),
        json!({ "title": "Bad", "description": null, "og_image_id": null, "canonical_url": "javascript:alert(1)", "noindex": false }),
        json!({ "title": "Bad\nControl", "description": null, "og_image_id": null, "canonical_url": null, "noindex": false }),
        json!({ "title": "a".repeat(121), "description": null, "og_image_id": null, "canonical_url": null, "noindex": false }),
    ] {
        let resp = client
            .put(format!(
                "{}/api/admin/content-types/post/entries/{post_id}/seo",
                srv.base_url
            ))
            .header(header::COOKIE, &ed_cookie)
            .header("X-CSRF-Token", &ed_csrf)
            .header(header::IF_MATCH, format!("\"v{post_ver}\""))
            .json(&bad_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 422);
    }

    // 4. Editor saves valid SEO with XSS characters + og_image_id on DRAFT post -> media STILL 404 publicly!
    let save_draft_seo = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/seo",
            srv.base_url
        ))
        .header(header::COOKIE, &ed_cookie)
        .header("X-CSRF-Token", &ed_csrf)
        .header(header::IF_MATCH, format!("\"v{post_ver}\""))
        .json(&json!({
            "title": "SEO Title <script>alert(1)</script>",
            "description": "Desc \"quoted\" & safe",
            "og_image_id": media_id,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_draft_seo.status(), 200);
    post_ver = save_draft_seo.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();

    let pub_media_draft = client
        .get(format!("{}/media/{media_id}/file", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_media_draft.status(), 404);

    // 5. Preview of draft has noindex,nofollow and private,no-store.
    let preview_resp = client
        .get(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/preview",
            srv.base_url
        ))
        .header(header::COOKIE, &ed_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(preview_resp.status(), 200);
    let cc = preview_resp
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(cc.contains("private") && cc.contains("no-store"));
    let preview_html = preview_resp.text().await.unwrap();
    assert!(preview_html.contains("noindex,nofollow"));

    // 6. Without public_origin configured, published post has NO canonical link (no Host fallback).
    let pub_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            srv.base_url
        ))
        .header(header::COOKIE, &ed_cookie)
        .header("X-CSRF-Token", &ed_csrf)
        .header(header::IF_MATCH, format!("\"v{post_ver}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), 200);
    post_ver = pub_resp.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    // Now og_image media MUST be publicly accessible (200)!
    let pub_media_after = client
        .get(format!("{}/media/{media_id}/file", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_media_after.status(), 200);

    let post_html_no_origin = client
        .get(format!("{}/content/post/seo-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!post_html_no_origin.contains("<link rel=\"canonical\""));
    assert!(post_html_no_origin.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!post_html_no_origin.contains("<script>alert(1)</script>"));
    assert!(post_html_no_origin.contains(&format!("/media/{media_id}/file")));

    // 7. Configure public_origin + content_language = "en-US" -> canonical uses public_origin and lang="en-US".
    let pub_settings_get: Value = client
        .get(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pver = pub_settings_get["version"].as_i64().unwrap();
    let pub_settings_put = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{pver}\""))
        .json(&json!({
            "public_origin": "https://cms.example.com",
            "content_language": "en-US",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_settings_put.status(), 200);

    let post_html = client
        .get(format!("{}/content/post/seo-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(post_html.contains("<html lang=\"en-US\">"));
    assert!(post_html.contains("<link rel=\"canonical\" href=\"https://cms.example.com/content/post/seo-post\">"));
    assert_eq!(post_html.matches("<head>").count(), 1);
    assert_eq!(post_html.matches("<title>").count(), 1);

    // 8. Unpublishing revokes OG image public access (404).
    let unpub_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            srv.base_url
        ))
        .header(header::COOKIE, &ed_cookie)
        .header("X-CSRF-Token", &ed_csrf)
        .header(header::IF_MATCH, format!("\"v{post_ver}\""))
        .json(&json!({ "status": "draft" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unpub_resp.status(), 200);

    let pub_media_revoked = client
        .get(format!("{}/media/{media_id}/file", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_media_revoked.status(), 404);

    // 9. Admin Home SEO CAS (412 stale, 200 valid) + public Home projection + 404 noindex.
    let home_get = client
        .get(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(home_get.status(), 200);
    let home_ver = home_get.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let home_stale = client
        .put(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{}\"", home_ver + 10))
        .json(&json!({
            "title": "Stale",
            "description": null,
            "og_image_id": null,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(home_stale.status(), 412);

    let home_save = client
        .put(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{home_ver}\""))
        .json(&json!({
            "title": "Custom Home SEO",
            "description": "Home meta desc",
            "og_image_id": null,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(home_save.status(), 200);

    let home_html = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home_html.contains("<title>Custom Home SEO</title>"));
    assert!(home_html.contains("<link rel=\"canonical\" href=\"https://cms.example.com/\">"));

    let not_found_resp = client
        .get(format!("{}/nonexistent-seo-route", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(not_found_resp.status(), 404);
    let not_found_html = not_found_resp.text().await.unwrap();
    assert!(not_found_html.contains("noindex,nofollow"));
}
