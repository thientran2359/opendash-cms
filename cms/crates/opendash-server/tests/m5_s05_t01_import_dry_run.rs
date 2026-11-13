use std::{
    fs,
    io::{Cursor, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

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

async fn login(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let res = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
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
async fn sample_import_dry_run_selects_groups_reports_deps_conflicts_and_stays_read_only() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m5-s05-t01-{nanos}"));
    fs::create_dir_all(&site_root).unwrap();

    let srv = spawn_server_at(site_root.clone(), true).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    // Create an Editor user to verify RBAC (403)
    let ed_res = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_res.status(), 201);
    let (ed_cookie, ed_csrf) = login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;

    // Install & activate theme with sample manifest
    let manifest = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "sample-theme",
        "name": "Sample Theme",
        "version": "1.0.0",
        "templates": { "home": "templates/home.html" },
        "options": [
            { "key": "brand_title", "label": "Brand", "type": "string", "default": "OpenDash", "public": true }
        ],
        "sample": {
            "sample_key": "starter",
            "media": [
                { "object_key": "hero", "path": "assets/hero.png", "alt": "Hero" },
                { "object_key": "logo", "path": "assets/logo.png", "alt": "Logo" }
            ],
            "pages": [
                {
                    "object_key": "home_page",
                    "slug": "welcome",
                    "title": "Welcome",
                    "content_type": "page",
                    "values": {},
                    "media_refs": ["hero"]
                },
                {
                    "object_key": "about_page",
                    "slug": "about-us",
                    "title": "About Us",
                    "content_type": "page",
                    "values": {}
                }
            ],
            "menus": [
                {
                    "object_key": "main_menu",
                    "menu_key": "main",
                    "title": "Main Menu",
                    "slot_key": "primary",
                    "items": [
                        { "label": "Welcome", "target_kind": "entry", "target_object_key": "home_page" }
                    ]
                }
            ],
            "settings": [
                {
                    "object_key": "brand_setting",
                    "option_key": "brand_title",
                    "value": "My Starter Site"
                }
            ]
        }
    }))
    .unwrap();
    let zip = build_zip(&[
        ("theme.json", &manifest),
        ("templates/home.html", b"<main>Home</main>"),
        ("assets/hero.png", b"\x89PNG\r\n\x1a\nHERO"),
        ("assets/logo.png", b"\x89PNG\r\n\x1a\nLOGO"),
    ]);
    let inst = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(zip)
        .send()
        .await
        .unwrap();
    assert_eq!(inst.status(), 201);
    let rev_id = inst.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    let act = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "revision_id": rev_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), 200);

    // 1. RBAC: Guest -> 401, Editor -> 403
    let guest_res = client
        .post(format!("{}/api/themes/sample-theme/import/dry-run", srv.base_url))
        .json(&json!({ "groups": ["pages"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(guest_res.status(), 401);

    let ed_try = client
        .post(format!("{}/api/themes/sample-theme/import/dry-run", srv.base_url))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({ "groups": ["pages"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_try.status(), 403);

    // 2. Selecting `pages` without `media` reports unselected dependency instead of pulling media
    let partial_plan: Value = client
        .post(format!("{}/api/themes/sample-theme/import/dry-run", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "groups": ["pages"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(partial_plan["unselected_dependencies"].as_array().unwrap().len(), 1);
    assert_eq!(partial_plan["unselected_dependencies"][0]["required_group"], "media");
    assert_eq!(partial_plan["unselected_dependencies"][0]["object_key"], "hero");
    let home_item = partial_plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["object_key"] == "home_page")
        .unwrap();
    assert_eq!(home_item["action"], "missing_dependency");

    // 3. Running dry-run twice with all groups produces identical `plan_digest`
    let plan1: Value = client
        .post(format!("{}/api/themes/sample-theme/import/dry-run", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "groups": ["pages", "menus", "media", "settings"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plan2: Value = client
        .post(format!("{}/api/themes/sample-theme/import/dry-run", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "groups": ["settings", "media", "menus", "pages"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(plan1["plan_digest"], plan2["plan_digest"]);
    assert_eq!(plan1["requires_public_ack"], true);

    // 4. Create site conflict on `about-us` + tombstone on `logo` + provenance on `brand_setting`
    let site_page = client
        .post(format!("{}/api/admin/content-types/page/entries", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "title": "Existing About", "slug": "about-us", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(site_page.status(), 201);

    drop(srv);
    let db_path = site_root.join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();

    // Verify dry-run wrote 0 rows to import_runs / import_provenance / media_assets / uploads
    let runs_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM import_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    let media_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_assets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs_count, 0);
    assert_eq!(media_count, 0);

    sqlx::query(
        "INSERT INTO import_tombstones (theme_id, sample_key, resource_kind, object_key)
         VALUES ('sample-theme', 'starter', 'media', 'logo')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO import_provenance (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
         VALUES ('sample-theme', 'starter', 'settings', 'brand_setting', 'rev-old', '0000000000000000000000000000000000000000000000000000000000000000', 'brand_title', 'hash1')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let srv2 = spawn_server_at(site_root.clone(), false).await;
    let (admin_cookie2, admin_csrf2) =
        login(&client, &srv2.base_url, "admin", "AdminPassword123!").await;
    let plan3: Value = client
        .post(format!("{}/api/themes/sample-theme/import/dry-run", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &admin_csrf2)
        .json(&json!({ "groups": ["pages", "menus", "media", "settings"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let items = plan3["items"].as_array().unwrap();
    let find_item = |kind: &str, key: &str| {
        items
            .iter()
            .find(|i| i["resource_kind"] == kind && i["object_key"] == key)
            .unwrap()
    };
    assert_eq!(find_item("pages", "about_page")["action"], "skip");
    assert_eq!(find_item("pages", "about_page")["reason"], "site_conflict");
    assert_eq!(find_item("media", "logo")["action"], "skip");
    assert_eq!(find_item("media", "logo")["reason"], "tombstone_exists");
    assert_eq!(find_item("settings", "brand_setting")["action"], "skip");
    assert_eq!(find_item("settings", "brand_setting")["reason"], "already_imported");
    assert_eq!(find_item("pages", "home_page")["action"], "create");

    drop(srv2);
    let _ = fs::remove_dir_all(&site_root);
}
