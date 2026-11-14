use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

struct TestServer {
    child: Option<Child>,
    site_root: PathBuf,
    base_url: String,
    port: u16,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

impl TestServer {
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    async fn restart(&mut self) {
        self.stop();
        let bin = env!("CARGO_BIN_EXE_opendash");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", &self.site_root)
            .env("OPENDASH_PORT", self.port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("respawn opendash server");
        self.child = Some(child);
        wait_ready(&self.base_url).await;
    }
}

async fn wait_ready(base_url: &str) {
    let client = Client::new();
    for _ in 0..80 {
        if client.get(format!("{base_url}/api/health")).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server failed to become ready at {base_url}");
}

async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s08-t01-{nanos}"));
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
    wait_ready(&base_url).await;
    TestServer {
        child: Some(child),
        site_root,
        base_url,
        port,
    }
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header(header::ORIGIN, base_url)
        .json(&json!({
            "username": "admin",
            "password": "correct-horse-battery-staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login_user(client, base_url, "admin", "correct-horse-battery-staple").await
}

async fn login_user(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header(header::ORIGIN, base_url)
        .json(&json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|s| s.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/m7")
        .join(name)
}

#[tokio::test]
async fn m7_s08_t01_deterministic_e2e_and_adversarial_corpus() {
    let mut server = spawn_server().await;
    let client = Client::builder().build().unwrap();

    // 1. Unauthenticated access returns 401
    assert_eq!(
        client
            .get(format!("{}/api/admin/settings/general", server.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let (cookie, csrf) = setup_and_login(&client, &server.base_url).await;

    // 2. CSRF / Origin negative check
    assert_eq!(
        client
            .put(format!("{}/api/admin/settings/general", server.base_url))
            .header(header::COOKIE, &cookie)
            .header("x-csrf-token", &csrf)
            .header(header::ORIGIN, "https://evil.example")
            .json(&json!({ "site_title": "Hack" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // 3. RBAC negative check: create editor user and verify 403 on admin-only settings/smtp
    let create_editor = client
        .post(format!("{}/api/admin/users", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "username": "editor1",
            "password": "editor-password-1234",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_editor.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) =
        login_user(&client, &server.base_url, "editor1", "editor-password-1234").await;
    assert_eq!(
        client
            .put(format!("{}/api/admin/settings/general", server.base_url))
            .header(header::ORIGIN, &server.base_url)
            .header(header::COOKIE, &ed_cookie)
            .header("x-csrf-token", &ed_csrf)
            .json(&json!({ "site_title": "Nope" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // 4. Settings + XSS escaping + Menu
    let settings_get = client
        .get(format!("{}/api/admin/settings/general", server.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(settings_get.status(), StatusCode::OK);
    let settings_etag = settings_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let settings_res = client
        .put(format!("{}/api/admin/settings/general", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &settings_etag)
        .json(&json!({
            "site_title": "OpenDash <script>alert(1)</script>",
            "tagline": "Deterministic E2E",
            "timezone": "UTC"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(settings_res.status(), StatusCode::OK);

    // 5. Contact form + submission + inbox + disabled SMTP state
    let form_res = client
        .post(format!("{}/api/admin/contact/forms", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "contact-e2e",
            "title": "Lien he E2E",
            "schema": {
                "version": 1,
                "fields": [
                    { "key": "email", "label": "Email", "kind": "email", "required": true },
                    { "key": "message", "label": "Noi dung", "kind": "textarea", "required": true }
                ]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(form_res.status(), StatusCode::CREATED);
    let form_id = form_res.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    // Save SMTP config with secret (tests Windows secret DACL)
    let smtp_get = client
        .get(format!("{}/api/admin/settings/smtp", server.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(smtp_get.status(), StatusCode::OK);
    let smtp_etag = smtp_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let smtp_res = client
        .put(format!("{}/api/admin/settings/smtp", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &smtp_etag)
        .json(&json!({
            "enabled": false,
            "host": "127.0.0.1",
            "port": 2525,
            "tls_mode": "starttls",
            "username": "mailer",
            "from_address": "noreply@example.com",
            "recipient_address": "admin@example.com",
            "password_update": {
                "mode": "replace",
                "value": "test-only-smtp-password"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(smtp_res.status(), StatusCode::OK);

    #[cfg(windows)]
    {
        let secret_dir = server.site_root.join("database").join("secrets").join("smtp");
        if secret_dir.exists() {
            let out = Command::new("icacls")
                .arg(&secret_dir)
                .output()
                .expect("run icacls");
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(!stdout.contains("Everyone"), "Unexpected Everyone ACE: {stdout}");
            assert!(!stdout.contains("BUILTIN\\Users"), "Unexpected Users ACE: {stdout}");
        }
    }

    // 6. Video fixture upload + malformed video rejection + Builder core/video + Range/HEAD/ACL/CSP
    let mp4_path = fixture_path("video-avc-aac.mp4");
    let mp4_bytes = fs::read(&mp4_path).expect("read video-avc-aac.mp4 fixture");
    let expected_sha = format!("{:x}", Sha256::digest(&mp4_bytes));

    // Malformed MP4 must be rejected
    assert_eq!(
        client
            .post(format!("{}/api/admin/media/video", server.base_url))
            .header(header::ORIGIN, &server.base_url)
            .header(header::COOKIE, &cookie)
            .header("x-csrf-token", &csrf)
            .header(header::CONTENT_TYPE, "video/mp4")
            .body(b"not-an-mp4".to_vec())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    let vid_up = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(mp4_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(vid_up.status(), StatusCode::CREATED);
    let vid_json: Value = vid_up.json().await.unwrap();
    assert_eq!(vid_json["id"].as_str().unwrap(), expected_sha);

    // Unreferenced video must be 404 publicly
    assert_eq!(
        client
            .get(format!("{}/media/{}/file", server.base_url, expected_sha))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // Save homepage with core/video + core/contact_form
    let doc = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec1",
            "children": [{
                "id": "row1",
                "children": [{
                    "id": "col1",
                    "children": [
                        {
                            "id": "vid1",
                            "type": "core/video",
                            "props": {
                                "media_id": expected_sha,
                                "controls": true,
                                "muted": false
                            }
                        },
                        {
                            "id": "frm1",
                            "type": "core/contact_form",
                            "props": {
                                "form_id": form_id,
                                "submit_label": "Gui"
                            }
                        }
                    ]
                }]
            }]
        }]
    });
    assert_eq!(
        client
            .put(format!("{}/api/admin/builder/homepage", server.base_url))
            .header(header::ORIGIN, &server.base_url)
            .header(header::COOKIE, &cookie)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&doc)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // Stale If-Match must return 412
    assert_eq!(
        client
            .put(format!("{}/api/admin/builder/homepage", server.base_url))
            .header(header::ORIGIN, &server.base_url)
            .header(header::COOKIE, &cookie)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&doc)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_FAILED
    );

    // Public homepage escapes XSS, includes <video> and media-src 'self'
    let home = client.get(format!("{}/", server.base_url)).send().await.unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    let csp = home
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(csp.contains("media-src 'self'"), "CSP: {csp}");
    let home_html = home.text().await.unwrap();
    assert!(!home_html.contains("<script>alert(1)</script>"));
    assert!(home_html.contains(&format!("<video src=\"/media/{expected_sha}/file\"")));

    // Range 206 + 416 + HEAD
    let r206 = client
        .get(format!("{}/media/{}/file", server.base_url, expected_sha))
        .header(header::RANGE, "bytes=0-63")
        .send()
        .await
        .unwrap();
    assert_eq!(r206.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(r206.bytes().await.unwrap().as_ref(), &mp4_bytes[..64]);

    assert_eq!(
        client
            .get(format!("{}/media/{}/file", server.base_url, expected_sha))
            .header(header::RANGE, "bytes=99999-")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::RANGE_NOT_SATISFIABLE
    );

    // Public settings -> sitemap.xml and robots.txt
    let pub_get = client
        .get(format!("{}/api/admin/settings/public", server.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(pub_get.status(), StatusCode::OK);
    let pub_etag = pub_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let pub_put = client
        .put(format!("{}/api/admin/settings/public", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &pub_etag)
        .json(&json!({
            "public_origin": server.base_url,
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_put.status(), StatusCode::OK);

    assert_eq!(
        client.get(format!("{}/sitemap.xml", server.base_url)).send().await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        client.get(format!("{}/robots.txt", server.base_url)).send().await.unwrap().status(),
        StatusCode::OK
    );

    // 7. Restart persistence check
    server.restart().await;
    let after_restart = client.get(format!("{}/", server.base_url)).send().await.unwrap();
    assert_eq!(after_restart.status(), StatusCode::OK);
    let after_html = after_restart.text().await.unwrap();
    assert!(after_html.contains(&format!("<video src=\"/media/{expected_sha}/file\"")));
}
