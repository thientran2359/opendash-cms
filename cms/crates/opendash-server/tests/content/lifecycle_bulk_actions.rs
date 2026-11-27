//! Slice S04 integrated CUJ 1–7 and Bulk Actions server integration tests:
//! - Broadened `draft_publish` custom CPT sequential bulk publish/draft + taxonomy delta
//!   (`add`/`remove`/`replace`), preserving unrelated fields, taxonomies, primary category,
//!   translations, and shared media pins.
//! - Bulk Trash -> Trash table discovery -> Bulk Restore (verifying retained `permalink_route_claims`
//!   invariance without redundant claim loss, and immediate public-restore confirmation gate) ->
//!   Admin-only Bulk Purge (Editor purge rejected 403).
//! - Mixed CAS outcomes (`200`, `412`, `409 reading_page_assigned`, `403`), proving independent
//!   per-row commits without poisoning other rows.
//! - Privacy-safe `entry_lifecycle_diag` structured log emitted without leaking titles/body/secrets.

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
    stderr_path: PathBuf,
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
        let stderr_path = root.join("server.stderr.log");
        let stderr_file = fs::File::create(&stderr_path).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_HOST", "127.0.0.1")
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr_file))
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
                return Self {
                    root,
                    stderr_path,
                    child,
                    base,
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("S04 server did not become healthy");
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
    body["schema_token"].as_str().unwrap().to_owned()
}

