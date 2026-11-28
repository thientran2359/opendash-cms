//! Slice S02 HTTP integration tests: Trash, Restore, Purge, legacy DELETE alias,
//! Reading Page protection, immediate CPT public restore confirmation,
//! Admin-only dormant retained-Trash maintenance, and locale guards.

use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;

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
        panic!("S02 server did not become healthy");
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

async fn locales_etag(client: &Client, base: &str, cookie: &str) -> String {
    let resp = client
        .get(format!("{base}/api/admin/locales"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    resp.headers()
        .get(header::ETAG)
        .expect("locales ETag")
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn s02_http_trash_restore_purge_legacy_delete_and_rbac() {
    let site = Site::start("s02-http").await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &site.base).await;

    // Create Editor account
    let mk_editor = client
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"username":"editor1","password":"EditorPassword123!","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(mk_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&client, &site.base, "editor1", "EditorPassword123!").await;

    // Create an immediate-mode public CPT `product` + FieldGroup
    let mk_cpt = client
        .post(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "product",
            "label": "Product",
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "immediate",
                "supports": ["editor"]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mk_cpt.status(), StatusCode::CREATED);
    let mk_fg = client
        .post(format!("{}/api/admin/content-field-groups", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "product_fields",
            "title": "Product Fields",
            "target_types": ["product"],
            "fields": [{"key": "sku", "label": "SKU", "type": "text", "required": false}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mk_fg.status(), StatusCode::CREATED);

    // 1. Create and publish a Post; verify published_at retained across Trash -> Restore -> Draft
    let post_create = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({"slug":"s02-post","title":"S02 Post","values":{"body":"hello"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(post_create.status(), StatusCode::CREATED);
    let post: Value = post_create.json().await.unwrap();
    let post_id = post["id"].as_i64().unwrap();

    let pub_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);
    let pub_json: Value = pub_resp.json().await.unwrap();
    assert!(pub_json.get("secondary").is_some(), "{pub_json}");

    assert_eq!(
        client
            .get(format!("{}/s02-post", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // Stale CAS on Trash -> 412
    let stale_trash = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/trash",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_trash.status(), StatusCode::PRECONDITION_FAILED);

    // Editor moves Post to Trash -> 200 with ETag "v3" and Cache-Control: no-store
    let trash_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/trash",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(trash_resp.status(), StatusCode::OK);
    assert_eq!(trash_resp.headers().get(header::ETAG).unwrap(), "\"v3\"");
    assert_eq!(
        trash_resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let trash_body: Value = trash_resp.json().await.unwrap();
    assert_eq!(trash_body["version"], json!(3));
    assert!(trash_body["trashed_at"].as_str().is_some());
    let retained_published_at = trash_body["published_at"].as_str().unwrap().to_owned();

    // Public URL and active admin GET both 404
    assert_eq!(
        client
            .get(format!("{}/s02-post", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/post/entries/{post_id}",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // entry-table?status=trash returns metadata-only row with capabilities
    let trash_table: Value = client
        .get(format!(
            "{}/api/admin/content-types/post/entry-table?status=trash",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(trash_table["status_counts"]["trash"], json!(1));
    assert_eq!(trash_table["items"].as_array().unwrap().len(), 1);
    let row = &trash_table["items"][0];
    assert_eq!(row["id"], json!(post_id));
    assert_eq!(row["lifecycle"], json!("trash"));
    assert_eq!(row["fields"], json!({}));
    assert_eq!(row["term_ids"], json!([]));
    assert!(row["public_url"].is_null());
    assert_eq!(row["capabilities"]["restore"], json!(true));
    assert_eq!(row["capabilities"]["purge"], json!(false));

    // Editor Purge -> 403
    let editor_purge = client
        .delete(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/purge",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(editor_purge.status(), StatusCode::FORBIDDEN);

    // Admin Purge with stale version -> 412 BEFORE 409 missing confirmation header
    let admin_stale_purge = client
        .delete(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/purge",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .send()
        .await
        .unwrap();
    assert_eq!(admin_stale_purge.status(), StatusCode::PRECONDITION_FAILED);

    // Admin Purge with current version but missing X-Confirm-Purge -> 409 confirmation_required
    let admin_no_hdr_purge = client
        .delete(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/purge",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .send()
        .await
        .unwrap();
    assert_eq!(admin_no_hdr_purge.status(), StatusCode::CONFLICT);
    assert_eq!(
        admin_no_hdr_purge
            .headers()
            .get("X-OpenDash-Error-Code")
            .unwrap(),
        "confirmation_required"
    );

    // Editor restores Post -> returns to draft, preserves published_at, remains private
    let restore_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/restore",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(restore_resp.status(), StatusCode::OK);
    let restore_json: Value = restore_resp.json().await.unwrap();
    assert_eq!(restore_json["publication_status"], json!("draft"));
    assert!(restore_json["trashed_at"].is_null());
    assert_eq!(
        restore_json["published_at"].as_str(),
        Some(retained_published_at.as_str())
    );
    assert_eq!(
        client
            .get(format!("{}/s02-post", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // 2. Reading-assigned Page blocks Trash and reports blocked_reasons in entry-table
    let page_create = client
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"s02-home","title":"S02 Home","values":{"body":"home"}}))
        .send()
        .await
        .unwrap();
    let page_id = page_create.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/page/entries/{page_id}/publication",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let reading_get = client
        .get(format!("{}/api/admin/settings/reading", site.base))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    let reading_etag = reading_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        client
            .put(format!("{}/api/admin/settings/reading", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &reading_etag)
            .json(&json!({"homepage_page_id": page_id, "posts_page_id": null}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let page_table: Value = client
        .get(format!(
            "{}/api/admin/content-types/page/entry-table",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let home_row = page_table["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(page_id))
        .unwrap();
    assert_eq!(home_row["capabilities"]["trash"], json!(false));
    assert_eq!(
        home_row["blocked_reasons"]["trash"],
        json!("reading_page_assigned")
    );

    let blocked_page_trash = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{page_id}/trash",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked_page_trash.status(), StatusCode::CONFLICT);

    // 3. Immediate CPT: legacy DELETE -> Trash, Restore requires confirm_public_restore: true,
    // then Admin Purge deletes row physically.
    let prod_create = client
        .post(format!(
            "{}/api/admin/content-types/product/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"widget-1","title":"Widget 1","values":{"sku":"W1"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(prod_create.status(), StatusCode::CREATED);
    let prod_id = prod_create.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let legacy_del = client
        .delete(format!(
            "{}/api/admin/content-types/product/entries/{prod_id}",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(legacy_del.status(), StatusCode::NO_CONTENT);

    // Restore immediate CPT without confirmation -> 409 confirmation_required
    let restore_no_confirm = client
        .post(format!(
            "{}/api/admin/content-types/product/entries/{prod_id}/restore",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(restore_no_confirm.status(), StatusCode::CONFLICT);
    assert_eq!(
        restore_no_confirm
            .headers()
            .get("X-OpenDash-Error-Code")
            .unwrap(),
        "confirmation_required"
    );

    // Restore immediate CPT with confirm_public_restore: true -> 200 published immediately
    let restore_confirmed = client
        .post(format!(
            "{}/api/admin/content-types/product/entries/{prod_id}/restore",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({"confirm_public_restore": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(restore_confirmed.status(), StatusCode::OK);
    let restore_prod: Value = restore_confirmed.json().await.unwrap();
    assert_eq!(restore_prod["publication_status"], json!("published"));
    assert_eq!(restore_prod["version"], json!(3));

    // Trash & explicitly Purge product
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/product/entries/{prod_id}/trash",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v3\"")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let purged = client
        .delete(format!(
            "{}/api/admin/content-types/product/entries/{prod_id}/purge",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v4\"")
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(purged.status(), StatusCode::NO_CONTENT);

    // 4. Pattern generic lifecycle rejected (403)
    let pat_trash = client
        .post(format!(
            "{}/api/admin/content-types/pattern/entries/1/trash",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pat_trash.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn s02_dormant_retained_trash_maintenance_and_locale_guards() {
    let site = Site::start("s02-dormant").await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &site.base).await;

    let mk_editor = client
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"username":"ed2","password":"EditorPassword123!","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(mk_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&client, &site.base, "ed2", "EditorPassword123!").await;

    // Enable multilingual and add `en`
    let loc_etag = locales_etag(&client, &site.base, &admin_cookie).await;
    assert_eq!(
        client
            .put(format!("{}/api/admin/locales/multilingual", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &loc_etag)
            .json(&json!({"enabled": true}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let loc_etag = locales_etag(&client, &site.base, &admin_cookie).await;
    assert_eq!(
        client
            .post(format!("{}/api/admin/locales", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &loc_etag)
            .json(&json!({"code": "en", "label": "English"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    // Create CPT `catalog`, add VI & EN entries, trash both
    assert_eq!(
        client
            .post(format!("{}/api/admin/content-types", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "key": "catalog",
                "label": "Catalog",
                "settings": {"public": true, "has_archive": false, "publication": "draft_publish", "supports": ["editor"]}
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    let vi_id = client
        .post(format!(
            "{}/api/admin/content-types/catalog/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"cat-vi","title":"Cat VI","locale":"vi","values":{}}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let en_id = client
        .post(format!(
            "{}/api/admin/content-types/catalog/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"cat-en","title":"Cat EN","locale":"en","values":{}}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    for id in [vi_id, en_id] {
        assert_eq!(
            client
                .post(format!(
                    "{}/api/admin/content-types/catalog/entries/{id}/trash",
                    site.base
                ))
                .header(header::COOKIE, &admin_cookie)
                .header("Origin", &site.base)
                .header("X-CSRF-Token", &admin_csrf)
                .header(header::IF_MATCH, "\"v1\"")
                .json(&json!({}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    // Active provider rejects retained_trash=true maintenance read & purge (409 provider_active)
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/catalog/entry-table?status=trash&retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .delete(format!(
                "{}/api/admin/content-types/catalog/entries/{vi_id}/purge?retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .header("X-Confirm-Purge", "true")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // Disable `en` locale: restore of `en_id` blocked with 409 locale_disabled
    let loc_etag = locales_etag(&client, &site.base, &admin_cookie).await;
    assert_eq!(
        client
            .put(format!("{}/api/admin/locales/en", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &loc_etag)
            .json(&json!({"label": "English", "direction": "ltr", "enabled": false, "position": 1}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/catalog/entries/{en_id}/restore",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // Re-enable `en` and mark `catalog` provider_inactive (dormant)
    let loc_etag = locales_etag(&client, &site.base, &admin_cookie).await;
    assert_eq!(
        client
            .put(format!("{}/api/admin/locales/en", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &loc_etag)
            .json(&json!({"label": "English", "direction": "ltr", "enabled": true, "position": 1}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let db = site.db().await;
    sqlx::query(
        "UPDATE content_types SET owner_kind = 'plugin', owner_id = 'dormant-plugin', provider_status = 'provider_inactive' WHERE key = 'catalog'",
    )
    .execute(&db)
    .await
    .unwrap();

    // Editor cannot read or purge retained Trash (403)
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/catalog/entry-table?status=trash&retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &editor_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .delete(format!(
                "{}/api/admin/content-types/catalog/entries/{vi_id}/purge?retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .header("X-Confirm-Purge", "true")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // Strict query allowlist: extra filter or non-trash status rejected (400)
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/catalog/entry-table?status=trash&retained_trash=true&q=cat",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .get(format!(
                "{}/api/admin/content-types/catalog/entry-table?status=all&retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    // Admin retained-Trash read succeeds with metadata-only rows and Purge-only capabilities
    let retained: Value = client
        .get(format!(
            "{}/api/admin/content-types/catalog/entry-table?status=trash&retained_trash=true",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(retained["total"], json!(2));
    assert_eq!(
        retained["status_counts"],
        json!({"all":0,"draft":0,"published":0,"trash":2})
    );
    assert!(retained["schema_token"].is_null());
    for item in retained["items"].as_array().unwrap() {
        assert_eq!(item["fields"], json!({}));
        assert_eq!(item["term_ids"], json!([]));
        assert!(item["public_url"].is_null());
        assert_eq!(item["capabilities"]["purge"], json!(true));
        assert_eq!(item["capabilities"]["restore"], json!(false));
        assert_eq!(item["capabilities"]["edit"], json!(false));
    }

    // Ordinary purge without retained_trash=true rejects dormant CPT (409)
    assert_eq!(
        client
            .delete(format!(
                "{}/api/admin/content-types/catalog/entries/{vi_id}/purge",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .header("X-Confirm-Purge", "true")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );

    // Maintenance purge with retained_trash=true removes vi_id physically (204)
    assert_eq!(
        client
            .delete(format!(
                "{}/api/admin/content-types/catalog/entries/{vi_id}/purge?retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .header("X-Confirm-Purge", "true")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );

    // Delete `en` locale -> orphan locale must NOT be listed or purged via retained_trash
    let loc_etag = locales_etag(&client, &site.base, &admin_cookie).await;
    assert_eq!(
        client
            .delete(format!("{}/api/admin/locales/en", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, &loc_etag)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let after_del_locale: Value = client
        .get(format!(
            "{}/api/admin/content-types/catalog/entry-table?status=trash&retained_trash=true",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after_del_locale["total"], json!(0));
    assert_eq!(
        client
            .delete(format!(
                "{}/api/admin/content-types/catalog/entries/{en_id}/purge?retained_trash=true",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .header("X-Confirm-Purge", "true")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    db.close().await;
}
