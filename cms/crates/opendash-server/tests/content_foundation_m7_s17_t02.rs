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

    let route_path = format!("/api/admin/content-types/post/entries/{published_id}/route");
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
    let custom_version = custom.json::<Value>().await.unwrap()["entry_version"]
        .as_i64()
        .unwrap();
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
    let final_table = table(&client, &srv.base, &cookie).await;
    assert_eq!(
        row(&final_table, published_id)["public_url"],
        "/blog/visible"
    );
    assert!(row(&final_table, draft_id)["public_url"].is_null());
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
