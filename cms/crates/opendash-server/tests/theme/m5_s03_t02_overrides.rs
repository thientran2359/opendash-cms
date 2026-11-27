use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::Client;
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
    for _ in 0..120 {
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

static NEXT_SITE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn spawn_server() -> TestServer {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let seq = NEXT_SITE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let site_root = std::env::temp_dir().join(format!(
        "opendash-m5-s03-t02-{}-{nanos}-{seq}",
        std::process::id()
    ));
    spawn_server_at(site_root, true).await
}

async fn login(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
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
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let json: Value = resp.json().await.unwrap();
    let csrf = json["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

fn build_theme_v3_zip(id: &str, header_html: &str, single_html: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = serde_json::json!({
            "manifest_version": 3,
            "theme_id": id,
            "name": id,
            "version": "1.0.0",
            "templates": [{ "kind": "single", "file": "single.html" }],
            "parts": { "header": "header.html", "post-card": "template-part/post-card.html" }
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("header.html", opts).unwrap();
        zip.write_all(header_html.as_bytes()).unwrap();
        // This valid entry-scoped part is never requested by the global
        // header. Eagerly evaluating it without `entry` must not break SSR.
        zip.start_file("template-part/post-card.html", opts)
            .unwrap();
        zip.write_all(b"<aside>{{ entry.title }}</aside>").unwrap();
        zip.start_file("single.html", opts).unwrap();
        zip.write_all(single_html.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn doc_with_heading(node_id: &str, text: &str, binding: Option<Value>) -> Value {
    let mut props = json!({ "level": 2, "text": text });
    if let Some(b) = binding {
        props["binding"] = b;
    }
    json!({
        "schema_version": 1,
        "sections": [{
            "id": format!("sec-{node_id}"),
            "children": [{
                "id": format!("row-{node_id}"),
                "children": [{
                    "id": format!("col-{node_id}"),
                    "children": [{
                        "id": node_id,
                        "type": "core/heading",
                        "props": props
                    }]
                }]
            }]
        }]
    })
}

#[tokio::test]
async fn template_overrides_preview_cas_route_context_and_restart() {
    let mut srv = spawn_server().await;
    let client = Client::new();
    let origin = srv.base_url.clone();

    // 1. Setup admin & login
    let setup = client
        .post(format!("{}/auth/setup", srv.base_url))
        .header("Origin", &origin)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        setup.status().is_success(),
        "setup failed: {}",
        setup.status()
    );
    let (cookie, csrf) = login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    // 2. Install & activate theme A and theme B
    let zip_a = build_theme_v3_zip(
        "theme-a",
        "<header>ThemeA Header</header>",
        "<article>{{ entry.title }}{{ od_content() }}</article>",
    );
    let zip_b = build_theme_v3_zip(
        "theme-b",
        "<header>ThemeB Header</header>",
        "<article>B:{{ entry.title }}{{ od_content() }}</article>",
    );
    let mut rev_ids = Vec::new();
    for zip_bytes in [zip_a, zip_b] {
        let res = client
            .post(format!("{}/api/admin/themes", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &origin)
            .header("X-CSRF-Token", &csrf)
            .header("Content-Type", "application/zip")
            .body(zip_bytes)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let j: Value = res.json().await.unwrap();
        rev_ids.push(j["revision_id"].as_str().unwrap().to_string());
    }
    let act_a = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "revision_id": rev_ids[0] }))
        .send()
        .await
        .unwrap();
    assert!(act_a.status().is_success());

    // 3. Create two published posts and one draft post
    for (title, slug, publish) in [
        ("First Post", "first-post", true),
        ("Second Post", "second-post", true),
        ("Secret Draft", "secret-draft", false),
    ] {
        let created = client
            .post(format!(
                "{}/api/admin/content-types/post/entries",
                srv.base_url
            ))
            .header("Cookie", &cookie)
            .header("Origin", &origin)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({ "title": title, "slug": slug, "values": {} }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), 201);
        let body: Value = created.json().await.unwrap();
        let id = body["id"].as_i64().unwrap();
        if publish {
            let pub_res = client
                .post(format!(
                    "{}/api/admin/content-types/post/entries/{id}/publication",
                    srv.base_url
                ))
                .header("Cookie", &cookie)
                .header("Origin", &origin)
                .header("X-CSRF-Token", &csrf)
                .header("If-Match", "\"v1\"")
                .json(&json!({ "status": "published" }))
                .send()
                .await
                .unwrap();
            assert_eq!(pub_res.status(), 200);
        }
    }

    // 4. The new typed API rejects malformed canonical targets; Home itself
    // is now a valid template identity, unlike the removed legacy endpoint.
    let header_preview_doc = doc_with_heading("hdr-1", "Unsaved Preview Header", None);
    let invalid_target = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": {"kind": "part", "part_key": "header", "entry_id": 1},
            "document": &header_preview_doc
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_target.status(), 400);

    // 5. Authentication and CSRF continue to protect the typed override API.
    let unauth_list = client
        .get(format!("{}/api/admin/template-overrides", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_list.status(), 401);
    let no_csrf_put = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .json(&json!({"target":{"kind":"part","part_key":"header"},
                      "document":&header_preview_doc}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf_put.status(), 403);

    // 6. D03 previews the submitted root without Save or a persisted override.
    // Legacy preview remains removed.
    let retired_preview = client
        .post(format!(
            "{}/api/admin/builder/overrides/header/preview",
            srv.base_url
        ))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&header_preview_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(retired_preview.status(), 404);
    let preview_url = format!("{}/api/admin/template-overrides/preview", srv.base_url);
    let preview_input = json!({
        "target":{"kind":"part","part_key":"header"},
        "document":&header_preview_doc
    });
    let anonymous = client
        .post(&preview_url)
        .json(&preview_input)
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    let preview_request = || {
        client
            .post(&preview_url)
            .header("Cookie", &cookie)
            .header("Origin", &origin)
    };
    let no_csrf = preview_request().json(&preview_input).send().await.unwrap();
    assert_eq!(no_csrf.status(), 403);
    let preview_request = || {
        client
            .post(&preview_url)
            .header("Cookie", &cookie)
            .header("Origin", &origin)
            .header("X-CSRF-Token", &csrf)
    };
    let wrong_type = preview_request()
        .body(preview_input.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_type.status(), 415);
    let invalid = preview_request()
        .json(&json!({
            "target":{"kind":"part","part_key":"header"}, "document":&header_preview_doc,
            "unexpected":true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    let oversized = preview_request()
        .header("Content-Type", "application/json")
        .body(" ".repeat(256 * 1024 + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), 400);
    let absent = preview_request()
        .header("If-Match", "\"v1\"")
        .json(&preview_input)
        .send()
        .await
        .unwrap();
    assert_eq!(absent.status(), 412);
    let preview = preview_request().json(&preview_input).send().await.unwrap();
    assert_eq!(preview.status(), 200);
    assert_eq!(preview.headers()["cache-control"], "no-store");
    assert_eq!(preview.headers()["x-content-type-options"], "nosniff");
    let preview_json: Value = preview.json().await.unwrap();
    assert!(
        preview_json["html"]
            .as_str()
            .unwrap()
            .contains("Unsaved Preview Header")
    );
    let saved_before: Value = client
        .get(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(saved_before["items"].as_array().unwrap().len(), 0);
    let public_post_1 = client
        .get(format!("{}/first-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!public_post_1.contains("Unsaved Preview Header"));
    assert!(public_post_1.contains("ThemeA Header"));

    // 7. Saving site-owned Header is a typed part target. CAS stays 428/412.
    let header_v1_doc = doc_with_heading("hdr-1", "Site Header v1", None);
    let header_target = json!({"kind":"part","part_key":"header"});
    let save_h1 = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"target":&header_target,"document":&header_v1_doc}))
        .send()
        .await
        .unwrap();
    assert_eq!(save_h1.status(), 201);
    let h1_json: Value = save_h1.json().await.unwrap();
    assert_eq!(h1_json["version"], 1);
    assert_eq!(h1_json["canonical_target_key"], "part:header");
    let header_override_id = h1_json["id"].as_i64().unwrap();

    let missing_cas = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"target":&header_target,
                      "document":doc_with_heading("hdr-1", "Site Header v2", None)}))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_cas.status(), 428);
    let stale_cas = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v99\"")
        .json(&json!({"target":&header_target,
                      "document":doc_with_heading("hdr-1", "Site Header v2", None)}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_cas.status(), 412);
    for (version, status) in [("\"v1\"", 200), ("\"v99\"", 412), ("invalid", 400)] {
        let result = preview_request()
            .header("If-Match", version)
            .json(&preview_input)
            .send()
            .await
            .unwrap();
        assert_eq!(result.status(), status);
        if status == 200 {
            let body: Value = result.json().await.unwrap();
            assert!(
                body["html"]
                    .as_str()
                    .unwrap()
                    .contains("Unsaved Preview Header")
            );
            assert!(!body["html"].as_str().unwrap().contains("Site Header v1"));
        }
    }
    let after: Value = client
        .get(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut saved_row = after["items"][0].clone();
    saved_row
        .as_object_mut()
        .unwrap()
        .remove("effective_source");
    assert_eq!(saved_row, h1_json);

    // 8. Typed Single(post) still resolves RouteContext title per published
    // entry. It must contain exactly one core/content_slot under S02.
    let mut single_doc = doc_with_heading(
        "sng-title",
        "Fallback Title",
        Some(json!({
            "type_key": "post",
            "entry_slug": "",
            "source": { "kind": "route_context", "field_key": "title" },
            "target": "text"
        })),
    );
    single_doc["sections"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"entry-slot","type":"core/content_slot","props":{}
        }));
    let preview_single = preview_request()
        .json(&json!({
            "target":{"kind":"single","content_type":"post","selector_slug":"secret-draft"},
            "document":&single_doc
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(preview_single.status(), 200);
    let draft_value: Value = preview_single.json().await.unwrap();
    assert!(
        draft_value["html"]
            .as_str()
            .unwrap()
            .contains("Secret Draft")
    );
    let no_entry = preview_request()
        .json(&json!({
            "target":{"kind":"single","content_type":"post"},"document":&single_doc
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_entry.status(), 200);
    let no_entry_value: Value = no_entry.json().await.unwrap();
    assert!(
        no_entry_value["html"]
            .as_str()
            .unwrap()
            .contains("data-template-preview-placeholder")
    );
    let missing = preview_request()
        .json(&json!({
            "target":{"kind":"single","content_type":"post","selector_slug":"missing"},
            "document":&single_doc
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    // Public Builder part resolution must honor site-owned parts even when
    // the active theme declares the same key, including nested Builder edges.
    for (key, document) in [
        (
            "post-card",
            doc_with_heading("db-card", "DB post card wins", None),
        ),
        ("public-card", preview_part_document(&["post-card"])),
    ] {
        let saved_part = client
            .put(format!("{}/api/admin/template-overrides", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &origin)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"target":{"kind":"part","part_key":key},"document":document}))
            .send()
            .await
            .unwrap();
        assert_eq!(saved_part.status(), 201);
    }
    single_doc["sections"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"id":"nested-public-card","type":"core/template_part",
                     "props":{"part_key":"public-card"}}),
        );
    let save_single = client
        .put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target":{"kind":"single","content_type":"post"},
            "document":&single_doc
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        save_single.status(),
        201,
        "{:?}",
        save_single.text().await.ok()
    );

    // Both published posts render their own dynamic title + Site Header v1; draft post stays 404
    let body1 = client
        .get(format!("{}/first-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let body2 = client
        .get(format!("{}/second-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let draft_res = client
        .get(format!("{}/secret-draft", srv.base_url))
        .send()
        .await
        .unwrap();
    assert!(body1.contains("Site Header v1"));
    assert!(body1.contains("First Post"));
    assert!(body1.contains("data-node-id=\"entry-slot\""));
    assert!(body1.contains("DB post card wins"));
    assert!(!body1.contains("<aside>First Post</aside>"));
    assert!(!body1.contains("data-doc-diagnostic"));
    assert_eq!(body1.matches("<!doctype html>").count(), 1);
    assert!(!body1.contains("Second Post"));
    assert!(body2.contains("Site Header v1"));
    assert!(body2.contains("Second Post"));
    assert!(body2.contains("data-node-id=\"entry-slot\""));
    assert!(body2.contains("DB post card wins"));
    assert!(!body2.contains("data-doc-diagnostic"));
    assert_eq!(body2.matches("<!doctype html>").count(), 1);
    assert!(!body2.contains("First Post"));
    assert_eq!(draft_res.status(), 404);

    // 9. Switch theme A -> B and restart server: overrides and RouteContext must persist!
    let act_b = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({ "revision_id": rev_ids[1] }))
        .send()
        .await
        .unwrap();
    assert!(act_b.status().is_success());

    let site_root = srv.site_root.clone();
    srv.cleanup_on_drop = false;
    drop(srv);
    let srv2 = spawn_server_at(site_root, true).await;
    let body1_after_restart = client
        .get(format!("{}/first-post", srv2.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body1_after_restart.contains("Site Header v1"));
    assert!(body1_after_restart.contains("First Post"));

    // 10. Reset requires a fresh version, deletes only the typed Header
    // override, and reveals the currently active Theme B part.
    let (cookie2, csrf2) = login(&client, &srv2.base_url, "admin", "AdminPassword123!").await;
    let reset_url = format!(
        "{}/api/admin/template-overrides/{header_override_id}/reset",
        srv2.base_url
    );
    let reset_without_match = client
        .post(&reset_url)
        .header("Cookie", &cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &csrf2)
        .send()
        .await
        .unwrap();
    assert_eq!(reset_without_match.status(), 428);
    let reset_stale = client
        .post(&reset_url)
        .header("Cookie", &cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &csrf2)
        .header("If-Match", "\"v99\"")
        .send()
        .await
        .unwrap();
    assert_eq!(reset_stale.status(), 412);
    let reset_ok = client
        .post(&reset_url)
        .header("Cookie", &cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &csrf2)
        .header("If-Match", "\"v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(reset_ok.status(), 200);
    let reset: Value = reset_ok.json().await.unwrap();
    assert_eq!(reset["canonical_target_key"], "part:header");
    assert_eq!(reset["next_effective_source"]["kind"], "active_theme");
    assert_eq!(reset["next_effective_source"]["target_key"], "part:header");

    let body1_after_reset = client
        .get(format!("{}/first-post", srv2.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!body1_after_reset.contains("Site Header v1"));
    assert!(body1_after_reset.contains("ThemeB Header"));
}

#[tokio::test]
async fn v3_page_canvas_and_custom_single_selections_survive_theme_switch() {
    let srv = spawn_server().await;
    let client = Client::new();
    let origin = &srv.base_url;
    let setup = client
        .post(format!("{origin}/auth/setup"))
        .header("Origin", origin)
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    let (cookie, csrf) = login(&client, origin, "admin", "AdminPassword123!").await;

    let mut bytes = Vec::new();
    {
        let mut writer = ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version":3,"theme_id":"layout-v3","name":"Layout V3","version":"1.0.0",
            "templates":[
                {"kind":"single","file":"single.html"},
                {"kind":"single_custom","custom_key":"feature","label":"Feature",
                    "allowed_types":["post"],"file":"feature.html"}
            ],
            "page_layouts":[{"key":"canvas","label":"Canvas","file":"page-layouts/canvas.html","chrome":"none"}],
            "parts":{"header":"header.html"}
        });
        for (path, contents) in [
            ("theme.json", manifest.to_string()),
            (
                "single.html",
                "<article>Ordinary {{ entry.title }}{{ od_content() }}</article>".into(),
            ),
            (
                "feature.html",
                "<article>Feature {{ entry.title }}{{ od_content() }}</article>".into(),
            ),
            (
                "page-layouts/canvas.html",
                "<main>Canvas {{ od_content() }}</main>".into(),
            ),
            ("header.html", "<header>HEADER_MARKER</header>".into()),
        ] {
            writer.start_file(path, options).unwrap();
            writer.write_all(contents.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }
    let installed = client
        .post(format!("{origin}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", origin)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(bytes)
        .send()
        .await
        .unwrap();
    let status = installed.status();
    let raw = installed.text().await.unwrap();
    assert_eq!(status, 201, "install v3: {raw}");
    let installed: Value = serde_json::from_str(&raw).unwrap();
    let revision = installed["revision_id"].as_str().unwrap();
    let activated = client
        .put(format!("{origin}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({"revision_id":revision}))
        .send()
        .await
        .unwrap();
    assert_eq!(activated.status(), 200);
    let choices: Value = client
        .get(format!("{origin}/api/admin/themes/active/layouts"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(choices["page_layouts"][0]["key"], "canvas");
    assert_eq!(choices["custom_templates"][0]["custom_key"], "feature");

    let make_entry = |kind: &'static str, slug: &'static str, selected: &'static str| {
        let base = origin.clone();
        let token = csrf.clone();
        let cookie = cookie.clone();
        let client = client.clone();
        async move {
            let config = client
                .get(format!(
                    "{base}/api/admin/content-types/{kind}/effective-editor-config"
                ))
                .header("Cookie", &cookie)
                .send()
                .await
                .unwrap();
            assert_eq!(config.status(), 200);
            let config: Value = config.json().await.unwrap();
            let mut body = json!({"title":"Fixture Entry","slug":slug,
                "values":{},"publication_status":"published",
                "expected_schema_token":config["schema_token"]});
            body[if kind == "page" {
                "page_layout_key"
            } else {
                "custom_template_key"
            }] = json!(selected);
            let res = client
                .post(format!(
                    "{base}/api/admin/content-types/{kind}/entries/composite"
                ))
                .header("Cookie", cookie)
                .header("Origin", base.as_str())
                .header("X-CSRF-Token", token)
                .json(&body)
                .send()
                .await
                .unwrap();
            let status = res.status();
            let text = res.text().await.unwrap();
            assert_eq!(status, 201, "create {kind}: {text}");
            serde_json::from_str::<Value>(&text).unwrap()
        }
    };
    let page = make_entry("page", "canvas-visit", "canvas").await;
    let post = make_entry("post", "feature-visit", "feature").await;
    assert_eq!(page["entry"]["page_layout_key"], "canvas");
    assert_eq!(post["entry"]["custom_template_key"], "feature");
    let page_public = client
        .get(format!("{origin}/canvas-visit"))
        .send()
        .await
        .unwrap();
    assert_eq!(page_public.status(), 200);
    let page_html = page_public.text().await.unwrap();
    assert!(page_html.contains("Canvas "), "{page_html}");
    assert!(
        !page_html.contains("HEADER_MARKER"),
        "canvas must omit site header"
    );
    let post_public = client
        .get(format!("{origin}/feature-visit"))
        .send()
        .await
        .unwrap();
    assert_eq!(post_public.status(), 200);
    let post_html = post_public.text().await.unwrap();
    assert!(post_html.contains("Feature Fixture Entry"), "{post_html}");
    assert!(post_html.contains("HEADER_MARKER"));

    let other = build_theme_v3_zip(
        "no-layout-v3",
        "<header>Other Header</header>",
        "<article>Other {{ entry.title }}{{ od_content() }}</article>",
    );
    let res = client
        .post(format!("{origin}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", origin)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(other)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let other_revision: Value = res.json().await.unwrap();
    let act = client
        .put(format!("{origin}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({"revision_id":other_revision["revision_id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), 200);
    // Previously saved keys remain durable after provider withdrawal; public
    // renders core/theme fallback instead of a broken or cross-type template.
    for (kind, slug, id, selection_key, selection) in [
        (
            "page",
            "canvas-visit",
            page["entry"]["id"].as_i64().unwrap(),
            "page_layout_key",
            "canvas",
        ),
        (
            "post",
            "feature-visit",
            post["entry"]["id"].as_i64().unwrap(),
            "custom_template_key",
            "feature",
        ),
    ] {
        let loaded = client
            .get(format!(
                "{origin}/api/admin/content-types/{kind}/entries/{id}"
            ))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(loaded.status(), 200);
        let record: Value = loaded.json().await.unwrap();
        assert_eq!(
            record[selection_key], selection,
            "saved key must survive switching theme"
        );
        let public_url = if matches!(kind, "post" | "page") {
            format!("{origin}/{slug}")
        } else {
            format!("{origin}/{kind}/{slug}")
        };
        let public = client.get(public_url).send().await.unwrap();
        assert_eq!(public.status(), 200);
    }
}

fn preview_part_document(keys: &[&str]) -> Value {
    let mut doc = doc_with_heading("preview", "Unsaved root", None);
    let children = doc["sections"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap();
    for (index, key) in keys.iter().enumerate() {
        children.push(
            json!({"id":format!("part-{index}"),"type":"core/template_part",
                             "props":{"part_key":key}}),
        );
    }
    doc
}

fn build_mixed_preview_theme_zip() -> Vec<u8> {
    let manifest = json!({
        "manifest_version":3, "theme_id":"mixed-preview", "name":"Mixed Preview",
        "version":"1.0.0",
        "templates":[{"kind":"single","file":"single.html"}],
        "parts":{
            "bridge":"template-part/bridge.html",
            "db-card":"template-part/db-card.html",
            "leaf":"template-part/leaf.html",
            "lexical":"template-part/lexical.html"
        }
    });
    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, source) in [
            ("theme.json", manifest.to_string()),
            ("single.html", "<main>{{ od_content() }}</main>".to_owned()),
            ("template-part/bridge.html", "{% set title = 'Lexical parent' %}<div>{{ od_part('lexical') }}{{ od_part('db-card') }}</div>".to_owned()),
            ("template-part/db-card.html", "<p>Theme card must lose</p>".to_owned()),
            ("template-part/leaf.html", "<b>Theme leaf</b>".to_owned()),
            ("template-part/lexical.html", "<i>{{ title }}</i>".to_owned()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(source.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buffer
}

#[tokio::test]
async fn typed_preview_mixed_graph_uses_db_precedence_and_rejects_draft_cycles() {
    let srv = spawn_server().await;
    let client = Client::new();
    let origin = &srv.base_url;
    let setup = client
        .post(format!("{origin}/auth/setup"))
        .header("Origin", origin)
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    let (cookie, csrf) = login(&client, origin, "admin", "AdminPassword123!").await;
    let auth = |request: reqwest::RequestBuilder| {
        request
            .header("Cookie", &cookie)
            .header("Origin", origin)
            .header("X-CSRF-Token", &csrf)
    };
    let install = auth(client.post(format!("{origin}/api/admin/themes")))
        .header("Content-Type", "application/zip")
        .body(build_mixed_preview_theme_zip())
        .send()
        .await
        .unwrap();
    assert_eq!(install.status(), 201);
    let revision: Value = install.json().await.unwrap();
    let activate = auth(client.put(format!("{origin}/api/admin/themes/active")))
        .header("If-Match", "\"v1\"")
        .json(&json!({"revision_id":revision["revision_id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(activate.status(), 200);
    let url = format!("{origin}/api/admin/template-overrides");
    let db_doc = preview_part_document(&["leaf"]);
    let saved = auth(client.put(&url))
        .json(&json!({
            "target":{"kind":"part","part_key":"db-card"},"document":&db_doc
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), 201);
    let parent = auth(client.put(&url))
        .json(&json!({
            "target":{"kind":"part","part_key":"parent"},
            "document":preview_part_document(&["bridge"])
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(parent.status(), 201);
    let before: Value = auth(client.get(&url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let preview = |key: &str, document: Value| {
        auth(client.post(format!("{url}/preview")))
            .json(&json!({"target":{"kind":"part","part_key":key},"document":document}))
    };
    let response = preview("root", preview_part_document(&["bridge"]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let value: Value = response.json().await.unwrap();
    let html = value["html"].as_str().unwrap();
    assert!(html.contains("Lexical parent"));
    assert!(html.contains("Theme leaf"));
    assert!(!html.contains("Theme card must lose"));
    assert_eq!(html.matches("Unsaved root").count(), 2);

    // The saved DB card is acyclic; the draft version shadows its identity.
    let cycle = preview("db-card", preview_part_document(&["bridge"]))
        .send()
        .await
        .unwrap();
    assert_eq!(cycle.status(), 400);
    let too_many = preview("root", preview_part_document(&["leaf"; 33]))
        .send()
        .await
        .unwrap();
    assert_eq!(too_many.status(), 400);
    let too_deep = preview("root", preview_part_document(&["parent"]))
        .send()
        .await
        .unwrap();
    assert_eq!(too_deep.status(), 400); // root -> parent -> bridge -> db-card -> leaf
    let missing = preview("root", preview_part_document(&["missing-part"]))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 200);
    let missing: Value = missing.json().await.unwrap();
    assert!(
        missing["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "template_part_unavailable")
    );
    let after: Value = auth(client.get(&url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(before, after);
}
