//! Slice S03 HTTP integration tests: narrow sparse metadata & single-taxonomy patch
//! (`PATCH /api/admin/content-types/{type_key}/entries/{id}/metadata`), strict site-timezone
//! published_at validation (including DST gap/ambiguous rejection), taxonomy delta preservation,
//! hidden/private/withdrawn field and Builder/SEO invariance, and scoped plugin prepare_save /
//! before_save / after_save / projection interactions.

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zip::{ZipWriter, write::SimpleFileOptions};

struct Site {
    root: PathBuf,
    child: Child,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    async fn start(prefix: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("opendash-{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_HOST", "127.0.0.1")
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..400 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self { root, child, base };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("S03 server did not become healthy");
    }

    async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(self.root.join("database/opendash.sqlite")),
        )
        .await
        .unwrap()
    }
}

async fn setup_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .header("Origin", base)
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, base, "admin", "AdminPassword123!").await
}

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .header("Origin", base)
        .json(&json!({"username":username,"password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|h| h.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn schema_token(client: &Client, base: &str, cookie: &str, type_key: &str) -> String {
    let resp = client
        .get(format!(
            "{base}/api/admin/content-types/{type_key}/entry-table?limit=1"
        ))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    body["schema_token"]
        .as_str()
        .expect("entry-table schema_token")
        .to_owned()
}

fn build_plugin_zip(manifest_json: &str, wasm_bytes: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest_json.as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(wasm_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn static_output_wasm(output_json: &str) -> Vec<u8> {
    let bytes = output_json.as_bytes();
    let escaped: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{escaped}")
            (func (export "opendash_run") (result i32)
                (drop (call $set_output (i32.const 1024) (i32.const {len})))
                i32.const 0)
        )"#,
        len = bytes.len()
    );
    wat::parse_str(&wat_src).unwrap()
}

fn kv_set_wasm(key: &str, value: &str) -> Vec<u8> {
    let req = json!({ "op": "kv_set", "key": key, "value": value }).to_string();
    let out = "{}";
    let req_esc: String = req
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let out_esc: String = out
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{req_esc}")
            (data (i32.const 1536) "{out_esc}")
            (func (export "opendash_run") (result i32)
                (drop (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 2048) (i32.const 512)))
                (drop (call $set_output (i32.const 1536) (i32.const {olen})))
                i32.const 0)
        )"#,
        rlen = req.len(),
        olen = out.len()
    );
    wat::parse_str(&wat_src).unwrap()
}

