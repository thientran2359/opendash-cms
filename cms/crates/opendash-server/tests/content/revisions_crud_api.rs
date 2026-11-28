//! S02 Revisions CRUD, Compare, 4-Phase Restore, Undo-Restore & Autosave Isolation Integration Tests.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, Method, Response, StatusCode, header};
use serde_json::{Value, json};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempSite {
    child: Child,
    root: PathBuf,
    base: String,
}

impl Drop for TempSite {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start_site(label: &str) -> TempSite {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = fs::canonicalize(std::env::temp_dir())
        .expect("canonical temp_dir")
        .join(format!(
            "opendash-rev-crud-s02-{label}-{}-{seq}",
            std::process::id()
        ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create site root");
    let stderr = fs::File::create(root.join("server.log")).expect("create server log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn opendash server");
    let base = format!("http://127.0.0.1:{port}");
    let http = Client::new();
    for _ in 0..400 {
        if let Some(status) = child.try_wait().expect("poll server") {
            let log = fs::read_to_string(root.join("server.log")).unwrap_or_default();
            panic!("server exited early ({status}): {log}");
        }
        if http
            .get(format!("{base}/api/health"))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return TempSite { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server health timeout at {base}");
}

struct Session {
    cookie: String,
    csrf: String,
}

async fn login(http: &Client, site: &TempSite, user: &str, pass: &str) -> Session {
    let resp = http
        .post(format!("{}/auth/login", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": user, "password": pass }))
        .send()
        .await
        .expect("login");
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    Session {
        cookie,
        csrf: body["csrf_token"].as_str().unwrap().to_owned(),
    }
}

async fn setup_admin_and_editor(http: &Client, site: &TempSite) -> (Session, Session) {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let admin = login(http, site, "admin", "AdminPassword123!").await;
    let ed = api(
        http,
        site,
        Some(&admin),
        Method::POST,
        "/api/admin/users",
        None,
        Some(json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" })),
    )
    .await;
    assert_eq!(ed.status(), StatusCode::CREATED);
    let editor = login(http, site, "editor1", "EditorPassword123!").await;
    (admin, editor)
}

async fn api(
    http: &Client,
    site: &TempSite,
    sess: Option<&Session>,
    method: Method,
    path: &str,
    if_match: Option<&str>,
    body: Option<Value>,
) -> Response {
    let mut req = http.request(method, format!("{}{path}", site.base));
    if let Some(s) = sess {
        req = req
            .header(header::COOKIE, &s.cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &s.csrf);
    }
    if let Some(etag) = if_match {
        req = req.header(header::IF_MATCH, etag);
    }
    if let Some(json_body) = body {
        req = req.json(&json_body);
    }
    req.send().await.expect("api request")
}

#[tokio::test]
async fn revisions_list_get_compare_restore_undo_and_autosave_isolation() {
    let site = start_site("crud").await;
    let http = Client::new();
    let (admin, editor) = setup_admin_and_editor(&http, &site).await;

    let tok_resp = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        "/api/admin/content-types/post/effective-editor-config",
        None,
        None,
    )
    .await;
    assert_eq!(tok_resp.status(), StatusCode::OK);
    let token = tok_resp.json::<Value>().await.unwrap()["schema_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let created = api(&http, &site, Some(&admin), Method::POST, "/api/admin/content-types/post/entries/composite", None,
        Some(json!({ "expected_schema_token": &token, "title": "Post V1", "slug": "rev-post-s02", "values": { "body": "<p>Body V1</p>" } }))).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let pid = created.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();

    let u1 = api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/post/entries/{pid}/composite"), None,
        Some(json!({ "expected_version": 1, "expected_schema_token": &token, "title": "Post V2", "slug": "rev-post-s02", "values": { "body": "<p>Body V2</p>" } }))).await;
    assert_eq!(u1.status(), StatusCode::OK);

    let pub_r = api(
        &http,
        &site,
        Some(&admin),
        Method::POST,
        &format!("/api/admin/content-types/post/entries/{pid}/publication"),
        Some("\"v2\""),
        Some(json!({ "status": "published" })),
    )
    .await;
    assert_eq!(pub_r.status(), StatusCode::OK);

    let u2 = api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/post/entries/{pid}/composite"), None,
        Some(json!({ "expected_version": 3, "expected_schema_token": &token, "title": "Post V3", "slug": "rev-post-s02", "values": { "body": "<p>Body V3</p>" } }))).await;
    assert_eq!(u2.status(), StatusCode::OK);

    let list_res = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/post/entries/{pid}/revisions?limit=2&offset=0"),
        None,
        None,
    )
    .await;
    assert_eq!(list_res.status(), StatusCode::OK);
    assert_eq!(list_res.headers()[header::ETAG], "\"v4\"");
    assert_eq!(list_res.headers()[header::CACHE_CONTROL], "no-store");
    let list_json: Value = list_res.json().await.unwrap();
    assert_eq!(list_json["items"].as_array().unwrap().len(), 2);
    assert_eq!(list_json["total"], 3);
    assert_eq!(list_json["current_version"], 4);
    assert_eq!(list_json["has_autosave"], false);

    let all: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/post/entries/{pid}/revisions"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let rev1 = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["revision_number"] == 1)
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let rev3 = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["revision_number"] == 3)
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    let det = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/post/entries/{pid}/revisions/{rev1}"),
        None,
        None,
    )
    .await;
    assert_eq!(det.status(), StatusCode::OK);
    assert_eq!(det.headers()[header::CACHE_CONTROL], "no-store");
    let det_j: Value = det.json().await.unwrap();
    assert_eq!(det_j["title"], "Post V1");
    assert_eq!(det_j["has_hidden_admin_fields"], false);

    // Static /revisions/compare route must take precedence over /{rev_id}
    let cmp_cur = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!(
            "/api/admin/content-types/post/entries/{pid}/revisions/compare?from={rev1}&to=current"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(cmp_cur.status(), StatusCode::OK);
    let cmp_j: Value = cmp_cur.json().await.unwrap();
    assert!(
        !cmp_j["diff"]["title_diff"]["hunks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !cmp_j["diff"]["body_text_diff"]["hunks"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Per-user autosave isolation + monotonic client_seq + compare to autosave
    let auto_put = api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/post/entries/{pid}/autosave"), None,
        Some(json!({ "source_surface": "editor", "base_version": 4, "client_seq": 2, "title": "Autosave WIP", "values": { "body": "<p>Auto</p>" } }))).await;
    assert_eq!(auto_put.status(), StatusCode::OK);
    let ap_j: Value = auto_put.json().await.unwrap();
    assert_eq!(ap_j["applied"], true);
    assert_eq!(ap_j["is_stale_base"], false);

    let stale: Value = api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/post/entries/{pid}/autosave"), None,
        Some(json!({ "source_surface": "editor", "base_version": 4, "client_seq": 1, "title": "Stale" }))).await.json().await.unwrap();
    assert_eq!(stale["applied"], false);
    assert_eq!(stale["client_seq"], 2);

    let cmp_auto: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!(
            "/api/admin/content-types/post/entries/{pid}/revisions/compare?from=current&to=autosave"
        ),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(cmp_auto["to_meta"]["target"]["kind"], "autosave");
    assert!(
        !cmp_auto["diff"]["title_diff"]["hunks"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let ed_auto: Value = api(
        &http,
        &site,
        Some(&editor),
        Method::GET,
        &format!("/api/admin/content-types/post/entries/{pid}/autosave"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(ed_auto["autosave"].is_null());
    assert_eq!(api(&http, &site, Some(&editor), Method::GET, &format!("/api/admin/content-types/post/entries/{pid}/revisions/compare?from=current&to=autosave"), None, None).await.status(), StatusCode::NOT_FOUND);

    let live: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/post/entries/{pid}"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(live["title"], "Post V3");
    assert_eq!(live["publication_status"], "published");

    // 4-phase restore clears autosave and preserves slug/publication_status
    let restored = api(
        &http,
        &site,
        Some(&admin),
        Method::POST,
        &format!("/api/admin/content-types/post/entries/{pid}/revisions/{rev1}/restore"),
        Some("\"v4\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(restored.status(), StatusCode::OK);
    assert_eq!(restored.headers()[header::ETAG], "\"v5\"");
    assert_eq!(restored.headers()[header::CACHE_CONTROL], "no-store");
    let rj: Value = restored.json().await.unwrap();
    assert_eq!(rj["entry"]["version"], 5);
    assert_eq!(rj["entry"]["title"], "Post V1");
    assert_eq!(rj["entry"]["slug"], "rev-post-s02");
    assert_eq!(rj["entry"]["publication_status"], "published");
    assert_eq!(rj["revision"]["source"], "restore");
    assert_eq!(rj["revision"]["restored_from_revision_number"], 1);

    let auto_after: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/post/entries/{pid}/autosave"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(auto_after["autosave"].is_null());

    // Undo-restore to pre-restore revision & idempotent DELETE /autosave
    let undo = api(
        &http,
        &site,
        Some(&admin),
        Method::POST,
        &format!("/api/admin/content-types/post/entries/{pid}/revisions/{rev3}/restore"),
        Some("\"v5\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(undo.status(), StatusCode::OK);
    assert_eq!(undo.headers()[header::ETAG], "\"v6\"");
    assert_eq!(
        undo.json::<Value>().await.unwrap()["entry"]["title"],
        "Post V3"
    );

    assert_eq!(api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/post/entries/{pid}/autosave"), None,
        Some(json!({ "source_surface": "editor", "base_version": 6, "client_seq": 1, "title": "Temp" }))).await.status(), StatusCode::OK);
    assert_eq!(
        api(
            &http,
            &site,
            Some(&admin),
            Method::DELETE,
            &format!("/api/admin/content-types/post/entries/{pid}/autosave"),
            None,
            None
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api(
            &http,
            &site,
            Some(&admin),
            Method::DELETE,
            &format!("/api/admin/content-types/post/entries/{pid}/autosave"),
            None,
            None
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
}
