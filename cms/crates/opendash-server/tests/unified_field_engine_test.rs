use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use opendash_core::{
    builder::BuilderDocument, builder_blocks::serialize_builder_document_to_blocks,
};
use reqwest::{Client, Response, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
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
            "opendash-ufe-s03-{}-{port}-{seq}-{nanos}",
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
    panic!("unified_field_engine_test server not ready at {base}; log: {log}");
}

async fn setup_and_login_admin(http: &Client, site: &Site) -> (String, String) {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);

    let res = http
        .post(format!("{}/auth/login", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
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

async fn admin_post(
    http: &Client,
    site: &Site,
    cookie: &str,
    csrf: &str,
    path: &str,
    body: &Value,
) -> Response {
    http.post(format!("{}{path}", site.base))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", csrf)
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn admin_put(
    http: &Client,
    site: &Site,
    cookie: &str,
    csrf: &str,
    path: &str,
    if_match: Option<&str>,
    body: &Value,
) -> Response {
    let mut req = http
        .put(format!("{}{path}", site.base))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", csrf);
    if let Some(etag) = if_match {
        req = req.header(header::IF_MATCH, etag);
    }
    req.json(body).send().await.unwrap()
}

async fn upload_png(http: &Client, site: &Site, cookie: &str, csrf: &str, name: &str) -> String {
    let mut png = Vec::new();
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([30, 144, 255, 255]));
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let res = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", name)
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

fn block_doc(el_id: &str, block_id: &str, values: Value) -> Value {
    json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1",
            "children": [{
                "id": "row-1",
                "children": [{
                    "id": "col-1",
                    "children": [{
                        "id": el_id,
                        "type": "plugin/block",
                        "props": {
                            "block_id": block_id,
                            "values": values
                        }
                    }]
                }]
            }]
        }]
    })
}

async fn db_pool(site: &Site) -> SqlitePool {
    SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(site.root.join("database/opendash.sqlite")),
    )
    .await
    .unwrap()
}

