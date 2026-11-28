use std::{
    fs,
    io::{Cursor, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TestServer {
    child: Child,
    site_root: PathBuf,
    base_url: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!(
        "opendash-m5-s04-t02-{}-{nanos}",
        std::process::id()
    ));
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
    for _ in 0..80 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    panic!("opendash server did not become healthy in time");
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

fn tiny_png(seed: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([seed, 120, 200, 255])));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
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
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let json: Value = resp.json().await.unwrap();
    let csrf = json["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

async fn upload_media(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    seed: u8,
) -> String {
    let resp = client
        .post(format!("{base_url}/api/admin/media"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base_url)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .body(tiny_png(seed))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn seed_builder_homepage(client: &Client, base_url: &str, cookie: &str, csrf: &str) -> i64 {
    let types: Vec<Value> = client
        .get(format!("{base_url}/api/admin/content-types"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let schema_token = types
        .iter()
        .find(|v| v["descriptor"]["key"] == "page")
        .unwrap()["schema_token"]
        .as_str()
        .unwrap();
    let created = client
        .post(format!(
            "{base_url}/api/admin/content-types/page/entries/composite"
        ))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base_url)
        .header("X-CSRF-Token", csrf)
        .json(&json!({
            "expected_schema_token": schema_token,
            "slug": "s02-home-fixture",
            "title": "S02 Home Fixture",
            "values": { "body": "S02 Body Content" },
            "publication_status": "published"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let page_id = created.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();

    let reading = client
        .get(format!("{base_url}/api/admin/settings/reading"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    let etag = reading.headers()[header::ETAG].to_str().unwrap().to_owned();
    let assigned = client
        .put(format!("{base_url}/api/admin/settings/reading"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base_url)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({ "homepage_page_id": page_id, "posts_page_id": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(assigned.status(), StatusCode::OK);
    page_id
}

fn etag_version(resp: &reqwest::Response) -> String {
    resp.headers()[header::ETAG].to_str().unwrap().to_owned()
}

#[tokio::test]
async fn m5_s04_t02_unified_options_richtext_icon_recursive_urls_studio_and_l10n_media() {
    let srv = spawn_server().await;
    let client = Client::new();

    let setup = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (cookie, csrf) = login(&client, &srv.base_url, "admin", "AdminPassword123!").await;

    // 1. Negative C-SEC checks on install (site_options shadowing, non-URL subfield href/src, unknown literal icon)
    let hero_group_manifest = json!({
        "manifest_version": 1,
        "theme_id": "bad-s02-csec",
        "name": "Bad CSEC",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "options": [
            {
                "key": "hero",
                "label": "Hero",
                "type": "group",
                "public": true,
                "fields": [
                    { "key": "caption", "label": "Caption", "type": "text" },
                    { "key": "bg", "label": "BG", "type": "media" },
                    { "key": "cta", "label": "CTA", "type": "link" }
                ]
            }
        ]
    })
    .to_string();

    for (label, bad_home) in [
        (
            "shadow_site_options",
            "{% set site_options = {'social_facebook': '/evil'} %}<main><a href=\"{{ site_options.social_facebook }}\">X</a>{{ od_content() }}</main>",
        ),
        (
            "text_subfield_in_href",
            "<main><a href=\"{{ options.hero.caption }}\">Bad</a>{{ od_content() }}</main>",
        ),
        (
            "text_subfield_in_src",
            "<main><img src=\"{{ options.hero.caption }}\" alt=\"Bad\">{{ od_content() }}</main>",
        ),
        (
            "unknown_literal_icon",
            "<main>{{ od_icon(\"does-not-exist\", \"w-5 h-5\") }}{{ od_content() }}</main>",
        ),
    ] {
        let resp = client
            .post(format!("{}/api/admin/themes", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header(header::CONTENT_TYPE, "application/zip")
            .body(build_zip(&[
                ("theme.json", hero_group_manifest.as_bytes()),
                ("index.html", bad_home.as_bytes()),
            ]))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "expected C-SEC 400 rejection for {label}"
        );
    }

    // 2. Upload 5 distinct images (initially private -> 404 publicly)
    let site_logo_img = upload_media(&client, &srv.base_url, &cookie, &csrf, 11).await;
    let vi_hero_img = upload_media(&client, &srv.base_url, &cookie, &csrf, 22).await;
    let en_hero_img = upload_media(&client, &srv.base_url, &cookie, &csrf, 33).await;
    let en_slide_img = upload_media(&client, &srv.base_url, &cookie, &csrf, 44).await;
    let en_gal_img = upload_media(&client, &srv.base_url, &cookie, &csrf, 55).await;
    for mid in [
        &site_logo_img,
        &vi_hero_img,
        &en_hero_img,
        &en_slide_img,
        &en_gal_img,
    ] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    // 3. Configure global site_options (site_logo + social_facebook)
    let site_opts_get = client
        .get(format!("{}/api/admin/settings/site-options", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(site_opts_get.status(), StatusCode::OK);
    let site_opts_etag = etag_version(&site_opts_get);
    let site_opts_put = client
        .put(format!("{}/api/admin/settings/site-options", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &site_opts_etag)
        .json(&json!({
            "values": {
                "site_logo": site_logo_img,
                "social_facebook": "https://facebook.com/opendash"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(site_opts_put.status(), StatusCode::OK);
    assert_eq!(
        client
            .get(format!("{}/media/{site_logo_img}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // 4. Install & activate S02 Unified Options theme
    let theme_manifest = json!({
        "manifest_version": 1,
        "theme_id": "unified-s02-theme",
        "name": "Unified S02 Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "options": [
            { "key": "tab_main", "label": "Main", "type": "tab" },
            { "key": "acc_hero", "label": "Hero Section", "type": "accordion", "open_by_default": true },
            { "key": "hint_msg", "label": "Hint", "type": "message", "body": "Only safe links allowed" },
            { "key": "hero_bio", "label": "Bio", "type": "richtext", "public": true },
            { "key": "plain_note", "label": "Plain Note", "type": "text", "public": true },
            { "key": "feature_icon", "label": "Feature Icon", "type": "icon", "default": "star", "public": true },
            { "key": "optional_icon", "label": "Optional Icon", "type": "icon", "public": true },
            {
                "key": "hero",
                "label": "Hero",
                "type": "group",
                "public": true,
                "translatable": true,
                "fields": [
                    { "key": "caption", "label": "Caption", "type": "text" },
                    { "key": "bg", "label": "Background", "type": "media" },
                    { "key": "cta", "label": "CTA", "type": "link" }
                ]
            },
            {
                "key": "slides",
                "label": "Slides",
                "type": "repeater",
                "public": true,
                "translatable": true,
                "fields": [
                    { "key": "photo", "label": "Photo", "type": "media" },
                    { "key": "cta", "label": "CTA", "type": "link" },
                    { "key": "caption", "label": "Caption", "type": "text" }
                ]
            },
            {
                "key": "gallery_imgs",
                "label": "Gallery",
                "type": "gallery",
                "public": true,
                "translatable": true
            }
        ]
    })
    .to_string();

    let home_html = br#"<main>
      {% if site_options.site_logo %}<img data-site-logo src="{{ site_options.site_logo.url }}" alt="SiteLogo">{% endif %}
      {% if site_options.social_facebook %}<a data-fb href="{{ site_options.social_facebook }}">FB</a>{% endif %}
      <div data-rt>{{ options.hero_bio }}</div>
      <div data-plain>{{ options.plain_note }}</div>
      <div data-ico>{% if options.feature_icon %}{{ od_icon(options.feature_icon.name, "w-5 h-5") }}{% endif %}</div>
      <div data-empty-ico>{% if options.optional_icon %}{{ od_icon(options.optional_icon.name, "w-5 h-5") }}{% endif %}</div>
      {% if options.hero.bg %}<img data-hero-bg src="{{ options.hero.bg.url }}" alt="{{ options.hero.caption }}">{% endif %}
      {% if options.hero.cta %}<a data-hero-cta href="{{ options.hero.cta.url }}">{{ options.hero.cta.label }}</a>{% endif %}
      {% for item in options.slides %}
        <div data-slide>
          {% if item.photo %}<img src="{{ item.photo.url }}" alt="{{ item.caption }}">{% endif %}
          {% if item.cta %}<a href="{{ item.cta.url }}">{{ item.cta.label }}</a>{% endif %}
        </div>
      {% endfor %}
      {% for g in options.gallery_imgs %}
        <img data-gal src="{{ g.url }}" alt="G">
      {% endfor %}
      {{ od_content() }}
    </main>"#;

    let inst = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_zip(&[
            ("theme.json", theme_manifest.as_bytes()),
            ("index.html", home_html),
        ]))
        .send()
        .await
        .unwrap();
    let inst_status = inst.status();
    let inst_body = inst.text().await.unwrap();
    assert_eq!(
        inst_status,
        StatusCode::CREATED,
        "install failed: {inst_body}"
    );
    let rev_s02 = serde_json::from_str::<Value>(&inst_body).unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let act = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "revision_id": rev_s02 }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), StatusCode::OK);
    let vi_page_id = seed_builder_homepage(&client, &srv.base_url, &cookie, &csrf).await;

    // 5. Save default-locale options (richtext + plain XSS + icon + group media/link)
    let save_vi = client
        .put(format!(
            "{}/api/themes/unified-s02-theme/options",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "values": {
                "hero_bio": "<p>Hi <strong>Safe</strong></p><script>alert(1)</script><a href=\"https://example.com/safe\">Ext</a>",
                "plain_note": "<b>Escaped</b>",
                "feature_icon": "home",
                "hero": {
                    "caption": "VI Hero",
                    "bg": vi_hero_img,
                    "cta": { "url": "/vi-go", "label": "VI Go", "new_tab": false, "nofollow": false }
                }
            }
        }))
        .send()
        .await
        .unwrap();
    let save_vi_status = save_vi.status();
    let save_vi_text = save_vi.text().await.unwrap();
    assert_eq!(
        save_vi_status,
        StatusCode::OK,
        "save vi failed: {save_vi_text}"
    );

    assert_eq!(
        client
            .get(format!("{}/media/{vi_hero_img}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let vi_home_html = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        vi_home_html.contains("<strong>Safe</strong>"),
        "{vi_home_html}"
    );
    assert!(
        vi_home_html.contains("https://example.com/safe"),
        "{vi_home_html}"
    );
    assert!(vi_home_html.contains("noopener"), "{vi_home_html}");
    assert!(
        !vi_home_html.contains("<script>alert(1)</script>"),
        "{vi_home_html}"
    );
    assert!(!vi_home_html.contains("<b>Escaped</b>"), "{vi_home_html}");
    assert!(
        vi_home_html.contains("&lt;b&gt;Escaped&lt;"),
        "{vi_home_html}"
    );
    assert!(vi_home_html.contains("<svg"), "{vi_home_html}");
    assert!(vi_home_html.contains("w-5 h-5"), "{vi_home_html}");
    assert!(
        vi_home_html.contains("<div data-empty-ico></div>"),
        "{vi_home_html}"
    );
    let decoded_vi = vi_home_html.replace("&#x2f;", "/").replace("&#x2F;", "/");
    assert!(decoded_vi.contains(&format!("/media/{site_logo_img}/file")));
    assert!(decoded_vi.contains("https://facebook.com/opendash"));
    assert!(decoded_vi.contains(&format!("/media/{vi_hero_img}/file")));
    assert!(decoded_vi.contains("/vi-go"));

    // 6. Enable multilingual + create `en`, then save translatable media/gallery/group/repeater in `en`
    let loc_get = client
        .get(format!("{}/api/admin/locales", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let loc_etag = etag_version(&loc_get);
    let en_mul = client
        .put(format!("{}/api/admin/locales/multilingual", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(en_mul.status(), StatusCode::OK);
    let loc_etag2 = etag_version(&en_mul);
    let add_en = client
        .post(format!("{}/api/admin/locales", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag2)
        .json(&json!({ "code": "en", "label": "English", "direction": "ltr" }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_en.status(), StatusCode::CREATED);

    let tr_en = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{vi_page_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "target_locale": "en", "slug": "s02-home-en", "title": "S02 Home EN" }))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_en.status(), StatusCode::CREATED);
    let en_page_id = tr_en.json::<Value>().await.unwrap()["id"].as_i64().unwrap();
    let pub_en = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{en_page_id}/publication",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_en.status(), StatusCode::OK);
    let en_read_get = client
        .get(format!(
            "{}/api/admin/settings/reading?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(en_read_get.status(), StatusCode::OK);
    let en_read_put = client
        .put(format!(
            "{}/api/admin/settings/reading?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag_version(&en_read_get))
        .json(&json!({ "homepage_page_id": en_page_id, "posts_page_id": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(en_read_put.status(), StatusCode::OK);

    let save_en = client
        .put(format!(
            "{}/api/themes/unified-s02-theme/options?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "values": {
                "hero": {
                    "caption": "EN Hero",
                    "bg": en_hero_img,
                    "cta": { "url": "/en-go", "label": "EN Go", "new_tab": false, "nofollow": false }
                },
                "slides": [
                    {
                        "photo": en_slide_img,
                        "cta": { "url": "/en-slide", "label": "Slide Link", "new_tab": false, "nofollow": false },
                        "caption": "Slide 1"
                    }
                ],
                "gallery_imgs": [en_gal_img]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_en.status(), StatusCode::OK);

    for mid in [&vi_hero_img, &en_hero_img, &en_slide_img, &en_gal_img] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK,
            "media {mid} must be publicly accessible"
        );
    }

    let en_home_html = client
        .get(format!("{}/en", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let decoded_en = en_home_html.replace("&#x2f;", "/").replace("&#x2F;", "/");
    assert!(decoded_en.contains("EN Go"), "{en_home_html}");
    assert!(decoded_en.contains("Slide Link"), "{en_home_html}");
    assert!(decoded_en.contains(&format!("/media/{en_hero_img}/file")));
    assert!(decoded_en.contains(&format!("/media/{en_slide_img}/file")));
    assert!(decoded_en.contains(&format!("/media/{en_gal_img}/file")));
    assert!(decoded_en.contains("<strong>Safe</strong>"));

    // 7. Studio Preview with transient RichText + dynamic icon + preview_locale = "en"
    let preview = client
        .post(format!("{}/api/admin/studio/preview", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "index" },
            "preview_annotations": false,
            "preview_locale": "en",
            "transient": {
                "theme_options": {
                    "hero_bio": "<p><em>PreviewRT</em><img src=x onerror=alert(1)></p>",
                    "feature_icon": "arrow-right"
                }
            }
        }))
        .send()
        .await
        .unwrap();
    let preview_status = preview.status();
    let preview_raw = preview.text().await.unwrap();
    assert_eq!(
        preview_status,
        StatusCode::OK,
        "preview failed: {preview_raw}"
    );
    let preview_html = serde_json::from_str::<Value>(&preview_raw).unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        preview_html.contains("<em>PreviewRT</em>"),
        "{preview_html}"
    );
    assert!(!preview_html.contains("onerror"), "{preview_html}");
    assert!(preview_html.contains("<svg"), "{preview_html}");
    assert!(preview_html.contains("EN Go"), "{preview_html}");

    // 8. Theme switch -> all unified-s02-theme media ACLs revoked (404), site_logo remains 200; switch back -> 200
    let inst_blank = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_zip(&[
            (
                "theme.json",
                br#"{"manifest_version":1,"theme_id":"blank-s02-theme","name":"Blank","version":"1.0.0","templates":[{"kind":"index","file":"index.html"}]}"#,
            ),
            ("index.html", b"<main>Blank{{ od_content() }}</main>"),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(inst_blank.status(), StatusCode::CREATED);
    let rev_blank = inst_blank.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({ "revision_id": rev_blank }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{}/media/{site_logo_img}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    for mid in [&vi_hero_img, &en_hero_img, &en_slide_img, &en_gal_img] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base_url))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &srv.base_url)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v3\"")
            .json(&json!({ "revision_id": rev_s02 }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // 9. Soft-delete `en` then purge orphan `en` -> revokes en media ACL while keeping default `vi_hero_img` & `site_logo_img`
    let loc_now = client
        .get(format!("{}/api/admin/locales", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let del_en = client
        .delete(format!("{}/api/admin/locales/en", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag_version(&loc_now))
        .send()
        .await
        .unwrap();
    assert_eq!(del_en.status(), StatusCode::OK);

    let orphans = client
        .get(format!("{}/api/admin/locales/orphans", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(orphans.status(), StatusCode::OK);
    let purge_en = client
        .post(format!(
            "{}/api/admin/locales/orphans/en/purge",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag_version(&orphans))
        .json(&json!({ "confirm_code": "en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(purge_en.status(), StatusCode::OK);

    for mid in [&en_hero_img, &en_slide_img, &en_gal_img] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "purged locale media {mid} must return 404"
        );
    }
    for mid in [&vi_hero_img, &site_logo_img] {
        assert_eq!(
            client
                .get(format!("{}/media/{mid}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK,
            "default locale/global media {mid} must remain 200"
        );
    }
}
