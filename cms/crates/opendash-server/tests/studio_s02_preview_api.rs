use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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
            "opendash-studio-s02-{}-{port}-{seq}-{nanos}",
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
    panic!("studio S02 test server not ready at {base}; log: {log}");
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

fn build_studio_theme_v3_zip() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 3,
            "theme_id": "theme-studio-s02",
            "name": "Theme Studio S02",
            "version": "1.0.0",
            "templates": [
                { "kind": "home", "file": "home.html" },
                { "kind": "page", "file": "page.html" }
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
                    "default": "Saved Banner",
                    "public": true
                },
                {
                    "key": "logo_ref",
                    "label": "Logo",
                    "type": "image_ref",
                    "default": null,
                    "public": true
                }
            ]
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("header.html", opts).unwrap();
        zip.write_all(
            b"<header data-theme-header=\"1\"><span>{{ site.title }}</span><em>{{ options.banner_text }}</em>{% if options.logo_ref %}{% set url = options.logo_ref %}<img src=\"{{ url }}\" alt=\"Logo\">{% endif %}</header>",
        )
        .unwrap();
        zip.start_file("footer.html", opts).unwrap();
        zip.write_all(b"<footer data-theme-footer=\"1\">Footer: {{ site.tagline }}</footer>")
            .unwrap();
        zip.start_file("home.html", opts).unwrap();
        zip.write_all(b"<article data-theme-home=\"1\">{{ od_content() }}</article>")
            .unwrap();
        zip.start_file("page.html", opts).unwrap();
        zip.write_all(b"<article data-theme-page=\"1\">{{ od_content() }}</article>")
            .unwrap();
        zip.finish().unwrap();
    }
    buf
}

