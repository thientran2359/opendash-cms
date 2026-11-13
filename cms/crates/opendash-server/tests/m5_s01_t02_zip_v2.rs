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
        "opendash-m5-s01-t02-{}-{nanos}",
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

#[tokio::test]
async fn m5_s01_t02_zip_v2_install_activate_rollback_and_asset_route() {
    let srv = spawn_server().await;
    let client = Client::new();

    // 1. Setup & login as admin
    let setup_resp = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup_resp.status(), 201);

    let login_resp = client
        .post(format!("{}/auth/login", srv.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp.status(), 200);
    let cookie = login_resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let login_json: Value = login_resp.json().await.unwrap();
    let csrf = login_json["csrf_token"].as_str().unwrap().to_string();

    // 2. Unauthenticated install fails with 401
    let anon = Client::new();
    let unauth = anon
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .body(vec![1, 2, 3])
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), 401);

    // 3. Malicious ZIP corpus rejected before touching revisions/
    let bad_manifest =
        br#"{"manifest_version":2,"theme_id":"bad-theme","name":"Bad","version":"1.0.0"}"#;
    for bad_zip in [
        build_zip(&[
            ("theme.json", bad_manifest, CompressionMethod::Stored),
            ("../escape.css", b"body{}", CompressionMethod::Stored),
        ]),
        build_zip(&[
            ("theme.json", bad_manifest, CompressionMethod::Stored),
            ("assets\\style.css", b"body{}", CompressionMethod::Stored),
        ]),
        build_zip(&[
            ("theme.json", bad_manifest, CompressionMethod::Stored),
            ("assets/app.css", b"a{}", CompressionMethod::Stored),
            ("Assets/App.css", b"b{}", CompressionMethod::Stored),
        ]),
        build_zip(&[
            ("theme.json", bad_manifest, CompressionMethod::Stored),
            ("assets/evil.js", b"alert(1)", CompressionMethod::Stored),
        ]),
    ] {
        let resp = client
            .post(format!("{}/api/admin/themes", srv.base_url))
            .header("Content-Type", "application/zip")
            .header("Cookie", &cookie)
            .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
            .body(bad_zip)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }
    let rev_dir = srv.site_root.join("themes").join("revisions");
    if rev_dir.exists() {
        assert_eq!(fs::read_dir(&rev_dir).unwrap().count(), 0);
    }

    // 4. v1 ZIP still installs cleanly (no regression)
    let v1_zip = build_zip(&[
        (
            "block.json",
            br#"{"id":"acme/hero","fields":{"title":"Title"}}"#,
            CompressionMethod::Deflated,
        ),
        (
            "block.html",
            b"<section>{{ values.title }}</section>",
            CompressionMethod::Deflated,
        ),
    ]);
    let v1_resp = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .body(v1_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(v1_resp.status(), 201);

    // 5. v2 ZIP install (1.0.0) succeeds without auto-activating
    let m1 = br#"{
        "manifest_version": 2,
        "theme_id": "editorial-clean",
        "name": "Editorial Clean",
        "version": "1.0.0",
        "templates": { "single": "templates/single.html" }
    }"#;
    let zip_v2_a = build_zip(&[
        ("theme.json", m1, CompressionMethod::Deflated),
        (
            "templates/single.html",
            b"<article>{{ entry.title }}</article>",
            CompressionMethod::Deflated,
        ),
        (
            "assets/style.css",
            b"body { color: #111; }",
            CompressionMethod::Deflated,
        ),
    ]);
    let install_a = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .body(zip_v2_a.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(install_a.status(), 201);
    let body_a: Value = install_a.json().await.unwrap();
    assert_eq!(body_a["activated"], false);
    let rev_a = body_a["revision_id"].as_str().unwrap().to_string();
    let digest_a = body_a["revision_digest"].as_str().unwrap().to_string();

    // Idempotent re-upload returns 200 with same revision
    let reinstall_a = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .body(zip_v2_a)
        .send()
        .await
        .unwrap();
    assert_eq!(reinstall_a.status(), 200);

    // 6. Install v2 2.0.0, then test If-Match guards + activation + rollback
    let m2 = br#"{
        "manifest_version": 2,
        "theme_id": "editorial-clean",
        "name": "Editorial Clean",
        "version": "2.0.0",
        "templates": { "single": "templates/single.html" }
    }"#;
    let zip_v2_b = build_zip(&[
        ("theme.json", m2, CompressionMethod::Deflated),
        (
            "templates/single.html",
            b"<main>{{ entry.title }}</main>",
            CompressionMethod::Deflated,
        ),
    ]);
    let install_b = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .body(zip_v2_b)
        .send()
        .await
        .unwrap();
    assert_eq!(install_b.status(), 201);
    let body_b: Value = install_b.json().await.unwrap();
    let rev_b = body_b["revision_id"].as_str().unwrap().to_string();

    // Missing If-Match -> 428
    let no_if_match = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "revision_id": rev_a }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_if_match.status(), 428);

    // Stale If-Match -> 412
    let stale_if_match = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v99\"")
        .json(&serde_json::json!({ "revision_id": rev_a }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_if_match.status(), 412);

    // Activate rev_a with "v1" -> 200, ETag "v2"
    let act_a = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&serde_json::json!({ "revision_id": rev_a }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_a.status(), 200);
    assert_eq!(act_a.headers().get("etag").unwrap(), "\"v2\"");

    // Activate rev_b with "v2" -> 200, ETag "v3"
    let act_b = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&serde_json::json!({ "revision_id": rev_b }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_b.status(), 200);
    assert_eq!(act_b.headers().get("etag").unwrap(), "\"v3\"");

    // 7. Revision containing declarations is rejected from activation (422) without mutating active
    let m_decl = br#"{
        "manifest_version": 2,
        "theme_id": "editorial-clean",
        "name": "Editorial Clean",
        "version": "3.0.0",
        "declarations": { "content_types": [{"key": "portfolio"}] }
    }"#;
    let zip_decl = build_zip(&[("theme.json", m_decl, CompressionMethod::Deflated)]);
    let install_decl = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .body(zip_decl)
        .send()
        .await
        .unwrap();
    assert_eq!(install_decl.status(), 201);
    let rev_decl = install_decl.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    let act_decl = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v3\"")
        .json(&serde_json::json!({ "revision_id": rev_decl }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_decl.status(), 422);
    let decl_err = act_decl.text().await.unwrap();
    assert!(decl_err.contains("unsupported_declarations_pending_cnt_bridge"));

    // Rollback to rev_a still works with "v3" (confirming failed declarations did not bump or corrupt)
    let rollback_a = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v3\"")
        .json(&serde_json::json!({ "revision_id": rev_a }))
        .send()
        .await
        .unwrap();
    assert_eq!(rollback_a.status(), 200);
    assert_eq!(rollback_a.headers().get("etag").unwrap(), "\"v4\"");

    // 8. /theme/{revision_digest}/{*path} asset route checks
    let asset_ok = anon
        .get(format!(
            "{}/theme/{digest_a}/assets/style.css",
            srv.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(asset_ok.status(), 200);
    assert_eq!(
        asset_ok.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        asset_ok.headers().get("content-type").unwrap(),
        "text/css; charset=utf-8"
    );
    assert_eq!(asset_ok.text().await.unwrap(), "body { color: #111; }");

    // Non-asset / manifest / template / traversal blocked with 404
    for blocked in [
        format!("{}/theme/{digest_a}/theme.json", srv.base_url),
        format!("{}/theme/{digest_a}/templates/single.html", srv.base_url),
        format!("{}/theme/{digest_a}/../style.css", srv.base_url),
    ] {
        let r = anon.get(blocked).send().await.unwrap();
        assert_eq!(r.status(), 404);
    }

    // Missing .complete marker blocks /theme/ serving
    let marker = srv
        .site_root
        .join("themes")
        .join("revisions")
        .join(&digest_a)
        .join(".complete");
    fs::remove_file(&marker).unwrap();
    let missing_marker = anon
        .get(format!(
            "{}/theme/{digest_a}/assets/style.css",
            srv.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_marker.status(), 404);
}
