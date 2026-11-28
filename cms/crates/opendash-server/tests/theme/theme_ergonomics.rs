use std::{
    fs,
    io::Write as _,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Method, Response, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

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

struct Admin {
    cookie: String,
    csrf: String,
}

impl Site {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert_ne!(port, 18964, "never target live dev server");

        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Canonicalize OS temp dir (/var -> /private/var on macOS).
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-theme-ergo-{}-{port}-{seq}-{nanos}",
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
        panic!("theme_ergonomics test server failed to start at {base}: {log}");
    }
}

async fn setup_and_login_admin(http: &Client, site: &Site) -> Admin {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "ergo_admin", "password": "ErgoAdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login = http
        .post(format!("{}/auth/login", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "ergo_admin", "password": "ErgoAdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    Admin { cookie, csrf }
}

async fn admin_req(
    http: &Client,
    site: &Site,
    admin: &Admin,
    method: Method,
    path: &str,
    if_match: Option<&str>,
    body: Option<Value>,
) -> Response {
    let mut req = http
        .request(method.clone(), format!("{}{path}", site.base))
        .header(header::COOKIE, &admin.cookie);
    if method != Method::GET {
        req = req
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &admin.csrf);
    }
    if let Some(etag) = if_match {
        req = req.header(header::IF_MATCH, etag);
    }
    if let Some(json_body) = body {
        req = req.json(&json_body);
    }
    req.send().await.unwrap()
}

async fn admin_json(
    http: &Client,
    site: &Site,
    admin: &Admin,
    method: Method,
    path: &str,
    if_match: Option<&str>,
    body: Option<Value>,
    expected: StatusCode,
) -> Value {
    let resp = admin_req(http, site, admin, method, path, if_match, body).await;
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert_eq!(status, expected, "{path} -> HTTP {status}: {text}");
    serde_json::from_str(&text).unwrap()
}

async fn upload_theme_zip(
    http: &Client,
    site: &Site,
    admin: &Admin,
    zip_bytes: Vec<u8>,
) -> Response {
    http.post(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(zip_bytes)
        .send()
        .await
        .unwrap()
}

fn build_zip_from_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (path, data) in entries {
            zip.start_file(*path, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn sample_png_bytes() -> Vec<u8> {
    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([20, 140, 220, 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

fn build_ergo_theme_zip(version: &str, declare_portfolio: bool) -> Vec<u8> {
    let mut manifest = json!({
        "manifest_version": 1,
        "theme_id": "theme-ergo-v3",
        "name": "Theme Ergonomics E2E",
        "version": version,
        "parts": {
            "header": "header.html",
            "footer": "footer.html",
            "card": "template-part/card.html"
        },
        "templates": [
            { "kind": "page", "file": "page.html" },
            { "kind": "single", "file": "single.html" },
            { "kind": "single", "content_type": "post", "file": "single-post.html" },
            { "kind": "archive", "file": "archive.html" },
            { "kind": "taxonomy", "file": "taxonomy.html" },
            { "kind": "not_found", "file": "404.html" },
            { "kind": "index", "file": "index.html" }
        ],
        "assets": ["assets/theme.css", "assets/logo.png"],
        "menus": ["primary", "footer"],
        "options": [
            {
                "key": "banner_text",
                "label": "Banner Text",
                "type": "text",
                "default": "Hello Ergonomics",
                "public": true
            }
        ]
    });
    if declare_portfolio {
        manifest["declarations"] = json!({
            "content_types": [{
                "key": "portfolio",
                "label": "Portfolio",
                "taxonomies": ["category", "tag"]
            }],
            "field_groups": [{
                "local_key": "portfolio_fields",
                "title": "Portfolio Fields",
                "target_types": ["portfolio"],
                "fields": [
                    { "key": "summary", "label": "Summary", "type": "text", "required": false },
                    { "key": "price", "label": "Price", "type": "number", "required": false }
                ]
            }]
        });
    }

    let manifest_str = manifest.to_string();
    let css = b"body { color: #222; font-size: 16px; }";
    let png = sample_png_bytes();

    let header_html = br#"<header data-ergo-header="1" data-origin="{{ site.origin }}" data-tz="{{ site.timezone }}" data-year="{{ site.current_year }}">
<link rel="stylesheet" href="{{ theme_assets['assets/theme.css'] }}">
<img src="{{ theme_assets['assets/logo.png'] }}" alt="Logo">
<nav data-route-kind="{{ route.kind }}" data-route-path="{{ route.path | default('') }}">
{% if menus.primary is defined %}
{% for item in menus.primary.items %}
<a data-menu-label="{{ item.label }}" data-active="{{ item.is_active }}" data-ancestor="{{ item.is_ancestor }}" data-ext="{{ item.is_external }}" data-depth="{{ item.depth }}" href="{{ item.url }}">{{ item.label }}</a>
{% for child in item.children %}
<a data-child-label="{{ child.label }}" data-active="{{ child.is_active }}" data-ancestor="{{ child.is_ancestor }}" data-depth="{{ child.depth }}" href="{{ child.url }}">{{ child.label }}</a>
{% endfor %}
{% endfor %}
{% endif %}
</nav>
</header>"#;

    let footer_html = br#"<footer data-ergo-footer="1" data-recent-count="{{ recent_posts | length }}" data-year="{{ site.current_year }}">
{% for rp in recent_posts %}<a data-recent-slug="{{ rp.slug }}" href="{{ rp.url }}">{{ rp.title }}</a>{% endfor %}
</footer>"#;

    let card_html = br#"<div class="od-card">{{ entry.title }}</div>"#;

    let page_html = br#"<main data-tpl="page">
<nav data-bc>{% for b in breadcrumbs %}<span data-bc-cur="{{ b.is_current }}">{{ b.name }}</span>{% endfor %}</nav>
<p data-opt>{{ options.banner_text }}</p>
<h1>{{ entry.title }}</h1>
<div class="slot">{{ od_content() }}</div>
</main>"#;

    let single_post_html = br#"<main data-tpl="single-post" data-type="{{ entry.type_key }}" data-read-min="{{ entry.reading_time_minutes }}" data-civil="{{ '2026-06-15' | date_format('%Y-%m-%d') }}" data-pub-hcm="{{ entry.published_at | date_format('%Y-%m-%d', 'Asia/Ho_Chi_Minh') }}" data-null-def="{{ entry.fields.featured_image | default('none-fallback') }}" data-num="{{ 12500 | format_number }}">
<nav data-bc>{% for b in breadcrumbs %}<span data-bc-cur="{{ b.is_current }}">{{ b.name }}</span>{% endfor %}</nav>
<article>
<h1>{{ entry.title }}</h1>
<p data-excerpt>{{ entry.excerpt | truncate(60) }}</p>
<span data-prim-cat>{{ entry.primary_category.name if entry.primary_category is defined and entry.primary_category is not none else '' }}</span>
<span data-cat-count>{{ entry.terms.category | length }}</span>
<span data-all-terms>{{ entry.all_terms | length }}</span>
{% if entry.title is string and entry.id is number and entry.fields is mapping and entry.all_terms is sequence %}<span data-tests="ok">yes</span>{% endif %}
<div class="plain-text"><p>1 < 2</p><p>1 < 2 it's ok</p><p>a < b</p>< !doctype></div>
<div class="slot">{{ od_content() }}</div>
</article>
<section data-part-slot>{{ od_part("card") }}</section>
<aside data-related-count="{{ related_entries | length }}">{% for rel in related_entries %}<a data-rel-slug="{{ rel.slug }}" href="{{ rel.url }}">{{ rel.title }}</a>{% endfor %}</aside>
</main>"#;

    let single_html = br#"<main data-tpl="single"><h1>{{ entry.title }}</h1><div>{{ od_content() }}</div></main>"#;

    let archive_html = br#"<main data-tpl="archive" data-total="{{ pagination.total_items }}" data-pages="{{ pagination.total_pages }}" data-prev="{{ pagination.has_prev }}" data-next="{{ pagination.has_next }}" data-win="{{ pagination.pages | length }}">
<nav data-bc>{% for b in breadcrumbs %}<span data-bc-cur="{{ b.is_current }}">{{ b.name }}</span>{% endfor %}</nav>
<p data-mapped>{{ entries | map(attribute='title') | join(', ') }}</p>
<p data-sel-count>{{ entries | selectattr('published_at') | list | length }}</p>
<p data-rej-count>{{ entries | rejectattr('slug', 'equalto', 'never-match') | list | length }}</p>
{% for row in entries | batch(2) %}<div data-batch-row="{{ loop.index }}">{% for e in row %}<a href="{{ e.url }}">{{ e.title }}</a>{% endfor %}</div>{% endfor %}
</main>"#;

    let taxonomy_html = br#"<main data-tpl="taxonomy" data-tax="{{ taxonomy.name }}" data-total="{{ pagination.total_items }}">
<nav data-bc>{% for b in breadcrumbs %}<span data-bc-cur="{{ b.is_current }}">{{ b.name }}</span>{% endfor %}</nav>
{% for e in entries %}<a data-tax-entry href="{{ e.url }}">{{ e.title }}</a>{% endfor %}
</main>"#;

    let not_found_html = br#"<main data-tpl="not_found">
<nav data-bc>{% for b in breadcrumbs %}<span data-bc-cur="{{ b.is_current }}">{{ b.name }}</span>{% endfor %}</nav>
<h1>404 Custom</h1>
</main>"#;

    let index_html =
        br#"<main data-tpl="index"><p data-opt>{{ options.banner_text }}</p>{% for e in entries %}<a href="{{ e.url }}">{{ e.title }}</a>{% endfor %}</main>"#;

    build_zip_from_entries(&[
        ("theme.json", manifest_str.as_bytes()),
        ("assets/theme.css", css),
        ("assets/logo.png", &png),
        ("header.html", header_html),
        ("footer.html", footer_html),
        ("template-part/card.html", card_html),
        ("page.html", page_html),
        ("single.html", single_html),
        ("single-post.html", single_post_html),
        ("archive.html", archive_html),
        ("taxonomy.html", taxonomy_html),
        ("404.html", not_found_html),
        ("index.html", index_html),
    ])
}

async fn create_post_composite(
    http: &Client,
    site: &Site,
    admin: &Admin,
    type_key: &str,
    slug: &str,
    title: &str,
    values: Value,
    term_ids: &[i64],
    primary_category_term_id: Option<i64>,
) -> Value {
    let all_types = admin_json(
        http,
        site,
        admin,
        Method::GET,
        "/api/admin/content-types",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let schema_token = all_types
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["descriptor"]["key"] == type_key)
        .unwrap_or_else(|| panic!("missing type {type_key}"))["schema_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut payload = json!({
        "title": title,
        "slug": slug,
        "values": values,
        "publication_status": "published",
        "term_ids": term_ids,
        "expected_schema_token": schema_token,
    });
    if let Some(pid) = primary_category_term_id {
        payload["primary_category_term_id"] = json!(pid);
    }
    admin_json(
        http,
        site,
        admin,
        Method::POST,
        &format!("/api/admin/content-types/{type_key}/entries/composite"),
        None,
        Some(payload),
        StatusCode::CREATED,
    )
    .await
}

#[tokio::test]
async fn theme_ergonomics_cuj_all_helpers_rich_context_zero_n_plus_1_and_withdrawn_cpt() {
    let site = Site::start().await;
    let http = Client::new();
    let admin = setup_and_login_admin(&http, &site).await;

    // 1. Studio preview parity on immutable embedded `opendash-starter`
    for target in [
        json!({ "kind": "taxonomy" }),
        json!({ "kind": "index" }),
        json!({ "kind": "single" }),
        json!({ "kind": "single_custom", "custom_key": "feature" }),
        json!({ "kind": "singular" }),
    ] {
        let preview = admin_json(
            &http,
            &site,
            &admin,
            Method::POST,
            "/api/admin/studio/preview",
            None,
            Some(json!({ "target": target })),
            StatusCode::OK,
        )
        .await;
        assert!(
            preview["html"].as_str().unwrap_or("").contains("<html"),
            "starter preview must render HTML for {target}"
        );
    }

    // Create sample Builder page + menu -> test transient.menu_slots & transient.general.timezone
    let transient_menu = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/menus",
        None,
        Some(json!({ "menu_key": "transient_nav", "title": "Transient Nav" })),
        StatusCode::CREATED,
    )
    .await;
    let tmenu_id = transient_menu["menu"]["id"].as_i64().unwrap();
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        &format!("/api/admin/menus/{tmenu_id}"),
        Some("\"v1\""),
        Some(json!({
            "title": "Transient Nav",
            "items": [{
                "id": null,
                "client_key": null,
                "parent_id": null,
                "parent_client_key": null,
                "position": 0,
                "label": "TransientItem",
                "target_kind": "external",
                "target_entry_id": null,
                "target_term_id": null,
                "external_url": "https://example.org/transient"
            }]
        })),
        StatusCode::OK,
    )
    .await;
    let tr_page = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/content-types/page/entries",
        None,
        Some(json!({ "title": "Builder Tr", "slug": "builder-tr", "values": {} })),
        StatusCode::CREATED,
    )
    .await;
    let tr_page_id = tr_page["id"].as_i64().unwrap();
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        &format!("/api/admin/entries/page/{tr_page_id}/builder"),
        Some("\"v1\""),
        Some(json!({
            "schema_version": 1,
            "blocks": [{
                "id": "tr-sec-1",
                "block_id": "core/section",
                "children": [{
                    "id": "tr-row-1",
                    "block_id": "core/row",
                    "children": [{
                        "id": "tr-col-1",
                        "block_id": "core/column",
                        "children": [{
                            "id": "tr-blk-1",
                            "block_id": "core/heading",
                            "props": { "level": "h2", "text": "Body Transient" }
                        }]
                    }]
                }]
            }]
        })),
        StatusCode::OK,
    )
    .await;
    let tr_preview = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/studio/preview",
        None,
        Some(json!({
            "target": { "kind": "page" },
            "preview_entry": { "type_key": "page", "entry_id": tr_page_id },
            "transient": {
                "general": {
                    "site_title": "Transient New Site",
                    "tagline": "Transient Tag",
                    "timezone": "America/New_York"
                },
                "menu_slots": {
                    "primary": tmenu_id
                }
            }
        })),
        StatusCode::OK,
    )
    .await;
    let tr_html = tr_preview["html"].as_str().unwrap();
    assert!(tr_html.contains("TransientItem"));
    assert!(tr_html.contains("Transient New Site"));

    // 2. Upload & activate independent Theme v3 fixture declaring `portfolio`
    let up = upload_theme_zip(&http, &site, &admin, build_ergo_theme_zip("1.0.0", true)).await;
    assert_eq!(up.status(), StatusCode::CREATED);
    let rev1 = up.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let themes_state = admin_json(
        &http,
        &site,
        &admin,
        Method::GET,
        "/api/admin/themes",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let sel_ver = themes_state["selection"]["version"].as_i64().unwrap();
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        "/api/admin/themes/active",
        Some(&format!("\"v{sel_ver}\"")),
        Some(json!({ "revision_id": rev1 })),
        StatusCode::OK,
    )
    .await;

    // 3. Configure General timezone + Public Origin
    let gen_cur = admin_json(
        &http,
        &site,
        &admin,
        Method::GET,
        "/api/admin/settings/general",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        "/api/admin/settings/general",
        Some(&format!("\"v{}\"", gen_cur["version"].as_i64().unwrap())),
        Some(json!({
            "site_title": "Ergo Live Site",
            "tagline": "Fast Rust CMS",
            "timezone": "Asia/Ho_Chi_Minh"
        })),
        StatusCode::OK,
    )
    .await;
    let pub_cur = admin_json(
        &http,
        &site,
        &admin,
        Method::GET,
        "/api/admin/settings/public",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        "/api/admin/settings/public",
        Some(&format!("\"v{}\"", pub_cur["version"].as_i64().unwrap())),
        Some(json!({
            "public_origin": "https://demo.example.com/",
            "content_language": "vi",
            "indexing_enabled": true
        })),
        StatusCode::OK,
    )
    .await;

    // 4. Create parent+child pages, parent+child categories, tag, and posts
    let home_pg = create_post_composite(
        &http,
        &site,
        &admin,
        "page",
        "home-ergo",
        "Trang chu Ergo",
        json!({ "body": "<p>Welcome Home</p>" }),
        &[],
        None,
    )
    .await;
    let home_pg_id = home_pg["entry"]["id"].as_i64().unwrap();

    let parent_pg = create_post_composite(
        &http,
        &site,
        &admin,
        "page",
        "parent-pg",
        "Parent Nav Page",
        json!({ "body": "<p>Parent Page Body</p>" }),
        &[],
        None,
    )
    .await;
    let parent_pg_id = parent_pg["entry"]["id"].as_i64().unwrap();

    let child_pg = create_post_composite(
        &http,
        &site,
        &admin,
        "page",
        "child-pg",
        "Child Nav Page",
        json!({ "body": "<p>Child Page Body</p>" }),
        &[],
        None,
    )
    .await;
    let child_pg_id = child_pg["entry"]["id"].as_i64().unwrap();

    let posts_pg = create_post_composite(
        &http,
        &site,
        &admin,
        "page",
        "posts-hub",
        "Tin tuc",
        json!({ "body": "<p>Archive Page</p>" }),
        &[],
        None,
    )
    .await;
    let posts_pg_id = posts_pg["entry"]["id"].as_i64().unwrap();

    let read_cur = admin_json(
        &http,
        &site,
        &admin,
        Method::GET,
        "/api/admin/settings/reading",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        "/api/admin/settings/reading",
        Some(&format!("\"v{}\"", read_cur["version"].as_i64().unwrap())),
        Some(json!({
            "homepage_page_id": home_pg_id,
            "posts_page_id": posts_pg_id
        })),
        StatusCode::OK,
    )
    .await;

    let root_cat = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/taxonomy/terms",
        None,
        Some(json!({ "kind": "category", "name": "Cong nghe", "slug": "cong-nghe" })),
        StatusCode::CREATED,
    )
    .await;
    let root_cat_id = root_cat["id"].as_i64().unwrap();

    let child_cat = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/taxonomy/terms",
        None,
        Some(json!({
            "kind": "category",
            "name": "Rust CMS",
            "slug": "rust-cms",
            "parent_id": root_cat_id
        })),
        StatusCode::CREATED,
    )
    .await;
    let child_cat_id = child_cat["id"].as_i64().unwrap();

    let tag_rust = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/taxonomy/terms",
        None,
        Some(json!({ "kind": "tag", "name": "Fast", "slug": "fast" })),
        StatusCode::CREATED,
    )
    .await;
    let tag_rust_id = tag_rust["id"].as_i64().unwrap();

    // Assign primary menu with nested parent/child + external link
    let nav = admin_json(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/menus",
        None,
        Some(json!({ "menu_key": "ergo_primary", "title": "Ergo Primary" })),
        StatusCode::CREATED,
    )
    .await;
    let nav_id = nav["menu"]["id"].as_i64().unwrap();
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        &format!("/api/admin/menus/{nav_id}"),
        Some("\"v1\""),
        Some(json!({
            "title": "Ergo Primary",
            "items": [
                {
                    "id": null,
                    "client_key": "p1",
                    "parent_id": null,
                    "parent_client_key": null,
                    "position": 0,
                    "label": "MenuParent",
                    "target_kind": "entry",
                    "target_entry_id": parent_pg_id,
                    "target_term_id": null,
                    "external_url": null
                },
                {
                    "id": null,
                    "client_key": "c1",
                    "parent_id": null,
                    "parent_client_key": "p1",
                    "position": 0,
                    "label": "MenuChild",
                    "target_kind": "entry",
                    "target_entry_id": child_pg_id,
                    "target_term_id": null,
                    "external_url": null
                },
                {
                    "id": null,
                    "client_key": null,
                    "parent_id": null,
                    "parent_client_key": null,
                    "position": 1,
                    "label": "MenuExt",
                    "target_kind": "external",
                    "target_entry_id": null,
                    "target_term_id": null,
                    "external_url": "https://external.example.org/docs"
                }
            ]
        })),
        StatusCode::OK,
    )
    .await;
    let slots_cur = admin_json(
        &http,
        &site,
        &admin,
        Method::GET,
        "/api/admin/menu-slots/theme-ergo-v3",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        "/api/admin/menu-slots/theme-ergo-v3",
        Some(&format!("\"v{}\"", slots_cur["version"].as_i64().unwrap())),
        Some(json!({ "assignments": { "primary": nav_id } })),
        StatusCode::OK,
    )
    .await;

    // Publish 2 related posts (>220 words in main post) + 1 portfolio entry
    let long_body = format!("<p>{}</p>", "word ".repeat(230));
    create_post_composite(
        &http,
        &site,
        &admin,
        "post",
        "main-story",
        "Main Ergo Story",
        json!({ "body": long_body }),
        &[root_cat_id, child_cat_id, tag_rust_id],
        Some(child_cat_id),
    )
    .await;
    create_post_composite(
        &http,
        &site,
        &admin,
        "post",
        "related-story",
        "Related Ergo Story",
        json!({ "body": "<p>Short sibling post.</p>" }),
        &[child_cat_id],
        Some(child_cat_id),
    )
    .await;
    create_post_composite(
        &http,
        &site,
        &admin,
        "portfolio",
        "case-one",
        "Case One",
        json!({ "summary": "Portfolio summary", "price": 1500 }),
        &[root_cat_id],
        None,
    )
    .await;

    // 5. Verify all 5 public HTTP routes + helpers + body classes + breadcrumbs + asset serving
    let home_html = http
        .get(format!("{}/", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let home_decoded = home_html.replace("&#x2f;", "/").replace("&#x2F;", "/");
    assert!(home_decoded.contains("od-route-home"));
    assert!(home_decoded.contains("data-origin=\"https://demo.example.com\""));
    assert!(home_decoded.contains("data-tz=\"Asia/Ho_Chi_Minh\""));
    assert!(home_decoded.contains("<link rel=\"stylesheet\" href=\"/theme/"));
    assert!(home_decoded.contains("/assets/logo.png"));
    assert!(home_decoded.contains("data-opt>Hello Ergonomics</p>"));

    let single_html = http
        .get(format!("{}/main-story", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        single_html.contains("od-route-single"),
        "missing od-route-single: {single_html}"
    );
    assert!(single_html.contains("od-type-post"));
    assert!(single_html.contains("data-tpl=\"single-post\""));
    assert!(single_html.contains("data-read-min=\"2\""));
    assert!(single_html.contains("data-civil=\"2026-06-15\""));
    assert!(single_html.contains("data-null-def=\"none-fallback\""));
    assert!(single_html.contains("data-num=\"12.500\""));
    assert!(single_html.contains("data-tests=\"ok\""));
    assert!(single_html.contains("data-prim-cat>Rust CMS</span>"));
    assert!(single_html.contains("data-cat-count>2</span>"));
    assert!(single_html.contains("data-related-count=\"1\""));
    assert!(single_html.contains("data-rel-slug=\"related-story\""));
    assert!(single_html.contains("data-recent-count=\"1\"")); // excludes current post!
    assert!(single_html.contains("od-card"));
    assert!(single_html.contains("<p>1 &lt; 2</p>") || single_html.contains("1 < 2"));

    let child_page_html = http
        .get(format!("{}/child-pg", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        child_page_html.contains(
            "data-menu-label=\"MenuParent\" data-active=\"False\" data-ancestor=\"True\""
        ),
        "child_page_html missing ancestor assertion: {child_page_html}"
    );
    assert!(
        child_page_html.contains("data-depth=\"1\""),
        "child_page_html missing depth 1: {child_page_html}"
    );
    assert!(
        child_page_html.contains(
            "data-child-label=\"MenuChild\" data-active=\"True\" data-ancestor=\"False\" data-depth=\"2\""
        ),
        "child_page_html missing MenuChild active assertion: {child_page_html}"
    );
    assert!(
        child_page_html
            .contains("data-menu-label=\"MenuExt\" data-active=\"False\" data-ancestor=\"False\" data-ext=\"True\""),
        "child_page_html missing MenuExt assertion: {child_page_html}"
    );

    let archive_html = http
        .get(format!("{}/posts-hub", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(archive_html.contains("od-route-archive"), "{archive_html}");
    assert!(archive_html.contains("od-type-post"), "{archive_html}");
    assert!(archive_html.contains("data-total=\"2\""), "{archive_html}");
    assert!(
        archive_html.contains("data-sel-count>2</p>"),
        "{archive_html}"
    );
    assert!(
        archive_html.contains("data-rej-count>2</p>"),
        "{archive_html}"
    );
    assert!(archive_html.contains("Main Ergo Story"), "{archive_html}");
    assert!(
        archive_html.contains("Related Ergo Story"),
        "{archive_html}"
    );

    let tax_html = http
        .get(format!("{}/category/rust-cms", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(tax_html.contains("od-route-taxonomy"), "{tax_html}");
    assert!(tax_html.contains("od-tax-category"), "{tax_html}");
    assert!(tax_html.contains("Cong nghe"), "{tax_html}"); // WITH RECURSIVE ancestor breadcrumb item!
    assert!(tax_html.contains("Rust CMS"), "{tax_html}");
    assert!(tax_html.contains("data-total=\"2\""), "{tax_html}");

    let nf_res = http
        .get(format!("{}/definitely-missing-url", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(nf_res.status(), StatusCode::NOT_FOUND);
    let nf_html = nf_res.text().await.unwrap();
    assert!(nf_html.contains("od-route-not-found"), "{nf_html}");
    assert!(nf_html.contains(">404</span>"), "{nf_html}");
    assert!(
        !nf_html.contains("data-active=\"True\"") && !nf_html.contains("data-active=\"true\""),
        "{nf_html}"
    );

    // 6. Withdraw `portfolio` CPT and confirm archive/single do not return 500
    let up2 = upload_theme_zip(&http, &site, &admin, build_ergo_theme_zip("2.0.0", false)).await;
    assert_eq!(up2.status(), StatusCode::CREATED);
    let rev2 = up2.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let themes_state2 = admin_json(
        &http,
        &site,
        &admin,
        Method::GET,
        "/api/admin/themes",
        None,
        None,
        StatusCode::OK,
    )
    .await;
    let sel_ver2 = themes_state2["selection"]["version"].as_i64().unwrap();
    admin_json(
        &http,
        &site,
        &admin,
        Method::PUT,
        "/api/admin/themes/active",
        Some(&format!("\"v{sel_ver2}\"")),
        Some(json!({ "revision_id": rev2 })),
        StatusCode::OK,
    )
    .await;

    let withdrawn_arch = http
        .get(format!("{}/portfolio", site.base))
        .send()
        .await
        .unwrap();
    assert_ne!(
        withdrawn_arch.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "withdrawn CPT archive must never 500"
    );
    let withdrawn_single = http
        .get(format!("{}/portfolio/case-one", site.base))
        .send()
        .await
        .unwrap();
    assert_ne!(
        withdrawn_single.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "withdrawn CPT single must never 500"
    );
}

#[tokio::test]
async fn theme_ergonomics_csec_anti_bypass_matrix_rejects_all_vectors() {
    let site = Site::start().await;
    let http = Client::new();
    let admin = setup_and_login_admin(&http, &site).await;

    let base_manifest = json!({
        "manifest_version": 1,
        "theme_id": "bypass-probe",
        "name": "Bypass Probe",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "assets": ["assets/theme.css"],
        "options": [
            { "key": "banner_text", "label": "Banner", "type": "text", "default": "x", "public": true },
            { "key": "flag", "label": "Flag", "type": "boolean", "default": false, "public": true }
        ]
    })
    .to_string();

    let vectors: Vec<(&str, &str)> = vec![
        (
            "split_closing_title_with_expr",
            r#"<title></titl{{ site.title }}e><div title="</title><script>alert(1)</script>"></div>"#,
        ),
        (
            "split_closing_title_trailing_expr",
            r#"<title></title{{ options.banner_text }}><script>alert(1)</script>"#,
        ),
        (
            "bogus_pi_comment_meta",
            r#"<?x <div title="><meta http-equiv='refresh' content='0'>"></div>"#,
        ),
        (
            "bogus_end_tag_digit",
            r#"</1 <div title="><script>alert(1)</script>"></div>"#,
        ),
        (
            "title_space_before_gt_smuggling_link",
            r#"<title>x</title ><a href="https://evil.example">x</a>"#,
        ),
        (
            "comment_expr_trying_to_close_comment",
            r#"<!-- a{{ options.banner_text }}><a href="https://evil.example">x</a> -->"#,
        ),
        (
            "set_field_image_url_taint",
            r#"{% set field = entry %}<img src="{{ field.media.url }}" alt="x">"#,
        ),
        (
            "set_row_entries_for_loop_taint",
            r#"{% set row = entries %}{% for item in row %}<a href="{{ item.url }}">{{ item.title }}</a>{% endfor %}"#,
        ),
        (
            "style_block_with_jinja_stmt",
            r#"<style>{% if options.flag %}body{color:red}{% endif %}</style>"#,
        ),
        (
            "meta_http_equiv_refresh",
            r#"<meta charset="utf-8" http-equiv="refresh" content="0">"#,
        ),
        (
            "dead_branch_javascript_href",
            r#"{% if false %}<a href="javascript:alert(1)">x</a>{% endif %}"#,
        ),
        ("dynamic_tag_name_interpolation", r#"<{{ "script" }}>"#),
        ("html_comment_bang_close", r#"<!-- bad --!>"#),
        (
            "set_block_unused_script_smuggling",
            r#"{% set unused %}<script>alert("{{ options.banner_text }}")</script>{% endset %}<p>ok</p>"#,
        ),
        (
            "with_block_shadowing_url_scope",
            r#"{% with item = entry %}<img src="{{ item.url }}" alt="x">{% endwith %}"#,
        ),
        (
            "for_else_open_lt_state",
            r#"{% for x in entries %}{{ x.title }}{% else %}<{% endfor %}script>"#,
        ),
        (
            "for_loop_second_iter_set_taint",
            r#"{% set safe_u = entry.url %}{% for x in entries %}<a href="{{ safe_u }}">x</a>{% set safe_u = options.banner_text %}{% endfor %}"#,
        ),
        (
            "if_branch_set_taint_join",
            r#"{% set safe_u = entry.url %}{% if options.flag %}{% set safe_u = options.banner_text %}{% endif %}<a href="{{ safe_u }}">x</a>"#,
        ),
        (
            "conditional_attr_splitting",
            r#"<a data-x="{% if options.flag %}a" href="{{ options.banner_text }}"{% else %}b"{% endif %}>x</a>"#,
        ),
        ("closing_link_void_tag", r#"</link><p>x</p>"#),
        ("closing_meta_void_tag", r#"</meta><p>x</p>"#),
        (
            "stylesheet_link_with_query_bypass",
            r#"<link rel="stylesheet" href="{{ theme_assets['assets/theme.css'] }}?v=1">"#,
        ),
        (
            "stylesheet_link_with_fragment_bypass",
            r#"<link rel="stylesheet" href="{{ theme_assets['assets/theme.css'] }}#frag">"#,
        ),
        (
            "stylesheet_link_with_whitespace_bypass",
            r#"<link rel="stylesheet" href=" {{ theme_assets['assets/theme.css'] }}">"#,
        ),
        ("doctype_typo_bogus_comment", r#"<!doctypex>"#),
        ("cdata_section_bogus", r#"<!CDATA[alert(1)]]>"#),
        ("style_end_tag_space", r#"<style>body{}</style >"#),
    ];

    for (name, html_payload) in vectors {
        let zip = build_zip_from_entries(&[
            ("theme.json", base_manifest.as_bytes()),
            ("assets/theme.css", b"body { color: #000; }"),
            ("index.html", html_payload.as_bytes()),
        ]);
        let res = upload_theme_zip(&http, &site, &admin, zip).await;
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "anti-bypass vector `{name}` MUST be rejected with 400 (got {})",
            res.status()
        );
    }

    // Also verify runtime C-SEC bounds (`batch(0)` & `Bool | format_number`) fail cleanly on render
    for (name, version, tpl) in [
        (
            "batch_zero_oom",
            "1.0.0",
            r#"<main>{{ entries | batch(0) }}{{ od_content() }}</main>"#,
        ),
        (
            "bool_format_number",
            "1.0.1",
            r#"<main>{{ options.flag | format_number }}{{ od_content() }}</main>"#,
        ),
    ] {
        let mut manifest: serde_json::Value =
            serde_json::from_str(&base_manifest).expect("valid base_manifest json");
        manifest["version"] = json!(version);
        let manifest_bytes = manifest.to_string();
        let zip = build_zip_from_entries(&[
            ("theme.json", manifest_bytes.as_bytes()),
            ("assets/theme.css", b"body { color: #000; }"),
            ("index.html", tpl.as_bytes()),
        ]);
        let up = upload_theme_zip(&http, &site, &admin, zip).await;
        assert_eq!(up.status(), StatusCode::CREATED, "upload valid AST {name}");
        let rev = up.json::<Value>().await.unwrap()["revision_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let st = admin_json(
            &http,
            &site,
            &admin,
            Method::GET,
            "/api/admin/themes",
            None,
            None,
            StatusCode::OK,
        )
        .await;
        let ver = st["selection"]["version"].as_i64().unwrap();
        admin_json(
            &http,
            &site,
            &admin,
            Method::PUT,
            "/api/admin/themes/active",
            Some(&format!("\"v{ver}\"")),
            Some(json!({ "revision_id": rev })),
            StatusCode::OK,
        )
        .await;
        let prev = admin_req(
            &http,
            &site,
            &admin,
            Method::POST,
            "/api/admin/studio/preview",
            None,
            Some(json!({ "target": { "kind": "index" } })),
        )
        .await;
        assert!(
            !prev.status().is_success(),
            "runtime guard `{name}` must fail render cleanly without panic, got {}",
            prev.status()
        );
        // Server must stay healthy after runtime template error
        assert_eq!(
            http.get(format!("{}/api/health", site.base))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
}

#[test]
fn cli_check_theme_validates_starter_zips_and_error_cases() {
    let bin = env!("CARGO_BIN_EXE_opendash");
    let starter_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtin_themes/opendash-starter");

    // 1. `--check-theme <dir>`
    let out_space = Command::new(bin)
        .arg("--check-theme")
        .arg(&starter_dir)
        .output()
        .unwrap();
    assert_eq!(out_space.status.code(), Some(0));
    let parsed_space: Value = serde_json::from_slice(&out_space.stdout).unwrap();
    assert_eq!(parsed_space["ok"], true);
    assert_eq!(parsed_space["theme_id"], "opendash-starter");
    assert!(parsed_space["options_count"].as_u64().unwrap() > 0);
    assert!(parsed_space["templates"].is_array());
    assert!(parsed_space["parts"].is_array());
    assert!(parsed_space["page_layouts"].is_array());
    assert!(parsed_space["patterns"].is_array());
    assert!(parsed_space["blocks"].is_array());
    assert!(parsed_space["assets"].is_array());

    // 2. `--check-theme=<dir>`
    let mut eq_arg = std::ffi::OsString::from("--check-theme=");
    eq_arg.push(starter_dir.as_os_str());
    let out_eq = Command::new(bin).arg(eq_arg).output().unwrap();
    assert_eq!(out_eq.status.code(), Some(0));
    let parsed_eq: Value = serde_json::from_slice(&out_eq.stdout).unwrap();
    assert_eq!(parsed_eq, parsed_space);

    // 3. Valid ZIP archive (both .zip and non-.zip extension)
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("opendash-cli-test-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&tmp).unwrap();
    let zip_bytes = build_ergo_theme_zip("1.0.0", false);
    let zip_path = tmp.join("valid.zip");
    let noext_path = tmp.join("valid-archive");
    fs::write(&zip_path, &zip_bytes).unwrap();
    fs::write(&noext_path, &zip_bytes).unwrap();
    for p in [&zip_path, &noext_path] {
        let out = Command::new(bin)
            .arg("--check-theme")
            .arg(p)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        let j: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["theme_id"], "theme-ergo-v3");
    }

    // 4. Error invocations -> exit 1 + JSON { ok: false, error: ... }
    let missing_dir = tmp.join("does-not-exist");
    let bad_dir = tmp.join("bad-hidden");
    fs::create_dir_all(&bad_dir).unwrap();
    fs::write(
        bad_dir.join("theme.json"),
        br#"{"manifest_version":1,"theme_id":"bad-h","name":"Bad","version":"1.0.0"}"#,
    )
    .unwrap();
    fs::write(bad_dir.join(".secret"), b"nope").unwrap();

    for args in [
        vec!["--check-theme".to_owned()],
        vec!["--check-theme=".to_owned()],
        vec![
            "--check-theme".to_owned(),
            starter_dir.to_string_lossy().into_owned(),
            "--extra".to_owned(),
        ],
        vec![
            "--check-theme".to_owned(),
            missing_dir.to_string_lossy().into_owned(),
        ],
        vec![
            "--check-theme".to_owned(),
            bad_dir.to_string_lossy().into_owned(),
        ],
    ] {
        let out = Command::new(bin).args(&args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(1),
            "expected exit 1 for args {args:?}, stdout={}",
            String::from_utf8_lossy(&out.stdout)
        );
        let j: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(j["ok"], false);
        assert!(!j["error"].as_str().unwrap_or("").is_empty());
    }

    let _ = fs::remove_dir_all(&tmp);
}
