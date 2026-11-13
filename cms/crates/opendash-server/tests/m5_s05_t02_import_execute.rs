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
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
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

async fn spawn_server_at(site_root: PathBuf, run_setup: bool) -> TestServer {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let exe = env!("CARGO_BIN_EXE_opendash");
    let mut child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let client = Client::new();
    for _ in 0..80 {
        if client.get(format!("{base_url}/api/health")).send().await.is_ok() {
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("server exited early: {status}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if run_setup {
        let res = client
            .post(format!("{base_url}/auth/setup"))
            .json(&json!({
                "username": "admin",
                "password": "AdminPassword123!"
            }))
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success());
    }

    TestServer { base_url, child }
}

async fn login(client: &Client, base_url: &str) -> (String, String) {
    let res = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let cookie = res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = res.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn sample_import_execute_g_recovery_idempotency_upgrade_and_crash_recovery() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m5-s05-t02-{nanos}"));
    fs::create_dir_all(&site_root).unwrap();

    let srv = spawn_server_at(site_root.clone(), true).await;
    let client = Client::new();
    let (cookie, csrf) = login(&client, &srv.base_url).await;
    let png = tiny_png();

    // Install & activate A1
    let manifest_a1 = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "exec-theme",
        "name": "Exec Theme",
        "version": "1.0.0",
        "templates": { "home": "templates/home.html" },
        "options": [
            { "key": "brand_title", "label": "Brand", "type": "string", "default": "Default", "public": true }
        ],
        "sample": {
            "sample_key": "starter",
            "media": [
                { "object_key": "hero", "path": "assets/hero.png", "alt": "Hero" }
            ],
            "pages": [
                {
                    "object_key": "welcome_page",
                    "slug": "welcome",
                    "title": "Welcome",
                    "content_type": "page",
                    "values": {},
                    "media_refs": ["hero"]
                }
            ],
            "menus": [
                {
                    "object_key": "main_menu",
                    "menu_key": "main",
                    "title": "Main",
                    "slot_key": "primary",
                    "items": [
                        { "label": "Welcome", "target_kind": "entry", "target_object_key": "welcome_page" }
                    ]
                }
            ],
            "settings": [
                { "object_key": "brand_set", "option_key": "brand_title", "value": "Starter Brand" }
            ]
        }
    }))
    .unwrap();

    let zip_a1 = build_zip(&[
        ("theme.json", &manifest_a1),
        ("templates/home.html", b"<main>Home</main>"),
        ("assets/hero.png", &png),
    ]);
    let inst1 = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .body(zip_a1)
        .send()
        .await
        .unwrap();
    assert_eq!(inst1.status(), 201);
    let rev_a1 = inst1.json::<Value>().await.unwrap()["revision_id"]
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
            .json(&json!({ "revision_id": rev_a1 }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // 1. RBAC: Guest -> 401
    assert_eq!(
        client
            .post(format!("{}/api/themes/exec-theme/import/execute", srv.base_url))
            .json(&json!({
                "groups": ["pages"],
                "plan_digest": "00",
                "expected_versions": { "theme_selection_version": 2, "theme_options_version": 0, "menu_versions": {} }
            }))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    // 2. Dry-run all 4 groups, verify stale 412 + missing ack 422, then execute with run_id
    let plan: Value = client
        .post(format!("{}/api/themes/exec-theme/import/dry-run", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "groups": ["pages", "menus", "media", "settings"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Missing ack -> 422
    assert_eq!(
        client
            .post(format!("{}/api/themes/exec-theme/import/execute", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "groups": plan["selected_groups"],
                "revision_id": plan["revision_id"],
                "plan_digest": plan["plan_digest"],
                "expected_versions": plan["expected_versions"],
                "acknowledge_public_changes": false
            }))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );

    // Stale plan_digest -> 412
    assert_eq!(
        client
            .post(format!("{}/api/themes/exec-theme/import/execute", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "groups": plan["selected_groups"],
                "revision_id": plan["revision_id"],
                "plan_digest": "stale",
                "expected_versions": plan["expected_versions"],
                "acknowledge_public_changes": true
            }))
            .send()
            .await
            .unwrap()
            .status(),
        412
    );

    // Valid execute with fixed run_id
    let exec_res = client
        .post(format!("{}/api/themes/exec-theme/import/execute", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "groups": plan["selected_groups"],
            "revision_id": plan["revision_id"],
            "plan_digest": plan["plan_digest"],
            "expected_versions": plan["expected_versions"],
            "acknowledge_public_changes": true,
            "run_id": "run-fixed-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(exec_res.status(), 200);
    let outcome1: Value = exec_res.json().await.unwrap();
    assert_eq!(outcome1["created"].as_array().unwrap().len(), 4);

    // Idempotent retry with same run_id returns same outcome
    let retry_res = client
        .post(format!("{}/api/themes/exec-theme/import/execute", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "groups": plan["selected_groups"],
            "revision_id": plan["revision_id"],
            "plan_digest": plan["plan_digest"],
            "expected_versions": plan["expected_versions"],
            "acknowledge_public_changes": true,
            "run_id": "run-fixed-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry_res.status(), 200);
    assert_eq!(retry_res.json::<Value>().await.unwrap(), outcome1);

    // 3. Upgrade A1 -> A2: logical provenance skips duplicates (0 created, 4 skipped)
    let manifest_a2 = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "exec-theme",
        "name": "Exec Theme",
        "version": "2.0.0",
        "templates": { "home": "templates/home.html" },
        "options": [
            { "key": "brand_title", "label": "Brand", "type": "string", "default": "Default", "public": true }
        ],
        "sample": {
            "sample_key": "starter",
            "media": [
                { "object_key": "hero", "path": "assets/hero.png", "alt": "Hero" }
            ],
            "pages": [
                {
                    "object_key": "welcome_page",
                    "slug": "welcome",
                    "title": "Welcome v2",
                    "content_type": "page",
                    "values": {},
                    "media_refs": ["hero"]
                }
            ],
            "menus": [
                {
                    "object_key": "main_menu",
                    "menu_key": "main",
                    "title": "Main",
                    "slot_key": "primary",
                    "items": [
                        { "label": "Welcome", "target_kind": "entry", "target_object_key": "welcome_page" }
                    ]
                }
            ],
            "settings": [
                { "object_key": "brand_set", "option_key": "brand_title", "value": "Starter Brand v2" }
            ]
        }
    }))
    .unwrap();
    let zip_a2 = build_zip(&[
        ("theme.json", &manifest_a2),
        ("templates/home.html", b"<main>Home v2</main>"),
        ("assets/hero.png", &png),
    ]);
    let inst2 = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .body(zip_a2)
        .send()
        .await
        .unwrap();
    assert_eq!(inst2.status(), 201);
    let rev_a2 = inst2.json::<Value>().await.unwrap()["revision_id"]
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
            .json(&json!({ "revision_id": rev_a2 }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let plan_a2: Value = client
        .post(format!("{}/api/themes/exec-theme/import/dry-run", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "groups": ["pages", "menus", "media", "settings"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let exec_a2: Value = client
        .post(format!("{}/api/themes/exec-theme/import/execute", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "groups": plan_a2["selected_groups"],
            "revision_id": plan_a2["revision_id"],
            "plan_digest": plan_a2["plan_digest"],
            "expected_versions": plan_a2["expected_versions"],
            "acknowledge_public_changes": true
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(exec_a2["created"].as_array().unwrap().len(), 0);
    assert_eq!(exec_a2["skipped"].as_array().unwrap().len(), 4);

    // 4. Verify imported page is `draft` and media is private (404 publicly)
    let media_id = outcome1["created"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["resource_kind"] == "media")
        .unwrap()["target_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        client
            .get(format!("{}/content/page/welcome", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!("{}/media/{}/file", srv.base_url, media_id))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    // 5. Crash cutpoint tests (Phase 1 & Phase 2) + startup recovery preserving shared media
    drop(srv);
    let db_path = site_root.join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();

    // Simulate orphaned Phase 2 `files_verified` run sharing `media_id` (`already_existed: true`) + a non-shared file (`already_existed: false`),
    // then verify server startup recovery quarantines it, deletes only the non-shared file, and preserves `media_id`.
    let orphan_id = "deadbeefcafebabe00112233445566778899aabbccddeeff0011223344556677";
    let orphan_dir = site_root.join("uploads").join("media").join(&orphan_id[..2]);
    fs::create_dir_all(&orphan_dir).unwrap();
    let orphan_file = orphan_dir.join(format!("{orphan_id}.png"));
    fs::write(&orphan_file, b"orphan-bytes").unwrap();
    let staging_dir = site_root.join("uploads").join(".staging").join("crash-run-p2");
    fs::create_dir_all(&staging_dir).unwrap();

    let manifest_json = serde_json::to_string(&json!([
        {
            "object_key": "shared_hero",
            "media_id": media_id,
            "sha256": media_id,
            "filename": "hero.png",
            "mime_type": "image/png",
            "extension": "png",
            "byte_size": 100,
            "width": 4,
            "height": 4,
            "thumb_width": 4,
            "thumb_height": 4,
            "already_existed": true
        },
        {
            "object_key": "orphan_img",
            "media_id": orphan_id,
            "sha256": orphan_id,
            "filename": "orphan.png",
            "mime_type": "image/png",
            "extension": "png",
            "byte_size": 12,
            "width": 4,
            "height": 4,
            "thumb_width": 4,
            "thumb_height": 4,
            "already_existed": false
        }
    ]))
    .unwrap();

    sqlx::query(
        "INSERT INTO import_runs (run_id, theme_id, sample_key, revision_id, plan_digest, state, media_manifest_json)
         VALUES ('crash-run-p2', 'alpha', 'default', 'rev-dummy', '0000000000000000000000000000000000000000000000000000000000000000', 'files_verified', ?)",
    )
    .bind(&manifest_json)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    // Restart server -> startup hook runs recover_orphaned_import_runs
    let srv_d = spawn_server_at(site_root.clone(), false).await;
    drop(srv_d);

    let pool2 = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    let state: String =
        sqlx::query_scalar("SELECT state FROM import_runs WHERE run_id = 'crash-run-p2'")
            .fetch_one(&pool2)
            .await
            .unwrap();
    assert_eq!(state, "quarantined");
    assert!(!orphan_file.exists(), "non-shared orphan file should be deleted");
    assert!(!staging_dir.exists(), "staging dir should be deleted");

    // Pre-existing shared hero.png MUST still exist on disk
    let shared_path = site_root
        .join("uploads")
        .join("media")
        .join(&media_id[..2])
        .join(format!("{media_id}.png"));
    assert!(shared_path.exists());

    pool2.close().await;
    let _ = fs::remove_dir_all(&site_root);
}
