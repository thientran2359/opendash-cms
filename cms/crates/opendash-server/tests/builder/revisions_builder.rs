//! S02 Builder/Studio/Pattern Revision & Autosave Integration Tests.
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
            "opendash-rev-bldr-s02-{label}-{}-{seq}",
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
        &admin,
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
    sess: &Session,
    method: Method,
    path: &str,
    if_match: Option<&str>,
    body: Option<Value>,
) -> Response {
    let mut req = http
        .request(method, format!("{}{path}", site.base))
        .header(header::COOKIE, &sess.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &sess.csrf);
    if let Some(etag) = if_match {
        req = req.header(header::IF_MATCH, etag);
    }
    if let Some(json_body) = body {
        req = req.json(&json_body);
    }
    req.send().await.expect("api request")
}

fn builder_doc(heading: &str, extra_para: Option<&str>) -> Value {
    let mut children = vec![
        json!({ "id": "hd-1", "block_id": "core/heading", "props": { "level": "h2", "text": heading } }),
    ];
    if let Some(copy) = extra_para {
        children
            .push(json!({ "id": "p-1", "block_id": "core/text", "props": { "content": copy } }));
    }
    json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec-1",
            "block_id": "core/section",
            "props": {},
            "children": [{
                "id": "row-1",
                "block_id": "core/row",
                "props": {},
                "children": [{
                    "id": "col-1",
                    "block_id": "core/column",
                    "props": {},
                    "children": children
                }]
            }]
        }]
    })
}

