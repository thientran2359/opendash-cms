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

async fn login(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
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
async fn theme_switch_missing_provider_editor_crud_restart_and_a2_preservation() {
    let site_root = std::env::temp_dir().join(format!(
        "opendash_m5_s06_t01_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db_path = site_root.join("database").join("opendash.sqlite");
    let srv = spawn_server_at(site_root.clone(), true).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) =
        login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    // 1. Create Editor user
    let create_editor = client
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
    assert_eq!(create_editor.status(), 201);
    let (_editor_cookie, _editor_csrf) =
        login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;

    // 2. Install & activate Theme A1 (CPT `portfolio` + fields + options_schema + menu_slots)
    let manifest_a1 = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "theme-a",
        "name": "Theme Alpha",
        "version": "1.0.0",
        "templates": {
            "home": "templates/home.html",
            "single": "templates/single.html",
            "archive": "templates/archive.html"
        },
        "declarations": {
            "content_types": [
                {
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "short_text", "required": true },
                        { "key": "budget", "label": "Budget", "type": "number", "required": false }
                    ],
                    "taxonomies": ["category", "tag"]
                }
            ]
        },
        "options": [
            { "key": "accent_color", "label": "Accent", "type": "string", "default": "#112233", "public": true }
        ]
    }))
    .unwrap();
    let zip_a1 = build_zip(&[
        ("theme.json", &manifest_a1),
        ("templates/home.html", b"<main>Alpha Home</main>"),
        ("templates/single.html", b"<main>Alpha Single: {{ entry.title }}</main>"),
        ("templates/archive.html", b"<main>Alpha Archive</main>"),
    ]);
    let inst_a1 = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(zip_a1)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_a1.status(), 201);
    let rev_a1 = inst_a1.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "revision_id": rev_a1 }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // 3. Create taxonomy terms, portfolio entry #1, publish it, set Theme Options, save override
    let cat_resp = client
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "kind": "category", "name": "Design", "slug": "design" }))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_resp.status(), 201);
    let cat_id = cat_resp.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    let tag_resp = client
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "kind": "tag", "name": "Featured", "slug": "featured" }))
        .send()
        .await
        .unwrap();
    assert_eq!(tag_resp.status(), 201);
    let tag_id = tag_resp.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    let entry1_resp = client
        .post(format!("{}/api/admin/content-types/portfolio/entries", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "slug": "case-study-1",
            "title": "Case Study One",
            "values": { "client": "Acme Corp", "budget": 5000 },
            "term_ids": [cat_id, tag_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(entry1_resp.status(), 201);
    let entry1 = entry1_resp.json::<Value>().await.unwrap();
    let entry1_id = entry1["id"].as_i64().unwrap();

    assert_eq!(
        client
            .put(format!("{}/api/themes/theme-a/options", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "values": { "accent_color": "#ff0000" } }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    assert_eq!(
        client
            .put(format!("{}/api/admin/builder/overrides/header", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "schema_version": 1,
                "sections": [{
                    "id": "sec-hdr-1",
                    "children": [{
                        "id": "row-hdr-1",
                        "children": [{
                            "id": "col-hdr-1",
                            "children": [{
                                "id": "hdr-1",
                                "type": "core/heading",
                                "props": { "level": 2, "text": "Preserved Site Header" }
                            }]
                        }]
                    }]
                }]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // 4. Install & activate Theme B (empty v2 theme, NO `portfolio`, NO `options_schema`)
    let manifest_b = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "theme-b",
        "name": "Theme Beta",
        "version": "1.0.0",
        "templates": { "home": "templates/home.html" }
    }))
    .unwrap();
    let zip_b = build_zip(&[
        ("theme.json", &manifest_b),
        ("templates/home.html", b"<main>Beta Home</main>"),
    ]);
    let inst_b = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(zip_b)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_b.status(), 201);
    let rev_b = inst_b.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header("If-Match", "\"v2\"")
            .json(&json!({ "revision_id": rev_b }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // Delete Theme A's files on disk + insert a site menu + slot assignment for `theme-a` to verify menu preservation too
    drop(srv);
    let theme_a_disk = site_root.join("themes").join("theme-a");
    if theme_a_disk.exists() {
        fs::remove_dir_all(&theme_a_disk).unwrap();
    }
    {
        let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .unwrap();
        sqlx::query("INSERT INTO site_menus (menu_key, title, version, origin_provenance) VALUES ('main-nav', 'Main Nav', 1, 'user')")
            .execute(&pool)
            .await
            .unwrap();
        let menu_id: i64 = sqlx::query_scalar("SELECT id FROM site_menus WHERE menu_key = 'main-nav'")
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO site_menu_slot_assignments (theme_id, slot_key, menu_id) VALUES ('theme-a', 'header', ?)")
            .bind(menu_id)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    // 5. Restart server with Theme B active and Theme A missing on disk
    let srv2 = spawn_server_at(site_root.clone(), false).await;
    let (admin_cookie2, admin_csrf2) =
        login(&client, &srv2.base_url, "admin", "AdminPassword123!").await;
    let (editor_cookie2, editor_csrf2) =
        login(&client, &srv2.base_url, "editor1", "EditorPassword123!").await;

    // 5a. Public routes still serve published `portfolio` content via core fallback (not 500, not hidden)
    let pub_arch = client
        .get(format!("{}/content/portfolio", srv2.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_arch.status(), 200);
    let pub_arch_body = pub_arch.text().await.unwrap();
    assert!(pub_arch_body.contains("Case Study One"));

    let pub_single = client
        .get(format!("{}/content/portfolio/case-study-1", srv2.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_single.status(), 200);
    let pub_single_body = pub_single.text().await.unwrap();
    assert!(pub_single_body.contains("Case Study One"));

    // 5b. Editor sees `provider_inactive` status and CAN list, create, and update `portfolio` entries + terms!
    let cts: Value = client
        .get(format!("{}/api/admin/content-types", srv2.base_url))
        .header("Cookie", &editor_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let portfolio_ct = cts
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["descriptor"]["key"] == "portfolio")
        .unwrap();
    assert_eq!(portfolio_ct["status"], "provider_inactive");

    let editor_create = client
        .post(format!("{}/api/admin/content-types/portfolio/entries", srv2.base_url))
        .header("Cookie", &editor_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &editor_csrf2)
        .json(&json!({
            "slug": "case-study-2",
            "title": "Case Study Two",
            "values": { "client": "Beta LLC", "budget": 9000 },
            "term_ids": [cat_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_create.status(), 201);
    let entry2_id = editor_create.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    // Editor updates entry #1 while provider is inactive/missing
    let cur_entry1: Value = client
        .get(format!("{}/api/admin/content-types/portfolio/entries/{entry1_id}", srv2.base_url))
        .header("Cookie", &editor_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cur_ver = cur_entry1["version"].as_i64().unwrap();
    let editor_upd = client
        .put(format!("{}/api/admin/content-types/portfolio/entries/{entry1_id}", srv2.base_url))
        .header("Cookie", &editor_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &editor_csrf2)
        .header("If-Match", format!("\"v{cur_ver}\""))
        .json(&json!({
            "slug": "case-study-1",
            "title": "Case Study One Updated",
            "values": { "client": "Acme Global", "budget": 7500 },
            "term_ids": [cat_id, tag_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_upd.status(), 200);

    // 5c. Editor still receives 403 on Admin-only endpoints
    assert_eq!(
        client
            .get(format!("{}/api/themes/theme-a/options", srv2.base_url))
            .header("Cookie", &editor_cookie2)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv2.base_url))
            .header("Cookie", &editor_cookie2)
            .header("Origin", &srv2.base_url)
            .header("X-CSRF-Token", &editor_csrf2)
            .header("If-Match", "\"v3\"")
            .json(&json!({ "revision_id": rev_b }))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("{}/api/themes/theme-a/import/dry-run", srv2.base_url))
            .header("Cookie", &editor_cookie2)
            .header("Origin", &srv2.base_url)
            .header("X-CSRF-Token", &editor_csrf2)
            .json(&json!({ "groups": ["pages"] }))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    // 6. Install & activate Theme A2 (v2.0.0 with `portfolio` + extra optional field `year`)
    let manifest_a2 = serde_json::to_vec(&json!({
        "manifest_version": 2,
        "theme_id": "theme-a",
        "name": "Theme Alpha",
        "version": "2.0.0",
        "templates": {
            "home": "templates/home.html",
            "single": "templates/single.html",
            "archive": "templates/archive.html"
        },
        "declarations": {
            "content_types": [
                {
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "short_text", "required": true },
                        { "key": "budget", "label": "Budget", "type": "number", "required": false },
                        { "key": "year", "label": "Year", "type": "number", "required": false }
                    ],
                    "taxonomies": ["category", "tag"]
                }
            ]
        },
        "options": [
            { "key": "accent_color", "label": "Accent", "type": "string", "default": "#112233", "public": true }
        ]
    }))
    .unwrap();
    let zip_a2 = build_zip(&[
        ("theme.json", &manifest_a2),
        ("templates/home.html", b"<main>Alpha v2 Home</main>"),
        ("templates/single.html", b"<main>Alpha v2 Single: {{ entry.title }}</main>"),
        ("templates/archive.html", b"<main>Alpha v2 Archive</main>"),
    ]);
    let inst_a2 = client
        .post(format!("{}/api/admin/themes", srv2.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &admin_csrf2)
        .body(zip_a2)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_a2.status(), 201);
    let rev_a2 = inst_a2.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv2.base_url))
            .header("Cookie", &admin_cookie2)
            .header("Origin", &srv2.base_url)
            .header("X-CSRF-Token", &admin_csrf2)
            .header("If-Match", "\"v3\"")
            .json(&json!({ "revision_id": rev_a2 }))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // 7. Verify 100% preservation on A2: no duplicate CPT, same entry IDs, preserved values & options & override & menu
    let cts2: Value = client
        .get(format!("{}/api/admin/content-types", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let portfolio_matches: Vec<_> = cts2
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["descriptor"]["key"] == "portfolio")
        .collect();
    assert_eq!(portfolio_matches.len(), 1);
    assert_eq!(portfolio_matches[0]["status"], "active");

    let entries_list: Value = client
        .get(format!("{}/api/admin/content-types/portfolio/entries", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items = entries_list.as_array().unwrap();
    assert_eq!(items.len(), 2);
    let e1 = items.iter().find(|e| e["id"] == entry1_id).unwrap();
    assert_eq!(e1["title"], "Case Study One Updated");
    assert_eq!(e1["values"]["client"], "Acme Global");
    assert_eq!(e1["values"]["budget"], 7500);
    assert!(items.iter().any(|e| e["id"] == entry2_id));

    let opts: Value = client
        .get(format!("{}/api/themes/theme-a/options", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(opts["values"]["accent_color"], "#ff0000");

    let ovr: Value = client
        .get(format!("{}/api/admin/builder/overrides/header", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ovr["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"], "Preserved Site Header");

    drop(srv2);
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    let slot_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM site_menu_slot_assignments WHERE theme_id = 'theme-a' AND slot_key = 'header'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(slot_count, 1);
    pool.close().await;

    let _ = fs::remove_dir_all(&site_root);
}