fn build_s03_test_theme_zip() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 3,
            "theme_id": "theme-s03-test",
            "name": "Theme S03 Test",
            "version": "1.0.0",
            "menus": ["main_slot"],
            "parts": {
                "header": "header.html",
                "footer": "footer.html",
                "part-a": "template-part/part-a.html",
                "part-b": "template-part/part-b.html"
            },
            "templates": [
                { "kind": "home", "file": "home.html" },
                { "kind": "page", "file": "page.html" },
                { "kind": "single", "file": "single.html" }
            ]
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("header.html", opts).unwrap();
        zip.write_all(b"<header>{{ site.title }}</header>").unwrap();
        zip.start_file("footer.html", opts).unwrap();
        zip.write_all(b"<footer>{{ site.tagline }}</footer>")
            .unwrap();
        zip.start_file("template-part/part-a.html", opts).unwrap();
        zip.write_all(b"<div>Part A</div>").unwrap();
        zip.start_file("template-part/part-b.html", opts).unwrap();
        zip.write_all(b"<div>Part B</div>").unwrap();
        zip.start_file("home.html", opts).unwrap();
        zip.write_all(b"<main>{{ od_content() }}</main>").unwrap();
        zip.start_file("page.html", opts).unwrap();
        zip.write_all(b"<main>{{ od_content() }}</main>").unwrap();
        zip.start_file("single.html", opts).unwrap();
        zip.write_all(b"<main>{{ od_content() }}</main>").unwrap();

        let all_sources_schema = json!({
            "schema_version": 1,
            "id": "theme-s03-test/all-sources",
            "title": "All Dynamic Sources",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "select", "key": "sel_menu", "label": "Menu", "options_source": "menus", "allow_null": true },
                { "type": "select", "key": "sel_slot", "label": "Slot", "options_source": "menu_slots", "allow_null": true },
                { "type": "select", "key": "sel_form", "label": "Form", "options_source": "contact_forms", "allow_null": true },
                { "type": "select", "key": "sel_ct", "label": "CT", "options_source": "content_types", "allow_null": true },
                { "type": "select", "key": "sel_tax", "label": "Tax", "options_source": "taxonomies", "allow_null": true },
                { "type": "select", "key": "sel_ctx", "label": "Ctx", "options_source": "context_fields", "allow_null": true },
                { "type": "select", "key": "sel_part", "label": "Part", "options_source": "template_parts", "allow_null": true },
                { "type": "select", "key": "sel_pat", "label": "Pat", "options_source": "patterns", "allow_null": true },
                { "type": "entry_ref", "key": "any_entry", "label": "Any Entry", "target_types": ["*"] },
                { "type": "select", "key": "dyn_tax", "label": "Dyn Tax", "options_source": "taxonomies", "allow_null": true },
                { "type": "term_ref", "key": "dyn_terms", "label": "Dyn Terms", "taxonomy_from_field": "dyn_tax", "multiple": true },
                { "type": "media", "key": "file_doc", "label": "File Doc", "media_kind": "file" }
            ]
        });
        zip.start_file("blocks/all-sources/block.json", opts)
            .unwrap();
        zip.write_all(all_sources_schema.to_string().as_bytes())
            .unwrap();
        zip.start_file("blocks/all-sources/render.html", opts)
            .unwrap();
        zip.write_all(
            br#"<div class="all-sources">{% if any_entry %}<a href="{{ any_entry.url }}">{{ any_entry.title }}</a>{% endif %}{% if dyn_terms %}{% for term in dyn_terms %}<a href="{{ term.url }}">{{ term.name }}</a>{% endfor %}{% endif %}{% if file_doc and file_doc.url %}<img src="{{ file_doc.url }}" alt="{{ file_doc.alt }}" />{% endif %}</div>"#,
        )
        .unwrap();

        let part_caller_schema = json!({
            "schema_version": 1,
            "id": "theme-s03-test/part-caller",
            "title": "Part Caller",
            "category": "layout",
            "render": "render.html",
            "fields": [
                { "type": "select", "key": "part_key", "label": "Part Key", "options_source": "template_parts" }
            ]
        });
        zip.start_file("blocks/part-caller/block.json", opts)
            .unwrap();
        zip.write_all(part_caller_schema.to_string().as_bytes())
            .unwrap();
        zip.start_file("blocks/part-caller/render.html", opts)
            .unwrap();
        zip.write_all(br#"<div data-part-caller="1">{{ part_key }}</div>"#)
            .unwrap();

        let pattern_picker_schema = json!({
            "schema_version": 1,
            "id": "theme-s03-test/pattern-picker",
            "title": "Pattern Picker",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "select", "key": "pat", "label": "Patterns", "options_source": "patterns", "multiple": true }
            ]
        });
        zip.start_file("blocks/pattern-picker/block.json", opts)
            .unwrap();
        zip.write_all(pattern_picker_schema.to_string().as_bytes())
            .unwrap();
        zip.start_file("blocks/pattern-picker/render.html", opts)
            .unwrap();
        zip.write_all(br#"<div data-pat-picker="1">pattern-picker</div>"#)
            .unwrap();

        let form_ref_schema = json!({
            "schema_version": 1,
            "id": "theme-s03-test/form-ref",
            "title": "Form Ref",
            "category": "marketing",
            "render": "render.html",
            "fields": [
                { "type": "select", "key": "fid", "label": "Form", "options_source": "contact_forms" }
            ]
        });
        zip.start_file("blocks/form-ref/block.json", opts).unwrap();
        zip.write_all(form_ref_schema.to_string().as_bytes())
            .unwrap();
        zip.start_file("blocks/form-ref/render.html", opts).unwrap();
        zip.write_all(br#"<div data-form-ref="{{ fid }}">form</div>"#)
            .unwrap();

        zip.finish().unwrap();
    }
    buf
}