#[tokio::test]
async fn studio_mode2_revisions_block_diff_reset_and_autosave_restore() {
    let site = start_site("studio").await;
    let http = Client::new();
    let (admin, _editor) = setup_admin_and_editor(&http, &site).await;

    let created = api(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/content-types/page/entries",
        None,
        Some(json!({ "title": "Landing", "slug": "landing", "values": { "body": "<p>Init</p>" } })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let pid = created.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let doc_a = builder_doc("Hero V1", None);
    let doc_b = builder_doc("Hero V2", Some("Added Subcopy"));

    assert_eq!(
        api(
            &http,
            &site,
            &admin,
            Method::PUT,
            &format!("/api/admin/entries/page/{pid}/builder"),
            Some("\"v1\""),
            Some(doc_a.clone())
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        api(
            &http,
            &site,
            &admin,
            Method::PUT,
            &format!("/api/admin/entries/page/{pid}/builder"),
            Some("\"v2\""),
            Some(doc_b)
        )
        .await
        .status(),
        StatusCode::OK
    );

    let revs: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!("/api/admin/content-types/page/entries/{pid}/revisions"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let rev_v2 = revs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["entry_version"] == 2 && r["source"] == "builder_save")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let rev_v3 = revs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["entry_version"] == 3 && r["source"] == "builder_save")
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    let cmp_b = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!(
            "/api/admin/content-types/page/entries/{pid}/revisions/compare?from={rev_v2}&to=current"
        ),
        None,
        None,
    )
    .await;
    assert_eq!(cmp_b.status(), StatusCode::OK);
    let cmp_j: Value = cmp_b.json().await.unwrap();
    assert!(cmp_j["diff"]["mode_transition"].is_null());
    let items = cmp_j["diff"]["body_block_diff"]["items"]
        .as_array()
        .unwrap();
    assert!(
        items
            .iter()
            .any(|i| i["node_id"] == "hd-1" && i["kind"] == "modified")
    );
    assert!(
        items
            .iter()
            .any(|i| i["node_id"] == "p-1" && i["kind"] == "added")
    );

    assert_eq!(
        api(
            &http,
            &site,
            &admin,
            Method::POST,
            &format!("/api/admin/entries/page/{pid}/builder/reset"),
            Some("\"v3\""),
            Some(json!({ "confirm": true }))
        )
        .await
        .status(),
        StatusCode::OK
    );

    let cmp_mode: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!(
            "/api/admin/content-types/page/entries/{pid}/revisions/compare?from={rev_v3}&to=current"
        ),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        cmp_mode["diff"]["mode_transition"],
        json!({ "from": "builder", "to": "editor" })
    );

    let sp = api(&http, &site, &admin, Method::PUT, &format!("/api/admin/content-types/page/entries/{pid}/autosave"), None,
        Some(json!({ "source_surface": "studio", "base_version": 4, "client_seq": 1, "builder_document": doc_a }))).await;
    assert_eq!(sp.status(), StatusCode::OK);
    assert_eq!(sp.json::<Value>().await.unwrap()["applied"], true);

    let sg: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!("/api/admin/content-types/page/entries/{pid}/autosave"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(sg["autosave"]["source_surface"], "studio");
    assert!(sg["autosave"]["builder_document"].is_object());

    let rest = api(
        &http,
        &site,
        &admin,
        Method::POST,
        &format!("/api/admin/content-types/page/entries/{pid}/revisions/{rev_v2}/restore"),
        Some("\"v4\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(rest.status(), StatusCode::OK);
    assert_eq!(rest.headers()[header::ETAG], "\"v5\"");
    let rj: Value = rest.json().await.unwrap();
    assert_eq!(rj["revision"]["source"], "restore");
    assert_eq!(rj["revision"]["content_mode"], "builder");

    let ba: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!("/api/admin/entries/page/{pid}/builder"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        ba["document"]["blocks"][0]["children"][0]["children"][0]["children"][0]["props"]["text"],
        "Hero V1"
    );

    let s_after: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!("/api/admin/content-types/page/entries/{pid}/autosave"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(s_after["autosave"].is_null());
}

#[tokio::test]
async fn pattern_revisions_autosave_and_disabled_studio_cpt_guards() {
    let site = start_site("pattern").await;
    let http = Client::new();
    let (admin, editor) = setup_admin_and_editor(&http, &site).await;

    let pc = api(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/patterns",
        None,
        Some(json!({ "title": "Promo Banner", "slug": "promo-banner" })),
    )
    .await;
    assert_eq!(pc.status(), StatusCode::CREATED);
    let pat_id = pc.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    let pat_a = builder_doc("Pattern V2", None);
    let pat_b = builder_doc("Pattern V3", Some("Sub"));
    assert_eq!(
        api(
            &http,
            &site,
            &admin,
            Method::PUT,
            &format!("/api/admin/entries/pattern/{pat_id}/builder"),
            Some("\"v1\""),
            Some(pat_a.clone())
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        api(
            &http,
            &site,
            &admin,
            Method::PUT,
            &format!("/api/admin/entries/pattern/{pat_id}/builder"),
            Some("\"v2\""),
            Some(pat_b)
        )
        .await
        .status(),
        StatusCode::OK
    );

    let preps: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!("/api/admin/content-types/pattern/entries/{pat_id}/revisions"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let pat_rev_v2 = preps["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["entry_version"] == 2)
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    // Editor strictly blocked (403) across pattern revision/autosave routes
    assert_eq!(
        api(
            &http,
            &site,
            &editor,
            Method::GET,
            &format!("/api/admin/content-types/pattern/entries/{pat_id}/revisions"),
            None,
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api(
            &http,
            &site,
            &editor,
            Method::GET,
            &format!("/api/admin/content-types/pattern/entries/{pat_id}/autosave"),
            None,
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(api(&http, &site, &editor, Method::PUT, &format!("/api/admin/content-types/pattern/entries/{pat_id}/autosave"), None,
        Some(json!({ "source_surface": "studio", "base_version": 3, "client_seq": 1, "builder_document": pat_a }))).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        api(
            &http,
            &site,
            &editor,
            Method::POST,
            &format!(
                "/api/admin/content-types/pattern/entries/{pat_id}/revisions/{pat_rev_v2}/restore"
            ),
            Some("\"v3\""),
            Some(json!({}))
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    assert_eq!(api(&http, &site, &admin, Method::PUT, &format!("/api/admin/content-types/pattern/entries/{pat_id}/autosave"), None,
        Some(json!({ "source_surface": "editor", "base_version": 3, "client_seq": 1, "title": "Nope" }))).await.status(), StatusCode::BAD_REQUEST);
    assert_eq!(api(&http, &site, &admin, Method::PUT, &format!("/api/admin/content-types/pattern/entries/{pat_id}/autosave"), None,
        Some(json!({ "source_surface": "studio", "base_version": 3, "client_seq": 1, "builder_document": pat_a }))).await.status(), StatusCode::OK);

    let cmp_p = api(&http, &site, &admin, Method::GET, &format!("/api/admin/content-types/pattern/entries/{pat_id}/revisions/compare?from={pat_rev_v2}&to=current"), None, None).await;
    assert_eq!(cmp_p.status(), StatusCode::OK);
    assert!(
        !cmp_p.json::<Value>().await.unwrap()["diff"]["body_block_diff"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let rest_p = api(
        &http,
        &site,
        &admin,
        Method::POST,
        &format!(
            "/api/admin/content-types/pattern/entries/{pat_id}/revisions/{pat_rev_v2}/restore"
        ),
        Some("\"v3\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(rest_p.status(), StatusCode::OK);
    assert_eq!(rest_p.headers()[header::ETAG], "\"v4\"");
    let rpj: Value = rest_p.json().await.unwrap();
    assert_eq!(rpj["entry_id"], pat_id);
    assert_eq!(rpj["type_key"], "pattern");
    assert_eq!(rpj["version"], 4);
    assert!(rpj["document"].is_object());
    assert_eq!(rpj["revision"]["source"], "restore");

    let pa_after: Value = api(
        &http,
        &site,
        &admin,
        Method::GET,
        &format!("/api/admin/content-types/pattern/entries/{pat_id}/autosave"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(pa_after["autosave"].is_null());

    // `post` has Builder disabled -> Studio autosave must fail 403
    let post_c = api(
        &http,
        &site,
        &admin,
        Method::POST,
        "/api/admin/content-types/post/entries",
        None,
        Some(json!({ "title": "Blog", "slug": "blog-post", "values": {} })),
    )
    .await;
    assert_eq!(post_c.status(), StatusCode::CREATED);
    let post_id = post_c.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(api(&http, &site, &admin, Method::PUT, &format!("/api/admin/content-types/post/entries/{post_id}/autosave"), None,
        Some(json!({ "source_surface": "studio", "base_version": 1, "client_seq": 1, "builder_document": pat_a }))).await.status(), StatusCode::FORBIDDEN);
}
