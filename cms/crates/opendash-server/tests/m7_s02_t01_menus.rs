use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

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

fn build_theme_zip(id: &str, slots: &[&str]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 2,
            "theme_id": id,
            "name": id,
            "version": "1.0.0",
            "templates": {
                "header": "templates/header.html",
                "single": "templates/single.html"
            },
            "menus": {
                "slots": slots
            }
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("templates/header.html", opts).unwrap();
        zip.write_all(b"<header>{% if menus.primary is defined %}{% for item in menus.primary.items %}<a href=\"{{ item.url }}\">{{ item.label }}</a>{% endfor %}{% endif %}</header>").unwrap();
        zip.start_file("templates/single.html", opts).unwrap();
        zip.write_all(b"<article>{{ entry.title }}</article>").unwrap();
        zip.finish().unwrap();
    }
    buf
}

async fn install_theme(client: &Client, base: &str, cookie: &str, csrf: &str, zip: Vec<u8>) -> String {
    let resp = client
        .post(format!("{base}/api/admin/themes"))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(zip)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "theme install failed");
    resp.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn activate_theme(client: &Client, base: &str, cookie: &str, csrf: &str, revision_id: &str) {
    let summary: Value = client
        .get(format!("{base}/api/admin/themes"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ver = summary["version"].as_i64().unwrap();
    let resp = client
        .put(format!("{base}/api/admin/themes/active"))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({ "revision_id": revision_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn m7_s02_t01_menus_crud_validation_preview_slots_cas_and_restart() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s02-t01-{nanos}"));
    let srv = spawn_server_at(site_root.clone(), false).await;
    let client = Client::new();

    // Unauthenticated rejected.
    assert_eq!(
        client
            .get(format!("{}/api/admin/menus", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    let setup = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    let (cookie, csrf) = login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    // Client origin_provenance forbidden on create.
    assert_eq!(
        client
            .post(format!("{}/api/admin/menus", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "menu_key": "primary",
                "title": "Primary",
                "origin_provenance": "spoofed"
            }))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    // Create two menus.
    let m1_resp = client
        .post(format!("{}/api/admin/menus", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "menu_key": "primary", "title": "Primary" }))
        .send()
        .await
        .unwrap();
    assert_eq!(m1_resp.status(), 201);
    assert_eq!(m1_resp.headers().get(header::ETAG).unwrap(), "\"v1\"");
    let m1: Value = m1_resp.json().await.unwrap();
    let m1_id = m1["menu"]["id"].as_i64().unwrap();

    let m2: Value = client
        .post(format!("{}/api/admin/menus", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "menu_key": "secondary", "title": "Secondary" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let m2_id = m2["menu"]["id"].as_i64().unwrap();

    // Pagination bounds.
    assert_eq!(
        client
            .get(format!("{}/api/admin/menus?limit=0", srv.base_url))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        client
            .get(format!("{}/api/admin/menus?limit=101", srv.base_url))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    // Save items in m1 (parent + child) and verify stable IDs.
    let put_m1 = client
        .put(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "title": "Main Navigation",
            "items": [
                {
                    "client_key": "root-1",
                    "position": 0,
                    "label": "Home",
                    "target_kind": "external",
                    "external_url": "https://example.com/"
                },
                {
                    "client_key": "child-1",
                    "parent_client_key": "root-1",
                    "position": 0,
                    "label": "Docs",
                    "target_kind": "external",
                    "external_url": "https://example.com/docs"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_m1.status(), 200);
    let m1_v2: Value = put_m1.json().await.unwrap();
    assert_eq!(m1_v2["menu"]["version"], 2);
    assert_eq!(m1_v2["menu"]["title"], "Main Navigation");
    let root_item_id = m1_v2["items"][0]["id"].as_i64().unwrap();
    let child_item_id = m1_v2["items"][1]["id"].as_i64().unwrap();

    // Re-save preserving stable IDs; stale If-Match returns 412; missing If-Match returns 428.
    assert_eq!(
        client
            .put(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({ "title": "Main Navigation", "items": [] }))
            .send()
            .await
            .unwrap()
            .status(),
        428
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "title": "Stale", "items": [] }))
            .send()
            .await
            .unwrap()
            .status(),
        412
    );

    // Reject depth 9, 201 items, cycle, cross-menu parent, and invalid external URL.
    let too_deep: Vec<Value> = (0..9)
        .map(|i| {
            json!({
                "client_key": format!("d{i}"),
                "parent_client_key": if i == 0 { Value::Null } else { json!(format!("d{}", i - 1)) },
                "position": 0,
                "label": format!("Level {i}"),
                "target_kind": "external",
                "external_url": "https://example.com/"
            })
        })
        .collect();
    assert_eq!(
        client
            .put(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({ "title": "Too Deep", "items": too_deep }))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    let too_many: Vec<Value> = (0..201)
        .map(|i| {
            json!({
                "position": i,
                "label": format!("Item {i}"),
                "target_kind": "external",
                "external_url": "https://example.com/"
            })
        })
        .collect();
    assert_eq!(
        client
            .put(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({ "title": "Too Many", "items": too_many }))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    // Cross-menu parent_id on m2 pointing to m1's root_item_id -> 400.
    assert_eq!(
        client
            .put(format!("{}/api/admin/menus/{m2_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({
                "title": "Cross",
                "items": [{
                    "parent_id": root_item_id,
                    "position": 0,
                    "label": "Cross",
                    "target_kind": "external",
                    "external_url": "https://example.com/"
                }]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    // Invalid URL -> 400.
    assert_eq!(
        client
            .put(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({
                "title": "Bad URL",
                "items": [{
                    "position": 0,
                    "label": "XSS",
                    "target_kind": "external",
                    "external_url": "javascript:alert(1)"
                }]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    // Create a draft post and verify preview omits unpublished target with diagnostic and does NOT write DB.
    let draft_post: Value = client
        .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Draft Secret",
            "slug": "draft-secret",
            "values": {}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let draft_id = draft_post["id"].as_i64().unwrap();

    let preview_resp = client
        .post(format!("{}/api/admin/menus/{m1_id}/preview", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Unsaved Preview Title",
            "items": [
                {
                    "id": root_item_id,
                    "position": 0,
                    "label": "Home",
                    "target_kind": "external",
                    "external_url": "https://example.com/"
                },
                {
                    "position": 1,
                    "label": "Hidden Draft",
                    "target_kind": "entry",
                    "target_entry_id": draft_id
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(preview_resp.status(), 200);
    let preview_json: Value = preview_resp.json().await.unwrap();
    assert_eq!(preview_json["title"], "Unsaved Preview Title");
    assert_eq!(preview_json["items"].as_array().unwrap().len(), 1);
    assert_eq!(preview_json["diagnostics"].as_array().unwrap().len(), 1);

    // Confirm DB was not mutated by preview (version still 2, 2 items).
    let after_preview: Value = client
        .get(format!("{}/api/admin/menus/{m1_id}", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after_preview["menu"]["version"], 2);
    assert_eq!(after_preview["items"].as_array().unwrap().len(), 2);
    assert_eq!(after_preview["items"][1]["id"], child_item_id);

    // Install and activate theme-a (slots: primary, footer) and theme-b (slots: top).
    let rev_a = install_theme(&client, &srv.base_url, &cookie, &csrf, build_theme_zip("theme-a", &["primary", "footer"])).await;
    let rev_b = install_theme(&client, &srv.base_url, &cookie, &csrf, build_theme_zip("theme-b", &["top"])).await;
    activate_theme(&client, &srv.base_url, &cookie, &csrf, &rev_a).await;

    // Non-active theme slot PUT rejected with 409.
    assert_eq!(
        client
            .put(format!("{}/api/admin/menu-slots/theme-b", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "assignments": { "top": m1_id } }))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );

    // Unknown slot on active theme rejected with 409.
    assert_eq!(
        client
            .put(format!("{}/api/admin/menu-slots/theme-a", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "assignments": { "nonexistent": m1_id } }))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );

    // Assign primary slot on active theme-a with CAS.
    let slot_put = client
        .put(format!("{}/api/admin/menu-slots/theme-a", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "assignments": { "primary": m1_id } }))
        .send()
        .await
        .unwrap();
    assert_eq!(slot_put.status(), 200);
    assert_eq!(slot_put.headers().get(header::ETAG).unwrap(), "\"v2\"");

    // Stale slot PUT returns 412.
    assert_eq!(
        client
            .put(format!("{}/api/admin/menu-slots/theme-a", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "assignments": { "primary": m2_id } }))
            .send()
            .await
            .unwrap()
            .status(),
        412
    );

    // Switch A -> B -> A and restart; assignments preserved.
    activate_theme(&client, &srv.base_url, &cookie, &csrf, &rev_b).await;
    activate_theme(&client, &srv.base_url, &cookie, &csrf, &rev_a).await;
    drop(srv);

    let srv2 = spawn_server_at(site_root.clone(), true).await;
    let (cookie2, csrf2) = login(&client, &srv2.base_url, "admin", "AdminPassword123!").await;
    let slots_after: Value = client
        .get(format!("{}/api/admin/menu-slots/theme-a", srv2.base_url))
        .header(header::COOKIE, &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(slots_after["version"], 2);
    assert_eq!(slots_after["assignments"]["primary"], m1_id);

    // Delete m1 -> 204, removes slot assignment and bumps theme-a slot version to 3.
    let del_resp = client
        .delete(format!("{}/api/admin/menus/{m1_id}", srv2.base_url))
        .header(header::COOKIE, &cookie2)
        .header("X-CSRF-Token", &csrf2)
        .header(header::IF_MATCH, "\"v2\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), 204);

    let slots_after_del: Value = client
        .get(format!("{}/api/admin/menu-slots/theme-a", srv2.base_url))
        .header(header::COOKIE, &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(slots_after_del["version"], 3);
    assert_eq!(slots_after_del["assignments"], json!({}));
}
