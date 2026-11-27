//! Dedicated taxonomy route: authentication, DTO, CAS, cyclic tree and links.
//! Native HTTP/SQLite tests always use an isolated loopback site.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

struct TestSite {
    base: String,
    root: PathBuf,
    child: Child,
}

impl Drop for TestSite {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl TestSite {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = temp.join(format!(
            "opendash-m7-s16-t04-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..160 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                return Self { base, root, child };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("native test server did not start");
    }
}

async fn setup(client: &Client, site: &TestSite) -> (String, String) {
    let response = client
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"term_admin", "password":"Term-Test-Admin-123456!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":"term_admin", "password":"Term-Test-Admin-123456!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = response.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn create(client: &Client, site: &TestSite, cookie: &str, csrf: &str, input: Value) -> Value {
    let response = client
        .post(format!("{}/api/admin/taxonomy/terms", site.base))
        .header("Cookie", cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", csrf)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "{:?}",
        response.text().await
    );
    response.json().await.unwrap()
}

#[tokio::test]
async fn term_get_is_authenticated_and_mutation_preserves_id_cas_links() {
    let site = TestSite::start().await;
    let client = Client::new();
    let (cookie, csrf) = setup(&client, &site).await;
    let path = format!("{}/api/admin/taxonomy/terms", site.base);

    let unauth = client.get(format!("{path}/1")).send().await.unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    let missing = client
        .get(format!("{path}/999999"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let parent = create(
        &client,
        &site,
        &cookie,
        &csrf,
        json!({"kind":"category","name":"Root","slug":"term-root"}),
    )
    .await;
    let id = parent["id"].as_i64().unwrap();
    let read = client
        .get(format!("{path}/{id}"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(read.headers().get("etag").unwrap(), "\"v1\"");
    assert_eq!(read.headers().get("cache-control").unwrap(), "no-store");
    let dto: Value = read.json().await.unwrap();
    assert_eq!(dto, parent);

    let no_csrf = client
        .patch(format!("{path}/{id}"))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("If-Match", "\"v1\"")
        .json(&json!({"name":"No CSRF"}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);
    let no_etag = client
        .patch(format!("{path}/{id}"))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"name":"No ETag"}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_etag.status(), StatusCode::PRECONDITION_REQUIRED);

    let renamed = client
        .patch(format!("{path}/{id}"))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({"name":"Root Renamed","slug":"term-renamed"}))
        .send()
        .await
        .unwrap();
    assert_eq!(renamed.status(), StatusCode::OK);
    assert_eq!(renamed.headers().get("etag").unwrap(), "\"v2\"");
    let renamed_dto: Value = renamed.json().await.unwrap();
    assert_eq!(renamed_dto["id"], id);
    let stale = client
        .patch(format!("{path}/{id}"))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({"name":"Stale"}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);

    let child = create(
        &client,
        &site,
        &cookie,
        &csrf,
        json!({"kind":"category","name":"Child","slug":"term-child","parent_id":id}),
    )
    .await;
    let child_id = child["id"].as_i64().unwrap();
    let cycle = client
        .patch(format!("{path}/{id}"))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({"parent_id":child_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(cycle.status(), StatusCode::CONFLICT);

    let tag = create(
        &client,
        &site,
        &cookie,
        &csrf,
        json!({"kind":"tag","name":"Test Tag","slug":"term-tag"}),
    )
    .await;
    assert_eq!(tag["kind"], "tag");
    let list: Value = client
        .get(format!("{path}?kind=tag&limit=20&offset=0"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["kind"] == "tag")
    );
    let bad_parent = client
        .patch(format!("{path}/{}", tag["id"].as_i64().unwrap()))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({"parent_id":id}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_parent.status(), StatusCode::BAD_REQUEST);
}
