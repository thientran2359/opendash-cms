use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::Client;
use serde_json::Value;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

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
    let site_root = std::env::temp_dir().join(format!(
        "opendash-m5-s04-t01-{}-{nanos}",
        std::process::id()
    ));
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
    for _ in 0..50 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("opendash server did not become healthy in time");
}

fn build_zip(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        for (name, data, method) in entries {
            let opts = SimpleFileOptions::default().compression_method(*method);
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

fn tiny_png() -> Vec<u8> {
    // Minimal 1x1 transparent PNG
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

async fn login(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let json: Value = resp.json().await.unwrap();
    let csrf = json["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn m5_s04_t01_theme_options_rbac_cas_acl_compat_and_ssr() {
    let srv = spawn_server().await;
    let client = Client::new();

    // 1. Setup admin & create editor user
    let setup = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&serde_json::json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    let (admin_cookie, admin_csrf) =
        login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    let create_editor = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .json(&serde_json::json!({
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

    // 2. Upload media asset (unreferenced -> 404 publicly)
    let upload_resp = client
        .post(format!("{}/api/admin/media", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header(reqwest::header::CONTENT_TYPE, "image/png")
        .body(tiny_png())
        .send()
        .await
        .unwrap();
    assert_eq!(upload_resp.status(), 201);
    let media_json: Value = upload_resp.json().await.unwrap();
    let media_id = media_json["id"].as_str().unwrap().to_string();

    assert_eq!(
        client
            .get(format!("{}/media/{media_id}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    // 3. Install & activate Theme A with 5 option types
    let manifest_a = br#"{
        "manifest_version": 2,
        "theme_id": "theme-alpha",
        "name": "Theme Alpha",
        "version": "1.0.0",
        "templates": {
            "header": "templates/header.html",
            "footer": "templates/footer.html",
            "home": "templates/home.html"
        },
        "options": [
            { "key": "banner_text", "label": "Banner", "type": "string", "default": "Welcome", "public": true },
            { "key": "cols", "label": "Columns", "type": "number", "default": 3, "min": 1, "max": 6, "public": true },
            { "key": "show_banner", "label": "Show Banner", "type": "boolean", "default": true, "public": true },
            { "key": "tone", "label": "Tone", "type": "select", "default": "light", "choices": [{"value":"light","label":"Light"},{"value":"dark","label":"Dark"}], "public": true },
            { "key": "logo_ref", "label": "Logo", "type": "image_ref", "default": null, "public": true },
            { "key": "internal_note", "label": "Internal", "type": "string", "default": "SECRET_ADMIN_ONLY", "public": false }
        ]
    }"#;
    let zip_a = build_zip(&[
        ("theme.json", manifest_a, CompressionMethod::Deflated),
        ("templates/header.html", b"<header>{% if options.show_banner %}<span>{{ options.banner_text }}</span>{% endif %}</header>", CompressionMethod::Deflated),
        ("templates/footer.html", b"<footer data-tone=\"{{ options.tone }}\">Cols:{{ options.cols }}</footer>", CompressionMethod::Deflated),
        ("templates/home.html", b"<main>Home</main>", CompressionMethod::Deflated),
    ]);
    let inst_a = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .body(zip_a)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_a.status(), 201);
    let rev_a = inst_a.json::<Value>().await.unwrap()["revision_id"].as_str().unwrap().to_string();

    let act_a = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&serde_json::json!({ "revision_id": rev_a }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_a.status(), 200);

    // 4. RBAC: Guest -> 401, Editor -> 403
    assert_eq!(
        client
            .get(format!("{}/api/themes/theme-alpha/options", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{}/api/themes/theme-alpha/options", srv.base_url))
            .header(reqwest::header::COOKIE, &editor_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .put(format!("{}/api/themes/theme-alpha/options", srv.base_url))
            .header(reqwest::header::COOKIE, &editor_cookie)
            .header("x-csrf-token", &editor_csrf)
            .header("If-Match", "\"v1\"")
            .json(&serde_json::json!({ "values": { "banner_text": "Hack" } }))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    // 5. Validation & CAS checks
    assert_eq!(
        client
            .put(format!("{}/api/themes/theme-alpha/options", srv.base_url))
            .header(reqwest::header::COOKIE, &admin_cookie)
            .header("x-csrf-token", &admin_csrf)
            .json(&serde_json::json!({ "values": { "banner_text": "Hi" } }))
            .send()
            .await
            .unwrap()
            .status(),
        428
    );
    assert_eq!(
        client
            .put(format!("{}/api/themes/theme-alpha/options", srv.base_url))
            .header(reqwest::header::COOKIE, &admin_cookie)
            .header("x-csrf-token", &admin_csrf)
            .header("If-Match", "\"v99\"")
            .json(&serde_json::json!({ "values": { "banner_text": "Hi" } }))
            .send()
            .await
            .unwrap()
            .status(),
        412
    );
    for bad_values in [
        serde_json::json!({ "banner_text": "a".repeat(4097) }),
        serde_json::json!({ "cols": 99 }),
        serde_json::json!({ "tone": "neon" }),
        serde_json::json!({ "logo_ref": "nonexistent_media_id" }),
    ] {
        assert_eq!(
            client
                .put(format!("{}/api/themes/theme-alpha/options", srv.base_url))
                .header(reqwest::header::COOKIE, &admin_cookie)
                .header("x-csrf-token", &admin_csrf)
                .header("If-Match", "\"v1\"")
                .json(&serde_json::json!({ "values": bad_values }))
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
    }

    // 6. Save valid options + image_ref -> SSR header/footer updates + media ACL becomes 200
    let save_ok = client
        .put(format!("{}/api/themes/theme-alpha/options", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&serde_json::json!({
            "values": {
                "banner_text": "Autumn Sale",
                "cols": 4,
                "show_banner": true,
                "tone": "dark",
                "logo_ref": media_id,
                "internal_note": "ADMIN_PRIVATE_VALUE"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_ok.status(), 200);

    let home_html = client.get(format!("{}/", srv.base_url)).send().await.unwrap().text().await.unwrap();
    assert!(home_html.contains("Autumn Sale"), "{home_html}");
    assert!(home_html.contains("Cols:4"), "{home_html}");
    assert!(home_html.contains("data-tone=\"dark\""), "{home_html}");
    assert!(!home_html.contains("ADMIN_PRIVATE_VALUE"));

    assert_eq!(
        client
            .get(format!("{}/media/{media_id}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // 7. Switch Theme A -> B: media ACL immediately revoked (404), then B -> A restores saved values & ACL (200)
    let zip_b = build_zip(&[
        ("theme.json", br#"{"manifest_version":2,"theme_id":"theme-beta","name":"Theme Beta","version":"1.0.0","templates":{"home":"templates/home.html"}}"#, CompressionMethod::Deflated),
        ("templates/home.html", b"<main>Beta</main>", CompressionMethod::Deflated),
    ]);
    let inst_b = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .body(zip_b)
        .send()
        .await
        .unwrap();
    let rev_b = inst_b.json::<Value>().await.unwrap()["revision_id"].as_str().unwrap().to_string();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header(reqwest::header::COOKIE, &admin_cookie)
            .header("x-csrf-token", &admin_csrf)
            .header("If-Match", "\"v2\"")
            .json(&serde_json::json!({ "revision_id": rev_b }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{}/media/{media_id}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header(reqwest::header::COOKIE, &admin_cookie)
            .header("x-csrf-token", &admin_csrf)
            .header("If-Match", "\"v3\"")
            .json(&serde_json::json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{}/media/{media_id}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // 8. Incompatible option type on Theme A v2.0.0 -> 409 Conflict when activating
    let manifest_a_bad = br#"{
        "manifest_version": 2,
        "theme_id": "theme-alpha",
        "name": "Theme Alpha",
        "version": "2.0.0",
        "templates": { "home": "templates/home.html" },
        "options": [
            { "key": "banner_text", "label": "Banner", "type": "number", "default": 1, "public": true }
        ]
    }"#;
    let zip_a_bad = build_zip(&[
        ("theme.json", manifest_a_bad, CompressionMethod::Deflated),
        ("templates/home.html", b"<main>Home 2</main>", CompressionMethod::Deflated),
    ]);
    let inst_a_bad = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .body(zip_a_bad)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_a_bad.status(), 201);
    let rev_a_bad = inst_a_bad.json::<Value>().await.unwrap()["revision_id"].as_str().unwrap().to_string();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header(reqwest::header::COOKIE, &admin_cookie)
            .header("x-csrf-token", &admin_csrf)
            .header("If-Match", "\"v4\"")
            .json(&serde_json::json!({ "revision_id": rev_a_bad }))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );

    // 9. Clear image_ref -> media ACL immediately revoked (404)
    assert_eq!(
        client
            .put(format!("{}/api/themes/theme-alpha/options", srv.base_url))
            .header(reqwest::header::COOKIE, &admin_cookie)
            .header("x-csrf-token", &admin_csrf)
            .header("If-Match", "\"v2\"")
            .json(&serde_json::json!({ "values": { "logo_ref": null } }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        client
            .get(format!("{}/media/{media_id}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}
