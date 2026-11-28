//! M7-S16-T05 native HTTP + SQLite snapshot/ACL/legacy regression.
//! Uses an isolated site root and a random ephemeral loopback port.

use std::{
    collections::HashSet,
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, StatusCode};
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
    async fn launch() -> Self {
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
                "opendash-m7-s16-t05-{}-{stamp}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let process = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch test binary");
        let base = format!("http://127.0.0.1:{port}");
        for _ in 0..160 {
            if Client::new()
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|resp| resp.status().is_success())
            {
                return Self {
                    base,
                    root,
                    process,
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("native test server did not start");
    }

    async fn database(&self) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(self.root.join("database/opendash.sqlite"))
            .create_if_missing(false)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        SqlitePool::connect_with(options)
            .await
            .expect("connect isolated site")
    }
}

async fn login(client: &Client, site: &Site, username: &str, password: &str) -> (String, String) {
    let response = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":username, "password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = response.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

async fn table(
    client: &Client,
    site: &Site,
    cookie: Option<&str>,
    type_key: &str,
    q: &str,
) -> Response {
    let mut req = client.get(format!(
        "{}/api/admin/content-types/{type_key}/entry-table{q}",
        site.base
    ));
    if let Some(cookie) = cookie {
        req = req.header("Cookie", cookie);
    }
    req.send().await.unwrap()
}

// This older test seeds raw SQLite rows after the C3 route-claim migration.
// Real entry writers synchronize claims automatically; the fixture triggers
// that same serialized writer path through a version-checked admin settings
// update, without weakening its table/filter/public_url assertions.
async fn sync_fixture_claims(client: &Client, site: &Site, cookie: &str, csrf: &str) {
    let response = client
        .get(format!("{}/api/admin/settings/permalinks", site.base))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let settings: Value = response.json().await.unwrap();
    let version = settings["version"].as_i64().unwrap();
    let input = json!({
        "post_structure":settings["post_structure"],
        "content_type_bases":settings["content_type_bases"],
        "taxonomy_bases":settings["taxonomy_bases"]
    });
    let response = client
        .put(format!("{}/api/admin/settings/permalinks", site.base))
        .header("Cookie", cookie)
        .header("x-csrf-token", csrf)
        .header("If-Match", format!("\"v{version}\""))
        .header("Origin", &site.base)
        .json(&input)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "fixture permalink sync: {body}");
}

