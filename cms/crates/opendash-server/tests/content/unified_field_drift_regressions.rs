use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

struct Site {
    base: String,
    root: PathBuf,
    process: Child,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    async fn start(label: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-drift-http-{label}-{}-{stamp}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let process = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..180 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                return Self {
                    base,
                    root,
                    process,
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("isolated test server failed to start at {base}");
    }

    async fn db(&self) -> SqlitePool {
        let opts = SqliteConnectOptions::new()
            .filename(self.root.join("database/opendash.sqlite"))
            .foreign_keys(true)
            .create_if_missing(false);
        SqlitePool::connect_with(opts).await.unwrap()
    }
}

fn make_png() -> Vec<u8> {
    let mut img = image::RgbaImage::new(4, 4);
    for px in img.pixels_mut() {
        *px = image::Rgba([24, 144, 255, 255]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

async fn login(client: &Client, site: &Site, user: &str, pass: &str) -> (String, String) {
    let resp = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username": user, "password": pass}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let v: Value = resp.json().await.unwrap();
    (cookie, v["csrf_token"].as_str().unwrap().to_string())
}

async fn setup_admin(client: &Client, site: &Site) -> (String, String) {
    let setup = client
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username": "admin_reg", "password": "Local-Drift-Reg-123456!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, site, "admin_reg", "Local-Drift-Reg-123456!").await
}

#[tokio::test]
async fn f02_f16_term_extensions_order_read_only_acl_validation_and_media_pin() {
    let site = Site::start("f02-f16").await;
    let client = Client::new();
    let (admin, admin_csrf) = setup_admin(&client, &site).await;
    let db = site.db().await;

    // Upload image asset for term extension media field
    let up = client
        .post(format!("{}/api/admin/media", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header("Content-Type", "image/png")
        .header("X-File-Name", "term-cover.png")
        .body(make_png())
        .send()
        .await
        .unwrap();
    assert_eq!(up.status(), StatusCode::CREATED);
    let img_id = up.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create published VI post and foreign EN post for EntryRef locale validation
    let created_post = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"title": "Vi Post", "slug": "vi-post", "values": {"body": "Xin chao"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(created_post.status(), StatusCode::CREATED);
    let vi_post: Value = created_post.json().await.unwrap();
    let vi_post_id = vi_post["id"].as_i64().unwrap();
    let vi_post_ver = vi_post["version"].as_i64().unwrap();
    let pub_vi_post = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{vi_post_id}/publication",
            site.base
        ))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{vi_post_ver}\""))
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_vi_post.status(), StatusCode::OK);
    sqlx::query(
        "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
         VALUES ('en', 'en', 'English', 'ltr', 0, 1, 2) ON CONFLICT(code) DO NOTHING",
    )
    .execute(&db)
    .await
    .unwrap();
    let en_post_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
         VALUES ('post', 'en', 'tg-en-post', 'en-post', 'EN Post', '{}', 'draft') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
        .bind(en_post_id)
        .execute(&db)
        .await
        .unwrap();

    // Create category term + editor user
    let create_term = client
        .post(format!("{}/api/admin/taxonomy/terms", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"kind": "category", "name": "Tin Nổi Bật", "slug": "tin-noi-bat"}))
        .send()
        .await
        .unwrap();
    assert_eq!(create_term.status(), StatusCode::CREATED);
    let term: Value = create_term.json().await.unwrap();
    let term_id = term["id"].as_i64().unwrap();
    let mut term_ver = term["version"].as_i64().unwrap();

