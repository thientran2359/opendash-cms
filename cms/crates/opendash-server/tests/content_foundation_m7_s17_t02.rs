//! #377 / M7-S17-T02: Admin row View URLs are resolved by the server and
//! private drafts remain previewable only after authentication.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, header, redirect::Policy};
use serde_json::{Value, json};

struct IsolatedServer {
    child: Child,
    root: PathBuf,
    base: String,
}

impl Drop for IsolatedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start() -> IsolatedServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("opendash-m7-s17-t02-{suffix}"));
    fs::create_dir_all(&root).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_DB_PATH", root.join("database/opendash.sqlite"))
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..120 {
        if let Ok(response) = client.get(format!("{base}/api/health")).send().await
            && response.status().is_success()
        {
            return IsolatedServer { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("isolated #377 server did not become healthy");
}

async fn auth(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({"username":"t02_admin","password":"Isolated-T02-Admin-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    let login = client
        .post(format!("{base}/auth/login"))
        .json(&json!({"username":"t02_admin","password":"Isolated-T02-Admin-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value))
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

async fn create_entry(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    slug: &str,
    publish: bool,
) -> (i64, i64) {
    let response = client
        .post(format!("{base}/api/admin/content-types/post/entries"))
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .json(&json!({"title":format!("T02 {slug}"),"slug":slug,"values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let entry = response.json::<Value>().await.unwrap();
    let id = entry["id"].as_i64().unwrap();
    let mut version = entry["version"].as_i64().unwrap();
    if publish {
        let response = client
            .post(format!(
                "{base}/api/admin/content-types/post/entries/{id}/publication"
            ))
            .header(header::COOKIE, cookie)
            .header("x-csrf-token", csrf)
            .header(header::IF_MATCH, format!("\"v{version}\""))
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        version = response.json::<Value>().await.unwrap()["version"]
            .as_i64()
            .unwrap();
    }
    (id, version)
}

async fn table(client: &Client, base: &str, cookie: &str) -> Value {
    let response = client
        .get(format!("{base}/api/admin/content-types/post/entry-table"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

fn row(rows: &Value, id: i64) -> &Value {
    rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .expect("entry must be in Admin table")
}

async fn settings(client: &Client, base: &str, cookie: &str) -> Value {
    let response = client
        .get(format!("{base}/api/admin/settings/permalinks"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

async fn put(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    path: &str,
    payload: Value,
    version: i64,
) -> Response {
    client
        .put(format!("{base}{path}"))
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&payload)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_view_url_tracks_saved_route_and_draft_preview_stays_private() {
    let srv = start().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = auth(&client, &srv.base).await;
    let (published_id, published_version) =
        create_entry(&client, &srv.base, &cookie, &csrf, "visible", true).await;
    let (draft_id, _) = create_entry(&client, &srv.base, &cookie, &csrf, "secret", false).await;
    let table_path = format!("{}/api/admin/content-types/post/entry-table", srv.base);
    assert_eq!(client.get(table_path).send().await.unwrap().status(), 401);
    let initial = table(&client, &srv.base, &cookie).await;
    assert_eq!(
        row(&initial, published_id)["public_url"],
        "/content/post/visible"
    );
    assert!(row(&initial, draft_id)["public_url"].is_null());
    let route_path = format!("/api/admin/content-types/post/entries/{published_id}/route");
    let draft_route_path = format!("/api/admin/content-types/post/entries/{draft_id}/route");
    assert_eq!(
        client
            .get(format!("{}{}", srv.base, route_path))
            .send()
            .await
            .unwrap()
            .status(),
        401,
    );
    let published_route = client
        .get(format!("{}{}", srv.base, route_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(published_route.status(), 200);
    assert_eq!(published_route.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        published_route.headers()[header::ETAG],
        format!("\"v{published_version}\"")
    );
    let published_snapshot = published_route.json::<Value>().await.unwrap();
    assert_eq!(published_snapshot["entry_version"], published_version);
    assert!(published_snapshot["custom_path"].is_null());
    assert_eq!(published_snapshot["public_url"], "/content/post/visible");
    let editor_creation = client
        .post(format!("{}/api/admin/users", srv.base))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({"username":"t02_editor",
            "password":"Native-C3-Editor-Password!2026","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_creation.status(), 201);
    let editor_login = client
        .post(format!("{}/auth/login", srv.base))
        .json(&json!({"username":"t02_editor",
            "password":"Native-C3-Editor-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_login.status(), 200);
    let editor_cookie = editor_login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value))
        .collect::<Vec<_>>()
        .join("; ");
    let editor_csrf = editor_login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let editor_read = client
        .get(format!("{}{}", srv.base, route_path))
        .header(header::COOKIE, &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(editor_read.status(), 200);
    assert_eq!(
        editor_read.json::<Value>().await.unwrap(),
        published_snapshot
    );
    let editor_noop = put(
        &client,
        &srv.base,
        &editor_cookie,
        &editor_csrf,
        &route_path,
        json!({"custom_path":null}),
        published_version,
    )
    .await;
    assert_eq!(editor_noop.status(), 200);
    assert_eq!(
        editor_noop.json::<Value>().await.unwrap(),
        published_snapshot
    );
    let draft_route = client
        .get(format!("{}{}", srv.base, draft_route_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(draft_route.status(), 200);
    assert_eq!(draft_route.headers()[header::CACHE_CONTROL], "no-store");
    assert!(draft_route.json::<Value>().await.unwrap()["public_url"].is_null());
    let missing_csrf = client
        .put(format!("{}{}", srv.base, route_path))
        .header(header::COOKIE, &cookie)
        .header(header::IF_MATCH, format!("\"v{published_version}\""))
        .json(&json!({"custom_path":"/unauthorized-change"}))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_csrf.status(), 403);
    let anonymous_put = client
        .put(format!("{}{}", srv.base, route_path))
        .header(header::IF_MATCH, format!("\"v{published_version}\""))
        .json(&json!({"custom_path":"/unauthorized-change"}))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous_put.status(), 401);
    let wrong_type = client
        .get(format!(
            "{}/api/admin/content-types/page/entries/{published_id}/route",
            srv.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_type.status(), 404);

    let private_type = client
        .post(format!("{}/api/admin/content-types", srv.base))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(
            &json!({"key":"t02_private","label":"Private T02","fields":[],
            "settings":{"public":false,"has_archive":false,
            "publication":"immediate","supports":["editor"]}}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(private_type.status(), 201);
    let private_entry = client
        .post(format!(
            "{}/api/admin/content-types/t02_private/entries",
            srv.base
        ))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({"title":"Private Type","slug":"private-t02","values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(private_entry.status(), 201);
    let private_created = private_entry.json::<Value>().await.unwrap();
    assert_eq!(private_created["publication_status"], "published");
    let private_id = private_created["id"].as_i64().unwrap();
    let private_version = private_created["version"].as_i64().unwrap();
    let private_path = format!("/api/admin/content-types/t02_private/entries/{private_id}/route");
    let private_route = client
        .get(format!("{}{}", srv.base, private_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(private_route.status(), 200);
    assert!(private_route.json::<Value>().await.unwrap()["public_url"].is_null());
    let private_update = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        &private_path,
        json!({"custom_path":"/t02-hidden/private"}),
        private_version,
    )
    .await;
    assert_eq!(private_update.status(), 200);
    let private_after_put = private_update.json::<Value>().await.unwrap();
    assert_eq!(private_after_put["custom_path"], "/t02-hidden/private");
    assert!(private_after_put["public_url"].is_null());
    let private_after_get = client
        .get(format!("{}{}", srv.base, private_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(private_after_get, private_after_put);
    assert_eq!(
        client
            .get(format!("{}/t02-hidden/private", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let section = client
        .post(format!("{}/api/admin/patterns", srv.base))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({"title":"T02 Private Section", "slug":"t02-section"}))
        .send()
        .await
        .unwrap();
    assert_eq!(section.status(), 201);
    let section_id = section.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let section_route = client
        .get(format!(
            "{}/api/admin/content-types/pattern/entries/{section_id}/route",
            srv.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(section_route.status(), 200);
    assert!(section_route.json::<Value>().await.unwrap()["public_url"].is_null());

    // Simulate a provider becoming inactive while an old canonical claim
    // still exists. Even before claim maintenance the read projection must
    // never expose a stale public View URL.
    let provider_type = client
        .post(format!("{}/api/admin/content-types", srv.base))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({"key":"t02_provider","label":"Provider Visibility","fields":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(provider_type.status(), 201);
    let provider_entry = client
        .post(format!(
            "{}/api/admin/content-types/t02_provider/entries",
            srv.base
        ))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({"title":"Provider Entry", "slug":"provider-entry","values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(provider_entry.status(), 201);
    let provider_record = provider_entry.json::<Value>().await.unwrap();
    assert_eq!(provider_record["publication_status"], "published");
    let provider_id = provider_record["id"].as_i64().unwrap();
    let provider_version = provider_record["version"].as_i64().unwrap();
    let provider_path =
        format!("/api/admin/content-types/t02_provider/entries/{provider_id}/route");
    let active_provider = client
        .get(format!("{}{}", srv.base, provider_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        active_provider["public_url"],
        "/content/t02_provider/provider-entry"
    );
    let fixture_db = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(srv.root.join("database/opendash.sqlite")),
    )
    .await
    .unwrap();
    let changed = sqlx::query(
        "UPDATE content_types SET provider_status='provider_inactive' WHERE key='t02_provider'",
    )
    .execute(&fixture_db)
    .await
    .unwrap();
    assert_eq!(changed.rows_affected(), 1);
    fixture_db.close().await;
    let inactive_provider = client
        .get(format!("{}{}", srv.base, provider_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(inactive_provider["entry_version"], provider_version);
    assert!(inactive_provider["public_url"].is_null());
    assert_eq!(
        client
            .get(format!("{}/content/t02_provider/provider-entry", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    let draft_preview = format!(
        "{}/api/admin/content-types/post/entries/{draft_id}/preview",
        srv.base
    );
    assert_eq!(
        client.get(&draft_preview).send().await.unwrap().status(),
        401
    );
    let preview = client
        .get(&draft_preview)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), 200);
    assert_eq!(
        preview.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert!(preview.text().await.unwrap().contains("T02 secret"));
    assert_eq!(
        client
            .get(format!("{}/content/post/secret", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    let custom = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        &route_path,
        json!({"custom_path":"/showcase/visible"}),
        published_version,
    )
    .await;
    assert_eq!(custom.status(), 200);
    assert_eq!(custom.headers()[header::CACHE_CONTROL], "no-store");
    let custom_etag = custom.headers()[header::ETAG].to_str().unwrap().to_owned();
    let custom_snapshot = custom.json::<Value>().await.unwrap();
    let custom_version = custom_snapshot["entry_version"].as_i64().unwrap();
    assert_eq!(custom_version, published_version + 1);
    assert_eq!(custom_etag, format!("\"v{custom_version}\""));
    assert_eq!(custom_snapshot["custom_path"], "/showcase/visible");
    assert_eq!(custom_snapshot["public_url"], "/showcase/visible");
    let fresh_custom = client
        .get(format!("{}{}", srv.base, route_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(fresh_custom.status(), 200);
    assert_eq!(fresh_custom.json::<Value>().await.unwrap(), custom_snapshot);
    let after_custom = table(&client, &srv.base, &cookie).await;
    assert_eq!(
        row(&after_custom, published_id)["public_url"],
        "/showcase/visible"
    );
    assert_eq!(
        client
            .get(format!("{}/content/post/visible", srv.base))
            .send()
            .await
            .unwrap()
            .headers()[header::LOCATION],
        "/showcase/visible"
    );

    let before = settings(&client, &srv.base, &cookie).await;
    let saved = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        "/api/admin/settings/permalinks",
        json!({"post_structure":"/blog/%postname%",
            "content_type_bases":before["content_type_bases"],
            "taxonomy_bases":before["taxonomy_bases"]}),
        before["version"].as_i64().unwrap(),
    )
    .await;
    assert_eq!(saved.status(), 200);
    assert_eq!(
        row(&table(&client, &srv.base, &cookie).await, published_id)["public_url"],
        "/showcase/visible",
        "custom path keeps precedence over global format"
    );
    let reset = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        &route_path,
        json!({"custom_path":null}),
        custom_version,
    )
    .await;
    assert_eq!(reset.status(), 200);
    let reset_snapshot = reset.json::<Value>().await.unwrap();
    assert_eq!(reset_snapshot["entry_version"], custom_version + 1);
    assert!(reset_snapshot["custom_path"].is_null());
    assert_eq!(reset_snapshot["public_url"], "/blog/visible");
    let final_table = table(&client, &srv.base, &cookie).await;
    assert_eq!(
        row(&final_table, published_id)["public_url"],
        "/blog/visible"
    );
    assert!(row(&final_table, draft_id)["public_url"].is_null());
    let (taken_id, taken_version) =
        create_entry(&client, &srv.base, &cookie, &csrf, "other", false).await;
    let taken_route = format!("/api/admin/content-types/post/entries/{taken_id}/route");
    let taken_response = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        &taken_route,
        json!({"custom_path":"/taken/visible"}),
        taken_version,
    )
    .await;
    assert_eq!(taken_response.status(), 200);
    assert!(taken_response.json::<Value>().await.unwrap()["public_url"].is_null());
    let collision = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        &route_path,
        json!({"custom_path":"/taken/visible"}),
        custom_version + 1,
    )
    .await;
    assert_eq!(collision.status(), 409);
    let route_after_collision = client
        .get(format!("{}{}", srv.base, route_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(route_after_collision.status(), 200);
    let unchanged = route_after_collision.json::<Value>().await.unwrap();
    assert_eq!(unchanged["entry_version"], custom_version + 1);
    assert!(unchanged["custom_path"].is_null());
    assert_eq!(unchanged["public_url"], "/blog/visible");
    let stale_route = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        &route_path,
        json!({"custom_path":"/old/visible"}),
        custom_version,
    )
    .await;
    assert_eq!(stale_route.status(), 412);
    let persisted_route = client
        .get(format!("{}{}", srv.base, route_path))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(persisted_route, unchanged);
    assert_eq!(
        client
            .get(format!("{}/blog/secret", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    let stale = put(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        "/api/admin/settings/permalinks",
        json!({"post_structure":"/%postname%",
            "content_type_bases":before["content_type_bases"],
            "taxonomy_bases":before["taxonomy_bases"]}),
        before["version"].as_i64().unwrap(),
    )
    .await;
    assert_eq!(stale.status(), 412);
    assert_eq!(
        row(&table(&client, &srv.base, &cookie).await, published_id)["public_url"],
        "/blog/visible",
        "stale save must not alter canonical View URL"
    );
}