#[tokio::test]
async fn table_filters_counts_acl_custom_routes_stable_pages_and_legacy_array() {
    let site = Site::launch().await;
    let client = Client::new();
    let setup = client
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"table_admin","password":"Strong-Table-Password-123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin_cookie, csrf) =
        login(&client, &site, "table_admin", "Strong-Table-Password-123!").await;
    assert_eq!(
        table(&client, &site, None, "post", "").await.status(),
        StatusCode::UNAUTHORIZED
    );

    let db = site.database().await;
    let mut tx = db.begin().await.unwrap();
    let parent: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Root','table-root','vi','tg-' || lower(hex(randomblob(8)))) RETURNING id"
    ).fetch_one(&mut *tx).await.unwrap();
    let child: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id) VALUES('category','Child','table-child',?,'vi','tg-' || lower(hex(randomblob(8)))) RETURNING id"
    ).bind(parent).fetch_one(&mut *tx).await.unwrap();
    let tag: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Tag','table-tag','vi','tg-' || lower(hex(randomblob(8)))) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let mut ids = Vec::new();
    let mut published = 0i64;
    for i in 0..137 {
        let title = match i {
            30 => "Percent 100% done".to_string(),
            31 => "Under_score".to_string(),
            _ => format!("Regular Article {i:03}"),
        };
        let month = if i < 30 { "2026-10" } else { "2026-11" };
        let when = format!("{month}-04 08:00:00");
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO content_entries
                (content_type_key,slug,title,values_json,publication_status,created_at,updated_at,locale,translation_group_id)
             VALUES('post',?,?,?,'draft',?,?,'vi','eg-' || lower(hex(randomblob(8)))) RETURNING id",
        )
        .bind(format!("entry-{i:03}"))
        .bind(title)
        .bind(json!({"body":format!("Story {i}"),"ghost":"never public"}).to_string())
        .bind(&when)
        .bind(&when)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        if i % 3 == 0 {
            sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            published += 1;
        }
        if i == 0 {
            sqlx::query("INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?,'vi',?)")
                .bind(id)
                .bind("/stories/table-custom")
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        if i == 1 || i == 2 || i == 3 {
            sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
                .bind(id)
                .bind(if i == 1 { parent } else { child })
                .execute(&mut *tx)
                .await
                .unwrap();
            if i != 3 {
                sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
                    .bind(id)
                    .bind(tag)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
        }
        ids.push(id);
    }
    // Real effective field roles: an Editor can see public_note, while an
    // admin-only field and a private-public-exposure field never reach them.
    let descriptor = json!({"key":"table_acl","label":"Table ACL"});
    sqlx::query(
        "INSERT INTO content_types
        (key,definition_json,owner_kind,provider_status,declaration_state,version)
        VALUES('table_acl',?,'user','active','declared',1)",
    )
    .bind(descriptor.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    let fg_fields = json!([
        {"key":"public_note","label":"Visible","type":"text"},
        {"key":"admin_secret","label":"Admin only","type":"text","acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}},
        {"key":"private_note","label":"Private","type":"text","acl":{"public_exposure":"private"}}
    ]);
    sqlx::query(
        "INSERT INTO content_field_groups
        (group_id,owner_kind,owner_id,local_key,title,description,fields_json,domain,location_rules_json,presentation_json,active,status,version)
        VALUES('user:table_acl_fields','user',NULL,'table_acl_fields','Table ACL Fields','',?,'content','[[{\"param\":\"content_type\",\"op\":\"eq\",\"value\":\"table_acl\"}]]','{\"region\":\"main_bottom\",\"order\":10,\"collapsible\":true,\"collapsed_by_default\":false,\"style\":\"card\",\"label_placement\":\"top\"}',1,'active',1)",
    )
    .bind(fg_fields.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_field_group_targets(group_id,target_kind,target_key,association_state) VALUES('user:table_acl_fields','content_type','table_acl','declared')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_entries
        (content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
        VALUES('table_acl','acl-entry','ACL Row',?,'published','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .bind(
        json!({"public_note":"visible","admin_secret":"secret","private_note":"private"})
            .to_string(),
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    sync_fixture_claims(&client, &site, &admin_cookie, &csrf).await;

    let snapshot: Value = table(&client, &site, Some(&admin_cookie), "post", "?limit=100")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(snapshot["schema_version"], 1);
    assert_eq!(snapshot["total"], 137);
    assert_eq!(snapshot["status_counts"]["all"], 137);
    assert_eq!(snapshot["status_counts"]["published"], published);
    assert_eq!(snapshot["status_counts"]["draft"], 137 - published);
    assert_eq!(snapshot["items"].as_array().unwrap().len(), 100);
    let mut visited = HashSet::new();
    let mut last_id = i64::MAX;
    for row in snapshot["items"].as_array().unwrap() {
        let id = row["id"].as_i64().unwrap();
        assert!(id < last_id, "default updated_at ties require id DESC");
        last_id = id;
        assert!(visited.insert(id), "duplicate in page1");
        assert!(
            row["fields"].get("ghost").is_none(),
            "unrecognized field leaked"
        );
        if row["publication_status"] == "draft" {
            assert!(row["public_url"].is_null(), "draft public URL leaked");
        }
    }
    let page2: Value = table(
        &client,
        &site,
        Some(&admin_cookie),
        "post",
        "?limit=100&offset=100",
    )
    .await
    .json()
    .await
    .unwrap();
    for row in page2["items"].as_array().unwrap() {
        assert!(visited.insert(row["id"].as_i64().unwrap()));
    }
    assert_eq!(visited.len(), 137);
    assert_eq!(page2["items"].as_array().unwrap().len(), 37);

    let exact_percent: Value = table(&client, &site, Some(&admin_cookie), "post", "?q=%25")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(exact_percent["total"], 1);
    assert_eq!(exact_percent["items"][0]["title"], "Percent 100% done");
    let exact_underscore: Value = table(&client, &site, Some(&admin_cookie), "post", "?q=_")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(exact_underscore["total"], 1);

    let selected: Value = table(
        &client,
        &site,
        Some(&admin_cookie),
        "post",
        &format!("?category_id={parent}&tag_id={tag}&include_descendants=true&sort=id&order=asc"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(selected["total"], 2);
    assert_eq!(selected["status_counts"]["all"], 2);
    assert_eq!(selected["items"][0]["id"], ids[1]);
    assert_eq!(selected["items"][1]["id"], ids[2]);
    let selected_exact: Value = table(
        &client,
        &site,
        Some(&admin_cookie),
        "post",
        &format!("?category_id={parent}&tag_id={tag}"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(selected_exact["total"], 1);
    let selected_and_status: Value = table(&client, &site, Some(&admin_cookie), "post", &format!(
        "?category_id={parent}&tag_id={tag}&include_descendants=true&status=draft&updated_month=2026-10"
    )).await.json().await.unwrap();
    assert_eq!(selected_and_status["total"], 2);
    assert_eq!(selected_and_status["status_counts"]["all"], 2);
    let selected_empty_month: Value = table(
        &client,
        &site,
        Some(&admin_cookie),
        "post",
        &format!(
            "?category_id={parent}&tag_id={tag}&include_descendants=true&updated_month=2026-11"
        ),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(selected_empty_month["total"], 0);
    let month: Value = table(
        &client,
        &site,
        Some(&admin_cookie),
        "post",
        "?updated_month=2026-10",
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(month["total"], 30);
    let published_only: Value = table(
        &client,
        &site,
        Some(&admin_cookie),
        "post",
        "?status=published",
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(published_only["total"], published);
    assert_eq!(published_only["status_counts"]["all"], 137);
    let custom: Value = table(&client, &site, Some(&admin_cookie), "post", "?q=entry-000")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(custom["total"], 1);
    assert_eq!(custom["items"][0]["public_url"], "/stories/table-custom");
    let default_url: Value = table(&client, &site, Some(&admin_cookie), "post", "?q=entry-003")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(default_url["items"][0]["public_url"], "/entry-003");

    let legacy: Value = client
        .get(format!(
            "{}/api/admin/content-types/post/entries?limit=2",
            site.base
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(legacy.is_array(), "legacy GET must remain a bare array");
    assert_eq!(legacy.as_array().unwrap().len(), 2);

    let trash_snap: Value = table(&client, &site, Some(&admin_cookie), "post", "?status=trash")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(trash_snap["total"], 0);
    assert_eq!(trash_snap["status_counts"]["trash"], 0);
    assert!(
        trash_snap["schema_token"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );

    for query in [
        "?limit=0",
        "?limit=101",
        "?offset=-1",
        "?offset=1000001",
        "?status=archived",
        "?status=all&status=draft",
        "?q=ok&q=again",
        "?sort=updated_at%20DROP%20TABLE",
        "?sort=whatever",
        "?order=up",
        "?category_id=-1",
        "?tag_id=0",
        "?include_descendants=true",
        "?include_descendants=maybe",
        "?updated_month=0000-01",
        "?updated_month=2026-13",
        "?updated_month=2026-1",
        "?unknown=123",
        "?limit=20&limit=40",
    ] {
        assert_eq!(
            table(&client, &site, Some(&admin_cookie), "post", query)
                .await
                .status(),
            StatusCode::BAD_REQUEST,
            "query={query}",
        );
    }
    assert_eq!(
        table(
            &client,
            &site,
            Some(&admin_cookie),
            "post",
            &format!("?category_id={tag}")
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        table(
            &client,
            &site,
            Some(&admin_cookie),
            "post",
            "?category_id=999999"
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        table(
            &client,
            &site,
            Some(&admin_cookie),
            "post",
            &format!("?q={}", "z".repeat(201))
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    let create_editor = client.post(format!("{}/api/admin/users", site.base))
        .header("Cookie", &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"username":"table_editor","password":"Strong-Editor-Password-456!","role":"editor"}))
        .send().await.unwrap();
    assert_eq!(create_editor.status(), StatusCode::CREATED);
    let (editor_cookie, _) = login(
        &client,
        &site,
        "table_editor",
        "Strong-Editor-Password-456!",
    )
    .await;
    let editor_acl: Value = table(&client, &site, Some(&editor_cookie), "table_acl", "")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(editor_acl["total"], 1);
    assert_eq!(
        editor_acl["items"][0]["fields"],
        json!({"public_note":"visible"})
    );
    let admin_acl: Value = table(&client, &site, Some(&admin_cookie), "table_acl", "")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        admin_acl["items"][0]["fields"],
        json!({"public_note":"visible"})
    );

    // A provider-disabled CPT is not a readable table and must not reveal
    // row counts from durable site-owned entries to an Editor.
    sqlx::query(
        "UPDATE content_types SET provider_status='provider_inactive' WHERE key='table_acl'",
    )
    .execute(&db)
    .await
    .unwrap();
    assert_eq!(
        table(&client, &site, Some(&editor_cookie), "table_acl", "")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    // Concurrent writers yield a fresh subsequent snapshot, never a claim
    // of cross-request cursor isolation. Old deserialized JSON stays stable.
    sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
      VALUES('post','new-after-snapshot','new snapshot','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap();
    let after: Value = table(&client, &site, Some(&admin_cookie), "post", "")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(snapshot["total"], 137);
    assert_eq!(after["total"], 138);
    db.close().await;
}
