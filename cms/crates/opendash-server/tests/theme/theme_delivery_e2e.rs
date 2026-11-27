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

// S03: Home is a published Page selected atomically in Reading Settings.
async fn seed_builder_homepage(client: &Client, base_url: &str, cookie: &str, csrf: &str) -> i64 {
    let types: Vec<Value> = client
        .get(format!("{base_url}/api/admin/content-types"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let schema_token = types
        .iter()
        .find(|v| v["descriptor"]["key"] == "page")
        .unwrap()["schema_token"]
        .as_str()
        .unwrap();
    let created = client
        .post(format!(
            "{base_url}/api/admin/content-types/page/entries/composite"
        ))
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .json(&serde_json::json!({
            "expected_schema_token": schema_token,
            "slug":"theme-home-fixture",
            "title":"Theme Home Fixture",
            "values":{"body":"Theme Home Fixture"},
            "publication_status":"published"
        }))
        .send()
        .await
        .unwrap();
    let status = created.status();
    let raw = created.text().await.unwrap();
    assert_eq!(status, 201, "S03 Home Page create: {raw}");
    let page_id = serde_json::from_str::<Value>(&raw).unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();
    let reading = client
        .get(format!("{base_url}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(reading.status(), 200);
    let etag = reading.headers()["etag"].to_str().unwrap().to_owned();
    let assigned = client
        .put(format!("{base_url}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", etag)
        .json(&serde_json::json!({"homepage_page_id":page_id,"posts_page_id":null}))
        .send()
        .await
        .unwrap();
    let status = assigned.status();
    let body = assigned.text().await.unwrap();
    assert_eq!(status, 200, "S03 Reading assignment: {body}");
    page_id
}

#[tokio::test]
async fn m5_s08_t01_cross_module_e2e_delivery_import_preservation_and_security() {
    let site_root = std::env::temp_dir().join(format!(
        "opendash_m5_s08_t01_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let srv = spawn_server_at(site_root.clone(), true).await;
    let client = Client::new();
    let (cookie, csrf) = login(&client, &srv.base_url).await;

    // 1. Install & activate Theme A with CPT, options, templates, and sample pack
    let png = tiny_png();
    let manifest_a = serde_json::to_vec(&json!({
        "manifest_version": 3,
        "theme_id": "release-theme-a",
        "name": "Release Theme A",
        "version": "1.0.0",
        "assets": ["assets/hero.png"],
        "templates": [{ "kind": "home", "file": "home.html" }, { "kind": "single", "file": "single.html" }, { "kind": "archive", "file": "archive.html" }], "parts": { "header": "header.html", "footer": "footer.html" },
        "declarations": {
            "content_types": [
                {
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "text", "required": true }
                    ],
                    "taxonomies": ["category", "tag"]
                }
            ]
        },
        "options": [
            { "key": "brand", "label": "Brand", "type": "text", "default": "OpenDash", "public": true }
        ],
        "sample": {
            "sample_key": "release_pack",
            "media": [
                { "object_key": "hero", "path": "assets/hero.png", "alt": "Hero" }
            ],
            "pages": [
                {
                    "object_key": "about",
                    "slug": "about-release",
                    "title": "About Release",
                    "content_type": "page",
                    "values": {},
                    "media_refs": ["hero"]
                }
            ]
        }
    }))
    .unwrap();
    let zip_a = build_zip(&[
        ("theme.json", &manifest_a),
        (
            "header.html",
            b"<header><nav><a href=\"/\">{{ options.brand }}</a></nav></header>",
        ),
        (
            "home.html",
            b"<main><h1>{{ options.brand }} Home</h1>{{ od_content() }}</main>",
        ),
        (
            "single.html",
            b"<main><h1>{{ entry.title }}</h1>{{ od_content() }}</main>",
        ),
        ("archive.html", b"<main><h1>Archive</h1></main>"),
        ("footer.html", b"<footer>Release Footer</footer>"),
        ("assets/hero.png", &png),
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

    seed_builder_homepage(&client, &srv.base_url, &cookie, &csrf).await;

    // 2. Selective sample import dry-run + execute + idempotent retry
    let plan: Value = client
        .post(format!(
            "{}/api/themes/release-theme-a/import/dry-run",
            srv.base_url
        ))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "groups": ["media", "pages"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let exec_body = json!({
        "groups": ["media", "pages"],
        "plan_digest": plan["plan_digest"],
        "expected_versions": plan["expected_versions"],
        "acknowledge_public_changes": true,
        "run_id": "e2e-run-1"
    });
    let out1: Value = client
        .post(format!(
            "{}/api/themes/release-theme-a/import/execute",
            srv.base_url
        ))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&exec_body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(out1["run_id"], "e2e-run-1");
    assert_eq!(out1["created"].as_array().unwrap().len(), 2);
    let out2: Value = client
        .post(format!(
            "{}/api/themes/release-theme-a/import/execute",
            srv.base_url
        ))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&exec_body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(out1, out2);

    // 3. Theme Options SSR + Override Save + Public delivery
    assert_eq!(
        client
            .put(format!(
                "{}/api/themes/release-theme-a/options",
                srv.base_url
            ))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "values": { "brand": "OpenDashV2" } }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let home_html = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home_html.contains("OpenDashV2"));

    // 4. Create published portfolio entry, switch to Theme B, verify preservation & restart
    let entry_res: Value = client
        .post(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv.base_url
        ))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "e2e-work",
            "title": "E2E Work",
            "values": { "client": "Global Co" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entry_id = entry_res["id"].as_i64().unwrap();

    let zip_b = build_zip(&[
        ("theme.json", br#"{"manifest_version":3,"theme_id":"release-theme-b","name":"B","version":"1.0.0","templates": [{ "kind": "home", "file": "home.html" }]}"#),
        ("home.html", b"<main>Beta{{ od_content() }}</main>"),
    ]);
    let rev_b = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .body(zip_b)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v2\"")
            .json(&json!({ "revision_id": rev_b }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    drop(srv);

    let srv2 = spawn_server_at(site_root.clone(), false).await;
    // C3/C4: persisted provider CPT data remains in Admin, but anonymous
    // archive and single routes are unavailable until that provider returns.
    assert_eq!(
        client
            .get(format!("{}/portfolio", srv2.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let pub_single = client
        .get(format!("{}/portfolio/e2e-work", srv2.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_single.status(), 404);

    let (cookie2, csrf2) = login(&client, &srv2.base_url).await;
    let entries: Value = client
        .get(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv2.base_url
        ))
        .header("Cookie", &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        entries
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == entry_id)
    );

    // Restore the identical provider revision; the existing row (same ID)
    // becomes public again without a re-import or content rewrite.
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv2.base_url))
            .header("Cookie", &cookie2)
            .header("Origin", &srv2.base_url)
            .header("X-CSRF-Token", &csrf2)
            .header("If-Match", "\"v3\"")
            .json(&json!({ "revision_id": rev_a }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let restored_archive = client
        .get(format!("{}/portfolio", srv2.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(restored_archive.status(), 200);
    let restored_single = client
        .get(format!("{}/portfolio/e2e-work", srv2.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(restored_single.status(), 200);
    assert!(restored_single.text().await.unwrap().contains("E2E Work"));
    let entries_after: Value = client
        .get(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv2.base_url
        ))
        .header("Cookie", &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        entries_after
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == entry_id)
    );

    drop(srv2);
    let _ = fs::remove_dir_all(&site_root);
}
