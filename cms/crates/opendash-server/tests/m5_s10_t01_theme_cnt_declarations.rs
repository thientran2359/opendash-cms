use std::{
    fs,
    io::{Cursor, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use reqwest::Client;
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TestServer {
    base_url: String,
    child: Child,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        for (name, bytes) in entries {
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

async fn spawn_server_at(site_root: PathBuf, run_setup: bool) -> TestServer {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let exe = env!("CARGO_BIN_EXE_opendash");
    let mut child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let client = Client::new();
    for _ in 0..80 {
        if client.get(format!("{base_url}/api/health")).send().await.is_ok() {
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("server exited early: {status}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    if run_setup {
        let res = client
            .post(format!("{base_url}/auth/setup"))
            .json(&json!({
                "username": "admin",
                "password": "AdminPassword123!"
            }))
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success());
    }

    TestServer { base_url, child }
}

async fn login(client: &Client, base_url: &str) -> (String, String) {
    let res = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let cookie = res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = res.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_string();
    (cookie, csrf)
}

async fn install_v2(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    manifest: Value,
) -> String {
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let zip = build_zip(&[
        ("theme.json", &manifest_bytes),
        ("templates/home.html", b"<main>Home</main>"),
    ]);
    let res = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Content-Type", "application/zip")
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .body(zip)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    res.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn activate_v2(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    version: i64,
    revision_id: &str,
) -> reqwest::Response {
    client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", format!("\"v{version}\""))
        .json(&json!({ "revision_id": revision_id }))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn theme_cnt_declarations_lifecycle_collisions_compatibility_withdrawal_and_restart() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m5-s10-t01-{nanos}"));
    fs::create_dir_all(&site_root).unwrap();

    let srv = spawn_server_at(site_root.clone(), true).await;
    let client = Client::new();
    let (cookie, csrf) = login(&client, &srv.base_url).await;

    // 1. Theme A v1 declares `portfolio` CPT + fields + category/tag eligibility
    let rev_a1 = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-a",
            "name": "Theme A",
            "version": "1.0.0",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "short_text", "required": false },
                        {
                            "key": "stage",
                            "label": "Stage",
                            "type": "select",
                            "required": false,
                            "options": [
                                { "value": "draft", "label": "Draft" },
                                { "value": "live", "label": "Live" }
                            ]
                        }
                    ],
                    "taxonomies": ["category", "tag"]
                }]
            }
        }),
    )
    .await;
    let act1 = activate_v2(&client, &srv.base_url, &cookie, &csrf, 1, &rev_a1).await;
    assert_eq!(act1.status(), 200);

    // Create a category term and a portfolio entry with term_ids
    let term_res = client
        .post(format!("{}/api/admin/taxonomy/terms", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "kind": "category", "name": "Design", "slug": "design" }))
        .send()
        .await
        .unwrap();
    assert_eq!(term_res.status(), 201);
    let term_id = term_res.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    let entry_res = client
        .post(format!("{}/api/admin/content-types/portfolio/entries", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Project One",
            "slug": "project-one",
            "values": { "client": "Acme", "stage": "live" },
            "term_ids": [term_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(entry_res.status(), 201);
    let entry_id = entry_res.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    // 2. Collision tests (409): Core `post` and other-owner `portfolio`
    let rev_core_claim = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-b",
            "name": "Theme B",
            "version": "1.0.0",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{ "key": "post", "label": "Hijack Post" }]
            }
        }),
    )
    .await;
    let act_core = activate_v2(&client, &srv.base_url, &cookie, &csrf, 2, &rev_core_claim).await;
    assert_eq!(act_core.status(), 409);

    let rev_b_collide = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-b",
            "name": "Theme B",
            "version": "1.0.1",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{ "key": "portfolio", "label": "Stolen Portfolio" }]
            }
        }),
    )
    .await;
    let act_collide = activate_v2(&client, &srv.base_url, &cookie, &csrf, 2, &rev_b_collide).await;
    assert_eq!(act_collide.status(), 409);

    // 3. Same-owner compatible re-declare (update label + add optional field) -> 200
    let rev_a2 = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-a",
            "name": "Theme A",
            "version": "2.0.0",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{
                    "key": "portfolio",
                    "label": "Portfolio Showcase",
                    "fields": [
                        { "key": "client", "label": "Client Name", "type": "short_text", "required": false },
                        {
                            "key": "stage",
                            "label": "Stage",
                            "type": "select",
                            "required": false,
                            "options": [
                                { "value": "draft", "label": "Draft" },
                                { "value": "live", "label": "Live" }
                            ]
                        },
                        { "key": "summary", "label": "Summary", "type": "long_text", "required": false }
                    ],
                    "taxonomies": ["category", "tag"]
                }]
            }
        }),
    )
    .await;
    let act2 = activate_v2(&client, &srv.base_url, &cookie, &csrf, 2, &rev_a2).await;
    assert_eq!(act2.status(), 200);

    // 4. Incompatible schema changes rejected (422) without mutating active or data:
    // 4a. Changing field type (`short_text` -> `number`)
    let rev_a_bad_type = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-a",
            "name": "Theme A",
            "version": "2.1.0",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "number", "required": false },
                        {
                            "key": "stage",
                            "label": "Stage",
                            "type": "select",
                            "required": false,
                            "options": [
                                { "value": "draft", "label": "Draft" },
                                { "value": "live", "label": "Live" }
                            ]
                        }
                    ]
                }]
            }
        }),
    )
    .await;
    assert_eq!(
        activate_v2(&client, &srv.base_url, &cookie, &csrf, 3, &rev_a_bad_type)
            .await
            .status(),
        422
    );

    // 4b. Adding a `required: true` field
    let rev_a_bad_req = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-a",
            "name": "Theme A",
            "version": "2.2.0",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "short_text", "required": false },
                        {
                            "key": "stage",
                            "label": "Stage",
                            "type": "select",
                            "required": false,
                            "options": [
                                { "value": "draft", "label": "Draft" },
                                { "value": "live", "label": "Live" }
                            ]
                        },
                        { "key": "must_have", "label": "Must Have", "type": "short_text", "required": true }
                    ]
                }]
            }
        }),
    )
    .await;
    assert_eq!(
        activate_v2(&client, &srv.base_url, &cookie, &csrf, 3, &rev_a_bad_req)
            .await
            .status(),
        422
    );

    // 4c. Removing a select choice (`live`) when existing entries exist
    let rev_a_bad_opt = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-a",
            "name": "Theme A",
            "version": "2.3.0",
            "templates": { "home": "templates/home.html" },
            "declarations": {
                "content_types": [{
                    "key": "portfolio",
                    "label": "Portfolio",
                    "fields": [
                        { "key": "client", "label": "Client", "type": "short_text", "required": false },
                        {
                            "key": "stage",
                            "label": "Stage",
                            "type": "select",
                            "required": false,
                            "options": [
                                { "value": "draft", "label": "Draft" }
                            ]
                        }
                    ]
                }]
            }
        }),
    )
    .await;
    assert_eq!(
        activate_v2(&client, &srv.base_url, &cookie, &csrf, 3, &rev_a_bad_opt)
            .await
            .status(),
        422
    );

    // 5. Omitting `portfolio` in Theme A v3 marks it `withdrawn`, keeping entries & term links editable
    let rev_a3_withdraw = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-a",
            "name": "Theme A",
            "version": "3.0.0",
            "templates": { "home": "templates/home.html" }
        }),
    )
    .await;
    let act3 = activate_v2(&client, &srv.base_url, &cookie, &csrf, 3, &rev_a3_withdraw).await;
    assert_eq!(act3.status(), 200);

    // Other owner still gets 409 on withdrawn `portfolio`
    assert_eq!(
        activate_v2(&client, &srv.base_url, &cookie, &csrf, 4, &rev_b_collide)
            .await
            .status(),
        409
    );

    // Retained entry is still editable even though `portfolio` is withdrawn
    let upd_entry = client
        .put(format!(
            "{}/api/admin/content-types/portfolio/entries/{}",
            srv.base_url, entry_id
        ))
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "title": "Project One Updated",
            "slug": "project-one",
            "values": { "client": "Acme", "stage": "live", "summary": "Retained" },
            "term_ids": [term_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd_entry.status(), 200);

    // 6. Switch to Theme B (empty) -> `portfolio` becomes `provider_inactive` + stays `withdrawn`
    let rev_b_clean = install_v2(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        json!({
            "manifest_version": 2,
            "theme_id": "theme-b",
            "name": "Theme B",
            "version": "2.0.0",
            "templates": { "home": "templates/home.html" }
        }),
    )
    .await;
    let act4 = activate_v2(&client, &srv.base_url, &cookie, &csrf, 4, &rev_b_clean).await;
    assert_eq!(act4.status(), 200);

    // 7. Restart server and verify state restored from DB
    drop(srv);
    let srv2 = spawn_server_at(site_root.clone(), false).await;
    let (cookie2, _) = login(&client, &srv2.base_url).await;

    let cts: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv2.base_url))
        .header("Cookie", &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let portfolio = cts
        .iter()
        .find(|c| c["descriptor"]["key"] == "portfolio")
        .expect("portfolio must survive withdrawal, theme switch, and restart");
    assert_eq!(portfolio["owner"]["kind"], "theme");
    assert_eq!(portfolio["owner"]["id"], "theme-a");
    assert_eq!(portfolio["status"], "provider_inactive");
    assert_eq!(portfolio["declaration_state"], "withdrawn");

    // Verify DB taxonomy eligibility & entry_terms preserved
    drop(srv2);
    let db_path = site_root.join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    let elig_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT taxonomy_kind, provider_status, declaration_state
         FROM content_type_taxonomy_eligibility
         WHERE content_type_key = 'portfolio'
         ORDER BY taxonomy_kind ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        elig_rows,
        vec![
            (
                "category".to_string(),
                "provider_inactive".to_string(),
                "withdrawn".to_string()
            ),
            (
                "tag".to_string(),
                "provider_inactive".to_string(),
                "withdrawn".to_string()
            ),
        ]
    );
    let term_links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(term_links, 1);
    pool.close().await;
    let _ = fs::remove_dir_all(&site_root);
}