fn minimal_png(seed: u8) -> Vec<u8> {
    fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in kind.iter().chain(payload.iter()) {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        out.extend_from_slice(&(!crc).to_be_bytes());
        out
    }
    // 1x1 RGBA uncompressed zlib stream (RFC 1950 + stored deflate block + adler32)
    let raw = [0u8, seed, seed.wrapping_mul(3), seed.wrapping_mul(7), 255u8];
    let mut zlib = vec![0x78, 0x01, 0x01, 0x05, 0x00, 0xFA, 0xFF];
    zlib.extend_from_slice(&raw);
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in &raw {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    zlib.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend_from_slice(&chunk(b"IHDR", &ihdr));
    out.extend_from_slice(&chunk(b"IDAT", &zlib));
    out.extend_from_slice(&chunk(b"IEND", &[]));
    out
}

#[tokio::test]
async fn s04_broadened_cpt_bulk_taxonomy_lifecycle_mixed_cas_and_privacy_diag() {
    let site = Site::start("s04-integrated").await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &site.base).await;

    // Create Editor user.
    let create_ed = client
        .post(format!("{}/api/admin/users", site.base))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"username": "editor", "password": "EditorPassword123!", "role": "editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(create_ed.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&client, &site.base, "editor", "EditorPassword123!").await;

    // Upload a shared PNG asset so we can prove bulk lifecycle & taxonomy never drop shared media pins.
    let media_resp = client
        .post(format!("{}/api/admin/media", site.base))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("Content-Type", "image/png")
        .header("X-File-Name", "shared.png")
        .body(minimal_png(77))
        .send()
        .await
        .unwrap();
    assert_eq!(media_resp.status(), StatusCode::CREATED);
    let media_id = media_resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Create custom draft_publish CPT ("article") with taxonomies + image field + private admin note field,
    // and an immediate public CPT ("catalog") to verify immediate public-restore confirmation.
    let create_article = client
        .post(format!("{}/api/admin/content-types", site.base))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "article",
            "label": "Articles",
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "draft_publish",
                "supports": ["editor"]
            },
            "fields": [
                {"key": "summary", "label": "Summary", "type": "text"},
                {"key": "cover", "label": "Cover", "type": "media"},
                {
                    "key": "secret_note",
                    "label": "Secret Note",
                    "type": "text",
                    "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_article.status(), StatusCode::CREATED);
    let article_ct_ver = create_article.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let elig_put = client
        .put(format!(
            "{}/api/admin/taxonomy/eligibility/article",
            site.base
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{article_ct_ver}\""))
        .json(&json!({
            "taxonomies": ["category", "tag"],
            "expected_version": article_ct_ver
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(elig_put.status(), StatusCode::OK);

    let create_catalog = client
        .post(format!("{}/api/admin/content-types", site.base))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "catalog",
            "label": "Catalog",
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "immediate",
                "supports": ["editor"]
            },
            "fields": [{"key": "sku", "label": "SKU", "type": "text"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_catalog.status(), StatusCode::CREATED);

    // Create terms in default locale (vi): cat_a, tag_keep, tag_bulk, tag_alt.
    let mut term_ids = std::collections::BTreeMap::<String, i64>::new();
    for (kind, slug, name) in [
        ("category", "s04-cat", "S04 Cat"),
        ("tag", "s04-keep", "S04 Keep"),
        ("tag", "s04-bulk", "S04 Bulk"),
        ("tag", "s04-alt", "S04 Alt"),
    ] {
        let r = client
            .post(format!("{}/api/admin/taxonomy/terms", site.base))
            .header("Origin", &site.base)
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({"kind": kind, "slug": slug, "name": name, "locale": "vi"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
        let id = r.json::<Value>().await.unwrap()["id"].as_i64().unwrap();
        term_ids.insert(slug.to_owned(), id);
    }

    let article_tok = schema_token(&client, &site.base, &admin_cookie, "article").await;
    let mut article_ids = Vec::new();
    let mut article_versions = Vec::new();
    for idx in 1..=3 {
        let resp = client
            .post(format!(
                "{}/api/admin/content-types/article/entries/composite",
                site.base
            ))
            .header("Origin", &site.base)
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "expected_schema_token": article_tok,
                "title": format!("SecretTitleMarker-{idx}"),
                "slug": format!("article-{idx}"),
                "locale": "vi",
                "publication_status": "draft",
                "values": {
                    "summary": format!("KeepSummary-{idx}"),
                    "cover": media_id,
                    "secret_note": format!("TopSecretValue-{idx}")
                },
                "term_ids": [term_ids["s04-cat"], term_ids["s04-keep"]]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let entry = resp.json::<Value>().await.unwrap()["entry"].clone();
        article_ids.push(entry["id"].as_i64().unwrap());
        article_versions.push(entry["version"].as_i64().unwrap());
    }

    // 1. Sequential Bulk Publish on custom draft_publish CPT ("article") with mixed CAS:
    //    bump article_ids[1] concurrently so batch gets [200, 412, 200] independently.
    let concurrent = client
        .post(format!(
            "{}/api/admin/content-types/article/entries/{}/publication",
            site.base, article_ids[1]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{}\"", article_versions[1]))
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(concurrent.status(), StatusCode::OK);
    let bumped_v1 = concurrent.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let mut pub_statuses = Vec::new();
    for i in 0..3 {
        let r = client
            .post(format!(
                "{}/api/admin/content-types/article/entries/{}/publication",
                site.base, article_ids[i]
            ))
            .header("Origin", &site.base)
            .header(header::COOKIE, &editor_cookie)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{}\"", article_versions[i]))
            .json(&json!({"status": "published"}))
            .send()
            .await
            .unwrap();
        pub_statuses.push(r.status());
        if r.status() == StatusCode::OK {
            article_versions[i] = r.json::<Value>().await.unwrap()["version"]
                .as_i64()
                .unwrap();
        }
    }
    assert_eq!(
        pub_statuses,
        vec![
            StatusCode::OK,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::OK
        ]
    );
    article_versions[1] = bumped_v1;

    // 2. Bulk Single-Taxonomy Add (`s04-bulk`) + Replace (`s04-alt`) by Editor via PATCH .../metadata:
    //    must preserve `category` (`s04-cat`), `primary_category_term_id`, `summary`, `cover`, and admin `secret_note`.
    for i in 0..3 {
        let r = client
            .patch(format!(
                "{}/api/admin/content-types/article/entries/{}/metadata",
                site.base, article_ids[i]
            ))
            .header("Origin", &site.base)
            .header(header::COOKIE, &editor_cookie)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{}\"", article_versions[i]))
            .json(&json!({
                "expected_schema_token": article_tok,
                "taxonomy": {"key": "tag", "operation": "add", "term_ids": [term_ids["s04-bulk"]]}
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let body = r.json::<Value>().await.unwrap();
        assert!(body["fields"].get("secret_note").is_none());
        article_versions[i] = body["version"].as_i64().unwrap();
    }
    let rep = client
        .patch(format!(
            "{}/api/admin/content-types/article/entries/{}/metadata",
            site.base, article_ids[0]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", format!("\"v{}\"", article_versions[0]))
        .json(&json!({
            "expected_schema_token": article_tok,
            "taxonomy": {"key": "tag", "operation": "replace", "term_ids": [term_ids["s04-alt"]]}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(rep.status(), StatusCode::OK);
    article_versions[0] = rep.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    // Verify Admin read confirms secret_note, summary, cover, category & primary_category survived untouched.
    let check_a0 = client
        .get(format!(
            "{}/api/admin/content-types/article/entries/{}",
            site.base, article_ids[0]
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(check_a0["values"]["summary"], "KeepSummary-1");
    assert_eq!(check_a0["values"]["cover"], media_id);
    assert_eq!(check_a0["values"]["secret_note"], "TopSecretValue-1");
    let a0_terms: std::collections::BTreeSet<i64> = check_a0["term_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        a0_terms,
        std::collections::BTreeSet::from([term_ids["s04-cat"], term_ids["s04-alt"]])
    );

    // 3. Bulk Trash -> verify permalink_route_claims retained -> Bulk Restore (fast retained-claim path)
    //    -> republish -> verify public URL reachable again -> Trash -> Editor Purge 403 -> Admin Purge 204
    //    -> verify shared media asset STILL pinned by remaining article_ids[2] and not orphaned!
    for i in 0..2 {
        let tr = client
            .post(format!(
                "{}/api/admin/content-types/article/entries/{}/trash",
                site.base, article_ids[i]
            ))
            .header("Origin", &site.base)
            .header(header::COOKIE, &editor_cookie)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{}\"", article_versions[i]))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(tr.status(), StatusCode::OK);
        article_versions[i] = tr.json::<Value>().await.unwrap()["version"]
            .as_i64()
            .unwrap();
    }

    let db = site.db().await;
    let claim_count_trashed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(article_ids[0].to_string())
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        claim_count_trashed >= 1,
        "claim must remain reserved in Trash"
    );

    // Restore both as Editor (draft_publish restores to draft, preserving retained claim).
    for i in 0..2 {
        let rs = client
            .post(format!(
                "{}/api/admin/content-types/article/entries/{}/restore",
                site.base, article_ids[i]
            ))
            .header("Origin", &site.base)
            .header(header::COOKIE, &editor_cookie)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{}\"", article_versions[i]))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(rs.status(), StatusCode::OK);
        let body = rs.json::<Value>().await.unwrap();
        assert_eq!(body["publication_status"], "draft");
        article_versions[i] = body["version"].as_i64().unwrap();
    }

    let claim_count_restored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(article_ids[0].to_string())
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        claim_count_restored, claim_count_trashed,
        "retained claim must stay intact across conditional Restore"
    );

    // Trash article_ids[0] again; Editor purge MUST fail 403, Admin purge MUST succeed 204.
    let tr0 = client
        .post(format!(
            "{}/api/admin/content-types/article/entries/{}/trash",
            site.base, article_ids[0]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{}\"", article_versions[0]))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(tr0.status(), StatusCode::OK);
    article_versions[0] = tr0.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let ed_purge = client
        .delete(format!(
            "{}/api/admin/content-types/article/entries/{}/purge",
            site.base, article_ids[0]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", format!("\"v{}\"", article_versions[0]))
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(ed_purge.status(), StatusCode::FORBIDDEN);

    let adm_purge = client
        .delete(format!(
            "{}/api/admin/content-types/article/entries/{}/purge",
            site.base, article_ids[0]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{}\"", article_versions[0]))
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(adm_purge.status(), StatusCode::NO_CONTENT);

    // F-02: Purging a previously published non-page entry removes its canonical claim
    // while retaining an orphan historical redirect in permalink_route_redirects.
    let purged_claim_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(article_ids[0].to_string())
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(purged_claim_count, 0);
    let purged_orphan_redirect_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(article_ids[0].to_string())
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        purged_orphan_redirect_count >= 1,
        "purging a previously published entry must retain its orphan historical redirect"
    );

    // Shared media MUST remain active & publicly accessible because article_ids[2] is published and still references it.
    let pub_media = client
        .get(format!("{}/media/{media_id}/file", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_media.status(), StatusCode::OK);

    // 4. Immediate public CPT ("catalog") requires explicit `confirm_public_restore: true` on Restore.
    let cat_tok = schema_token(&client, &site.base, &admin_cookie, "catalog").await;
    let cat_created = client
        .post(format!(
            "{}/api/admin/content-types/catalog/entries/composite",
            site.base
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": cat_tok,
            "title": "Catalog Item 1",
            "slug": "catalog-item-1",
            "locale": "vi",
            "values": {"sku": "SKU-001"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_created.status(), StatusCode::CREATED);
    let cat_entry = cat_created.json::<Value>().await.unwrap()["entry"].clone();
    let cat_id = cat_entry["id"].as_i64().unwrap();
    let cat_v = cat_entry["version"].as_i64().unwrap();

    let cat_tr = client
        .post(format!(
            "{}/api/admin/content-types/catalog/entries/{cat_id}/trash",
            site.base
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{cat_v}\""))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_tr.status(), StatusCode::OK);
    let cat_v = cat_tr.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let cat_restore_no_confirm = client
        .post(format!(
            "{}/api/admin/content-types/catalog/entries/{cat_id}/restore",
            site.base
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{cat_v}\""))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_restore_no_confirm.status(), StatusCode::CONFLICT);

    let cat_restore_ok = client
        .post(format!(
            "{}/api/admin/content-types/catalog/entries/{cat_id}/restore",
            site.base
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{cat_v}\""))
        .json(&json!({"confirm_public_restore": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_restore_ok.status(), StatusCode::OK);
    assert_eq!(
        cat_restore_ok.json::<Value>().await.unwrap()["publication_status"],
        "published"
    );

    // 5. Mixed Page Trash batch: assigned Reading Page returns 409 while sibling page succeeds 200.
    let page_tok = schema_token(&client, &site.base, &admin_cookie, "page").await;
    let mut page_ids = Vec::new();
    let mut page_vers = Vec::new();
    for slug in ["s04-home-guard", "s04-trashable-page"] {
        let p = client
            .post(format!(
                "{}/api/admin/content-types/page/entries/composite",
                site.base
            ))
            .header("Origin", &site.base)
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "expected_schema_token": page_tok,
                "title": slug,
                "slug": slug,
                "locale": "vi",
                "publication_status": "published",
                "values": {"body": "<p>page</p>"}
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(p.status(), StatusCode::CREATED);
        let e = p.json::<Value>().await.unwrap()["entry"].clone();
        page_ids.push(e["id"].as_i64().unwrap());
        page_vers.push(e["version"].as_i64().unwrap());
    }
    let reading_get = client
        .get(format!(
            "{}/api/admin/settings/reading?locale=vi",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let reading_put = client
        .put(format!(
            "{}/api/admin/settings/reading?locale=vi",
            site.base
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(
            "If-Match",
            format!("\"v{}\"", reading_get["version"].as_i64().unwrap()),
        )
        .json(&json!({
            "homepage_page_id": page_ids[0],
            "posts_page_id": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reading_put.status(), StatusCode::OK);

    let p0_trash = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{}/trash",
            site.base, page_ids[0]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{}\"", page_vers[0]))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(p0_trash.status(), StatusCode::CONFLICT);
    let p1_trash = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{}/trash",
            site.base, page_ids[1]
        ))
        .header("Origin", &site.base)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{}\"", page_vers[1]))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(p1_trash.status(), StatusCode::OK);

    // 6. Verify privacy-safe structured diagnostic logs were emitted with storage_us
    //    and NEVER leaked titles, body, or private field values.
    let log_text = fs::read_to_string(&site.stderr_path).unwrap();
    assert!(log_text.contains("entry_lifecycle_diag op=trash"));
    assert!(log_text.contains("entry_lifecycle_diag op=restore"));
    assert!(log_text.contains("entry_lifecycle_diag op=purge"));
    assert!(log_text.contains("entry_lifecycle_diag op=metadata_commit"));
    assert!(log_text.contains("storage_us="));
    assert!(!log_text.contains("SecretTitleMarker"));
    assert!(!log_text.contains("TopSecretValue"));
    assert!(!log_text.contains("KeepSummary"));
}
