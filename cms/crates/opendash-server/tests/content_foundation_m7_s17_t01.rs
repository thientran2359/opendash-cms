//! M7-S17-T01 / #376: public route authority and immutable permalink configuration.
//! A real binary and isolated SQLite site prove HTTP, persistence and privacy.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, header, redirect::Policy};
use serde_json::{Value, json};

struct Server {
    child: Child,
    root: PathBuf,
    base: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start() -> Server {
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
        .join(format!("opendash-m7-s17-t01-{suffix}"));
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
        if let Ok(resp) = client.get(format!("{base}/api/health")).send().await
            && resp.status().is_success()
        {
            return Server { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("isolated C3 server failed health");
}

async fn login(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({"username":"c3_admin","password":"Isolated-C3-Admin-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 201);
    let response = client
        .post(format!("{base}/auth/login"))
        .json(&json!({"username":"c3_admin","password":"Isolated-C3-Admin-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = response.json().await.unwrap();
    (cookies, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn entry(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    kind: &str,
    slug: &str,
    publish: bool,
) -> (i64, i64) {
    let response = client
        .post(format!("{base}/api/admin/content-types/{kind}/entries"))
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .json(&json!({"title":format!("C3 {slug}"),"slug":slug,"values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let saved: Value = response.json().await.unwrap();
    let id = saved["id"].as_i64().unwrap();
    let mut version = saved["version"].as_i64().unwrap();
    if publish {
        let response = client
            .post(format!(
                "{base}/api/admin/content-types/{kind}/entries/{id}/publication"
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

fn config(post_structure: &str) -> Value {
    json!({
        "post_structure":post_structure,
        "content_type_bases":{},
        "taxonomy_bases":{}
    })
}

async fn put_settings(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    version: i64,
    input: &Value,
) -> reqwest::Response {
    client
        .put(format!("{base}/api/admin/settings/permalinks"))
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(input)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn c3_permalinks_preserve_legacy_routes_then_redirect_single_hop_with_cas() {
    let srv = start().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = login(&client, &srv.base).await;
    let (post_id, _) = entry(&client, &srv.base, &cookie, &csrf, "post", "alpha", true).await;
    let (_draft_id, _) = entry(&client, &srv.base, &cookie, &csrf, "post", "secret", false).await;
    let (page_id, _) = entry(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        "page",
        "collision",
        false,
    )
    .await;
    let before = client
        .get(format!("{}/api/admin/settings/permalinks", srv.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(before.status(), 200);
    assert_eq!(before.headers()[header::CACHE_CONTROL], "no-store");
    let settings: Value = before.json().await.unwrap();
    let version = settings["version"].as_i64().unwrap();
    assert_eq!(settings["post_structure"], "/content/post/%postname%");
    assert_eq!(
        client
            .get(format!("{}/content/post/alpha", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let invalid = put_settings(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        version,
        &config("/blog/%date%"),
    )
    .await;
    assert_eq!(invalid.status(), 400);
    let unauthorized = client
        .get(format!("{}/api/admin/settings/permalinks", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);
    let missing_match = client
        .put(format!("{}/api/admin/settings/permalinks", srv.base))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&config("/blog/%postname%"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_match.status(), 428);

    let changed = put_settings(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        version,
        &config("/blog/%postname%"),
    )
    .await;
    assert_eq!(changed.status(), 200, "{:?}", changed.text().await.ok());
    let new_route = client
        .get(format!("{}/blog/alpha", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(new_route.status(), 200);
    let old_route = client
        .get(format!("{}/content/post/alpha", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(old_route.status(), 301);
    assert_eq!(old_route.headers()[header::LOCATION], "/blog/alpha");
    assert_eq!(
        client
            .get(format!("{}/blog/secret", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let stale = put_settings(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        version,
        &config("/%postname%"),
    )
    .await;
    assert_eq!(stale.status(), 412);

    let db = srv.root.join("database/opendash.sqlite");
    assert!(db.exists());
    let _ = (post_id, page_id);
}

#[tokio::test]
async fn c3_settings_collision_does_not_change_committed_namespace() {
    let srv = start().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = login(&client, &srv.base).await;
    let _post = entry(&client, &srv.base, &cookie, &csrf, "post", "taken", true).await;
    let (other_id, other_version) =
        entry(&client, &srv.base, &cookie, &csrf, "page", "page-a", true).await;
    let custom = client
        .put(format!(
            "{}/api/admin/content-types/page/entries/{other_id}/route",
            srv.base
        ))
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, format!("\"v{other_version}\""))
        .json(&json!({"custom_path":"/blog/taken"}))
        .send()
        .await
        .unwrap();
    assert_eq!(custom.status(), 200);
    let settings: Value = client
        .get(format!("{}/api/admin/settings/permalinks", srv.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let response = put_settings(
        &client,
        &srv.base,
        &cookie,
        &csrf,
        settings["version"].as_i64().unwrap(),
        &config("/blog/%postname%"),
    )
    .await;
    assert_eq!(response.status(), 409);
    let now: Value = client
        .get(format!("{}/api/admin/settings/permalinks", srv.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        now, settings,
        "collision must rollback version, config and claims"
    );
    assert_eq!(
        client
            .get(format!("{}/content/post/taken", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
}