#[tokio::test]
async fn s03_showcase_e2e_and_scoped_margin_css_public_render() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = setup_and_login_admin(&http, &site).await;

    let img_cover = upload_png(&http, &site, &cookie, &csrf, "cover.png").await;
    let img_gallery = upload_png(&http, &site, &cookie, &csrf, "gallery.png").await;

    let menu_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/menus",
        &json!({ "menu_key": "promo-menu", "title": "Promo Menu" }),
    )
    .await;
    assert_eq!(menu_res.status(), StatusCode::CREATED);
    let menu_id = menu_res.json::<Value>().await.unwrap()["menu"]["id"]
        .as_i64()
        .unwrap();

    let upd_menu = admin_put(
        &http,
        &site,
        &cookie,
        &csrf,
        &format!("/api/admin/menus/{menu_id}"),
        Some("\"v1\""),
        &json!({
            "title": "Promo Menu",
            "items": [{
                "id": null,
                "client_key": "m1",
                "parent_id": null,
                "parent_client_key": null,
                "position": 0,
                "label": "Khuyen Mai",
                "target_kind": "external",
                "target_entry_id": null,
                "target_term_id": null,
                "external_url": "https://example.com/menu-link"
            }]
        }),
    )
    .await;
    assert_eq!(upd_menu.status(), StatusCode::OK);

    let post_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/content-types/post/entries",
        &json!({ "title": "Tin Noi Bat S03", "slug": "tin-noi-bat", "values": {} }),
    )
    .await;
    assert_eq!(post_res.status(), StatusCode::CREATED);
    let post_id = post_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        http.post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );

    let term_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/taxonomy/terms",
        &json!({
            "kind": "category",
            "name": "Cam nang S03",
            "slug": "cam-nang-s03",
            "parent_id": null
        }),
    )
    .await;
    assert_eq!(term_res.status(), StatusCode::CREATED);
    let term_id = term_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let page_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/content-types/page/entries",
        &json!({ "title": "Showcase V1 Page", "slug": "showcase-v1", "values": {} }),
    )
    .await;
    assert_eq!(page_res.status(), StatusCode::CREATED);
    let page_id = page_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let mut showcase_vals = json!({
        "heading": "Showcase V1 Title",
        "summary": "Tom tat S03",
        "rich_body": "<p>Noi dung <strong>rich</strong></p>",
        "hero_card": {
            "cover_image": img_cover,
            "primary_link": {
                "url": "https://example.com/cta",
                "label": "Nut CTA V1",
                "new_tab": true,
                "nofollow": false
            },
            "badge_icon": "sparkles"
        },
        "photos": [img_gallery],
        "contact_email": "hi@example.com",
        "launch_date": "2026-11-27",
        "linked_menu": "promo-menu",
        "taxonomy_key": "category",
        "featured_terms": [term_id],
        "featured_entry": post_id,
        "highlights": [{
            "title": "Diem 1",
            "detail": "Chi tiet 1",
            "meta": { "item_icon": "check-circle" }
        }],
        "text_align": { "desktop": "center" },
        "theme_variant": "dark",
        "accent_color": "#16a34a",
        "badge_count": 42,
        "radius_px": { "desktop": 20 },
        "box_padding": { "desktop": { "top": 16, "right": 16, "bottom": 16, "left": 16 } },
        "outer_margin": {
            "desktop": { "top": -16, "bottom": -16 },
            "mobile": { "top": -10 }
        },
        "show_cta": false,
        "reference_url": "https://example.com/hidden-ref",
        "features": ["fast", "accessible"]
    });

    // Save with show_cta = false; hidden reference_url must be preserved cleanly.
    let save1 = admin_put(
        &http,
        &site,
        &cookie,
        &csrf,
        &format!("/api/admin/entries/page/{page_id}/builder"),
        Some("\"v1\""),
        &block_doc("el-showcase", "core/showcase", showcase_vals.clone()),
    )
    .await;
    assert_eq!(save1.status(), StatusCode::OK);

    let fetched: Value = http
        .get(format!(
            "{}/api/admin/entries/page/{page_id}/builder",
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
        fetched["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["values"]
            ["reference_url"],
        "https://example.com/hidden-ref"
    );

    // Re-enable show_cta = true and publish the page.
    showcase_vals["show_cta"] = json!(true);
    let save2 = admin_put(
        &http,
        &site,
        &cookie,
        &csrf,
        &format!("/api/admin/entries/page/{page_id}/builder"),
        Some("\"v2\""),
        &block_doc("el-showcase", "core/showcase", showcase_vals),
    )
    .await;
    assert_eq!(save2.status(), StatusCode::OK);

    let pub_res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{page_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_res.status(), StatusCode::OK);

    let live = http
        .get(format!("{}/showcase-v1", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);
    let html = live.text().await.unwrap();
    let decoded_html = html.replace("&#x2f;", "/").replace("&#x2F;", "/");

    assert!(html.contains("Showcase V1 Title"), "{html}");
    assert!(html.contains("od-block-showcase border p-4"), "{html}");
    assert!(html.contains("margin:-16px 0px -16px 0px;"), "{html}");
    assert!(html.contains("data-od-spacing=\"outer_margin\""), "{html}");
    assert!(html.contains("data-od-spacing=\"box_padding\""), "{html}");
    assert!(html.contains("margin:-10px 0px 0px 0px;"), "{html}");
    assert!(html.contains("Nut CTA V1"), "{html}");
    assert!(decoded_html.contains("https://example.com/cta"), "{html}");
    assert!(
        decoded_html.contains("https://example.com/hidden-ref"),
        "{html}"
    );
    assert!(html.contains("<svg "), "{html}");
    assert!(html.contains("Khuyen Mai"), "{html}");
    assert!(html.contains("Cam nang S03"), "{html}");
    assert!(html.contains("Tin Noi Bat S03"), "{html}");
    assert!(html.contains("hi@example.com"), "{html}");
    assert!(html.contains("2026-11-27"), "{html}");

    for media_id in [&img_cover, &img_gallery] {
        assert_eq!(
            http.get(format!("{}/media/{media_id}/file", site.base))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn s03_dynamic_options_sources_overrides_patterns_file_acl_and_skip_pre_v1() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = setup_and_login_admin(&http, &site).await;

    // 1. Install and activate test v3 theme with V1 blocks.
    let theme_res = http
        .post(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_s03_test_theme_zip())
        .send()
        .await
        .unwrap();
    if theme_res.status() != StatusCode::CREATED {
        let st = theme_res.status();
        let txt = theme_res.text().await.unwrap();
        panic!("theme install failed {st}: {txt}");
    }
    let rev_id = theme_res.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            "/api/admin/themes/active",
            Some("\"v1\""),
            &json!({ "revision_id": rev_id }),
        )
        .await
        .status(),
        StatusCode::OK
    );

    // 2. Provision targets for all 8 options_source types + file media + entries + terms + 17 patterns.
    let file_media = upload_png(&http, &site, &cookie, &csrf, "doc.png").await;
    let menu_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/menus",
        &json!({ "menu_key": "nav-a", "title": "Nav A" }),
    )
    .await;
    assert_eq!(menu_res.status(), StatusCode::CREATED);
    let menu_id = menu_res.json::<Value>().await.unwrap()["menu"]["id"]
        .as_i64()
        .unwrap();

    let form_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/contact/forms",
        &json!({
            "slug": "form-s03",
            "title": "Form S03",
            "schema": {
                "version": 1,
                "fields": [{ "key": "name", "label": "Name", "kind": "text", "required": true }]
            }
        }),
    )
    .await;
    assert_eq!(form_res.status(), StatusCode::CREATED);
    let form_id = form_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let cat_term = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/taxonomy/terms",
        &json!({ "kind": "category", "name": "Cat A", "slug": "cat-a", "parent_id": null }),
    )
    .await;
    assert_eq!(cat_term.status(), StatusCode::CREATED);
    let cat_id = cat_term.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let tax_def = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/taxonomy/definitions",
        &json!({ "key": "post_tag", "label": "Post Tag", "hierarchical": false, "public": true }),
    )
    .await;
    assert_eq!(tax_def.status(), StatusCode::CREATED);

    let tag_term = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/taxonomy/terms",
        &json!({ "kind": "post_tag", "name": "Tag B", "slug": "tag-b", "parent_id": null }),
    )
    .await;
    assert_eq!(tag_term.status(), StatusCode::CREATED);
    let tag_id = tag_term.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let mut pattern_ids = Vec::new();
    for i in 0..17 {
        let p = admin_post(
            &http,
            &site,
            &cookie,
            &csrf,
            "/api/admin/patterns",
            &json!({ "title": format!("Pat {i}"), "slug": format!("pat-{i}") }),
        )
        .await;
        assert_eq!(p.status(), StatusCode::CREATED);
        pattern_ids.push(p.json::<Value>().await.unwrap()["id"].as_i64().unwrap());
    }

    let page_res = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/content-types/page/entries",
        &json!({ "title": "Dynamic Target Page", "slug": "dyn-page", "values": {} }),
    )
    .await;
    assert_eq!(page_res.status(), StatusCode::CREATED);
    let page_id = page_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let other_post = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/content-types/page/entries",
        &json!({ "title": "Ref Post", "slug": "ref-post", "values": {} }),
    )
    .await;
    assert_eq!(other_post.status(), StatusCode::CREATED);
    let other_post_id = other_post.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let valid_vals = json!({
        "sel_menu": "nav-a",
        "sel_slot": "main_slot",
        "sel_form": form_id.to_string(),
        "sel_ct": "post",
        "sel_tax": "category",
        "sel_ctx": "title",
        "sel_part": "part-a",
        "sel_pat": pattern_ids[0].to_string(),
        "any_entry": other_post_id,
        "dyn_tax": "post_tag",
        "dyn_terms": [tag_id],
        "file_doc": file_media
    });

    // 3. Reject each invalid options_source and taxonomy_from_field mismatch.
    for (bad_key, bad_val) in [
        ("sel_menu", json!("missing_menu")),
        ("sel_slot", json!("undeclared_slot")),
        ("sel_form", json!("999999")),
        ("sel_ct", json!("pattern")),
        ("sel_tax", json!("missing_tax")),
        ("sel_ctx", json!("secret_field")),
        ("sel_part", json!("missing_part")),
        ("sel_pat", json!("999999")),
        ("dyn_terms", json!([cat_id])),
        ("dyn_tax", json!(null)),
    ] {
        let mut bad_vals = valid_vals.clone();
        bad_vals[bad_key] = bad_val;
        if bad_key == "dyn_tax" {
            bad_vals["dyn_terms"] = json!([tag_id]);
        }
        let resp = admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            &format!("/api/admin/entries/page/{page_id}/builder"),
            Some("\"v1\""),
            &block_doc("el-dyn", "theme-s03-test/all-sources", bad_vals),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 for {bad_key}"
        );
    }

    // Valid save -> 200 OK & entry_pattern_refs synced for options_source patterns.
    let valid_resp = admin_put(
        &http,
        &site,
        &cookie,
        &csrf,
        &format!("/api/admin/entries/page/{page_id}/builder"),
        Some("\"v1\""),
        &block_doc("el-dyn", "theme-s03-test/all-sources", valid_vals.clone()),
    )
    .await;
    let valid_status = valid_resp.status();
    let valid_body = valid_resp.text().await.unwrap_or_default();
    assert_eq!(
        valid_status,
        StatusCode::OK,
        "valid save failed: {valid_body}"
    );
    let pool = db_pool(&site).await;
    let ref_cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_pattern_refs WHERE source_entry_id = ? AND target_pattern_id = ?",
    )
    .bind(page_id)
    .bind(pattern_ids[0])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ref_cnt, 1);

    // Deleting nav_a preserves unchanged previous sel_menu while editing unrelated fields; new bad menu still fails.
    assert_eq!(
        http.delete(format!("{}/api/admin/menus/{menu_id}", site.base))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    let mut keep_vals = valid_vals.clone();
    keep_vals["any_entry"] = json!(page_id);
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            &format!("/api/admin/entries/page/{page_id}/builder"),
            Some("\"v2\""),
            &block_doc("el-dyn", "theme-s03-test/all-sources", keep_vals),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let mut switch_bad_menu = valid_vals.clone();
    switch_bad_menu["sel_menu"] = json!("another_missing");
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            &format!("/api/admin/entries/page/{page_id}/builder"),
            Some("\"v3\""),
            &block_doc("el-dyn", "theme-s03-test/all-sources", switch_bad_menu),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    // 4. Overrides reject Patterns blocks & detect part cycles via options_source template_parts.
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            "/api/admin/template-overrides",
            None,
            &json!({
                "target": { "kind": "page" },
                "document": block_doc("el-ov-pat", "theme-s03-test/pattern-picker", json!({ "pat": [pattern_ids[0].to_string()] }))
            }),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            "/api/admin/template-overrides",
            None,
            &json!({
                "target": { "kind": "part", "part_key": "part-b" },
                "document": block_doc("el-part-b", "theme-s03-test/part-caller", json!({ "part_key": "part-a" }))
            }),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            "/api/admin/template-overrides",
            None,
            &json!({
                "target": { "kind": "part", "part_key": "part-a" },
                "document": block_doc("el-part-a", "theme-s03-test/part-caller", json!({ "part_key": "part-b" }))
            }),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    // 5. Pattern entry forbids options_source patterns & page caps at 16 distinct pattern IDs.
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            &format!("/api/admin/entries/pattern/{}/builder", pattern_ids[1]),
            Some("\"v1\""),
            &block_doc(
                "el-nest",
                "theme-s03-test/pattern-picker",
                json!({ "pat": [pattern_ids[0].to_string()] })
            ),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let seventeen: Vec<String> = pattern_ids.iter().map(|id| id.to_string()).collect();
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            &format!("/api/admin/entries/page/{page_id}/builder"),
            Some("\"v3\""),
            &block_doc(
                "el-17",
                "theme-s03-test/pattern-picker",
                json!({ "pat": seventeen })
            ),
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    // 6. Comment-block entry path: validates options_source, syncs entry_pattern_refs, and grants file_ids media ACL.
    let comment_doc_val = block_doc(
        "el-comment-file",
        "theme-s03-test/all-sources",
        json!({
            "sel_ct": "post",
            "sel_pat": pattern_ids[2].to_string(),
            "file_doc": file_media
        }),
    );
    let comment_doc: BuilderDocument = serde_json::from_value(comment_doc_val).unwrap();
    let comment_body = serialize_builder_document_to_blocks(&comment_doc).unwrap();
    let cb_post = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/content-types/page/entries",
        &json!({
            "title": "Comment Block Post",
            "slug": "cb-post-s03",
            "values": { "body": comment_body }
        }),
    )
    .await;
    assert_eq!(cb_post.status(), StatusCode::CREATED);
    let cb_post_id = cb_post.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    // Draft -> private media returns 404; entry_pattern_refs already synced.
    assert_eq!(
        http.get(format!("{}/media/{file_media}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let cb_pat_cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_pattern_refs WHERE source_entry_id = ? AND target_pattern_id = ?",
    )
    .bind(cb_post_id)
    .bind(pattern_ids[2])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cb_pat_cnt, 1);

    assert_eq!(
        http.post(format!(
            "{}/api/admin/content-types/page/entries/{cb_post_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        http.get(format!("{}/media/{file_media}/file", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // 7. Block with options_source = contact_forms enables active form CSP (form-action 'self').
    let form_page = admin_post(
        &http,
        &site,
        &cookie,
        &csrf,
        "/api/admin/content-types/page/entries",
        &json!({ "title": "Form Ref Page", "slug": "form-ref-page", "values": {} }),
    )
    .await;
    assert_eq!(form_page.status(), StatusCode::CREATED);
    let form_page_id = form_page.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        admin_put(
            &http,
            &site,
            &cookie,
            &csrf,
            &format!("/api/admin/entries/page/{form_page_id}/builder"),
            Some("\"v1\""),
            &block_doc(
                "el-fref",
                "theme-s03-test/form-ref",
                json!({ "fid": form_id.to_string() })
            ),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let form_pub = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{form_page_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(form_pub.status(), StatusCode::OK);
    let form_live = http
        .get(format!("{}/form-ref-page", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(form_live.status(), StatusCode::OK);
    let csp = form_live
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp.contains("form-action 'self'"), "{csp}");

    // 8. Incompatible pre-V1 non-core block in builder_block_definitions is skipped with warning, never 500.
    sqlx::query(
        "INSERT INTO builder_block_definitions(block_id, owner_kind, owner_id, provider_status, declaration_state, schema_json, template, revision_digest)
         VALUES ('old-plugin/pre-v1', 'plugin', 'old-plugin', 'active', 'declared', ?, '<p>{{ label }}</p>', 'rev-pre-v1')",
    )
    .bind(r#"{"schema_version":1,"id":"old-plugin/pre-v1","title":"Pre V1","category":"plugin","render":"render.html","fields":[{"type":"text","key":"label","label":"Label","show_if":{"field":"x","equals":true}}]}"#)
    .execute(&pool)
    .await
    .unwrap();
    let catalog_res = http
        .get(format!("{}/api/admin/builder/blocks", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(catalog_res.status(), StatusCode::OK);
    let catalog_json: Value = catalog_res.json().await.unwrap();
    let items = catalog_json["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["block_id"] == "core/showcase"));
    assert!(!items.iter().any(|i| i["block_id"] == "old-plugin/pre-v1"));
}
