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
    for _ in 0..50 {
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

async fn spawn_server() -> TestServer {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!(
        "opendash-m5-s03-t02-{}-{nanos}",
        std::process::id()
    ));
    spawn_server_at(site_root, true).await
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
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let json: Value = resp.json().await.unwrap();
    let csrf = json["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

fn build_theme_v2_zip(id: &str, header_html: &str, single_html: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = serde_json::json!({
            "manifest_version": 2,
            "theme_id": id,
            "name": id,
            "version": "1.0.0",
            "templates": {
                "header": "templates/header.html",
                "single": "templates/single.html"
            }
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("templates/header.html", opts).unwrap();
        zip.write_all(header_html.as_bytes()).unwrap();
        zip.start_file("templates/single.html", opts).unwrap();
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
    assert!(setup.status().is_success(), "setup failed: {}", setup.status());
    let (cookie, csrf) = login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    // 2. Install & activate theme A and theme B
    let zip_a = build_theme_v2_zip("theme-a", "<header>ThemeA Header</header>", "<article>{{ entry.title }}</article>");
    let zip_b = build_theme_v2_zip("theme-b", "<header>ThemeB Header</header>", "<article>B:{{ entry.title }}</article>");
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
            .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
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
                .post(format!("{}/api/admin/content-types/post/entries/{id}/publication", srv.base_url))
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

    // 4. Reject `home` override (homepage remains authoritative)
    let home_rej = client
        .get(format!("{}/api/admin/builder/overrides/home", srv.base_url))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(home_rej.status(), 400);

    // 5. Unauthenticated preview must be 401; missing CSRF must be 403
    let header_preview_doc = doc_with_heading("hdr-1", "Unsaved Preview Header", None);
    let unauth_prev = client
        .post(format!("{}/api/admin/builder/overrides/header/preview", srv.base_url))
        .header("Origin", &origin)
        .json(&header_preview_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_prev.status(), 401);

    let no_csrf_prev = client
        .post(format!("{}/api/admin/builder/overrides/header/preview", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .json(&header_preview_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf_prev.status(), 403);

    // 6. Preview without Save renders unsaved header but does NOT leak to public
    let prev_res = client
        .post(format!("{}/api/admin/builder/overrides/header/preview", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&header_preview_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(prev_res.status(), 200);
    assert_eq!(prev_res.headers().get("cache-control").unwrap(), "no-store");
    let prev_json: Value = prev_res.json().await.unwrap();
    assert!(prev_json["html"].as_str().unwrap().contains("Unsaved Preview Header"));

    let public_post_1 = client
        .get(format!("{}/content/post/first-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!public_post_1.contains("Unsaved Preview Header"));
    assert!(public_post_1.contains("ThemeA Header"));

    // 7. Save header override -> CAS checks (428 / 412)
    let header_v1_doc = doc_with_heading("hdr-1", "Site Header v1", None);
    let save_h1 = client
        .put(format!("{}/api/admin/builder/overrides/header", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&header_v1_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(save_h1.status(), 200);
    let h1_json: Value = save_h1.json().await.unwrap();
    assert_eq!(h1_json["version"], 1);

    // Missing If-Match on existing override -> 428
    let missing_cas = client
        .put(format!("{}/api/admin/builder/overrides/header", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&doc_with_heading("hdr-1", "Site Header v2", None))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_cas.status(), 428);

    // Stale If-Match -> 412
    let stale_cas = client
        .put(format!("{}/api/admin/builder/overrides/header", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v99\"")
        .json(&doc_with_heading("hdr-1", "Site Header v2", None))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_cas.status(), 412);

    // 8. Save single override with RouteContext binding (dynamic title across entries)
    let single_doc = doc_with_heading(
        "sng-title",
        "Fallback Title",
        Some(json!({
            "type_key": "post",
            "entry_slug": "",
            "source": { "kind": "route_context", "field_key": "title" },
            "target": "text"
        })),
    );
    let save_single = client
        .put(format!("{}/api/admin/builder/overrides/single?type_key=post", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&single_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(save_single.status(), 200);

    // Both published posts render their own dynamic title + Site Header v1; draft post stays 404
    let body1 = client
        .get(format!("{}/content/post/first-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let body2 = client
        .get(format!("{}/content/post/second-post", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let draft_res = client
        .get(format!("{}/content/post/secret-draft", srv.base_url))
        .send()
        .await
        .unwrap();
    assert!(body1.contains("Site Header v1"));
    assert!(body1.contains("First Post"));
    assert!(!body1.contains("Second Post"));
    assert!(body2.contains("Site Header v1"));
    assert!(body2.contains("Second Post"));
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
        .get(format!("{}/content/post/first-post", srv2.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body1_after_restart.contains("Site Header v1"));
    assert!(body1_after_restart.contains("First Post"));

    // 10. Re-login on restarted server and test Reset confirmation + CAS
    let (cookie2, csrf2) = login(&client, &srv2.base_url, "admin", "AdminPassword123!").await;

    // Reset without confirm=true -> 400
    let reset_no_confirm = client
        .post(format!("{}/api/admin/builder/overrides/header/reset", srv2.base_url))
        .header("Cookie", &cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &csrf2)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "confirm": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(reset_no_confirm.status(), 400);

    // Reset with confirm=true and valid CAS -> 200, falls back to ThemeB Header
    let reset_ok = client
        .post(format!("{}/api/admin/builder/overrides/header/reset", srv2.base_url))
        .header("Cookie", &cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &csrf2)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "confirm": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(reset_ok.status(), 200);

    let body1_after_reset = client
        .get(format!("{}/content/post/first-post", srv2.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!body1_after_reset.contains("Site Header v1"));
    assert!(body1_after_reset.contains("ThemeB Header"));
}
