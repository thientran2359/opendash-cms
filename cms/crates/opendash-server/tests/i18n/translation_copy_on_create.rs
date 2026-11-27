//! S03: Copy-on-Create Translations, Locale-Filtered Lookups, Plugin ABI & Studio Preview Locale.
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions};

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
            "opendash-s03-{label}-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create isolated root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for IsolatedRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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

    async fn db_rw(&self) -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(self.site_root.join("database/opendash.sqlite"))
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await
            .unwrap()
    }
}

fn client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .build()
        .expect("client")
}

fn etag_version(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(header::ETAG)
        .expect("ETag header")
        .to_str()
        .unwrap()
        .to_owned()
}

async fn setup_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "StrongPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, base, "admin", "StrongPassword123!").await
}

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let login = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .expect("session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let body: Value = login.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_owned();
    (cookie, csrf)
}

async fn add_locale(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    bcp47_tag: &str,
    label: &str,
    direction: &str,
) {
    let list_resp = client
        .get(format!("{base}/api/admin/locales"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let etag = etag_version(&list_resp);
    let enabled = client
        .put(format!("{base}/api/admin/locales/multilingual"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, &etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let etag = etag_version(&enabled);
    let create = client
        .post(format!("{base}/api/admin/locales"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("x-csrf-token", csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({
            "code": bcp47_tag,
            "label": label,
            "direction": direction,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        create.status(),
        StatusCode::CREATED,
        "add_locale {bcp47_tag} failed: {}",
        create.text().await.unwrap()
    );
}

#[tokio::test]
async fn s03_copy_on_create_entry_translations_seo_terms_builder_and_lifecycle() {
    let root = IsolatedRoot::new("copy-entry");
    let srv = RunningServer::start(root.path()).await;
    let c = client();
    let (cookie, csrf) = setup_admin(&c, &srv.base_url).await;

    add_locale(&c, &srv.base_url, &cookie, &csrf, "en", "English", "ltr").await;
    add_locale(&c, &srv.base_url, &cookie, &csrf, "ar-SA", "Arabic", "rtl").await;

    // Seed an active media asset for entry_seo.og_image_id
    let pool = srv.db_rw().await;
    let og_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    sqlx::query(
        "INSERT INTO media_assets
         (id, kind, filename, mime_type, extension, byte_size, width, height,
          thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
          lifecycle_state, generation, version)
         VALUES (?, 'image', 'og.png', 'image/png', 'png', 100, 10, 10, 10, 10, NULL, NULL, NULL, 'active', 1, 1)",
    )
    .bind(og_id)
    .execute(&pool)
    .await
    .unwrap();

    // Create category in vi + translation in en, and a vi-only tag
    let cat_vi: Value = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "kind": "category", "name": "Tin tức", "slug": "tin-tuc", "locale": "vi" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cat_vi_id = cat_vi["id"].as_i64().unwrap();

    let cat_en: Value = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "kind": "category",
            "name": "News",
            "slug": "news",
            "locale": "en",
            "source_term_id": cat_vi_id
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cat_en_id = cat_en["id"].as_i64().unwrap();

    let tag_vi_only: Value = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "kind": "tag", "name": "Chỉ VI", "slug": "chi-vi", "locale": "vi" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let tag_vi_only_id = tag_vi_only["id"].as_i64().unwrap();

    // Build a block document with core/list referencing both cat_vi_id and tag_vi_only_id
    let doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec1",
            "type": "core/section",
            "children": [{
                "id": "row1",
                "type": "core/row",
                "children": [{
                    "id": "col1",
                    "type": "core/column",
                    "children": [{
                        "id": "list1",
                        "type": "core/list",
                        "props": {
                            "type_key": "post",
                            "taxonomy_key": "category",
                            "term_id": cat_vi_id,
                            "page_size": 5
                        }
                    }]
                }]
            }]
        }]
    });
    let doc: opendash_core::builder::BuilderDocument = serde_json::from_value(doc).unwrap();
    let block_body =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&doc).unwrap();

    // Create source VI post and publish it
    let create_src = c
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "bai-viet-goc",
            "title": "Bài viết gốc",
            "locale": "vi",
            "values": { "body": block_body },
            "term_ids": [cat_vi_id, tag_vi_only_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_src.status(), StatusCode::CREATED);
    let src_etag = etag_version(&create_src);
    let src_entry: Value = create_src.json().await.unwrap();
    let src_id = src_entry["id"].as_i64().unwrap();
    let group_id = src_entry["translation_group_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Store Builder blocks with core/list directly into values.body (avoids editor HTML sanitization stripping block comments)
    sqlx::query("UPDATE content_entries SET values_json = json_set(values_json, '$.body', ?), content_mode = 'builder' WHERE id = ?")
        .bind(&block_body)
        .bind(src_id)
        .execute(&pool)
        .await
        .unwrap();

    // Set primary category + entry_seo (including custom canonical_url) & publish
    let pub_resp = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/publication",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);
    let src_etag = etag_version(&pub_resp);

    sqlx::query("UPDATE content_entries SET primary_category_term_id = ? WHERE id = ?")
        .bind(cat_vi_id)
        .bind(src_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO entry_seo (entry_id, title, description, canonical_url, og_image_id, noindex)
         VALUES (?, 'SEO Gốc', 'Mô tả gốc', 'https://example.com/custom-vi', ?, 1)",
    )
    .bind(src_id)
    .bind(og_id)
    .execute(&pool)
    .await
    .unwrap();

    // Error gates for translation endpoint
    let no_if_match = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "target_locale": "en", "slug": "hello-en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_if_match.status(), StatusCode::PRECONDITION_REQUIRED);

    let wrong_if_match = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v999\"")
        .json(&json!({ "target_locale": "en", "slug": "hello-en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_if_match.status(), StatusCode::PRECONDITION_FAILED);

    let same_locale = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({ "target_locale": "vi", "slug": "hello-vi" }))
        .send()
        .await
        .unwrap();
    assert_eq!(same_locale.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let unknown_locale = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({ "target_locale": "fr", "slug": "bonjour" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown_locale.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let pattern_forbidden = c
        .post(format!(
            "{}/api/admin/content-types/pattern/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({ "target_locale": "en", "slug": "pat-en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pattern_forbidden.status(), StatusCode::FORBIDDEN);

    // Valid Copy-on-Create to EN
    let tr_resp = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({
            "target_locale": "en",
            "slug": "hello-en",
            "title": "Hello EN"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_resp.status(), StatusCode::CREATED);
    let en_entry: Value = tr_resp.json().await.unwrap();
    let en_id = en_entry["id"].as_i64().unwrap();
    assert_eq!(en_entry["locale"], "en");
    assert_eq!(en_entry["translation_group_id"], group_id);
    assert_eq!(en_entry["publication_status"], "draft");
    assert_eq!(en_entry["term_ids"], json!([cat_en_id]));
    assert_eq!(en_entry["primary_category_term_id"], json!(cat_en_id));
    assert_eq!(en_entry["translations"].as_array().unwrap().len(), 2);

    // Verify core/list block term_ids remapped to cat_en_id (tag_vi_only_id dropped)
    let en_body = en_entry["values"]["body"].as_str().unwrap();
    let parsed_en = opendash_core::builder_blocks::parse_blocks_lenient_or_recover(en_body);
    let el = &parsed_en.document.blocks[0].children[0].children[0].children[0];
    assert_eq!(
        el.block_id, "core/list",
        "expected core/list block, got: {el:#?}, en_body={en_body}, block_body={block_body}"
    );
    assert_eq!(
        el.props.get("term_id").and_then(Value::as_i64),
        Some(cat_en_id)
    );

    // Verify entry_seo copied EXCEPT canonical_url, and no routes/redirects copied
    let seo_row: (String, String, Option<String>, Option<String>, i64) = sqlx::query_as(
        "SELECT title, description, canonical_url, og_image_id, noindex FROM entry_seo WHERE entry_id = ?",
    )
    .bind(en_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(seo_row.0, "SEO Gốc");
    assert_eq!(seo_row.1, "Mô tả gốc");
    assert_eq!(seo_row.2, None);
    assert_eq!(seo_row.3.as_deref(), Some(og_id));
    assert_eq!(seo_row.4, 1);

    let copied_paths: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_public_paths WHERE entry_id = ?")
            .bind(en_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(copied_paths, 0);

    // Duplicate translation to same target_locale -> 409 Conflict
    let dup_tr = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({ "target_locale": "en", "slug": "hello-en-2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup_tr.status(), StatusCode::CONFLICT);

    // Same slug in another locale (ar-sa) succeeds, and preview emits lang="ar-SA" dir="rtl"
    let ar_tr = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &src_etag)
        .json(&json!({ "target_locale": "ar-sa", "slug": "hello-en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ar_tr.status(), StatusCode::CREATED);
    let ar_entry: Value = ar_tr.json().await.unwrap();
    let ar_id = ar_entry["id"].as_i64().unwrap();

    let preview_en = c
        .get(format!(
            "{}/api/admin/content-types/post/entries/{en_id}/preview",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(preview_en.status(), StatusCode::OK);
    let html_en = preview_en.text().await.unwrap();
    assert!(html_en.contains("lang=\"en\""), "html_en: {html_en}");
    assert!(html_en.contains("dir=\"ltr\""));

    let preview_ar = c
        .get(format!(
            "{}/api/admin/content-types/post/entries/{ar_id}/preview",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(preview_ar.status(), StatusCode::OK);
    let html_ar = preview_ar.text().await.unwrap();
    assert!(html_ar.contains("lang=\"ar-SA\""), "html_ar: {html_ar}");
    assert!(html_ar.contains("dir=\"rtl\""));

    // Locale filters on entries & entry-table
    let en_list: Value = c
        .get(format!(
            "{}/api/admin/content-types/post/entries?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(en_list.as_array().unwrap().len(), 1);
    assert_eq!(en_list[0]["id"], en_id);

    let bad_filter = c
        .get(format!(
            "{}/api/admin/content-types/post/entries?locale=xx",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(bad_filter.status(), StatusCode::BAD_REQUEST);

    let table_en: Value = c
        .get(format!(
            "{}/api/admin/content-types/post/entry-table?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(table_en["items"].as_array().unwrap().len(), 1);
    assert_eq!(table_en["items"][0]["locale"], "en");
    assert_eq!(
        table_en["items"][0]["translations"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // Cross-locale term_ids rejected when creating an entry
    let cross_term = c
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "cross-locale-post",
            "title": "Cross",
            "locale": "en",
            "values": { "body": "Hi" },
            "term_ids": [cat_vi_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cross_term.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // auto_unique_slug=true in ar-sa generates hello-en-2
    let src2: Value = c
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "second-vi",
            "title": "Second VI",
            "locale": "vi",
            "values": { "body": "Body" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let src2_id = src2["id"].as_i64().unwrap();
    let auto_ar = c
        .post(format!(
            "{}/api/admin/content-types/post/entries/{src2_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "target_locale": "ar-sa",
            "slug": "hello-en",
            "auto_unique_slug": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(auto_ar.status(), StatusCode::CREATED);
    let auto_ar_json: Value = auto_ar.json().await.unwrap();
    assert_eq!(auto_ar_json["slug"], "hello-en-2");
}

#[tokio::test]
async fn s03_copy_on_create_rbac_and_immediate_cpt() {
    let root = IsolatedRoot::new("rbac-cpt");
    let srv = RunningServer::start(root.path()).await;
    let c = client();
    let (admin_cookie, admin_csrf) = setup_admin(&c, &srv.base_url).await;

    add_locale(
        &c,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "en",
        "English",
        "ltr",
    )
    .await;

    // Create editor user
    let create_editor = c
        .post(format!("{}/api/admin/users", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&c, &srv.base_url, "editor1", "EditorPassword123!").await;

    // Create immediate CPT with an optional admin-only field
    let def_opt = json!({
        "key": "faq",
        "label": "FAQ",
        "settings": { "public": true, "has_archive": false, "publication": "immediate", "supports": ["editor"] },
        "fields": [
            { "key": "answer", "label": "Answer", "type": "text", "required": true },
            {
                "key": "admin_note",
                "label": "Admin Note",
                "type": "text",
                "required": false,
                "acl": {
                    "min_view_role": "admin",
                    "min_edit_role": "admin",
                    "public_exposure": "private"
                }
            }
        ]
    });
    let ct_resp = c
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&def_opt)
        .send()
        .await
        .unwrap();
    let ct_status = ct_resp.status();
    if ct_status != StatusCode::CREATED {
        panic!(
            "create_content_type failed ({ct_status}): {}",
            ct_resp.text().await.unwrap()
        );
    }

    let faq_vi: Value = c
        .post(format!(
            "{}/api/admin/content-types/faq/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({
            "slug": "faq-1",
            "title": "FAQ 1",
            "locale": "vi",
            "values": { "answer": "Trả lời", "admin_note": "Secret" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(faq_vi["publication_status"], "published");
    let faq_id = faq_vi["id"].as_i64().unwrap();

    // Editor copies FAQ -> succeeds, immediate CPT starts published, admin_note stripped
    let tr_editor = c
        .post(format!(
            "{}/api/admin/content-types/faq/entries/{faq_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &editor_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "target_locale": "en", "slug": "faq-1-en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_editor.status(), StatusCode::CREATED);
    let faq_en: Value = tr_editor.json().await.unwrap();
    assert_eq!(faq_en["publication_status"], "published");
    assert!(faq_en["values"].get("admin_note").is_none());

    // Second CPT with required admin-only field + default_value -> Editor copy-on-create receives default_value (never leaks admin's "Secret")
    let ct_req = c
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({
            "key": "faq_req",
            "label": "FAQ Req",
            "settings": { "public": true, "has_archive": false, "publication": "immediate", "supports": ["editor"] },
            "fields": [
                { "key": "answer", "label": "Answer", "type": "text", "required": true },
                {
                    "key": "admin_note",
                    "label": "Admin Note",
                    "type": "text",
                    "required": true,
                    "default": "admin-default",
                    "acl": {
                        "min_view_role": "admin",
                        "min_edit_role": "admin",
                        "public_exposure": "private"
                    }
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ct_req.status(), StatusCode::CREATED);

    let faq_req_vi: Value = c
        .post(format!(
            "{}/api/admin/content-types/faq_req/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({
            "slug": "faq-req-1",
            "title": "FAQ Req 1",
            "locale": "vi",
            "values": { "answer": "Trả lời", "admin_note": "Secret" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let faq_req_id = faq_req_vi["id"].as_i64().unwrap();

    let tr_defaulted = c
        .post(format!(
            "{}/api/admin/content-types/faq_req/entries/{faq_req_id}/translations",
            srv.base_url
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &editor_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "target_locale": "en", "slug": "faq-req-1-en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_defaulted.status(), StatusCode::CREATED);
    let faq_req_en: Value = tr_defaulted.json().await.unwrap();
    // Editor response filters out admin-only field, while DB stores default_value ("admin-default"), never leaking "Secret"
    assert!(faq_req_en["values"].get("admin_note").is_none());
    let faq_req_en_id = faq_req_en["id"].as_i64().unwrap();
    let admin_read: Value = c
        .get(format!(
            "{}/api/admin/content-types/faq_req/entries/{faq_req_en_id}",
            srv.base_url
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(admin_read["values"]["admin_note"], "admin-default");
}

#[tokio::test]
async fn s03_taxonomy_copy_on_create_parent_mapping_and_patch_rejects_locale() {
    let root = IsolatedRoot::new("tax-copy");
    let srv = RunningServer::start(root.path()).await;
    let c = client();
    let (cookie, csrf) = setup_admin(&c, &srv.base_url).await;
    add_locale(&c, &srv.base_url, &cookie, &csrf, "en", "English", "ltr").await;

    let p_vi: Value = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "kind": "category", "name": "Cha VI", "slug": "cha-vi", "locale": "vi" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let p_vi_id = p_vi["id"].as_i64().unwrap();

    let c_vi: Value = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "kind": "category",
            "name": "Con VI",
            "slug": "con-vi",
            "parent_id": p_vi_id,
            "locale": "vi"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let c_vi_id = c_vi["id"].as_i64().unwrap();

    let p_en: Value = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "kind": "category",
            "name": "Parent EN",
            "slug": "parent-en",
            "locale": "en",
            "source_term_id": p_vi_id
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let p_en_id = p_en["id"].as_i64().unwrap();

    // Child translation inherits translation_group_id and auto-maps parent_id -> p_en_id
    let c_en_resp = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "kind": "category",
            "name": "Child EN",
            "slug": "child-en",
            "locale": "en",
            "source_term_id": c_vi_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(c_en_resp.status(), StatusCode::CREATED);
    let c_en: Value = c_en_resp.json().await.unwrap();
    assert_eq!(c_en["parent_id"], json!(p_en_id));
    assert_eq!(c_en["translation_group_id"], c_vi["translation_group_id"]);

    // Duplicate translation -> 409
    let dup = c
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "kind": "category",
            "name": "Child EN 2",
            "slug": "child-en-2",
            "locale": "en",
            "source_term_id": c_vi_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), StatusCode::CONFLICT);

    // Filter ?locale=en
    let list_en: Value = c
        .get(format!(
            "{}/api/admin/taxonomy/terms?kind=category&locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list_en["items"].as_array().unwrap().len(), 2);

    // PATCH with locale rejected (#[serde(deny_unknown_fields)])
    let patch_locale = c
        .patch(format!(
            "{}/api/admin/taxonomy/terms/{p_en_id}",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "name": "Renamed", "locale": "vi" }))
        .send()
        .await
        .unwrap();
    let patch_status = patch_locale.status();
    if patch_status != StatusCode::BAD_REQUEST && patch_status != StatusCode::UNPROCESSABLE_ENTITY {
        panic!(
            "expected 400/422 for PATCH locale, got {patch_status}: {}",
            patch_locale.text().await.unwrap()
        );
    }
}

#[tokio::test]
async fn s03_studio_preview_locale_and_binding_list_isolation() {
    let root = IsolatedRoot::new("studio-preview");
    let srv = RunningServer::start(root.path()).await;
    let c = client();
    let (cookie, csrf) = setup_admin(&c, &srv.base_url).await;
    add_locale(&c, &srv.base_url, &cookie, &csrf, "en", "English", "ltr").await;

    // Configure localized site_title for EN
    let gen_en = c
        .get(format!(
            "{}/api/admin/settings/general?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(gen_en.status(), StatusCode::OK);
    let gen_etag = etag_version(&gen_en);
    let put_gen = c
        .put(format!(
            "{}/api/admin/settings/general?locale=en",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, gen_etag)
        .json(&json!({
            "site_title": "English Site Title",
            "tagline": "English Tagline",
            "timezone": "UTC"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_gen.status(), StatusCode::OK);

    // Create two published posts with the same slug in VI and EN
    let p_vi: Value = c
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "shared-post",
            "title": "Bài viết tiếng Việt",
            "locale": "vi",
            "values": { "body": "Nội dung VI" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let p_vi_id = p_vi["id"].as_i64().unwrap();
    c.post(format!(
        "{}/api/admin/content-types/post/entries/{p_vi_id}/publication",
        srv.base_url
    ))
    .header(header::COOKIE, &cookie)
    .header("Origin", &srv.base_url)
    .header("x-csrf-token", &csrf)
    .header(header::IF_MATCH, "\"v1\"")
    .json(&json!({ "status": "published" }))
    .send()
    .await
    .unwrap();

    let p_en: Value = c
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "shared-post",
            "title": "English Post Title",
            "locale": "en",
            "values": { "body": "English Body" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let p_en_id = p_en["id"].as_i64().unwrap();
    c.post(format!(
        "{}/api/admin/content-types/post/entries/{p_en_id}/publication",
        srv.base_url
    ))
    .header(header::COOKIE, &cookie)
    .header("Origin", &srv.base_url)
    .header("x-csrf-token", &csrf)
    .header(header::IF_MATCH, "\"v1\"")
    .json(&json!({ "status": "published" }))
    .send()
    .await
    .unwrap();

    // Studio preview for selector_slug="shared-post" with preview_locale="en"
    let single_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec1",
            "type": "core/section",
            "children": [{
                "id": "row1",
                "type": "core/row",
                "children": [{
                    "id": "col1",
                    "type": "core/column",
                    "children": [{
                        "id": "slot1",
                        "type": "core/content-slot",
                        "props": {}
                    }]
                }]
            }]
        }]
    });
    let studio_single = c
        .post(format!("{}/api/admin/studio/preview", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "target": {
                "kind": "single",
                "content_type": "post",
                "selector_slug": "shared-post"
            },
            "document": single_doc,
            "preview_locale": "en"
        }))
        .send()
        .await
        .unwrap();
    if studio_single.status() != StatusCode::OK {
        let st = studio_single.status();
        panic!(
            "studio_single failed ({st}): {}",
            studio_single.text().await.unwrap()
        );
    }
    let single_json: Value = studio_single.json().await.unwrap();
    let single_html = single_json["html"].as_str().unwrap();
    assert!(single_html.contains("lang=\"en\""));
    assert!(single_html.contains("English Post Title"));
    assert!(!single_html.contains("Bài viết tiếng Việt"));

    // Studio preview with custom BuilderDocument containing core/list -> filters by preview_locale="en"
    let list_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec1",
            "type": "core/section",
            "children": [{
                "id": "row1",
                "type": "core/row",
                "children": [{
                    "id": "col1",
                    "type": "core/column",
                    "children": [
                        {
                            "id": "listen",
                            "type": "core/list",
                            "props": {
                                "type_key": "post",
                                "page_size": 10
                            }
                        },
                        {
                            "id": "slot1",
                            "type": "core/content-slot",
                            "props": {}
                        }
                    ]
                }]
            }]
        }]
    });
    let studio_list = c
        .post(format!("{}/api/admin/studio/preview", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "target": { "kind": "home" },
            "document": list_doc,
            "preview_locale": "en"
        }))
        .send()
        .await
        .unwrap();
    if studio_list.status() != StatusCode::OK {
        let st = studio_list.status();
        panic!(
            "studio_list failed ({st}): {}",
            studio_list.text().await.unwrap()
        );
    }
    let list_html = studio_list.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(list_html.contains("English Post Title"));
    assert!(!list_html.contains("Bài viết tiếng Việt"));
    assert!(list_html.contains("English Site Title"));

    // Invalid preview_locale returns 400
    let bad_studio = c
        .post(format!("{}/api/admin/studio/preview", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("Origin", &srv.base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "target": { "kind": "home" },
            "preview_locale": "xx"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_studio.status(), StatusCode::BAD_REQUEST);
}