#[tokio::test]
async fn studio_s02_rbac_transient_preview_and_annotation_scoping() {
    let site = start().await;
    let http = Client::new();

    // 1. Setup admin + editor
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin_cookie, admin_csrf) = login(&http, &site, "admin", "AdminPassword123!").await;

    let editor_create = http
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_create.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) = login(&http, &site, "editor1", "EditorPassword123!").await;

    // 2. Editor gets 403 on Studio Mode 1 endpoints; Admin gets 200 on targets
    let ed_targets = http
        .get(format!("{}/api/admin/studio/targets", site.base))
        .header(header::COOKIE, &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ed_targets.status(), StatusCode::FORBIDDEN);

    let ed_preview = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &editor_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({ "target": { "kind": "page" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_preview.status(), StatusCode::FORBIDDEN);

    // 3. Install & activate Studio S02 v3 theme
    let theme_res = http
        .post(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_studio_theme_v3_zip())
        .send()
        .await
        .unwrap();
    assert_eq!(theme_res.status(), StatusCode::CREATED);
    let revision_id = theme_res.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let act_res = http
        .put(format!("{}/api/admin/themes/active", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "revision_id": revision_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_res.status(), StatusCode::OK);

    // Check /api/admin/studio/targets metadata
    let targets_res = http
        .get(format!("{}/api/admin/studio/targets", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(targets_res.status(), StatusCode::OK);
    let targets_json: Value = targets_res.json().await.unwrap();
    assert_eq!(targets_json["active_theme_id"], "theme-studio-s02");
    let items = targets_json["items"].as_array().unwrap();
    let header_item = items
        .iter()
        .find(|i| i["canonical_target_key"] == "part:header")
        .unwrap();
    assert_eq!(header_item["forbids_content_slot"], true);
    assert_eq!(header_item["requires_content_slot"], false);
    assert_eq!(header_item["effective_source"]["kind"], "active_theme");
    let page_item = items
        .iter()
        .find(|i| i["canonical_target_key"] == "page")
        .unwrap();
    assert_eq!(page_item["requires_content_slot"], true);
    assert_eq!(page_item["forbids_content_slot"], false);
    assert!(
        items
            .iter()
            .any(|i| i["canonical_target_key"] == "single:type:post")
    );
    assert!(
        items
            .iter()
            .any(|i| i["canonical_target_key"] == "archive:type:post")
    );

    // 4. Upload unpublished PNG media -> verify 404 publicly
    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 120, 200, 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let media_res = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", "logo.png")
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(media_res.status(), StatusCode::CREATED);
    let media_id = media_res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        http.get(format!("{}/media/{media_id}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // Capture general settings before preview
    let general_before: Value = http
        .get(format!("{}/api/admin/settings/general", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let original_site_title = general_before["site_title"].as_str().unwrap().to_owned();

    // 5. Transient Studio Preview renders unsaved site_title, banner_text, and unpublished image_ref without DB write
    let studio_prev = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "target": { "kind": "page" },
            "document": null,
            "preview_annotations": false,
            "transient": {
                "general": {
                    "site_title": "Transient Studio Title",
                    "tagline": "Transient Tagline",
                    "timezone": "UTC"
                },
                "theme_options": {
                    "banner_text": "Unsaved Banner Note",
                    "logo_ref": media_id
                }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(studio_prev.status(), StatusCode::OK);
    let studio_prev_body: Value = studio_prev.json().await.unwrap();
    let studio_html = studio_prev_body["html"].as_str().unwrap();
    assert!(
        studio_html.contains("data-theme-header=\"1\""),
        "{studio_html}"
    );
    assert!(
        studio_html.contains("data-theme-footer=\"1\""),
        "{studio_html}"
    );
    assert!(
        studio_html.contains("Transient Studio Title"),
        "{studio_html}"
    );
    assert!(studio_html.contains("Transient Tagline"), "{studio_html}");
    assert!(studio_html.contains("Unsaved Banner Note"), "{studio_html}");
    let decoded_html = studio_html.replace("&#x2f;", "/").replace("&#x2F;", "/");
    assert!(
        decoded_html.contains(&format!("/api/admin/media/{media_id}/file")),
        "{studio_html}"
    );
    assert!(!studio_html.contains("data-od-editable"), "{studio_html}");

    // Verify DB remains untouched
    let general_after: Value = http
        .get(format!("{}/api/admin/settings/general", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(general_after["site_title"], original_site_title);
    assert_eq!(
        http.get(format!("{}/media/{media_id}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // Invalid transient input -> 422
    let bad_transient = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "target": { "kind": "page" },
            "transient": {
                "theme_options": { "nonexistent_option_key": "abc" }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_transient.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 6. Create classic text page + verify EntryBuilderOpen additive metadata (freeform wrap)
    let page_res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Studio S02 Page",
            "slug": "studio-s02-page",
            "values": { "body": "Classic Wrap Body" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(page_res.status(), StatusCode::CREATED);
    let page_id = page_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let open_res = http
        .get(format!(
            "{}/api/admin/entries/page/{page_id}/builder",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(open_res.status(), StatusCode::OK);
    let open_json: Value = open_res.json().await.unwrap();
    assert_eq!(open_json["publication_status"], "draft");
    assert_eq!(open_json["publication_policy"], "draft_publish");
    assert_eq!(open_json["had_freeform_wrap"], true);
    assert_eq!(open_json["had_recovery"], false);
    assert!(open_json["diagnostics"].is_array());

    // 7. Create a pattern and verify Mode 2 Editor preview isolates annotations to entry nodes only
    let pat_create = http
        .post(format!("{}/api/admin/patterns", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "title": "Studio Pattern", "slug": "studio-pattern" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pat_create.status(), StatusCode::CREATED);
    let pat_id = pat_create.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let pat_doc = json!({
        "schema_version": 1,
        "sections": [{
            "id": "pat-sec-1",
            "children": [{
                "id": "pat-row-1",
                "children": [{
                    "id": "pat-col-1",
                    "children": [{
                        "id": "pat-h1",
                        "type": "core/heading",
                        "props": { "level": 3, "text": "Inside Pattern Node" }
                    }]
                }]
            }]
        }]
    });
    let pat_save = http
        .put(format!(
            "{}/api/admin/entries/pattern/{pat_id}/builder",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&pat_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(pat_save.status(), StatusCode::OK);

    let entry_doc = json!({
        "schema_version": 1,
        "sections": [{
            "id": "entry-sec-1",
            "children": [{
                "id": "entry-row-1",
                "children": [{
                    "id": "entry-col-1",
                    "children": [
                        {
                            "id": "entry-h1",
                            "type": "core/heading",
                            "props": { "level": 2, "text": "Editable Entry Heading" }
                        },
                        {
                            "id": "entry-pat-ref",
                            "type": "core/pattern_ref",
                            "props": { "pattern_id": pat_id }
                        }
                    ]
                }]
            }]
        }]
    });
    let mode2_prev = http
        .post(format!(
            "{}/api/admin/entries/page/{page_id}/builder/preview",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&entry_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(mode2_prev.status(), StatusCode::OK);
    let mode2_html = mode2_prev.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    // Header, footer, and theme outer page shell are present & read-only
    assert!(mode2_html.contains("<header data-theme-header=\"1\">"));
    assert!(mode2_html.contains("<footer data-theme-footer=\"1\">"));
    assert!(mode2_html.contains("<article data-theme-page=\"1\">"));
    assert!(!mode2_html.contains("data-theme-header=\"1\" data-od-editable"));
    assert!(!mode2_html.contains("data-theme-page=\"1\" data-od-editable"));
    // Entry body nodes have data-od-editable="1"
    assert!(mode2_html.contains(
        "<section data-node-id=\"entry-sec-1\" data-od-editable=\"1\" data-od-kind=\"section\">"
    ));
    assert!(mode2_html.contains(
        "<div class=\"builder-element\" data-node-id=\"entry-h1\" data-od-editable=\"1\" data-od-kind=\"element\">"
    ));
    assert!(mode2_html.contains(
        "<div class=\"builder-element\" data-node-id=\"entry-pat-ref\" data-od-editable=\"1\" data-od-kind=\"element\">"
    ));
    // Inner pattern nodes are rendered without data-od-editable
    assert!(mode2_html.contains("<section data-node-id=\"pat-sec-1\">"));
    assert!(mode2_html.contains("<div class=\"builder-element\" data-node-id=\"pat-h1\">"));

    // 8. Save & publish page, verify Mode 1 annotated template preview isolates content_slot, and verify Public routes are clean
    let save_page = http
        .put(format!(
            "{}/api/admin/entries/page/{page_id}/builder",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&entry_doc)
        .send()
        .await
        .unwrap();
    assert_eq!(save_page.status(), StatusCode::OK);

    let mode1_annotated = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "target": { "kind": "page" },
            "preview_annotations": true,
            "preview_entry": { "type_key": "page", "entry_id": page_id },
            "document": {
                "schema_version": 1,
                "sections": [{
                    "id": "tpl-sec-1",
                    "children": [{
                        "id": "tpl-row-1",
                        "children": [{
                            "id": "tpl-col-1",
                            "children": [{
                                "id": "tpl-slot",
                                "type": "core/content_slot",
                                "props": {}
                            }]
                        }]
                    }]
                }]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mode1_annotated.status(), StatusCode::OK);
    let mode1_html = mode1_annotated.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(mode1_html.contains(
        "<div class=\"builder-element\" data-node-id=\"tpl-slot\" data-od-editable=\"1\" data-od-kind=\"element\">"
    ));
    // Inner entry nodes inside content_slot must NOT carry data-od-editable in Mode 1 template preview
    assert!(mode1_html.contains("<section data-node-id=\"entry-sec-1\">"));
    assert!(mode1_html.contains("<div class=\"builder-element\" data-node-id=\"entry-h1\">"));

    // Publish page and configure Reading Homepage
    let pub_page = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{page_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_page.status(), StatusCode::OK);

    let set_home = http
        .put(format!("{}/api/admin/settings/reading", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "homepage_page_id": page_id, "posts_page_id": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(set_home.status(), StatusCode::OK);

    for public_path in ["/", "/content/page/studio-s02-page"] {
        let res = http
            .get(format!("{}{public_path}", site.base))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "path {public_path}");
        let html = res.text().await.unwrap();
        assert!(html.contains("Editable Entry Heading"));
        assert!(!html.contains("data-od-editable"));
        assert!(!html.contains("data-od-kind"));
        assert!(!html.contains("<script"));
    }
}
