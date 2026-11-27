//! S02 Missing-Ref Preflight, Required-Field 422, Role ACL & Error Precedence Integration Tests.
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
            "opendash-rev-pref-s02-{label}-{}-{seq}",
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

async fn setup_admin_and_editor(http: &Client, site: &TempSite) -> (Session, Session) {
    let setup = api(
        http,
        site,
        None,
        Method::POST,
        "/auth/setup",
        None,
        Some(json!({ "username": "admin", "password": "AdminPassword123!" })),
    )
    .await;
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

async fn upload_png(
    http: &Client,
    site: &TempSite,
    sess: &Session,
    name: &str,
    rgb: [u8; 3],
) -> String {
    let mut img = image::RgbaImage::new(4, 4);
    for px in img.pixels_mut() {
        *px = image::Rgba([rgb[0], rgb[1], rgb[2], 255]);
    }
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    let resp = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, &sess.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &sess.csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", name)
        .body(bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn delete_media(http: &Client, site: &TempSite, sess: &Session, media_id: &str) {
    let info = api(
        http,
        site,
        Some(sess),
        Method::GET,
        &format!("/api/admin/media/{media_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(info.status(), StatusCode::OK);
    let etag = info.headers()[header::ETAG].to_str().unwrap().to_owned();
    let del = api(
        http,
        site,
        Some(sess),
        Method::DELETE,
        &format!("/api/admin/media/{media_id}"),
        Some(&etag),
        None,
    )
    .await;
    assert!(
        del.status().is_success(),
        "delete media {media_id} failed: {}",
        del.text().await.unwrap()
    );
}

#[tokio::test]
async fn missing_refs_preflight_required_veto_acl_and_error_precedence() {
    let site = start_site("preflight").await;
    let http = Client::new();
    let (admin, editor) = setup_admin_and_editor(&http, &site).await;

    let img_opt = upload_png(&http, &site, &admin, "opt.png", [210, 20, 20]).await;
    let img_a_req = upload_png(&http, &site, &admin, "a_req.png", [20, 210, 20]).await;
    let img_b_req1 = upload_png(&http, &site, &admin, "b_req1.png", [20, 20, 210]).await;
    let img_b_req2 = upload_png(&http, &site, &admin, "b_req2.png", [210, 210, 20]).await;

    let cpt_body = json!({
        "key": "story",
        "label": "Stories",
        "fields": [
            { "key": "cover", "label": "Cover", "type": "media", "required": false },
            { "key": "req_photo", "label": "Required Photo", "type": "media", "required": true },
            { "key": "admin_secret", "label": "Admin Secret", "type": "text",
              "acl": { "min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private" } }
        ],
        "settings": { "public": false, "has_archive": false, "publication": "draft_publish", "supports": ["editor", "featured_image"] }
    });
    assert_eq!(
        api(
            &http,
            &site,
            Some(&admin),
            Method::POST,
            "/api/admin/content-types",
            None,
            Some(cpt_body)
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let token = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        "/api/admin/content-types/story/effective-editor-config",
        None,
        None,
    )
    .await
    .json::<Value>()
    .await
    .unwrap()["schema_token"]
        .as_str()
        .unwrap()
        .to_owned();

    // Entry A: optional media removal + Admin-only field preserved across Editor restore
    let ca = api(&http, &site, Some(&admin), Method::POST, "/api/admin/content-types/story/entries/composite", None,
        Some(json!({ "expected_schema_token": &token, "title": "Story A V1", "slug": "story-a",
            "values": { "cover": &img_opt, "req_photo": &img_a_req, "admin_secret": "SECRET_V1" } }))).await;
    assert_eq!(ca.status(), StatusCode::CREATED);
    let a_id = ca.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();

    assert_eq!(api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/story/entries/{a_id}/composite"), None,
        Some(json!({ "expected_version": 1, "expected_schema_token": &token, "title": "Story A V2", "slug": "story-a",
            "values": { "cover": null, "req_photo": &img_a_req, "admin_secret": "SECRET_V2" } }))).await.status(), StatusCode::OK);
    delete_media(&http, &site, &admin, &img_opt).await;

    let revs_a: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/story/entries/{a_id}/revisions"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let rev_a1 = revs_a["items"][1]["id"].as_i64().unwrap();
    let rev_a2 = revs_a["items"][0]["id"].as_i64().unwrap();

    let ed_det: Value = api(
        &http,
        &site,
        Some(&editor),
        Method::GET,
        &format!("/api/admin/content-types/story/entries/{a_id}/revisions/{rev_a1}"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(ed_det["has_hidden_admin_fields"], true);
    assert!(
        !ed_det["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["key"] == "admin_secret")
    );

    let ed_cmp: Value = api(&http, &site, Some(&editor), Method::GET, &format!("/api/admin/content-types/story/entries/{a_id}/revisions/compare?from={rev_a1}&to={rev_a2}"), None, None).await.json().await.unwrap();
    assert_eq!(ed_cmp["diff"]["has_hidden_admin_changes"], true);
    assert!(
        !ed_cmp["diff"]["field_diffs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["key"] == "admin_secret")
    );

    let missing_r = api(
        &http,
        &site,
        Some(&editor),
        Method::POST,
        &format!("/api/admin/content-types/story/entries/{a_id}/revisions/{rev_a1}/restore"),
        Some("\"v2\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(missing_r.status(), StatusCode::CONFLICT);
    assert_eq!(
        missing_r.headers()["x-opendash-error-code"],
        "revision.missing_refs"
    );
    let mj: Value = missing_r.json().await.unwrap();
    assert_eq!(mj["code"], "revision_missing_refs");
    assert_eq!(mj["missing_refs"]["missing_media"][0]["media_id"], img_opt);

    let ed_rest = api(
        &http,
        &site,
        Some(&editor),
        Method::POST,
        &format!("/api/admin/content-types/story/entries/{a_id}/revisions/{rev_a1}/restore"),
        Some("\"v2\""),
        Some(json!({ "acknowledge_missing_refs": true })),
    )
    .await;
    assert_eq!(ed_rest.status(), StatusCode::OK);
    let erj: Value = ed_rest.json().await.unwrap();
    assert!(erj["entry"]["values"].get("admin_secret").is_none());
    assert!(
        erj["entry"]["values"]
            .get("cover")
            .is_none_or(Value::is_null)
    );

    let admin_check: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/story/entries/{a_id}"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(admin_check["values"]["admin_secret"], "SECRET_V2");

    // Entry B: required media missing after sanitization -> 422 required_field_missing
    let cb = api(&http, &site, Some(&admin), Method::POST, "/api/admin/content-types/story/entries/composite", None,
        Some(json!({ "expected_schema_token": &token, "title": "Story B V1", "slug": "story-b", "values": { "req_photo": &img_b_req1 } }))).await;
    assert_eq!(cb.status(), StatusCode::CREATED);
    let b_id = cb.json::<Value>().await.unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();

    assert_eq!(api(&http, &site, Some(&admin), Method::PUT, &format!("/api/admin/content-types/story/entries/{b_id}/composite"), None,
        Some(json!({ "expected_version": 1, "expected_schema_token": &token, "title": "Story B V2", "slug": "story-b", "values": { "req_photo": &img_b_req2 } }))).await.status(), StatusCode::OK);
    delete_media(&http, &site, &admin, &img_b_req1).await;

    let revs_b: Value = api(
        &http,
        &site,
        Some(&admin),
        Method::GET,
        &format!("/api/admin/content-types/story/entries/{b_id}/revisions"),
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let rev_b1 = revs_b["items"][1]["id"].as_i64().unwrap();

    let veto = api(
        &http,
        &site,
        Some(&admin),
        Method::POST,
        &format!("/api/admin/content-types/story/entries/{b_id}/revisions/{rev_b1}/restore"),
        Some("\"v2\""),
        Some(json!({ "acknowledge_missing_refs": true })),
    )
    .await;
    assert_eq!(veto.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        veto.headers()["x-opendash-error-code"],
        "revision.required_field_missing"
    );
    let vj: Value = veto.json().await.unwrap();
    assert_eq!(vj["code"], "required_field_missing");
    assert!(vj["field_errors"].get("req_photo").is_some());

    // Error precedence: 401, 403, 400, 415, 428, and 412 CAS checked BEFORE 409 entry_trashed
    let b_revs = format!("/api/admin/content-types/story/entries/{b_id}/revisions");
    let b_auto = format!("/api/admin/content-types/story/entries/{b_id}/autosave");
    let b_rest =
        format!("/api/admin/content-types/story/entries/{b_id}/revisions/{rev_b1}/restore");
    for (sess, method, path, em, body, want) in [
        (
            None,
            Method::GET,
            b_revs.as_str(),
            None,
            None,
            StatusCode::UNAUTHORIZED,
        ),
        (
            Some(&admin),
            Method::GET,
            &format!("/api/admin/content-types/section/entries/{b_id}/revisions"),
            None,
            None,
            StatusCode::FORBIDDEN,
        ),
        (
            Some(&admin),
            Method::GET,
            &format!("/api/admin/content-types/BAD!/entries/{b_id}/revisions"),
            None,
            None,
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(&admin),
            Method::GET,
            "/api/admin/content-types/story/entries/0/revisions",
            None,
            None,
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(&admin),
            Method::GET,
            &format!("{b_revs}?limit=99"),
            None,
            None,
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(&admin),
            Method::PUT,
            b_auto.as_str(),
            None,
            Some(
                json!({ "source_surface": "editor", "base_version": 2, "client_seq": 1, "title": null }),
            ),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(&admin),
            Method::POST,
            b_rest.as_str(),
            None,
            Some(json!({})),
            StatusCode::PRECONDITION_REQUIRED,
        ),
    ] {
        assert_eq!(
            api(&http, &site, sess, method.clone(), path, em, body)
                .await
                .status(),
            want,
            "{method} {path}"
        );
    }
    let non_json = http
        .put(format!("{}{b_auto}", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::CONTENT_TYPE, "text/plain")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(non_json.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    assert_eq!(
        api(
            &http,
            &site,
            Some(&admin),
            Method::POST,
            &format!("/api/admin/content-types/story/entries/{b_id}/trash"),
            Some("\"v2\""),
            Some(json!({}))
        )
        .await
        .status(),
        StatusCode::OK
    );
    for (method, path, em, body, want) in [
        (
            Method::POST,
            b_rest.as_str(),
            Some("\"v1\""),
            Some(json!({})),
            StatusCode::PRECONDITION_FAILED,
        ),
        (
            Method::POST,
            b_rest.as_str(),
            Some("\"v3\""),
            Some(json!({})),
            StatusCode::CONFLICT,
        ),
        (
            Method::GET,
            b_revs.as_str(),
            None,
            None,
            StatusCode::CONFLICT,
        ),
        (
            Method::GET,
            b_auto.as_str(),
            None,
            None,
            StatusCode::CONFLICT,
        ),
        (
            Method::PUT,
            b_auto.as_str(),
            None,
            Some(
                json!({ "source_surface": "editor", "base_version": 3, "client_seq": 1, "title": "Trashed" }),
            ),
            StatusCode::CONFLICT,
        ),
        (
            Method::DELETE,
            b_auto.as_str(),
            None,
            None,
            StatusCode::CONFLICT,
        ),
    ] {
        assert_eq!(
            api(&http, &site, Some(&admin), method.clone(), path, em, body)
                .await
                .status(),
            want,
            "trashed {method} {path}"
        );
    }
}