    let add_editor = client
        .post(format!("{}/api/admin/users", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"username": "editor_term", "password": "Local-Editor-Term-123456!", "role": "editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(add_editor.status(), StatusCode::CREATED);
    let (editor, editor_csrf) =
        login(&client, &site, "editor_term", "Local-Editor-Term-123456!").await;

    // Create two Term field groups attached to `category`: one inactive (skipped) and one active ordered group.
    let inactive_fg = client
        .post(format!("{}/api/admin/content-field-groups", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "ui_only",
            "title": "UI Only Inactive",
            "active": false,
            "location": [[{"param": "taxonomy_term", "op": "eq", "value": "category"}]],
            "presentation": {"region": "main_bottom", "order": 1, "collapsible": true},
            "fields": [{"key": "info_msg", "label": "Ghi chú", "type": "message", "body": "Thông tin bổ sung"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(inactive_fg.status(), StatusCode::CREATED);

    let ordered_fg = client
        .post(format!("{}/api/admin/content-field-groups", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "seo_meta",
            "title": "Mở rộng",
            "location": [[{"param": "taxonomy_term", "op": "eq", "value": "category"}]],
            "presentation": {"region": "main_bottom", "order": 2, "collapsible": true},
            "fields": [
                {"key": "tab_intro", "label": "Tổng quan", "type": "tab"},
                {"key": "zeta_note", "label": "Ghi chú Z", "type": "text", "acl": {"min_view_role": "editor", "min_edit_role": "editor"}},
                {"key": "admin_lock", "label": "Khóa Admin", "type": "text", "acl": {"min_view_role": "editor", "min_edit_role": "admin"}},
                {"key": "hero_img", "label": "Ảnh", "type": "media", "media_kind": "image"},
                {"key": "ref_post", "label": "Bài ghim", "type": "entry_ref", "target_types": ["post"]}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ordered_fg.status(), StatusCode::CREATED);

    let cfg_resp = client
        .get(format!(
            "{}/api/admin/taxonomy/terms/{term_id}/effective-editor-config",
            site.base
        ))
        .header("Cookie", &editor)
        .send()
        .await
        .unwrap();
    assert_eq!(cfg_resp.status(), StatusCode::OK);
    let cfg: Value = cfg_resp.json().await.unwrap();
    assert_eq!(cfg["groups"].as_array().unwrap().len(), 1);
    let panel = &cfg["groups"][0];
    assert_eq!(panel["group_id"], "user:seo_meta");
    let keys: Vec<&str> = panel["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        vec![
            "tab_intro",
            "zeta_note",
            "admin_lock",
            "hero_img",
            "ref_post"
        ]
    );
    assert_eq!(panel["read_only_keys"], json!(["admin_lock"]));
    let schema_token = cfg["schema_token"].as_str().unwrap().to_string();

    // F-02: ACL 403 check must run before schema_token 409 check.
    let stale_forged = client
        .patch(format!("{}/api/admin/taxonomy/terms/{term_id}", site.base))
        .header("Cookie", &editor)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", format!("\"v{term_ver}\""))
        .json(&json!({"values": {"admin_lock": "x"}, "schema_token": format!("sha256:{}", "0".repeat(64))}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_forged.status(), StatusCode::FORBIDDEN);

    // F-02: Cross-locale EntryRef fails 400.
    let wrong_loc = client
        .patch(format!("{}/api/admin/taxonomy/terms/{term_id}", site.base))
        .header("Cookie", &editor)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", format!("\"v{term_ver}\""))
        .json(&json!({"values": {"ref_post": en_post_id}, "schema_token": schema_token}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_loc.status(), StatusCode::BAD_REQUEST);

    // Save valid term field group values (image media + same-locale EntryRef + text).
    let ok_patch = client
        .patch(format!("{}/api/admin/taxonomy/terms/{term_id}", site.base))
        .header("Cookie", &editor)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", format!("\"v{term_ver}\""))
        .json(&json!({
            "values": {"zeta_note": "first", "hero_img": img_id, "ref_post": vi_post_id},
            "schema_token": schema_token
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_patch.status(), StatusCode::OK);
    term_ver = ok_patch.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    // Inject stale key into taxonomy_terms.values_json -> subsequent valid edit still succeeds.
    sqlx::query(
        "UPDATE taxonomy_terms SET values_json = json_set(values_json, '$.legacy_removed_key', 'stale') WHERE id = ?",
    )
    .bind(term_id)
    .execute(&db)
    .await
    .unwrap();
    let ok_after_stale = client
        .patch(format!("{}/api/admin/taxonomy/terms/{term_id}", site.base))
        .header("Cookie", &editor)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", format!("\"v{term_ver}\""))
        .json(&json!({
            "values": {"zeta_note": "second"},
            "schema_token": schema_token
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_after_stale.status(), StatusCode::OK);

    // F-16: Deleting media pinned inside taxonomy_terms.values_json/term_field_media_refs fails with 409 pin conflict.
    let del_pinned = client
        .delete(format!("{}/api/admin/media/{img_id}", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_pinned.status(), StatusCode::CONFLICT);
    let pin_body: Value = del_pinned.json().await.unwrap();
    assert_eq!(pin_body["pins"]["pins"][0]["source_kind"], "term_meta");
}

#[tokio::test]
async fn f08_f11_entry_relational_refs_across_groups_and_private_context_options() {
    let site = Site::start("f08-f11").await;
    let client = Client::new();
    let (admin, csrf) = setup_admin(&client, &site).await;
    let db = site.db().await;

    sqlx::query(
        "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
         VALUES ('en', 'en', 'English', 'ltr', 0, 1, 2) ON CONFLICT(code) DO NOTHING",
    )
    .execute(&db)
    .await
    .unwrap();
    let vi_cat_resp = client
        .post(format!("{}/api/admin/taxonomy/terms", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"kind": "category", "name": "Dịch vụ", "slug": "dich-vu"}))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_cat_resp.status(), StatusCode::CREATED);
    let vi_cat_id = vi_cat_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let en_cat_id: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind,slug,name,locale,translation_group_id) VALUES ('category','services-en','Services EN','en','tg-cat-en') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();

    let vi_post_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title": "Base Post", "slug": "base-post", "values": {"body": "Hello"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_post_resp.status(), StatusCode::CREATED);
    let vi_post_val: Value = vi_post_resp.json().await.unwrap();
    let vi_post_id = vi_post_val["id"].as_i64().unwrap();
    let pub_vi_post = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{vi_post_id}/publication",
            site.base
        ))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(
            "If-Match",
            format!("\"v{}\"", vi_post_val["version"].as_i64().unwrap()),
        )
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_vi_post.status(), StatusCode::OK);

