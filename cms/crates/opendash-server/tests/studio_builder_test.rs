use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use opendash_core::builder::BuilderDocument;
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Site {
    child: Child,
    root: PathBuf,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start() -> Site {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "opendash-studio-s04-{}-{port}-{seq}-{nanos}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    let stderr = fs::File::create(root.join("server.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let http = Client::new();
    for _ in 0..400 {
        if http
            .get(format!("{base}/api/health"))
            .send()
            .await
            .is_ok_and(|res| res.status() == StatusCode::OK)
        {
            return Site { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log = fs::read_to_string(root.join("server.log")).unwrap_or_default();
    panic!("studio S04 test server not ready at {base}; log: {log}");
}

async fn login(http: &Client, site: &Site, username: &str, password: &str) -> (String, String) {
    let res = http
        .post(format!("{}/auth/login", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = res.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn setup_admin(http: &Client, site: &Site) -> (String, String) {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(http, site, "admin", "AdminPassword123!").await
}

async fn schema_token(http: &Client, site: &Site, cookie: &str, type_key: &str) -> String {
    let cfg: Value = http
        .get(format!(
            "{}/api/admin/content-types/{type_key}/effective-editor-config",
            site.base
        ))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    cfg["schema_token"].as_str().unwrap().to_owned()
}

async fn upload_png(
    http: &Client,
    site: &Site,
    cookie: &str,
    csrf: &str,
    filename: &str,
    rgb: [u8; 3],
) -> String {
    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let res = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", filename)
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn build_theme_v3_zip() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 3,
            "theme_id": "studio-s04-theme",
            "name": "Studio S04 Theme",
            "version": "1.0.0",
            "templates": [
                { "kind": "home", "file": "home.html" },
                { "kind": "page", "file": "page.html" },
                { "kind": "single", "file": "single.html" }
            ],
            "parts": {
                "header": "header.html",
                "footer": "footer.html"
            },
            "options": [
                {
                    "key": "banner_text",
                    "label": "Banner",
                    "type": "string",
                    "default": "Safe Default Banner",
                    "public": true
                }
            ]
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("header.html", opts).unwrap();
        zip.write_all(
            b"<header data-theme-header=\"1\"><span>{{ site.title }}</span><em>{{ options.banner_text }}</em></header>",
        )
        .unwrap();
        zip.start_file("footer.html", opts).unwrap();
        zip.write_all(b"<footer data-theme-footer=\"1\">{{ site.tagline }}</footer>")
            .unwrap();
        zip.start_file("home.html", opts).unwrap();
        zip.write_all(b"<article data-theme-home=\"1\">{{ od_content() }}</article>")
            .unwrap();
        zip.start_file("page.html", opts).unwrap();
        zip.write_all(b"<article data-theme-page=\"1\">{{ od_content() }}</article>")
            .unwrap();
        zip.start_file("single.html", opts).unwrap();
        zip.write_all(b"<article data-theme-single=\"1\">{{ od_content() }}</article>")
            .unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn build_plugin_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

fn escape_wat_bytes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        out.push_str(&format!("\\{b:02x}"));
    }
    out
}

fn build_content_save_action_wasm(cmd_json: &str) -> Vec<u8> {
    assert!(cmd_json.len() < 4096);
    let out_json = r#"{"ok":true}"#;
    let wat = format!(
        r#"(module
          (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
          (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
          (memory (export "memory") 2)
          (data (i32.const 0) "{}")
          (data (i32.const 4096) "{}")
          (func (export "opendash_run") (result i32)
            (drop (call $call (i32.const 0) (i32.const {}) (i32.const 8192) (i32.const 4096)))
            (drop (call $out (i32.const 4096) (i32.const {})))
            (i32.const 0)
          )
        )"#,
        escape_wat_bytes(cmd_json),
        escape_wat_bytes(out_json),
        cmd_json.len(),
        out_json.len(),
    );
    wat::parse_str(&wat).expect("valid wat")
}

#[tokio::test]
async fn studio_s04_unified_body_roundtrip_and_cpt_builder() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = setup_admin(&http, &site).await;

    // 1. Create custom CPT "service" with ContentSupport::Builder enabled (no editor support).
    let cpt_res = http
        .post(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "key": "service",
            "label": "Dịch vụ",
            "fields": [],
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "draft_publish",
                "supports": ["builder"]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cpt_res.status(), StatusCode::CREATED);

    // 2. Create draft Page and draft Service with plain text via Classic Editor composite API.
    for (type_key, slug, title, text) in [
        ("page", "intro-page", "Giới thiệu", "Plain intro line 1"),
        (
            "service",
            "web-design",
            "Thiết kế Web",
            "Plain service line 1",
        ),
    ] {
        let st = schema_token(&http, &site, &cookie, type_key).await;
        let created = http
            .post(format!(
                "{}/api/admin/content-types/{type_key}/entries/composite",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "expected_schema_token": st,
                "title": title,
                "slug": slug,
                "publication_status": "draft",
                "values": { "body": text }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let created_json: Value = created.json().await.unwrap();
        let id = created_json["entry"]["id"].as_i64().unwrap();

        // 3. Open in UX Builder -> verifies freeform auto-wrap & policy metadata.
        let open_res = http
            .get(format!(
                "{}/api/admin/entries/{type_key}/{id}/builder",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(open_res.status(), StatusCode::OK);
        let open_json: Value = open_res.json().await.unwrap();
        assert_eq!(open_json["had_freeform_wrap"], true);
        assert_eq!(open_json["had_recovery"], false);
        assert_eq!(open_json["publication_policy"], "draft_publish");
        assert_eq!(open_json["publication_status"], "draft");
        assert_eq!(
            open_json["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["type"],
            "core/text"
        );
        let version = open_json["version"].as_i64().unwrap();

        // 4. Save structured AST in UX Builder.
        let doc = json!({
            "schema_version": 1,
            "sections": [{
                "id": "sec-1",
                "children": [{
                    "id": "row-1",
                    "children": [{
                        "id": "col-1",
                        "children": [
                            { "id": "hd-1", "type": "core/heading", "props": { "level": 2, "text": format!("{title} Heading") } },
                            { "id": "tx-1", "type": "core/text", "props": { "text": format!("{title} Body A") } },
                            { "id": "bt-1", "type": "core/button", "props": { "label": "Liên hệ", "url": "/lien-he" } }
                        ]
                    }]
                }]
            }]
        });
        let put_res = http
            .put(format!(
                "{}/api/admin/entries/{type_key}/{id}/builder",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, format!("\"v{version}\""))
            .json(&doc)
            .send()
            .await
            .unwrap();
        assert_eq!(put_res.status(), StatusCode::OK);
        let put_json: Value = put_res.json().await.unwrap();
        let v2 = put_json["version"].as_i64().unwrap();

        // 5. Verify unified values_json["body"] contains clean <!-- od:... --> comments & zero wrapper div.
        let rec: Value = http
            .get(format!(
                "{}/api/admin/content-types/{type_key}/entries/{id}",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let body = rec["values"]["body"].as_str().unwrap();
        assert!(body.contains("<!-- od:section"));
        assert!(body.contains("<!-- od:heading"));
        assert!(body.contains("<!-- od:text"));
        assert!(!body.contains("<div class=\"od-section\">"));

        // 6. Edit via Classic Editor & publish -> verify round-trip back to UX Builder + public route.
        let edited_body = body.replace(
            &format!("{title} Body A"),
            &format!("{title} Body Classic Edited"),
        );
        let classic_save = http
            .put(format!(
                "{}/api/admin/content-types/{type_key}/entries/{id}/composite",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "expected_version": v2,
                "expected_schema_token": st,
                "title": title,
                "slug": slug,
                "publication_status": "published",
                "values": { "body": edited_body }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(classic_save.status(), StatusCode::OK);

        // Verify UX Builder parses the Classic Editor update cleanly.
        let reopen: Value = http
            .get(format!(
                "{}/api/admin/entries/{type_key}/{id}/builder",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            reopen["document"]["sections"][0]["children"][0]["children"][0]["children"][1]["props"]
                ["text"],
            format!("{title} Body Classic Edited")
        );

        // Verify public page renders edited text cleanly with trusted-theme CSP & zero data-od-* attributes.
        let route_info: Value = http
            .get(format!(
                "{}/api/admin/content-types/{type_key}/entries/{id}/route",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let public_url = route_info["public_url"].as_str().unwrap();
        let pub_res = http
            .get(format!("{}{public_url}", site.base))
            .send()
            .await
            .unwrap();
        assert_eq!(pub_res.status(), StatusCode::OK);
        let csp = pub_res
            .headers()
            .get(header::CONTENT_SECURITY_POLICY)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(csp.contains("script-src 'self' 'unsafe-inline'"));
        let pub_html = pub_res.text().await.unwrap();
        assert!(pub_html.contains(&format!("{title} Body Classic Edited")));
        assert!(!pub_html.contains("data-od-editable"));
        assert!(!pub_html.contains("data-od-kind"));
        assert!(!pub_html.contains("<script"));
    }
}

#[tokio::test]
async fn studio_s04_plugin_writer_ref_sync_and_one_hop_pattern_acl() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = setup_admin(&http, &site).await;

    let plugin_img = upload_png(&http, &site, &cookie, &csrf, "plugin.png", [10, 90, 200]).await;
    let pat_img = upload_png(&http, &site, &cookie, &csrf, "pattern.png", [220, 60, 40]).await;

    // 1. Plugin content_save writing unified comment-block body on a draft page syncs media ACL pins.
    let block_doc: BuilderDocument = serde_json::from_value(json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-plug",
            "children": [{
                "id": "row-plug",
                "children": [{
                    "id": "col-plug",
                    "children": [{
                        "id": "img-plug",
                        "type": "core/image",
                        "props": { "media_id": plugin_img, "alt": "Plugin Image" }
                    }]
                }]
            }]
        }]
    }))
    .unwrap();
    let block_body =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&block_doc).unwrap();
    let manifest = json!({
        "id": "studio-s04-writer",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "min_host_version": "0.1.0",
        "name": "Studio S04 Writer",
        "permissions": {
            "content_write": ["page"]
        },
        "actions": [{
            "id": "save_block_page",
            "title": "Save Block Page",
            "required_role": "admin",
            "capabilities": ["content_write"]
        }]
    })
    .to_string();
    let cmd = json!({
        "op": "content_save",
        "type_key": "page",
        "slug": "plugin-s04-page",
        "title": "Plugin S04 Page",
        "publication_status": "draft",
        "values": { "body": block_body }
    })
    .to_string();
    let pzip = build_plugin_zip(&[
        ("manifest.json", manifest.as_bytes()),
        ("plugin.wasm", &build_content_save_action_wasm(&cmd)),
    ]);
    let inst = http
        .post(format!("{}/api/admin/plugins", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(pzip)
        .send()
        .await
        .unwrap();
    assert_eq!(inst.status(), StatusCode::CREATED);
    let run = http
        .post(format!(
            "{}/api/admin/plugins/studio-s04-writer/actions/save_block_page",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(run.status(), StatusCode::OK);

    // Media referenced by draft plugin page cannot be deleted and is not yet public.
    let del_plugin_img = http
        .delete(format!("{}/api/admin/media/{plugin_img}", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_plugin_img.status(), StatusCode::CONFLICT);
    let pub_plugin_draft = http
        .get(format!("{}/media/{plugin_img}/file", site.base))
        .send()
        .await
        .unwrap();
    assert_ne!(pub_plugin_draft.status(), StatusCode::OK);

    // 2. Pattern CPT + 1-hop PatternRef from draft -> published Page controls public media ACL.
    let pat_create = http
        .post(format!("{}/api/admin/patterns", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "title": "Hero Pattern S04", "slug": "hero-pattern-s04" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pat_create.status(), StatusCode::CREATED);
    let pat_json: Value = pat_create.json().await.unwrap();
    let pat_id = pat_json["id"].as_i64().unwrap();
    let pat_v = pat_json["version"].as_i64().unwrap();
    let pat_save = http
        .put(format!(
            "{}/api/admin/entries/pattern/{pat_id}/builder",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{pat_v}\""))
        .json(&json!({
            "schema_version": 1,
            "sections": [{
                "id": "psec-1",
                "children": [{
                    "id": "prow-1",
                    "children": [{
                        "id": "pcol-1",
                        "children": [{
                            "id": "pimg-1",
                            "type": "core/image",
                            "props": { "media_id": pat_img, "alt": "Pattern Image" }
                        }]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pat_save.status(), StatusCode::OK);

    let page_st = schema_token(&http, &site, &cookie, "page").await;
    let host_page = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": page_st,
            "title": "Pattern Host",
            "slug": "pattern-host",
            "publication_status": "draft",
            "values": { "body": "initial" }
        }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let host_id = host_page["entry"]["id"].as_i64().unwrap();
    let host_v = host_page["entry"]["version"].as_i64().unwrap();

    let host_save = http
        .put(format!(
            "{}/api/admin/entries/page/{host_id}/builder",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{host_v}\""))
        .json(&json!({
            "schema_version": 1,
            "sections": [{
                "id": "hsec-1",
                "children": [{
                    "id": "hrow-1",
                    "children": [{
                        "id": "hcol-1",
                        "children": [{
                            "id": "href-1",
                            "type": "core/pattern_ref",
                            "props": { "pattern_id": pat_id }
                        }]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(host_save.status(), StatusCode::OK);
    let host_v2 = host_save.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    // Still draft -> pat_img cannot be deleted (409), and is blocked on public /media/{id}/file.
    let del_pat_img = http
        .delete(format!("{}/api/admin/media/{pat_img}", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_pat_img.status(), StatusCode::CONFLICT);
    assert_ne!(
        http.get(format!("{}/media/{pat_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // Publish host page -> 1-hop Pattern ACL opens /media/{pat_img}/file to 200 OK.
    let pub_host = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{host_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{host_v2}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_host.status(), StatusCode::OK);
    assert_eq!(
        http.get(format!("{}/media/{pat_img}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let host_route: Value = http
        .get(format!(
            "{}/api/admin/content-types/page/entries/{host_id}/route",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let host_url = host_route["public_url"].as_str().unwrap();
    let host_res = http
        .get(format!("{}{host_url}", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(host_res.status(), StatusCode::OK);
    let host_html = host_res.text().await.unwrap();
    assert!(host_html.contains(&format!("/media/{pat_img}/file")));
    assert!(!host_html.contains("data-od-editable"));
}

#[tokio::test]
async fn studio_s04_rbac_xss_and_preview_csp_hardening() {
    let site = start().await;
    let http = Client::new();
    let (admin_cookie, admin_csrf) = setup_admin(&http, &site).await;

    // Install & activate Theme v3
    let inst = http
        .post(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_theme_v3_zip())
        .send()
        .await
        .unwrap();
    assert_eq!(inst.status(), StatusCode::CREATED);
    let rev_id = inst.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let tlist: Value = http
        .get(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let tv = tlist["version"].as_i64().unwrap();
    let act = http
        .put(format!("{}/api/admin/themes/active", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{tv}\""))
        .json(&json!({ "revision_id": rev_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), StatusCode::OK);

    // Create Editor
    let ed_create = http
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_create.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) = login(&http, &site, "editor", "EditorPassword123!").await;

    // Editor gets 403 on Mode 1 Studio endpoints.
    assert_eq!(
        http.get(format!("{}/api/admin/studio/targets", site.base))
            .header(header::COOKIE, &ed_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        http.post(format!("{}/api/admin/studio/preview", site.base))
            .header(header::COOKIE, &ed_cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &ed_csrf)
            .json(&json!({ "target": { "kind": "home" }, "transient": { "general": { "site_title": "Hack", "tagline": "x", "timezone": "UTC" } } }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // Editor creates Page, saves XSS payloads inside Heading & Text + verifies javascript: URL rejection.
    let xss_script = "<script>alert(1)</script>";
    let xss_img = "<img src=x onerror=alert(2)>";
    let xss_svg = "<svg onload=alert(3)>";

    let ed_page_st = schema_token(&http, &site, &ed_cookie, "page").await;
    let page_res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &ed_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({
            "expected_schema_token": ed_page_st,
            "title": "XSS Guard",
            "slug": "xss-guard",
            "publication_status": "published",
            "values": { "body": xss_svg }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(page_res.status(), StatusCode::CREATED);
    let page: Value = page_res.json().await.unwrap();
    let pid = page["entry"]["id"].as_i64().unwrap();
    let pv = page["entry"]["version"].as_i64().unwrap();

    // Verify javascript: button URL is rejected with 400.
    let bad_url = http
        .put(format!(
            "{}/api/admin/entries/page/{pid}/builder",
            site.base
        ))
        .header(header::COOKIE, &ed_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &ed_csrf)
        .header(header::IF_MATCH, format!("\"v{pv}\""))
        .json(&json!({
            "schema_version": 1,
            "sections": [{
                "id": "xsec-1",
                "children": [{
                    "id": "xrow-1",
                    "children": [{
                        "id": "xcol-1",
                        "children": [{
                            "id": "xbtn-1",
                            "type": "core/button",
                            "props": { "label": "Click", "url": "javascript:alert(1)" }
                        }]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_url.status(), StatusCode::BAD_REQUEST);

    // Save XSS payloads in safe Heading & Text nodes.
    let save_xss = http
        .put(format!("{}/api/admin/entries/page/{pid}/builder", site.base))
        .header(header::COOKIE, &ed_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &ed_csrf)
        .header(header::IF_MATCH, format!("\"v{pv}\""))
        .json(&json!({
            "schema_version": 1,
            "sections": [{
                "id": "xsec-2",
                "children": [{
                    "id": "xrow-2",
                    "children": [{
                        "id": "xcol-2",
                        "children": [
                            { "id": "xhd-1", "type": "core/heading", "props": { "level": 2, "text": xss_script } },
                            { "id": "xtx-1", "type": "core/text", "props": { "text": xss_img } }
                        ]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_xss.status(), StatusCode::OK);

    // Editor Mode 2 preview has Header/Footer read-only, Entry annotated, CSP headers set, and escaped XSS.
    let m2_prev = http
        .post(format!(
            "{}/api/admin/entries/page/{pid}/builder/preview",
            site.base
        ))
        .header(header::COOKIE, &ed_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({
            "schema_version": 1,
            "sections": [{
                "id": "xsec-2",
                "children": [{
                    "id": "xrow-2",
                    "children": [{
                        "id": "xcol-2",
                        "children": [
                            { "id": "xhd-1", "type": "core/heading", "props": { "level": 2, "text": xss_script } },
                            { "id": "xtx-1", "type": "core/text", "props": { "text": xss_svg } }
                        ]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(m2_prev.status(), StatusCode::OK);
    let m2_csp = m2_prev
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(m2_csp.contains("script-src 'self' 'unsafe-inline'"));
    let m2_html = m2_prev.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(m2_html.contains("data-theme-header=\"1\""));
    assert!(m2_html.contains("data-od-editable=\"1\""));
    assert!(!m2_html.contains("<script>alert(1)</script>"));
    assert!(!m2_html.contains("<svg onload=alert(3)>"));
    assert!(
        m2_html.contains("&lt;script&gt;alert(1)&lt;/script&gt;")
            || m2_html.contains("&lt;script&gt;")
    );

    // Admin Mode 1 transient studio preview escapes XSS in transient general/theme_options and carries strict headers.
    let m1_prev = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "target": { "kind": "home" },
            "transient": {
                "general": {
                    "site_title": xss_script,
                    "tagline": xss_svg,
                    "timezone": "UTC"
                },
                "theme_options": {
                    "banner_text": xss_img
                }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(m1_prev.status(), StatusCode::OK);
    assert_eq!(
        m1_prev
            .headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .unwrap(),
        "nosniff"
    );
    let m1_csp = m1_prev
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(m1_csp.contains("script-src 'self' 'unsafe-inline'"));
    let m1_html = m1_prev.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!m1_html.contains(xss_script));
    assert!(!m1_html.contains(xss_img));
    assert!(!m1_html.contains(xss_svg));

    // Public route stays clean (zero data-od-editable, escaped XSS, trusted-theme CSP).
    let page_route: Value = http
        .get(format!(
            "{}/api/admin/content-types/page/entries/{pid}/route",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let page_url = page_route["public_url"].as_str().unwrap();
    let pub_page = http
        .get(format!("{}{page_url}", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_page.status(), StatusCode::OK);
    let pub_html = pub_page.text().await.unwrap();
    assert!(!pub_html.contains("data-od-editable"));
    assert!(!pub_html.contains(xss_script));
    assert!(!pub_html.contains(xss_img));
}
