use std::{
    fs,
    io::{Cursor, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use image::{DynamicImage, ImageFormat};
use reqwest::Client;
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TestServer {
    base_url: String,
    child: Child,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn tiny_png() -> Vec<u8> {
    let img = DynamicImage::new_rgb8(4, 4);
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        for (name, bytes) in entries {
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

async fn spawn_server_at(site_root: PathBuf, setup_admin: bool) -> TestServer {
    fs::create_dir_all(&site_root).unwrap();
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let exe = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(exe)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", &site_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let client = Client::new();
    for _ in 0..80 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
            if resp.status().is_success() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if setup_admin {
        let res = client
            .post(format!("{base_url}/auth/setup"))
            .json(&json!({
                "username": "admin",
                "password": "AdminPassword123!"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
    }

    TestServer { base_url, child }
}

async fn login(client: &Client, base_url: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn damaged_package_fallback_adversarial_csec_acl_csrf_cas_and_accessibility() {
    let site_root = std::env::temp_dir().join(format!(
        "opendash_m5_s07_t01_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let srv = spawn_server_at(site_root.clone(), true).await;
    let client = Client::new();
    let (cookie, csrf) = login(&client, &srv.base_url).await;

    // 1. Install & activate valid Theme A with `public: false` option
    let manifest_a = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "safe-theme",
        "name": "Safe Theme",
        "version": "1.0.0",
        "templates": {
            "header": "templates/header.html",
            "home": "templates/home.html",
            "single": "templates/single.html",
            "archive": "templates/archive.html",
            "footer": "templates/footer.html"
        },
        "options": [
            { "key": "public_title", "label": "Title", "type": "string", "default": "SafeSite", "public": true },
            { "key": "secret_key", "label": "Secret", "type": "string", "default": "TOP_SECRET_VALUE_999", "public": false }
        ]
    }))
    .unwrap();
    let zip_a = build_zip(&[
        ("theme.json", &manifest_a),
        ("templates/header.html", b"<header><nav><a href=\"/\">{{ options.public_title }}</a></nav></header>"),
        ("templates/home.html", b"<main><h1>{{ options.public_title }}</h1></main>"),
        ("templates/single.html", b"<main><h1>{{ entry.title }}</h1></main>"),
        ("templates/archive.html", b"<main><h1>Archive</h1>{% for e in entries %}<article><h2>{{ e.title }}</h2></article>{% endfor %}</main>"),
        ("templates/footer.html", b"<footer><p>Footer</p></footer>"),
    ]);
    let inst_a = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .body(zip_a)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_a.status(), 201);
    let rev_a = inst_a.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // Create published post & draft post & unreferenced private media
    let p_pub: Value = client
        .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "public-post", "title": "Public Post", "values": {} }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pub_id = p_pub["id"].as_i64().unwrap();
    assert_eq!(
        client
            .post(format!("{}/api/admin/content-types/post/entries/{pub_id}/publication", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let _p_draft = client
        .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "secret-draft", "title": "SECRET_DRAFT_TITLE", "values": {} }))
        .send()
        .await
        .unwrap();

    let media_resp: Value = client
        .post(format!("{}/api/admin/media", srv.base_url))
        .header("Content-Type", "image/png")
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .body(tiny_png())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let media_id = media_resp["id"].as_str().unwrap();

    assert_eq!(
        client
            .put(format!("{}/api/themes/safe-theme/options", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "values": { "public_title": "SafeSite", "secret_key": "TOP_SECRET_VALUE_999" } }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // Private media stays 404 publicly; public:false option NEVER leaks
    assert_eq!(
        client
            .get(format!("{}/media/{media_id}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let home_res = client.get(format!("{}/", srv.base_url)).send().await.unwrap();
    assert_eq!(home_res.status(), 200);
    assert!(home_res.headers().contains_key("content-security-policy"));
    let home_html = home_res.text().await.unwrap();
    assert!(home_html.contains("SafeSite"));
    assert!(!home_html.contains("TOP_SECRET_VALUE_999"));

    // 2. Accessibility & semantic landmarks smoke on public pages
    for path in ["/", "/content/post", "/content/post/public-post"] {
        let resp = client.get(format!("{}{path}", srv.base_url)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "path {path}");
        let body = resp.text().await.unwrap();
        assert!(body.contains("<header"), "missing <header> on {path}");
        assert!(body.contains("<main"), "missing <main> on {path}");
        assert!(body.contains("<footer"), "missing <footer> on {path}");
        assert!(body.contains("<h1"), "missing <h1> on {path}");
        assert!(!body.contains("SECRET_DRAFT_TITLE"), "draft leaked on {path}");
    }

    // 3. Adversarial C-SEC corpus: all dangerous templates must be rejected at install
    let bad_templates: &[(&str, &[u8])] = &[
        ("script_tag", b"<main><script>alert(1)</script></main>"),
        ("svg_tag", b"<main><svg onload=alert(1)></svg></main>"),
        ("math_tag", b"<main><math><mi>x</mi></math></main>"),
        ("event_handler", b"<main><div onclick=\"alert(1)\">x</div></main>"),
        ("js_url", b"<main><a href=\"javascript:alert(1)\">x</a></main>"),
        ("forged_filter", b"<main>{{ entry.title | safe }}</main>"),
        ("include_stmt", b"<main>{% include 'other.html' %}</main>"),
        ("import_stmt", b"<main>{% import 'macros.html' as m %}</main>"),
        ("extends_stmt", b"<main>{% extends 'base.html' %}</main>"),
        ("css_import", b"<main><style>@import url('https://evil.example/a.css');</style></main>"),
        ("css_expression", b"<main><style>body{width:expression(alert(1))}</style></main>"),
    ];
    for (idx, (label, tpl)) in bad_templates.iter().enumerate() {
        let bad_manifest = serde_json::to_vec(&json!({
            "manifest_version": 2,
            "theme_id": format!("evil-{idx}"),
            "name": "Evil",
            "version": "1.0.0",
            "templates": { "home": "templates/home.html" }
        }))
        .unwrap();
        let bad_zip = build_zip(&[("theme.json", &bad_manifest), ("templates/home.html", tpl)]);
        let resp = client
            .post(format!("{}/api/admin/themes", srv.base_url))
            .header("Content-Type", "application/zip")
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .body(bad_zip)
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_client_error(),
            "adversarial template `{label}` was not rejected: {}",
            resp.status()
        );
    }

    // 4. CSRF / Origin / CAS matrix
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", "https://evil.example")
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v2\"")
            .json(&json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("If-Match", "\"v2\"")
            .json(&json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        428
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v99\"")
            .json(&json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        412
    );

    // 5. Corrupt/delete active theme files & `.complete` on disk -> core fallback returns 200 without 500 or host path leak!
    let themes_dir = site_root.join("themes").join("safe-theme");
    if themes_dir.exists() {
        fs::remove_dir_all(&themes_dir).unwrap();
    }
    for path in ["/", "/content/post", "/content/post/public-post"] {
        let resp = client.get(format!("{}{path}", srv.base_url)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "damaged theme fallback failed on {path}");
        let body = resp.text().await.unwrap();
        assert!(!body.contains("opendash_m5_s07_t01"), "host temp path leaked on {path}");
        assert!(!body.contains("TOP_SECRET_VALUE_999"));
    }

    drop(srv);
    let _ = fs::remove_dir_all(&site_root);
}
