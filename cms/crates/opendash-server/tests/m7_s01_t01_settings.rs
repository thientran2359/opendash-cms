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
    cleanup_on_drop: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.cleanup_on_drop {
            let _ = fs::remove_dir_all(&self.site_root);
        }
    }
}

async fn spawn_server_at(site_root: PathBuf, cleanup_on_drop: bool) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
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
                cleanup_on_drop,
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
    assert_eq!(setup.status(), 201);
    login(client, base_url, "admin", "AdminPassword123!").await
}

async fn login(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cookie = extract_cookies(resp.headers());
    let body: Value = resp.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_owned();
    (cookie, csrf)
}

#[tokio::test]
async fn m7_s01_t01_public_settings_validation_rbac_cas_and_restart() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s01-t01-{nanos}"));
    let srv = spawn_server_at(site_root.clone(), false).await;

    let client = Client::new();

    // Unauthenticated access rejected with 401.
    let unauth_get = client
        .get(format!("{}/api/admin/settings/public", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_get.status(), 401);

    let (admin_cookie, csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    // Fresh backfill defaults: public_origin=null, content_language="vi", indexing_enabled=true, version=1.
    let get_resp = client
        .get(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("Forwarded", "host=spoofed-forwarded.example;proto=https")
        .header("X-Forwarded-Host", "spoofed-fwd.example")
        .header("Referer", "https://spoofed-referer.example/admin")
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);
    assert_eq!(
        get_resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(get_resp.headers().get(header::ETAG).unwrap(), "\"v1\"");
    let initial: Value = get_resp.json().await.unwrap();
    assert_eq!(
        initial,
        json!({
            "public_origin": Value::Null,
            "content_language": "vi",
            "indexing_enabled": true,
            "version": 1
        })
    );

    // Public home still works when public_origin is null.
    let public_home = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(public_home.status(), 200);

    // General settings remain intact and independent.
    let general_before: Value = client
        .get(format!("{}/api/admin/settings/general", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(general_before["version"], 1);

    // Missing CSRF -> 403.
    let no_csrf = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "public_origin": "https://example.com",
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), 403);

    // Missing If-Match -> 428.
    let no_if_match = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "public_origin": "https://example.com",
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_if_match.status(), 428);

    // Invalid origins rejected with 422.
    for bad_origin in [
        "https://user:pass@example.com",
        "https://example.com/blog",
        "https://example.com?x=1",
        "https://example.com?",
        "https://example.com#frag",
        "https://example.com#",
        "https://example.com//",
        "https://example.com\n",
        "https:// example.com",
        "ftp://example.com",
        "javascript:alert(1)",
        "   ",
    ] {
        let resp = client
            .put(format!("{}/api/admin/settings/public", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({
                "public_origin": bad_origin,
                "content_language": "vi",
                "indexing_enabled": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            422,
            "expected 422 for bad origin {bad_origin:?}"
        );
    }

    // Invalid content_language (including >35 bytes) rejected with 422.
    let too_long_lang = "en-US-x-12345678-12345678-1234567890";
    assert!(too_long_lang.len() > 35);
    for bad_lang in [
        "",
        "a",
        "-vi",
        "vi-",
        "vi_VN",
        "tiếng",
        "vi en",
        too_long_lang,
    ] {
        let resp = client
            .put(format!("{}/api/admin/settings/public", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({
                "public_origin": Value::Null,
                "content_language": bad_lang,
                "indexing_enabled": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            422,
            "expected 422 for bad language {bad_lang:?}"
        );
    }

    // Body > 4KiB rejected with 413.
    let oversized_origin = format!("https://{}.example.com", "a".repeat(4200));
    let too_big = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "public_origin": oversized_origin,
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(too_big.status(), 413);

    // Valid save normalizes origin (lowercase host, strip trailing slash) and bumps version to 2.
    let save_resp = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .header("X-Forwarded-Host", "evil.example")
        .json(&json!({
            "public_origin": "https://Example.COM:8443/",
            "content_language": "en-US",
            "indexing_enabled": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_resp.status(), 200);
    assert_eq!(save_resp.headers().get(header::ETAG).unwrap(), "\"v2\"");
    let saved: Value = save_resp.json().await.unwrap();
    assert_eq!(
        saved,
        json!({
            "public_origin": "https://example.com:8443",
            "content_language": "en-US",
            "indexing_enabled": false,
            "version": 2
        })
    );

    // Stale version returns 412 without overwriting.
    let stale_resp = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "public_origin": Value::Null,
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_resp.status(), 412);

    // General settings version untouched.
    let general_after: Value = client
        .get(format!("{}/api/admin/settings/general", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(general_after, general_before);

    // Editor role gets 403 on both GET and PUT.
    let create_editor = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_editor.status(), 201);

    let (editor_cookie, editor_csrf) =
        login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;
    assert_eq!(
        client
            .get(format!("{}/api/admin/settings/public", srv.base_url))
            .header(header::COOKIE, &editor_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/settings/public", srv.base_url))
            .header(header::COOKIE, &editor_cookie)
            .header("X-CSRF-Token", &editor_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({
                "public_origin": Value::Null,
                "content_language": "vi",
                "indexing_enabled": true
            }))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    // Restart server and verify persistence.
    drop(srv);
    let srv2 = spawn_server_at(site_root.clone(), true).await;
    let (admin2_cookie, csrf2) =
        login(&client, &srv2.base_url, "admin", "AdminPassword123!").await;
    let persisted: Value = client
        .get(format!("{}/api/admin/settings/public", srv2.base_url))
        .header(header::COOKIE, &admin2_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(persisted, saved);

    // Empty string origin clears back to null.
    let cleared_resp = client
        .put(format!("{}/api/admin/settings/public", srv2.base_url))
        .header(header::COOKIE, &admin2_cookie)
        .header("X-CSRF-Token", &csrf2)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({
            "public_origin": "",
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cleared_resp.status(), 200);
    let cleared: Value = cleared_resp.json().await.unwrap();
    assert_eq!(
        cleared,
        json!({
            "public_origin": Value::Null,
            "content_language": "vi",
            "indexing_enabled": true,
            "version": 3
        })
    );
}