    let create_cpt = client
        .post(format!("{}/api/admin/content-types", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "key": "case_study",
            "label": "Case Study"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_cpt.status(), StatusCode::CREATED);
    let create_base_fg = client
        .post(format!("{}/api/admin/content-field-groups", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "local_key": "case_base_fields",
            "title": "Case Base Fields",
            "target_types": ["case_study"],
            "fields": [
                {"key": "featured_post", "label": "Bài nổi bật", "type": "entry_ref", "target_types": ["post"]},
                {"key": "secret_note", "label": "Ghi chú riêng", "type": "text", "acl": {"min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "private"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_base_fg.status(), StatusCode::CREATED);

    let create_fg = client
        .post(format!("{}/api/admin/content-field-groups", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "local_key": "case_group",
            "title": "Case Group",
            "target_types": ["case_study"],
            "fields": [{"key": "topic_cat", "label": "Chuyên mục", "type": "term_ref", "taxonomy": "category"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_fg.status(), StatusCode::CREATED);

    // F-11: options-source=context_fields omits private field `secret_note`, keeps public `featured_post`.
    let opts_resp = client
        .get(format!(
            "{}/api/admin/fields/options-source?source=context_fields&type_key=case_study",
            site.base
        ))
        .header("Cookie", &admin)
        .send()
        .await
        .unwrap();
    assert_eq!(opts_resp.status(), StatusCode::OK);
    let opts: Value = opts_resp.json().await.unwrap();
    let opt_vals: Vec<&str> = opts["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["value"].as_str().unwrap())
        .collect();
    assert!(opt_vals.contains(&"featured_post"));
    assert!(!opt_vals.contains(&"secret_note"));

    // F-08: Invalid attached-group TermRef (cross-locale) and missing base EntryRef reject (400).
    let cfg_resp = client
        .get(format!(
            "{}/api/admin/content-types/case_study/effective-editor-config",
            site.base
        ))
        .header("Cookie", &admin)
        .send()
        .await
        .unwrap();
    assert_eq!(cfg_resp.status(), StatusCode::OK);
    let schema_token = cfg_resp.json::<Value>().await.unwrap()["schema_token"]
        .as_str()
        .unwrap()
        .to_string();

    let composite_url = format!(
        "{}/api/admin/content-types/case_study/entries/composite",
        site.base
    );
    let bad_group_term = client
        .post(&composite_url)
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": &schema_token,
            "title": "Bad Group Term",
            "slug": "bad-group-term",
            "values": {"featured_post": vi_post_id, "topic_cat": en_cat_id}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_group_term.status(), StatusCode::BAD_REQUEST);
    assert!(bad_group_term.text().await.unwrap().contains("term_id"));

    let bad_base_entry = client
        .post(format!(
            "{}/api/admin/content-types/case_study/entries",
            site.base
        ))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Bad Base Entry",
            "slug": "bad-base-entry",
            "values": {"featured_post": 999_999}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_base_entry.status(), StatusCode::BAD_REQUEST);
    let bad_base_body = bad_base_entry.text().await.unwrap();
    assert!(
        bad_base_body.contains("ID nội dung") || bad_base_body.contains("Invalid content ID"),
        "unexpected bad_base_entry 400 body: {bad_base_body}"
    );

    let ok_save = client
        .post(&composite_url)
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": &schema_token,
            "title": "Valid Case",
            "slug": "valid-case",
            "values": {"featured_post": vi_post_id, "topic_cat": vi_cat_id}
        }))
        .send()
        .await
        .unwrap();
    let ok_status = ok_save.status();
    let ok_body = ok_save.text().await.unwrap();
    assert_eq!(ok_status, StatusCode::CREATED, "ok_save failed: {ok_body}");
}
