//! S01: Core locale registry, migration 0047 rebuild, overlays & single-locale parity.
use std::{
    fs,
    io::{Cursor, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct IsolatedRoot(PathBuf);

impl IsolatedRoot {
    fn new(label: &str) -> Self {
        let temp = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = temp.join(format!(
            "opendash-s01-{label}-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create isolated root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for IsolatedRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningServer {
    child: Child,
    base_url: String,
    site_root: PathBuf,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .env_remove("OPENDASH_PUBLIC_ORIGIN")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash");
        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..120 {
            if client
                .get(format!("{base_url}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self {
                    child,
                    base_url,
                    site_root: site_root.to_path_buf(),
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server did not become healthy at {base_url}");
    }

    async fn db(&self) -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(self.site_root.join("database/opendash.sqlite"))
                    .read_only(true),
            )
            .await
            .unwrap()
    }
}

async fn setup_and_login(
    client: &Client,
    base: &str,
    username: &str,
    password: &str,
    do_setup: bool,
) -> (String, String) {
    if do_setup {
        let r = client
            .post(format!("{base}/auth/setup"))
            .json(&json!({ "username": username, "password": password }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
    }
    let login = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

fn tiny_png(seed: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([seed, 120, 200, 255])));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

fn build_theme_zip(id: &str, version: &str, options: Value) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 3,
            "theme_id": id,
            "name": id,
            "version": version,
            "templates": [
                { "kind": "index", "file": "index.html" },
                { "kind": "home", "file": "home.html" }
            ],
            "parts": { "header": "header.html" },
            "menus": { "slots": ["header"] },
            "options": options
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("header.html", opts).unwrap();
        zip.write_all(b"<header><nav>{% if menus.header is defined %}{% for item in menus.header.items %}<a href=\"{{ item.url }}\">{{ item.label }}</a>{% endfor %}{% endif %}</nav></header>").unwrap();
        zip.start_file("home.html", opts).unwrap();
        zip.write_all(b"<main>{{ options.hero_title }}{{ od_content() }}</main>")
            .unwrap();
        zip.start_file("index.html", opts).unwrap();
        zip.write_all(b"<main>{{ options.hero_title }}</main>")
            .unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn etag_version(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .expect("ETag header present")
        .to_owned()
}

#[test]
fn core_reserved_entry_keys_and_theme_default_locale_validation() {
    assert!(
        opendash_core::content::RESERVED_ENTRY_KEYS.contains(&"locale"),
        "locale must be reserved entry field key"
    );
    assert!(
        opendash_core::content::RESERVED_ENTRY_KEYS.contains(&"translation_group_id"),
        "translation_group_id must be reserved entry field key"
    );
    assert!(opendash_core::id::valid_lowercase_locale_code("vi"));
    assert!(opendash_core::id::valid_lowercase_locale_code("en-us"));
    assert!(!opendash_core::id::valid_lowercase_locale_code("en-US"));
    assert!(!opendash_core::id::valid_lowercase_locale_code("x"));

    for (raw_locale, expect_ok) in [
        (None, true),
        (Some("vi"), true),
        (Some("en-us"), true),
        (Some("en-US"), false),
        (Some("invalid_locale"), false),
    ] {
        let mut manifest_val = json!({
            "manifest_version": 3,
            "theme_id": "core-locale-theme",
            "name": "Core Locale Theme",
            "version": "1.0.0",
            "templates": [{ "kind": "index", "file": "index.html" }]
        });
        if let Some(loc) = raw_locale {
            manifest_val["default_locale"] = json!(loc);
        }
        let parsed: opendash_core::theme::ThemeV3Manifest =
            serde_json::from_value(manifest_val).unwrap();
        assert_eq!(
            parsed.validate().is_ok(),
            expect_ok,
            "default_locale={raw_locale:?}"
        );
    }
}

#[tokio::test]
async fn migration_0047_fresh_and_populated_preserves_data_sequences_and_triggers() {
    // 1. Fresh DB migration budget & integrity.
    let fresh = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = sqlx::migrate!("../../migrations");
    migrator.run(&fresh).await.unwrap();

    let schema_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&fresh)
        .await
        .unwrap();
    assert!(
        schema_count < 256,
        "sqlite_schema object budget exceeded: {schema_count}"
    );
    let fk_violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&fresh)
        .await
        .unwrap();
    assert!(fk_violations.is_empty(), "fk violations: {fk_violations:?}");
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&fresh)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");

    // 2. Populated 0001..=0046 DB upgrade.
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrator.run_to(46, &db).await.unwrap();

    let media_id = "a".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES(?, 'image', 'home.png', 'image/png', 'png', 100, 10, 10, 10, 10)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();

    let parent_cat: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind, name, slug) VALUES('category', 'News', 'news') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let child_cat: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind, name, slug, parent_id) VALUES('category', 'Tech', 'tech', ?) RETURNING id",
    )
    .bind(parent_cat)
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO term_extension_values(term_id, owner_kind, owner_id, local_key, values_json)
         VALUES(?, 'theme', 'theme-ext', 'badge', '{\"color\":\"blue\"}')",
    )
    .bind(parent_cat)
    .execute(&db)
    .await
    .unwrap();

    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, primary_category_term_id)
         VALUES('post', 'hello-post', 'Hello', '{}', 'draft', ?) RETURNING id",
    )
    .bind(child_cat)
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(post_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_terms(entry_id, term_id) VALUES(?, ?)")
        .bind(post_id)
        .bind(child_cat)
        .execute(&db)
        .await
        .unwrap();

    let page_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status)
         VALUES('page', 'about-us', 'About Us', '{}', 'draft') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_public_paths(entry_id, path) VALUES(?, '/custom-about')")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_redirects(source_path, entry_id) VALUES('/old-about', ?)")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();

    let pattern_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode)
         VALUES('pattern', 'hero-pat', 'Hero', '{}', 'draft', 'builder') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO entry_pattern_refs(source_entry_id, target_pattern_id) VALUES(?, ?)")
        .bind(page_id)
        .bind(pattern_id)
        .execute(&db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT OR REPLACE INTO site_home_seo(id, title, description, og_image_id, canonical_url, noindex, version)
         VALUES(1, 'Home Title', 'Desc', ?, NULL, 0, 3)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR REPLACE INTO seo_public_media_refs(source_kind, source_id, media_id)
         VALUES('home_seo', '1', ?)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE site_reading_settings SET homepage_page_id = ? WHERE id = 1")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();

    let menu_id: i64 = sqlx::query_scalar(
        "INSERT INTO site_menus(menu_key, title, version) VALUES('primary', 'Primary', 1) RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO site_menu_slot_assignments(theme_id, slot_key, menu_id) VALUES('starter', 'header', ?)",
    )
    .bind(menu_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO site_menu_slot_versions(theme_id, version) VALUES('starter', 4)")
        .execute(&db)
        .await
        .unwrap();

    let entry_seq_before: i64 =
        sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name='content_entries'")
            .fetch_one(&db)
            .await
            .unwrap();
    let term_seq_before: i64 =
        sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name='taxonomy_terms'")
            .fetch_one(&db)
            .await
            .unwrap();

    migrator.run(&db).await.unwrap();

    let pop_schema: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(pop_schema < 256, "pop_schema={pop_schema}");
    let pop_fk: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(pop_fk.is_empty(), "fk check after 0047: {pop_fk:?}");
    let pop_int: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(pop_int, "ok");

    let home_seo_refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM seo_public_media_refs WHERE source_kind='home_seo'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        home_seo_refs, 0,
        "0047 must purge obsolete home_seo media ref rows"
    );

    let (post_loc, post_grp, post_pcat): (String, String, Option<i64>) = sqlx::query_as(
        "SELECT locale, translation_group_id, primary_category_term_id FROM content_entries WHERE id=?",
    )
    .bind(post_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(post_loc, "vi");
    assert_eq!(post_grp, format!("eg-{post_id}"));
    assert_eq!(post_pcat, Some(child_cat));

    let (term_loc, term_grp, term_parent): (String, String, Option<i64>) = sqlx::query_as(
        "SELECT locale, translation_group_id, parent_id FROM taxonomy_terms WHERE id=?",
    )
    .bind(child_cat)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(term_loc, "vi");
    assert_eq!(term_grp, format!("tg-{child_cat}"));
    assert_eq!(term_parent, Some(parent_cat));

    let ext_val: String = sqlx::query_scalar(
        "SELECT values_json FROM term_extension_values WHERE term_id=? AND owner_kind='theme'",
    )
    .bind(parent_cat)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(ext_val, r#"{"color":"blue"}"#);

    let (home_seo_loc, home_seo_og, home_seo_ver): (String, Option<String>, i64) =
        sqlx::query_as("SELECT locale, og_image_id, version FROM site_home_seo")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(
        (home_seo_loc.as_str(), home_seo_og.as_deref(), home_seo_ver),
        ("vi", Some(media_id.as_str()), 3)
    );

    let (reading_loc, reading_home): (String, Option<i64>) =
        sqlx::query_as("SELECT locale, homepage_page_id FROM site_reading_settings")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!((reading_loc.as_str(), reading_home), ("vi", Some(page_id)));

    let slot_loc: String = sqlx::query_scalar(
        "SELECT locale FROM site_menu_slot_assignments WHERE theme_id='starter'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(slot_loc, "vi");

    let pat_target: i64 = sqlx::query_scalar(
        "SELECT target_pattern_id FROM entry_pattern_refs WHERE source_entry_id=?",
    )
    .bind(page_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(pat_target, pattern_id);

    // Check preserved autoincrement sequence
    let next_entry: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES('page', 'after-0047', 'After', '{}', 'draft', 'vi', 'eg-after') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(next_entry > entry_seq_before);
    let next_term: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind, name, slug, locale, translation_group_id)
         VALUES('tag', 'After', 'after-tag', 'vi', 'tg-after') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(next_term > term_seq_before);

    // 3. DB Locale Integrity Triggers (second locale 'en')
    sqlx::query(
        "INSERT INTO site_locales(code, bcp47_tag, label, direction, is_default, enabled, position)
         VALUES('en', 'en', 'English', 'ltr', 0, 1, 1)",
    )
    .execute(&db)
    .await
    .unwrap();

    // taxonomy_terms_parent_insert: cross-locale parent aborts
    assert!(
        sqlx::query(
            "INSERT INTO taxonomy_terms(kind, name, slug, parent_id, locale, translation_group_id)
             VALUES('category', 'EN Child', 'en-child', ?, 'en', 'tg-en-c')",
        )
        .bind(parent_cat)
        .execute(&db)
        .await
        .is_err()
    );

    // content_entries_primary_cat_locale_insert: cross-locale primary_category_term_id aborts
    assert!(
        sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id, primary_category_term_id)
             VALUES('post', 'en-bad-pcat', 'EN', '{}', 'draft', 'en', 'eg-en-1', ?)",
        )
        .bind(child_cat)
        .execute(&db)
        .await
        .is_err()
    );

    let en_post_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES('post', 'en-good', 'EN Good', '{}', 'draft', 'en', 'eg-en-2') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();

    // content_entries_primary_cat_locale_update
    assert!(
        sqlx::query("UPDATE content_entries SET primary_category_term_id = ? WHERE id = ?")
            .bind(child_cat)
            .bind(en_post_id)
            .execute(&db)
            .await
            .is_err()
    );

    // entry_terms_locale_insert
    assert!(
        sqlx::query("INSERT INTO entry_terms(entry_id, term_id) VALUES(?, ?)")
            .bind(en_post_id)
            .bind(child_cat)
            .execute(&db)
            .await
            .is_err()
    );

    // entry_public_paths_locale_insert
    assert!(
        sqlx::query(
            "INSERT INTO entry_public_paths(entry_id, locale, path) VALUES(?, 'vi', '/en-bad-path')"
        )
        .bind(en_post_id)
        .execute(&db)
        .await
        .is_err()
    );

    // entry_redirects_locale_insert
    assert!(
        sqlx::query("INSERT INTO entry_redirects(locale, source_path, entry_id) VALUES('vi', '/en-bad-redir', ?)")
            .bind(en_post_id)
            .execute(&db)
            .await
            .is_err()
    );

    // content_entries_locale_guard & taxonomy_terms_locale_guard
    assert!(
        sqlx::query("UPDATE content_entries SET locale = 'en' WHERE id = ?")
            .bind(post_id)
            .execute(&db)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE taxonomy_terms SET locale = 'en' WHERE id = ?")
            .bind(child_cat)
            .execute(&db)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn locale_registry_single_locale_rename_overlays_and_theme_options_e2e() {
    let root = IsolatedRoot::new("e2e");
    let srv = RunningServer::start(root.path()).await;
    let client = Client::new();
    let origin = srv.base_url.clone();

    // Unauthenticated locales request -> 401
    assert_eq!(
        client
            .get(format!("{origin}/api/admin/locales"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let (admin_cookie, admin_csrf) =
        setup_and_login(&client, &origin, "admin", "AdminPass123!", true).await;

    let ed_resp = client
        .post(format!("{origin}/api/admin/users"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor", "password": "EditorPass123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_resp.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) =
        setup_and_login(&client, &origin, "editor", "EditorPass123!", false).await;

    // Editor can GET /api/admin/locales (200) but mutations -> 403
    assert_eq!(
        client
            .get(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &ed_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &ed_cookie)
            .header("X-CSRF-Token", &ed_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "code": "en", "label": "English" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .put(format!("{origin}/api/admin/locales/vi"))
            .header(header::COOKIE, &ed_cookie)
            .header("X-CSRF-Token", &ed_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "label": "VI", "direction": "ltr", "enabled": true, "position": 0 }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales/vi/set-default"))
            .header(header::COOKIE, &ed_cookie)
            .header("X-CSRF-Token", &ed_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // Seed initial default-locale data ('vi') across entries, terms, general, reading, home_seo, menus, menu-slots
    let en_page_resp = client
        .post(format!("{origin}/api/admin/content-types/page/entries"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "title": "Init Page", "slug": "init-page", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(en_page_resp.status(), StatusCode::CREATED);
    let en_page_id = en_page_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let pub_page = client
        .post(format!(
            "{origin}/api/admin/content-types/page/entries/{en_page_id}/publication"
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_page.status(), StatusCode::OK);

    let term_resp = client
        .post(format!("{origin}/api/admin/taxonomy/terms"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "kind": "category", "name": "Init Cat", "slug": "init-cat" }))
        .send()
        .await
        .unwrap();
    assert_eq!(term_resp.status(), StatusCode::CREATED);

    let menu_resp = client
        .post(format!("{origin}/api/admin/menus"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "menu_key": "main-nav", "title": "Main Nav" }))
        .send()
        .await
        .unwrap();
    assert_eq!(menu_resp.status(), StatusCode::CREATED);
    let menu_id = menu_resp.json::<Value>().await.unwrap()["menu"]["id"]
        .as_i64()
        .unwrap();

    // Schema rejects translatable=true on UI-only types and forbids secret in Options (400)
    for bad_type in ["tab", "accordion", "message"] {
        let def = match bad_type {
            "tab" => {
                json!({ "key": "bad_opt", "label": "Bad", "type": "tab", "translatable": true })
            }
            "accordion" => {
                json!({ "key": "bad_opt", "label": "Bad", "type": "accordion", "translatable": true })
            }
            _ => {
                json!({ "key": "bad_opt", "label": "Bad", "type": "message", "body": "Info", "translatable": true })
            }
        };
        let zip_bad = build_theme_zip("theme-bad-tr", "1.0.0", json!([def]));
        let r = client
            .post(format!("{origin}/api/admin/themes"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::CONTENT_TYPE, "application/zip")
            .body(zip_bad)
            .send()
            .await
            .unwrap();
        let status = r.status();
        let body = r.text().await.unwrap();
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "bad_type={bad_type}, body={body}"
        );
        assert!(
            body.contains("translatable"),
            "expected translatable error for {bad_type}, got: {body}"
        );
    }

    let r_secret = client
        .post(format!("{origin}/api/admin/themes"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_theme_zip(
            "theme-bad-secret",
            "1.0.0",
            json!([{ "key": "bad_sec", "label": "Bad", "type": "secret", "translatable": true }]),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r_secret.status(), StatusCode::BAD_REQUEST);

    // Unified Field Engine V1: select, media, gallery, group, repeater ARE translatable-capable in Options
    let r_complex_tr = client
        .post(format!("{origin}/api/admin/themes"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_theme_zip(
            "theme-complex-tr",
            "1.0.0",
            json!([
                { "key": "hero_title", "label": "Hero", "type": "text", "default": "H", "public": true, "translatable": true },
                { "key": "tr_sel", "label": "Sel", "type": "select", "default": "a", "options": [{ "value": "a", "label": "A" }], "public": true, "translatable": true },
                { "key": "tr_media", "label": "Media", "type": "media", "public": true, "translatable": true },
                { "key": "tr_gal", "label": "Gal", "type": "gallery", "public": true, "translatable": true },
                { "key": "tr_grp", "label": "Grp", "type": "group", "public": true, "translatable": true, "fields": [{ "key": "sub", "label": "Sub", "type": "text" }] },
                { "key": "tr_rep", "label": "Rep", "type": "repeater", "public": true, "translatable": true, "fields": [{ "key": "cap", "label": "Cap", "type": "text" }] }
            ]),
        ))
        .send()
        .await
        .unwrap();
    let tr_status = r_complex_tr.status();
    let tr_body = r_complex_tr.text().await.unwrap();
    assert_eq!(
        tr_status,
        StatusCode::CREATED,
        "complex translatable failed: {tr_body}"
    );

    // Install & activate theme-s01 with translatable and non-translatable options
    let theme_opts = json!([
        { "key": "hero_title", "label": "Hero", "type": "text", "default": "Default Hero", "public": true, "translatable": true },
        { "key": "cta_url", "label": "CTA URL", "type": "url", "default": "/contact", "public": true, "translatable": true },
        { "key": "accent_hex", "label": "Accent", "type": "text", "default": "#111111", "public": true, "translatable": false }
    ]);
    let zip_v1 = build_theme_zip("theme-s01", "1.0.0", theme_opts);
    let inst_v1 = client
        .post(format!("{origin}/api/admin/themes"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(zip_v1)
        .send()
        .await
        .unwrap();
    let inst_status = inst_v1.status();
    let inst_body_text = inst_v1.text().await.unwrap();
    assert_eq!(
        inst_status,
        StatusCode::CREATED,
        "install body={inst_body_text}"
    );
    let inst_json: Value = serde_json::from_str(&inst_body_text).unwrap();
    let rev_v1 = inst_json["revision_id"].as_str().unwrap().to_owned();
    let act_v1 = client
        .put(format!("{origin}/api/admin/themes/active"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "revision_id": rev_v1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_v1.status(), StatusCode::OK);
    let active_theme_etag = etag_version(&act_v1);

    // Assign default-locale slot header -> menu_id
    let slot_put = client
        .put(format!("{origin}/api/admin/menu-slots/theme-s01"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "assignments": { "header": menu_id } }))
        .send()
        .await
        .unwrap();
    assert_eq!(slot_put.status(), StatusCode::OK);

    // Single-locale PUT /api/admin/settings/public "vi" -> "en-US" cascades ON UPDATE CASCADE
    let pub_get = client
        .get(format!("{origin}/api/admin/settings/public"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    let pub_etag = etag_version(&pub_get);
    let pub_rename = client
        .put(format!("{origin}/api/admin/settings/public"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &pub_etag)
        .json(&json!({
            "public_origin": null,
            "content_language": "en-US",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_rename.status(), StatusCode::OK);
    let renamed_etag = etag_version(&pub_rename);

    {
        let pool = srv.db().await;
        let (code, tag): (String, String) =
            sqlx::query_as("SELECT code, bcp47_tag FROM site_locales WHERE is_default = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((code.as_str(), tag.as_str()), ("en-us", "en-US"));
        let rename_flag: i64 =
            sqlx::query_scalar("SELECT rename_in_progress FROM site_locales_state WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rename_flag, 0);
        let entry_loc: String =
            sqlx::query_scalar("SELECT locale FROM content_entries WHERE id = ?")
                .bind(en_page_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(entry_loc, "en-us");
        let term_loc: String =
            sqlx::query_scalar("SELECT locale FROM taxonomy_terms WHERE slug = 'init-cat'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(term_loc, "en-us");
        let slot_loc: String = sqlx::query_scalar(
            "SELECT locale FROM site_menu_slot_assignments WHERE theme_id = 'theme-s01'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(slot_loc, "en-us");
        let gen_loc: String =
            sqlx::query_scalar("SELECT locale FROM site_general_settings_locales")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(gen_loc, "en-us");
        let read_loc: String = sqlx::query_scalar("SELECT locale FROM site_reading_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(read_loc, "en-us");
        let home_loc: String = sqlx::query_scalar("SELECT locale FROM site_home_seo")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(home_loc, "en-us");
        let claim_loc: String = sqlx::query_scalar(
            "SELECT locale FROM permalink_route_claims WHERE target_kind='entry' AND target_id=?",
        )
        .bind(en_page_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(claim_loc, "en-us");
    }

    // /api/admin/locales validation & Collection CAS guards
    let loc_get = client
        .get(format!("{origin}/api/admin/locales"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    let loc_etag = etag_version(&loc_get);

    // Missing If-Match -> 428
    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({ "code": "vi", "label": "Tiếng Việt" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_REQUIRED
    );
    // Stale If-Match -> 412
    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v999\"")
            .json(&json!({ "code": "vi", "label": "Tiếng Việt" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_FAILED
    );
    // Invalid BCP47 & reserved roots -> 422
    for bad in ["bad_code", "a", "admin", "api", "media"] {
        assert_eq!(
            client
                .post(format!("{origin}/api/admin/locales"))
                .header(header::COOKIE, &admin_cookie)
                .header("X-CSRF-Token", &admin_csrf)
                .header(header::IF_MATCH, &loc_etag)
                .json(&json!({ "code": bad, "label": "Bad" }))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "bad locale code={bad}"
        );
    }

    let enabled = client
        .put(format!("{origin}/api/admin/locales/multilingual"))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let loc_etag = etag_version(&enabled);

    // Create secondary locale "vi" -> 201 Created
    let create_vi = client
        .post(format!("{origin}/api/admin/locales"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({ "code": "vi", "label": "Tiếng Việt", "direction": "ltr" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_vi.status(), StatusCode::CREATED);
    let mut current_loc_etag = etag_version(&create_vi);

    // Duplicate locale -> 409 Conflict
    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &current_loc_etag)
            .json(&json!({ "code": "VI", "label": "Dup" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // Multi-locale settings/public guard: changing content_language -> 409, indexing_enabled only -> 200
    assert_eq!(
        client
            .put(format!("{origin}/api/admin/settings/public"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &renamed_etag)
            .json(&json!({
                "public_origin": null,
                "content_language": "fr",
                "indexing_enabled": true
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let ok_multi_pub = client
        .put(format!("{origin}/api/admin/settings/public"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &renamed_etag)
        .json(&json!({
            "public_origin": null,
            "content_language": "en-US",
            "indexing_enabled": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_multi_pub.status(), StatusCode::OK);

    // PUT disable default ("en-us") -> 422
    assert_eq!(
        client
            .put(format!("{origin}/api/admin/locales/en-us"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &current_loc_etag)
            .json(&json!({ "label": "EN", "direction": "ltr", "enabled": false, "position": 0 }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Disable secondary "vi" -> 200, then set-default on disabled "vi" -> 409
    let disable_vi = client
        .put(format!("{origin}/api/admin/locales/vi"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &current_loc_etag)
        .json(
            &json!({ "label": "Vietnamese", "direction": "ltr", "enabled": false, "position": 1 }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(disable_vi.status(), StatusCode::OK);
    current_loc_etag = etag_version(&disable_vi);

    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales/vi/set-default"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &current_loc_etag)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // Re-enable "vi" and set "vi" as default -> 200 (now default = "vi", secondary = "en-us")
    let enable_vi = client
        .put(format!("{origin}/api/admin/locales/vi"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &current_loc_etag)
        .json(&json!({ "label": "Tiếng Việt", "direction": "ltr", "enabled": true, "position": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(enable_vi.status(), StatusCode::OK);
    current_loc_etag = etag_version(&enable_vi);

    let set_def_vi = client
        .post(format!("{origin}/api/admin/locales/vi/set-default"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &current_loc_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(set_def_vi.status(), StatusCode::OK);
    current_loc_etag = etag_version(&set_def_vi);
    let set_def_body: Value = set_def_vi.json().await.unwrap();
    assert_eq!(set_def_body["default_code"], "vi");
    let pub_after_def: Value = client
        .get(format!("{origin}/api/admin/settings/public"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(pub_after_def["content_language"], "vi");

    // 1. General Settings overlay (?locale=fr on fresh secondary locale without override)
    let create_fr = client
        .post(format!("{origin}/api/admin/locales"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &current_loc_etag)
        .json(&json!({ "code": "fr", "label": "Français" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_fr.status(), StatusCode::CREATED);
    current_loc_etag = etag_version(&create_fr);

    let def_gen_before: Value = client
        .get(format!("{origin}/api/admin/settings/general"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(def_gen_before.get("locale").is_none());

    let sec_gen_get = client
        .get(format!("{origin}/api/admin/settings/general?locale=fr"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(sec_gen_get.status(), StatusCode::OK);
    let sec_gen_etag = etag_version(&sec_gen_get);
    let sec_gen_body: Value = sec_gen_get.json().await.unwrap();
    assert_eq!(sec_gen_body["has_override"], false);
    assert_eq!(sec_gen_body["locale"], "fr");
    let global_tz = sec_gen_body["timezone"].as_str().unwrap().to_owned();

    // Changing timezone in secondary locale -> 400
    assert_eq!(
        client
            .put(format!("{origin}/api/admin/settings/general?locale=fr"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &sec_gen_etag)
            .json(&json!({
                "site_title": "FR Title",
                "tagline": "FR Tag",
                "timezone": "Asia/Tokyo"
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    let sec_gen_put = client
        .put(format!("{origin}/api/admin/settings/general?locale=fr"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &sec_gen_etag)
        .json(&json!({
            "site_title": "Site Français",
            "tagline": "Slogan Français",
            "timezone": global_tz
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(sec_gen_put.status(), StatusCode::OK);
    let sec_gen_saved: Value = sec_gen_put.json().await.unwrap();
    assert_eq!(sec_gen_saved["has_override"], true);
    assert_eq!(sec_gen_saved["version"], 2);
    assert_eq!(sec_gen_saved["locale"], "fr");

    let def_gen_after: Value = client
        .get(format!("{origin}/api/admin/settings/general"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(def_gen_after, def_gen_before);

    // 2. Reading Settings overlay (?locale=en-us): create a 'vi' page (current default = 'vi') -> 422 on en-us
    let vi_page_resp = client
        .post(format!("{origin}/api/admin/content-types/page/entries"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "title": "VI Page", "slug": "vi-page", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_page_resp.status(), StatusCode::CREATED);
    let vi_page_id = vi_page_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "{origin}/api/admin/content-types/page/entries/{vi_page_id}/publication"
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // Default ('vi') reading cannot use en-us page -> 422
    assert_eq!(
        client
            .put(format!("{origin}/api/admin/settings/reading"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "homepage_page_id": en_page_id, "posts_page_id": null }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Wait: en-us reading was created at v1 during initial migration and cascaded from vi!
    let sec_read_get = client
        .get(format!("{origin}/api/admin/settings/reading?locale=en-us"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    let sec_read_etag = etag_version(&sec_read_get);
    let sec_read_put = client
        .put(format!("{origin}/api/admin/settings/reading?locale=en-us"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &sec_read_etag)
        .json(&json!({ "homepage_page_id": en_page_id, "posts_page_id": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(sec_read_put.status(), StatusCode::OK);
    let sec_read_saved: Value = sec_read_put.json().await.unwrap();
    assert_eq!(sec_read_saved["locale"], "en-us");
    assert_eq!(sec_read_saved["homepage_page_id"], en_page_id);

    // Default 'vi' reading accepts vi_page_id independently
    let def_read_put = client
        .put(format!("{origin}/api/admin/settings/reading"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "homepage_page_id": vi_page_id, "posts_page_id": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(def_read_put.status(), StatusCode::OK);

    // 3. Home SEO (?locale=en-us) + media pin blocks delete (409)
    let up = client
        .post(format!("{origin}/api/admin/media"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", "home-og.png")
        .body(tiny_png(7))
        .send()
        .await
        .unwrap();
    assert_eq!(up.status(), StatusCode::CREATED);
    let media_id = up.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let media_meta = client
        .get(format!("{origin}/api/admin/media/{media_id}"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    let media_etag = etag_version(&media_meta);

    let sec_seo_get = client
        .get(format!("{origin}/api/admin/seo/home?locale=en-us"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(sec_seo_get.status(), StatusCode::OK);
    let sec_seo_etag = etag_version(&sec_seo_get);

    let sec_seo_put = client
        .put(format!("{origin}/api/admin/seo/home?locale=en-us"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &sec_seo_etag)
        .json(&json!({
            "title": "EN Home SEO",
            "description": "English desc",
            "og_image_id": media_id,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(sec_seo_put.status(), StatusCode::OK);
    let sec_seo_body: Value = sec_seo_put.json().await.unwrap();
    assert_eq!(sec_seo_body["locale"], "en-us");

    // Media deletion must return 409 because secondary locale site_home_seo.og_image_id pins it
    assert_eq!(
        client
            .delete(format!("{origin}/api/admin/media/{media_id}"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &media_etag)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // 4. Menu Slots (?locale=vi vs empty default for new default 'vi')
    let vi_slots_get = client
        .get(format!("{origin}/api/admin/menu-slots/theme-s01"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(vi_slots_get.status(), StatusCode::OK);
    let vi_slots_body: Value = vi_slots_get.json().await.unwrap();
    // 'vi' has no slot assignment (initial slot belonged to 'en-us') and must NOT fallback
    assert_eq!(vi_slots_body["assignments"], json!({}));
    assert_eq!(vi_slots_body["version"], 1);
    let vi_slots_put = client
        .put(format!("{origin}/api/admin/menu-slots/theme-s01"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "assignments": { "header": menu_id } }))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_slots_put.status(), StatusCode::OK);
    assert_eq!(vi_slots_put.json::<Value>().await.unwrap()["version"], 2);

    // 5. Theme Options (?locale=en-us) + non-translatable rejection (400) + CAS + compat check (409)
    // Secondary locale rejects non-translatable key 'accent_hex' -> 400
    assert_eq!(
        client
            .put(format!(
                "{origin}/api/themes/theme-s01/options?locale=en-us"
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "values": { "accent_hex": "#ffffff" } }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    // Stale If-Match -> 412
    assert_eq!(
        client
            .put(format!(
                "{origin}/api/themes/theme-s01/options?locale=en-us"
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v9\"")
            .json(&json!({ "values": { "hero_title": "English Hero" } }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_FAILED
    );
    // Save translatable CTA URL ONLY in secondary locale 'en-us' (so compat 409 proves locale table is checked)
    let en_opts_put = client
        .put(format!(
            "{origin}/api/themes/theme-s01/options?locale=en-us"
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "values": { "cta_url": "/en/contact" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(en_opts_put.status(), StatusCode::OK);
    let en_opts_body: Value = en_opts_put.json().await.unwrap();
    assert_eq!(en_opts_body["locale"], "en-us");
    assert_eq!(en_opts_body["version"], 2);
    assert_eq!(en_opts_body["values"]["cta_url"], "/en/contact");

    // Upgrade to v2 where cta_url becomes 'number' -> check_theme_options_compatibility rejects 409!
    let zip_v2 = build_theme_zip(
        "theme-s01",
        "2.0.0",
        json!([
            { "key": "hero_title", "label": "Hero", "type": "text", "default": "Default Hero", "public": true, "translatable": true },
            { "key": "cta_url", "label": "CTA Num", "type": "number", "default": 1, "public": true },
            { "key": "accent_hex", "label": "Accent", "type": "text", "default": "#111111", "public": true }
        ]),
    );
    let inst_v2 = client
        .post(format!("{origin}/api/admin/themes"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(zip_v2)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_v2.status(), StatusCode::CREATED);
    let rev_v2 = inst_v2.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        client
            .put(format!("{origin}/api/admin/themes/active"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &active_theme_etag)
            .json(&json!({ "revision_id": rev_v2 }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // 6. MAX_SITE_LOCALES = 20 cap -> 20th succeeds, 21st returns 422
    // (already created: vi, en-us, fr = 3; adding 17 more reaches 20)
    let extra_codes = [
        "de", "es", "it", "pt", "nl", "pl", "sv", "no", "da", "fi", "cs", "ro", "hu", "el", "tr",
        "ja", "ko",
    ];
    for code in extra_codes {
        let r = client
            .post(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &current_loc_etag)
            .json(&json!({ "code": code, "label": code }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED, "creating {code}");
        current_loc_etag = etag_version(&r);
    }
    assert_eq!(
        client
            .post(format!("{origin}/api/admin/locales"))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &current_loc_etag)
            .json(&json!({ "code": "zh", "label": "Chinese" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