async fn install_and_enable_plugin(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    plugin_id: &str,
    manifest: Value,
    wasm_bytes: &[u8],
) {
    let zip = build_plugin_zip(&manifest.to_string(), wasm_bytes);
    let ins = client
        .post(format!("{base}/api/admin/plugins"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("Content-Type", "application/zip")
        .header("X-File-Name", format!("{plugin_id}.zip"))
        .body(zip)
        .send()
        .await
        .unwrap();
    assert!(
        ins.status().is_success(),
        "install {plugin_id}: {}",
        ins.status()
    );
    let en = client
        .post(format!("{base}/api/admin/plugins/{plugin_id}/enable"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(en.status(), StatusCode::OK, "enable {plugin_id}");
}

async fn disable_plugin(client: &Client, base: &str, cookie: &str, csrf: &str, plugin_id: &str) {
    let resp = client
        .post(format!("{base}/api/admin/plugins/{plugin_id}/disable"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "disable {plugin_id}");
}

#[tokio::test]
async fn s03_metadata_patch_validation_dates_dst_taxonomy_and_invariants() {
    let site = Site::start("s03-meta").await;
    let client = Client::builder().build().unwrap();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &site.base).await;

    // Create an Editor account for RBAC and private-field projection verification.
    let ed_create = client
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"username":"editor1","password":"EditorPassword123!","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_create.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&client, &site.base, "editor1", "EditorPassword123!").await;

    // Set site timezone to America/New_York to verify real DST gap & fall-back ambiguity handling.
    let general_cfg = client
        .get(format!("{}/api/admin/settings/general", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let gen_ver = general_cfg["version"].as_i64().unwrap();
    let set_tz = client
        .put(format!("{}/api/admin/settings/general", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{gen_ver}\""))
        .json(&json!({
            "site_title": general_cfg["site_title"],
            "tagline": general_cfg["tagline"],
            "timezone": "America/New_York"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(set_tz.status(), StatusCode::OK);

    // Enable multilingual FIRST, then add secondary locale `en` for foreign-locale term rejection checks.
    let loc_get = client
        .get(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    let mut loc_etag = loc_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let en_ml = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_ml.status(), StatusCode::OK);
    loc_etag = en_ml
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let add_en = client
        .post(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"code":"en","label":"English"}))
        .send()
        .await
        .unwrap();
    assert_eq!(add_en.status(), StatusCode::CREATED);

    // Add an Admin-only private field group on `post` to verify private fields are preserved & never leaked.
    let fg = client
        .post(format!("{}/api/admin/content-field-groups", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "internal_meta",
            "title": "Internal Meta",
            "target_types": ["post"],
            "fields": [{
                "key": "secret_note",
                "label": "Secret Note",
                "type": "text",
                "required": false,
                "options": [],
                "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fg.status(), StatusCode::CREATED);

    // Create terms: vi categories (cat1, cat2), vi tags (tag1, tag2), and an en tag (en_tag).
    let make_term = |kind: &str, name: &str, slug: &str, locale: &str| {
        let client = client.clone();
        let base = site.base.clone();
        let cookie = admin_cookie.clone();
        let csrf = admin_csrf.clone();
        let kind = kind.to_owned();
        let name = name.to_owned();
        let slug = slug.to_owned();
        let locale = locale.to_owned();
        async move {
            let r = client
                .post(format!("{base}/api/admin/taxonomy/terms"))
                .header(header::COOKIE, cookie)
                .header("Origin", base)
                .header("X-CSRF-Token", csrf)
                .json(&json!({"kind": kind, "name": name, "slug": slug, "locale": locale}))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::CREATED);
            r.json::<Value>().await.unwrap()["id"].as_i64().unwrap()
        }
    };
    let cat1 = make_term("category", "Cat One", "cat-one", "vi").await;
    let cat2 = make_term("category", "Cat Two", "cat-two", "vi").await;
    let tag1 = make_term("tag", "Tag One", "tag-one", "vi").await;
    let tag2 = make_term("tag", "Tag Two", "tag-two", "vi").await;
    let en_tag = make_term("tag", "EN Tag", "en-tag", "en").await;

    // Create an immediate CPT (`item`) to test `publication_policy_unsupported` (409).
    let item_ct = client
        .post(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "item",
            "label": "Items",
            "settings": { "public": true, "has_archive": true, "publication": "immediate", "supports": ["editor"] },
            "fields": [{ "key": "body", "label": "Body", "type": "richtext", "required": false, "options": [] }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(item_ct.status(), StatusCode::CREATED);

    let post_token = schema_token(&client, &site.base, &admin_cookie, "post").await;
    let item_token = schema_token(&client, &site.base, &admin_cookie, "item").await;

    // Create two posts via composite save so we have body, secret_note, SEO, primary_category, and sibling slug collision target.
    let create_p1 = client
        .post(format!("{}/api/admin/content-types/post/entries/composite", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": post_token,
            "title": "Draft Original",
            "slug": "draft-original",
            "locale": "vi",
            "publication_status": "draft",
            "values": {
                "body": "<p>Keep this canonical body intact</p>",
                "secret_note": "top-secret-admin-only"
            },
            "term_ids": [cat1, cat2, tag1],
            "primary_category_term_id": cat1,
            "seo": { "title": "Canonical SEO Title", "description": "Keep SEO intact", "noindex": false }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_p1.status(), StatusCode::CREATED);
    let p1 = create_p1.json::<Value>().await.unwrap()["entry"].clone();
    let p1_id = p1["id"].as_i64().unwrap();
    let p1_created_at = p1["created_at"].as_str().unwrap().to_owned();
    let mut p1_ver = p1["version"].as_i64().unwrap();

    let create_p2 = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": post_token,
            "title": "Existing Published",
            "slug": "taken-slug",
            "locale": "vi",
            "publication_status": "published",
            "values": { "body": "<p>Other</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_p2.status(), StatusCode::CREATED);

    let create_item = client
        .post(format!(
            "{}/api/admin/content-types/item/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": item_token,
            "title": "Immediate Item",
            "slug": "immediate-item",
            "locale": "vi",
            "values": { "body": "<p>Item</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_item.status(), StatusCode::CREATED);
    let item_id = create_item.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();

    // 1. Missing If-Match -> 428; missing expected_schema_token -> 428; stale schema_token -> 412; stale version -> 412.
    let no_if_match = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"expected_schema_token": post_token, "title": "X"}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_if_match.status(), StatusCode::PRECONDITION_REQUIRED);

    let no_schema = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({"title": "X"}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_schema.status(), StatusCode::PRECONDITION_REQUIRED);

    let stale_schema = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({"expected_schema_token": "stale-token", "title": "X"}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_schema.status(), StatusCode::PRECONDITION_FAILED);

    let stale_ver = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v999\"")
        .json(&json!({"expected_schema_token": post_token, "title": "X"}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_ver.status(), StatusCode::PRECONDITION_FAILED);

    // 2. Unknown fields, explicit null, empty patch, and unchanged no-op patch -> 422.
    for bad_body in [
        json!({"expected_schema_token": post_token}),
        json!({"expected_schema_token": post_token, "title": null}),
        json!({"expected_schema_token": post_token, "published_at": null}),
        json!({"expected_schema_token": post_token, "taxonomy": null}),
        json!({"expected_schema_token": post_token, "title": "A", "values": {"body": "hack"}}),
        json!({"expected_schema_token": post_token, "title": "A", "created_at": "2020-01-01 00:00:00"}),
        json!({"expected_schema_token": post_token, "title": "Draft Original"}), // exact no-op
    ] {
        let resp = client
            .patch(format!(
                "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
            .json(&bad_body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "expected 422 for {bad_body}"
        );
    }

    // 3. Immediate CPT cannot change publication_status -> 409 publication_policy_unsupported.
    let imm_status = client
        .patch(format!(
            "{}/api/admin/content-types/item/entries/{item_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"expected_schema_token": item_token, "publication_status": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(imm_status.status(), StatusCode::CONFLICT);
    assert!(
        imm_status
            .text()
            .await
            .unwrap()
            .contains("publication_policy_unsupported")
    );

    // 4. Date & Timezone validation (draft cannot set date without explicit Publish; DST spring gap & fall ambiguity; format; future; changed timezone).
    let draft_date = client
        .patch(format!("{}/api/admin/content-types/post/entries/{p1_id}/metadata", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "published_at": { "local_datetime": "2025-01-15T09:30:00", "timezone": "America/New_York" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_date.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        draft_date
            .text()
            .await
            .unwrap()
            .contains("draft_cannot_set_published_at")
    );

    // Changed timezone (echoes UTC when site is America/New_York) -> 409 timezone_changed.
    let tz_mismatch = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "publication_status": "published",
            "published_at": { "local_datetime": "2025-01-15T09:30:00", "timezone": "UTC" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(tz_mismatch.status(), StatusCode::CONFLICT);
    assert!(
        tz_mismatch
            .text()
            .await
            .unwrap()
            .contains("timezone_changed")
    );

    for bad_dt in [
        "2025-01-15T09:30",        // missing seconds
        "2025-01-15T09:30:00Z",    // Z suffix forbidden
        "2025-01-15T09:30:00.500", // fraction forbidden
        "2025-01-15 09:30:00",     // space forbidden
        "2026-02-30T10:00:00",     // invalid calendar day
        "2026-03-08T02:30:00",     // America/New_York spring-forward DST gap
        "2026-11-01T01:30:00",     // America/New_York fall-back DST ambiguous time
        "2099-01-01T00:00:00",     // future date
    ] {
        let r = client
            .patch(format!(
                "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
            .json(&json!({
                "expected_schema_token": post_token,
                "publication_status": "published",
                "published_at": { "local_datetime": bad_dt, "timezone": "America/New_York" }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "bad_dt={bad_dt}"
        );
    }

    // 5. Explicit atomic Publish + valid site-timezone date + Editor role (verifying no private fields leaked!).
    let pub_ok = client
        .patch(format!("{}/api/admin/content-types/post/entries/{p1_id}/metadata", site.base))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "title": "Published Quick Edit",
            "publication_status": "published",
            "published_at": { "local_datetime": "2025-01-15T09:30:00", "timezone": "America/New_York" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_ok.status(), StatusCode::OK);
    let etag_hdr = pub_ok
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let body: Value = pub_ok.json().await.unwrap();
    p1_ver = body["version"].as_i64().unwrap();
    assert_eq!(etag_hdr, format!("\"v{p1_ver}\""));
    assert_eq!(body["title"], "Published Quick Edit");
    assert_eq!(body["publication_status"], "published");
    // 2025-01-15T09:30:00 EST (-05:00) == 2025-01-15 14:30:00 UTC!
    assert_eq!(body["published_at"], "2025-01-15 14:30:00");
    assert_eq!(body["created_at"], p1_created_at);
    // Private Admin-only field MUST NOT appear in Editor's response!
    assert!(body["fields"].get("secret_note").is_none());

    // 6. Unpublish preserves historical published_at; subsequent date edit on previously-published draft succeeds without republishing!
    let unpub = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({"expected_schema_token": post_token, "publication_status": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unpub.status(), StatusCode::OK);
    let unpub_body: Value = unpub.json().await.unwrap();
    p1_ver = unpub_body["version"].as_i64().unwrap();
    assert_eq!(unpub_body["publication_status"], "draft");
    assert_eq!(unpub_body["published_at"], "2025-01-15 14:30:00");

    let edit_hist = client
        .patch(format!("{}/api/admin/content-types/post/entries/{p1_id}/metadata", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "published_at": { "local_datetime": "2025-07-04T12:00:00", "timezone": "America/New_York" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(edit_hist.status(), StatusCode::OK);
    let hist_body: Value = edit_hist.json().await.unwrap();
    p1_ver = hist_body["version"].as_i64().unwrap();
    assert_eq!(hist_body["publication_status"], "draft"); // date edit NEVER implicitly publishes!
    // 2025-07-04T12:00:00 EDT (-04:00) == 2025-07-04 16:00:00 UTC!
    assert_eq!(hist_body["published_at"], "2025-07-04 16:00:00");

    // 7. Slug conflict returns 409 without silent auto-renaming.
    let slug_conflict = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({"expected_schema_token": post_token, "slug": "taken-slug"}))
        .send()
        .await
        .unwrap();
    assert_eq!(slug_conflict.status(), StatusCode::CONFLICT);

    // 8. Taxonomy delta validation & preservation (add, remove, replace/clear, primary category preservation/clear, foreign locale & wrong taxonomy rejection).
    for bad_tax in [
        json!({"key": "tag", "operation": "add", "term_ids": []}),
        json!({"key": "tag", "operation": "remove", "term_ids": []}),
        json!({"key": "tag", "operation": "add", "term_ids": [tag2, tag2]}),
        json!({"key": "tag", "operation": "add", "term_ids": [cat1]}), // wrong taxonomy
        json!({"key": "tag", "operation": "add", "term_ids": [en_tag]}), // wrong locale
    ] {
        let r = client
            .patch(format!(
                "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
            .json(&json!({"expected_schema_token": post_token, "taxonomy": bad_tax}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "bad_tax={bad_tax}"
        );
    }

    // Add tag2 -> preserves cat1, cat2, tag1 AND primary_category_term_id = cat1!
    let add_tag = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "taxonomy": {"key": "tag", "operation": "add", "term_ids": [tag2]}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_tag.status(), StatusCode::OK);
    let add_tag_body: Value = add_tag.json().await.unwrap();
    p1_ver = add_tag_body["version"].as_i64().unwrap();
    let terms_after_add: Vec<i64> = add_tag_body["term_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert!(
        terms_after_add.contains(&cat1)
            && terms_after_add.contains(&cat2)
            && terms_after_add.contains(&tag1)
            && terms_after_add.contains(&tag2)
    );

    // Empty replace on `tag` clears ONLY tags, preserving categories + primary_category!
    let clear_tags = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "taxonomy": {"key": "tag", "operation": "replace", "term_ids": []}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(clear_tags.status(), StatusCode::OK);
    let clear_tags_body: Value = clear_tags.json().await.unwrap();
    p1_ver = clear_tags_body["version"].as_i64().unwrap();
    let terms_after_clear: Vec<i64> = clear_tags_body["term_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(terms_after_clear, vec![cat1, cat2]);

    // Verify primary_category_term_id is still cat1, body/secret_note/SEO completely preserved in DB!
    let db = site.db().await;
    let (primary_before, values_json): (Option<i64>, String) = sqlx::query_as(
        "SELECT primary_category_term_id, values_json FROM content_entries WHERE id = ?",
    )
    .bind(p1_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(primary_before, Some(cat1));
    assert!(values_json.contains("Keep this canonical body intact"));
    assert!(values_json.contains("top-secret-admin-only"));

    // Remove primary category `cat1` -> clears primary_category_term_id to NULL (never invents cat2 as primary!).
    let rem_primary = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "taxonomy": {"key": "category", "operation": "remove", "term_ids": [cat1]}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(rem_primary.status(), StatusCode::OK);
    p1_ver = rem_primary.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let primary_after: Option<i64> =
        sqlx::query_scalar("SELECT primary_category_term_id FROM content_entries WHERE id = ?")
            .bind(p1_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(primary_after, None);

    // 9. Trash p1, then prove stale version hits 412 BEFORE 409 entry_trashed, and current version hits 409 entry_trashed!
    let trash_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/trash",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(trash_resp.status(), StatusCode::OK);
    let trashed_ver = trash_resp.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let stale_on_trashed = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{p1_ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "After Trash"}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_on_trashed.status(), StatusCode::PRECONDITION_FAILED);

    let curr_on_trashed = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{p1_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{trashed_ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "After Trash"}))
        .send()
        .await
        .unwrap();
    assert_eq!(curr_on_trashed.status(), StatusCode::CONFLICT);
    assert!(
        curr_on_trashed
            .text()
            .await
            .unwrap()
            .contains("entry_trashed")
    );
}

#[tokio::test]
async fn s03_metadata_prepare_save_scope_veto_sequential_and_projections() {
    let site = Site::start("s03-hooks").await;
    let client = Client::builder().build().unwrap();
    let (cookie, csrf) = setup_admin(&client, &site.base).await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;

    // Create a post to test hooks & projections.
    let created = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": post_token,
            "title": "Hooked Post",
            "slug": "hooked-post",
            "locale": "vi",
            "publication_status": "published",
            "values": { "body": "<p>Original body</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let entry = created.json::<Value>().await.unwrap()["entry"].clone();
    let entry_id = entry["id"].as_i64().unwrap();
    let mut ver = entry["version"].as_i64().unwrap();

    // Install projection plugin + after_save observer plugin.
    install_and_enable_plugin(
        &client,
        &site.base,
        &cookie,
        &csrf,
        "s03-proj",
        json!({
            "manifest_schema": 3,
            "api_version": 1,
            "min_host_version": "0.1.0",
            "id": "s03-proj",
            "name": "S03 Projection",
            "version": "1.0.0",
            "permissions": { "content_read": ["post"] },
            "filters": ["filter:entry/view"]
        }),
        &static_output_wasm(r#"{"badge":"quick-ok"}"#),
    )
    .await;

    install_and_enable_plugin(
        &client,
        &site.base,
        &cookie,
        &csrf,
        "s03-obs",
        json!({
            "manifest_schema": 3,
            "api_version": 1,
            "min_host_version": "0.1.0",
            "id": "s03-obs",
            "name": "S03 Observer",
            "version": "1.0.0",
            "permissions": { "sqlite": ["kv_write"], "content_read": ["post"] },
            "hooks": ["content/after_save"]
        }),
        &kv_set_wasm("after_save_seen", "yes"),
    )
    .await;

    // 1. No-op echo filter (echoes identical title + body) MUST be accepted on metadata patch!
    install_and_enable_plugin(
        &client,
        &site.base,
        &cookie,
        &csrf,
        "s03-echo",
        json!({
            "manifest_schema": 3,
            "api_version": 1,
            "min_host_version": "0.1.0",
            "id": "s03-echo",
            "name": "S03 No-Op Echo",
            "version": "1.0.0",
            "permissions": { "content_read": ["post"], "content_write": ["post"] },
            "filters": ["filter:content/prepare_save"]
        }),
        &static_output_wasm(
            r#"{"title":"Normalized Title","values_patch":{"body":"<p>Original body</p>"}}"#,
        ),
    )
    .await;

    // Patch requesting title change -> filter echoes identical body (no-op) & normalizes title -> ACCEPTED!
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;
    let ok_norm = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "User Typed Title"}))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_norm.status(), StatusCode::OK);
    let ok_body: Value = ok_norm.json().await.unwrap();
    ver = ok_body["version"].as_i64().unwrap();
    assert_eq!(ok_body["title"], "Normalized Title");
    assert_eq!(ok_body["secondary"]["projection"]["status"], "current");

    // Verify projection was recomputed & after_save async hook ran!
    let db = site.db().await;
    let proj_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = ? AND plugin_id = 's03-proj'",
    )
    .bind(entry_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(proj_count, 1);

    let mut obs_seen = false;
    for _ in 0..40 {
        let val: Option<String> = sqlx::query_scalar(
            "SELECT value_json FROM plugin_kv WHERE plugin_id = 's03-obs' AND key = 'after_save_seen'",
        )
        .fetch_optional(&db)
        .await
        .unwrap();
        if val.is_some() {
            obs_seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        obs_seen,
        "after_save hook should fire for committed metadata patch"
    );

    // 2. Same filter now runs when user only edits slug (title NOT requested), and filter tries to change title -> 422 metadata_hook_scope_conflict!
    // First update title to "Other Title" via disabling s03-echo, then re-enabling s03-echo so its "Normalized Title" is an actual unrequested delta.
    disable_plugin(&client, &site.base, &cookie, &csrf, "s03-echo").await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;
    let rename = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "Other Title"}))
        .send()
        .await
        .unwrap();
    assert_eq!(rename.status(), StatusCode::OK);
    ver = rename.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    // Re-enable s03-echo: when we only patch slug, s03-echo tries to change title -> 422 metadata_hook_scope_conflict!
    let re_en = client
        .post(format!("{}/api/admin/plugins/s03-echo/enable", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(re_en.status(), StatusCode::OK);
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;

    let unreq_title = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "slug": "hooked-post-2"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unreq_title.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        unreq_title
            .text()
            .await
            .unwrap()
            .contains("metadata_hook_scope_conflict")
    );
    disable_plugin(&client, &site.base, &cookie, &csrf, "s03-echo").await;

    // 3. Filter trying to modify `values_patch.body` (out-of-scope field delta) -> 422 metadata_hook_scope_conflict!
    install_and_enable_plugin(
        &client,
        &site.base,
        &cookie,
        &csrf,
        "s03-bad-body",
        json!({
            "manifest_schema": 3,
            "api_version": 1,
            "min_host_version": "0.1.0",
            "id": "s03-bad-body",
            "name": "S03 Bad Body Patch",
            "version": "1.0.0",
            "permissions": { "content_read": ["post"], "content_write": ["post"] },
            "filters": ["filter:content/prepare_save"]
        }),
        &static_output_wasm(r#"{"values_patch":{"body":"<p>Tampered body</p>"}}"#),
    )
    .await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;
    let bad_body = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "Will Fail"}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_body.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        bad_body
            .text()
            .await
            .unwrap()
            .contains("metadata_hook_scope_conflict")
    );
    disable_plugin(&client, &site.base, &cookie, &csrf, "s03-bad-body").await;

    // 4. Filter trying to emit actor-only field `publication_status` -> rejected as invalid filter output (422)!
    install_and_enable_plugin(
        &client,
        &site.base,
        &cookie,
        &csrf,
        "s03-bad-status",
        json!({
            "manifest_schema": 3,
            "api_version": 1,
            "min_host_version": "0.1.0",
            "id": "s03-bad-status",
            "name": "S03 Bad Status Output",
            "version": "1.0.0",
            "permissions": { "content_read": ["post"], "content_write": ["post"] },
            "filters": ["filter:content/prepare_save"]
        }),
        &static_output_wasm(r#"{"publication_status":"draft"}"#),
    )
    .await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;
    let bad_status = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "Actor Only Check"}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_status.status(), StatusCode::UNPROCESSABLE_ENTITY);
    disable_plugin(&client, &site.base, &cookie, &csrf, "s03-bad-status").await;

    // 5. `content/before_save` veto blocks metadata patch cleanly with 422 JSON veto and zero version bump!
    install_and_enable_plugin(
        &client,
        &site.base,
        &cookie,
        &csrf,
        "s03-veto",
        json!({
            "manifest_schema": 3,
            "api_version": 1,
            "min_host_version": "0.1.0",
            "id": "s03-veto",
            "name": "S03 Veto",
            "version": "1.0.0",
            "permissions": { "content_read": ["post"] },
            "hooks": ["content/before_save"]
        }),
        &static_output_wasm(
            r#"{"decision":"veto","code":"policy_blocked","message":"Blocked by before_save"}"#,
        ),
    )
    .await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;
    let vetoed = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "title": "Vetoed Title"}))
        .send()
        .await
        .unwrap();
    assert_eq!(vetoed.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let veto_json: Value = vetoed.json().await.unwrap();
    assert_eq!(veto_json["code"], "policy_blocked");
    disable_plugin(&client, &site.base, &cookie, &csrf, "s03-veto").await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;

    // 6. Unpublish via metadata patch invalidates public projections transactionally!
    let unpub = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"expected_schema_token": post_token, "publication_status": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unpub.status(), StatusCode::OK);
    let proj_after_unpub: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = ? AND plugin_id = 's03-proj'",
    )
    .bind(entry_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(proj_after_unpub, 0);
}

#[tokio::test]
async fn s03_draft_quick_edit_slug_collision_publish_and_parked_locale_guard() {
    let site = Site::start("s03-drift-f01-f03-f06").await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = setup_admin(&client, &site.base).await;
    let post_token = schema_token(&client, &site.base, &cookie, "post").await;

    // F-01: Create never-published draft post, Quick Edit slug (collision rejected + valid rename),
    // then Publish via /publication and verify public URL serves 200 at new slug & 404 at old slug.
    let created = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": post_token,
            "title": "Draft Rename Target",
            "slug": "draft-orig-slug",
            "locale": "vi",
            "publication_status": "draft",
            "values": { "body": "<p>Draft body</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let entry = created.json::<Value>().await.unwrap()["entry"].clone();
    let id = entry["id"].as_i64().unwrap();
    let ver = entry["version"].as_i64().unwrap();

    // Reserved root slug collision ("admin") MUST be rejected even on a never-published draft.
    let bad_slug = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "slug": "admin"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        bad_slug.status() == StatusCode::CONFLICT
            || bad_slug.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "reserved slug collision on draft quick edit must fail, got {}",
        bad_slug.status()
    );

    // Valid slug rename on never-published draft MUST update permalink_route_claims without creating 301 redirect.
    let renamed = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({
            "expected_schema_token": post_token,
            "slug": "draft-renamed-slug"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(renamed.status(), StatusCode::OK);
    let ver = renamed.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let db = site.db().await;
    let claimed_paths: Vec<String> = sqlx::query_scalar(
        "SELECT path FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(id.to_string())
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(claimed_paths, vec!["/draft-renamed-slug".to_string()]);
    let redirect_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(id.to_string())
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(redirect_count, 0);

    // Publish via /publication endpoint -> new slug returns 200, old slug returns 404.
    let pub_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);
    let pub_body = pub_resp.json::<Value>().await.unwrap();
    let pub_ver = pub_body["version"].as_i64().unwrap();
    assert_eq!(pub_ver, ver + 1);

    assert_eq!(
        client
            .get(format!("{}/draft-renamed-slug", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{}/draft-orig-slug", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // F-06: Idempotent no-op publish MUST NOT increment version or enqueue hooks/projections.
    let noop_pub = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{pub_ver}\""))
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(noop_pub.status(), StatusCode::OK);
    let noop_body = noop_pub.json::<Value>().await.unwrap();
    assert_eq!(noop_body["version"].as_i64().unwrap(), pub_ver);
    assert_eq!(noop_body["secondary"]["hook_enqueued"].as_u64().unwrap(), 0);
    assert_eq!(noop_body["secondary"]["projection"]["status"], "not_needed");

    // F-03: Enable multilingual, create secondary locale ("en") entry, Trash it, then park secondary locale
    // (multilingual_enabled = false). Entry table MUST exclude parked secondary-locale rows.
    let loc_state = client
        .get(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let mut loc_ver = loc_state["version"].as_i64().unwrap();
    let ml_on = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{loc_ver}\""))
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(ml_on.status(), StatusCode::OK);
    loc_ver = ml_on.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let add_en = client
        .post(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{loc_ver}\""))
        .json(&json!({"code": "en", "label": "English", "direction": "ltr"}))
        .send()
        .await
        .unwrap();
    assert_eq!(add_en.status(), StatusCode::CREATED);
    loc_ver = add_en.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let en_created = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": post_token,
            "title": "Parked EN Post",
            "slug": "parked-en-post",
            "locale": "en",
            "publication_status": "draft",
            "values": { "body": "<p>EN body</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(en_created.status(), StatusCode::CREATED);
    let en_entry = en_created.json::<Value>().await.unwrap()["entry"].clone();
    let en_id = en_entry["id"].as_i64().unwrap();
    let en_ver = en_entry["version"].as_i64().unwrap();
    let en_trash = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{en_id}/trash",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{en_ver}\""))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_trash.status(), StatusCode::OK);

    // Park secondary locale by disabling multilingual mode.
    let ml_off = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{loc_ver}\""))
        .json(&json!({"enabled": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(ml_off.status(), StatusCode::OK);

    let table_trash = client
        .get(format!(
            "{}/api/admin/content-types/post/entry-table?status=trash",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        table_trash["total"].as_i64().unwrap(),
        0,
        "parked secondary-locale Trash rows must be excluded from ordinary entry-table"
    );
    assert_eq!(table_trash["status_counts"]["trash"].as_i64().unwrap(), 0);

    // Explicit ?locale=<parked> on both trash and active views MUST also exclude parked rows.
    for query in ["status=trash&locale=en", "status=all&locale=en"] {
        let explicit_resp = client
            .get(format!(
                "{}/api/admin/content-types/post/entry-table?{query}",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(explicit_resp.status(), StatusCode::OK);
        let body = explicit_resp.json::<Value>().await.unwrap();
        assert_eq!(
            body["total"].as_i64().unwrap(),
            0,
            "explicit parked {query} must return 0 total"
        );
        assert_eq!(body["items"].as_array().unwrap().len(), 0);
        assert_eq!(body["status_counts"]["trash"].as_i64().unwrap(), 0);
        assert_eq!(body["status_counts"]["all"].as_i64().unwrap(), 0);
    }
}
