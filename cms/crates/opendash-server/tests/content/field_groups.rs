use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = fs::canonicalize(std::env::temp_dir())
            .expect("canonical temp root")
            .join(format!(
                "opendash-m7-s09-t01-{label}-{}-{id}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningServer {
    child: Option<Child>,
    base_url: String,
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let bin = env!("CARGO_BIN_EXE_opendash");
        let log_path = site_root.join("server-startup.log");
        let stderr = fs::File::create(&log_path).expect("create startup log");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..300 {
            if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
                if resp.status() == StatusCode::OK {
                    return Self {
                        child: Some(child),
                        base_url,
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "server failed to become ready at {base_url}: {}",
            fs::read_to_string(&log_path).unwrap_or_else(|_| "missing startup log".into())
        );
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        for (name, data) in entries {
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

async fn setup_and_login_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, base, "admin", "Password123!").await
}

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

// Core Pattern remains private and supports the unified entry Builder.
fn assert_core_builtins(cts: &[Value]) {
    let keys = cts
        .iter()
        .map(|ct| ct["descriptor"]["key"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["page", "pattern", "post"]);
    for ct in cts {
        assert_eq!(ct["version"], 1);
        assert!(ct["schema_token"].as_str().unwrap().starts_with("sha256:"));
        assert_eq!(ct["owner"]["kind"], "core");
    }
    let section = &cts[1];
    assert_eq!(
        section["descriptor"]["settings"],
        json!({
            "public": false,
            "has_archive": false,
            "publication": "immediate",
            "supports": ["builder"]
        })
    );
    assert_eq!(section["status"], "active");
}

#[tokio::test]
async fn migration_0034_normalization_roundtrip_and_compat_rules() {
    let dir = TempDirGuard::new("main-flow");
    let site_root = dir.path();
    let db_path = site_root.join("database").join("opendash.sqlite");

    let srv = RunningServer::start(site_root).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    // Create Editor user to verify RBAC 403 on admin schema/group endpoints
    let ed_resp = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_resp.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) = login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;

    // 1. Check GET /api/admin/content-types includes immutable private Pattern.
    let cts: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_core_builtins(&cts);
    let mutate_section = client
        .put(format!("{}/api/admin/content-types/pattern", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(
            &json!({"key":"pattern", "label":"Public Pattern", "fields":[],
            "settings":{"public":true,"has_archive":true,"publication":"draft_publish",
                "supports":["editor","featured_image"]}}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(mutate_section.status(), StatusCode::FORBIDDEN);

    // 2. AC-05a Case 1: Base CPT field missing public_exposure -> normalized to "public"
    let create_cpt = client
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                { "key": "client_name", "label": "Khách hàng", "type": "text", "required": true },
                { "key": "budget", "label": "Ngân sách", "type": "number", "required": false }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_cpt.status(), StatusCode::CREATED);
    let created_cpt: Value = create_cpt.json().await.unwrap();
    assert_eq!(created_cpt["version"], 1);
    assert!(
        created_cpt["descriptor"]["fields"][0]["acl"].is_null()
            || created_cpt["descriptor"]["fields"][0]["acl"]["public_exposure"] == "public"
    );
    let old_schema_token = created_cpt["schema_token"].as_str().unwrap().to_string();

    // Create an entry so has_existing_entries = true for portfolio
    let create_entry = client
        .post(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv.base_url
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({
            "title": "Biệt thự Thảo Điền",
            "slug": "biet-thu-thao-dien",
            "values": { "client_name": "Anh Minh", "budget": 500 }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_entry.status(), StatusCode::CREATED);

    // 3. PUT /api/admin/content-types/{type_key}: If-Match 428 / 412, Editor 403, Core 403
    assert_eq!(
        client
            .put(format!(
                "{}/api/admin/content-types/portfolio",
                srv.base_url
            ))
            .header("Cookie", &ed_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &ed_csrf)
            .header("If-Match", "\"v1\"")
            .json(&json!({ "key": "portfolio", "label": "Hack", "fields": [] }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .put(format!(
                "{}/api/admin/content-types/portfolio",
                srv.base_url
            ))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({ "key": "portfolio", "label": "Dự án", "fields": [] }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_REQUIRED
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/content-types/portfolio", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header("If-Match", "\"v99\"")
            .json(&json!({
                "key": "portfolio",
                "label": "Dự án",
                "fields": [
                    { "key": "client_name", "label": "Khách hàng", "type": "text", "required": true }
                ]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_FAILED
    );

    // 4. AC-03a,b,d,e,f & AC-02: Rule COMPAT-4 + ACL + default_value + incompatible_schema_change
    // AC-03a: min_edit_role = editor, min_view_role = admin -> 422 invalid_field_acl
    let acl_err = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "bad_acl",
            "title": "Bad ACL",
            "target_types": ["portfolio"],
            "fields": [{
                "key": "secret_code",
                "label": "Secret",
                "type": "text",
                "acl": {"min_view_role": "admin", "min_edit_role": "editor"}
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(acl_err.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(acl_err.text().await.unwrap().contains("invalid_field_acl"));

    // AC-03b: default_value on Image field -> 422 invalid_field_default
    let def_img_err = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "bad_def",
            "title": "Bad Default",
            "target_types": ["portfolio"],
            "fields": [{
                "key": "bad_img",
                "label": "Img",
                "type": "media",
                "default": "abc"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert!(matches!(
        def_img_err.status(),
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
    ));
    assert!(!def_img_err.text().await.unwrap().is_empty());

    // AC-03d: required: true + min_edit_role: admin without default_value -> 422 admin_required_field_missing_default
    let req_admin_missing = client
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "bad_cpt",
            "label": "Bad CPT",
            "fields": [{
                "key": "internal_code",
                "label": "Internal Code",
                "type": "text",
                "required": true,
                "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(req_admin_missing.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        req_admin_missing
            .text()
            .await
            .unwrap()
            .contains("admin_required_field_missing_default")
    );

    // AC-03e: portfolio has_existing_entries = true, client_name is required=true, min_edit_role=editor, default_value=None
    // Attempt PUT changing min_edit_role -> admin WITHOUT default_value -> 422 admin_required_field_missing_default
    let e_fail = client
        .put(format!(
            "{}/api/admin/content-types/portfolio",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"}
                },
                { "key": "budget", "label": "Ngân sách", "type": "number", "required": false }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(e_fail.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        e_fail
            .text()
            .await
            .unwrap()
            .contains("admin_required_field_missing_default")
    );

    // Now provide valid default_value -> 200 OK (version becomes 2)
    let e_ok = client
        .put(format!(
            "{}/api/admin/content-types/portfolio",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"},
                    "default": "Khách lẻ"
                },
                { "key": "budget", "label": "Ngân sách", "type": "number", "required": false }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(e_ok.status(), StatusCode::OK);
    let updated_cpt: Value = e_ok.json().await.unwrap();
    assert_eq!(updated_cpt["version"], 2);
    assert_ne!(
        updated_cpt["schema_token"].as_str().unwrap(),
        old_schema_token
    );

    // AC-03f: Try removing default_value while required=true, min_edit_role=admin -> 422 admin_required_field_missing_default
    let f_fail = client
        .put(format!(
            "{}/api/admin/content-types/portfolio",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"}
                },
                { "key": "budget", "label": "Ngân sách", "type": "number", "required": false }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(f_fail.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        f_fail
            .text()
            .await
            .unwrap()
            .contains("admin_required_field_missing_default")
    );

    // AC-02: Try turning optional field `budget` into required=true (even with default_value) -> 409 incompatible_schema_change
    let opt_to_req = client
        .put(format!("{}/api/admin/content-types/portfolio", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"},
                    "default": "Khách lẻ"
                },
                { "key": "budget", "label": "Ngân sách", "type": "number", "required": true, "default": 100 }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(opt_to_req.status(), StatusCode::CONFLICT);
    assert!(
        opt_to_req
            .text()
            .await
            .unwrap()
            .contains("incompatible_schema_change")
    );

    // 5. AC-05a Cases 2 & 3: Extension Field Group normalization (missing -> private, explicit "public" -> public)
    let create_fg = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "project_meta",
            "title": "Thông tin mở rộng",
            "target_types": ["page", "portfolio"],
            "fields": [
                { "key": "internal_notes", "label": "Ghi chú nội bộ", "type": "textarea", "acl": {"public_exposure": "private"} },
                { "key": "public_badge", "label": "Nhãn công khai", "type": "text", "acl": {"public_exposure": "public"} },
                { "key": "hero_photo", "label": "Ảnh Hero", "type": "media" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_fg.status(), StatusCode::CREATED);
    let fg_body: Value = create_fg.json().await.unwrap();
    assert_eq!(fg_body["group_id"], "user:project_meta");
    assert_eq!(fg_body["fields"][0]["acl"]["public_exposure"], "private");
    assert_eq!(fg_body["fields"][1]["acl"]["public_exposure"], "public");
    assert_eq!(fg_body["fields"][2]["acl"]["public_exposure"], "public");

    // Reserved layout: prefix rejected 422
    let layout_prefix = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "group_id": "layout:hijack",
            "title": "Hijack",
            "target_types": ["post"],
            "fields": [{ "key": "x_note", "label": "X", "type": "text" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(layout_prefix.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // AC-02: Symmetric field_key_collision across User Field Group, Base CPT PUT, and Theme Manifest
    let collide_fg = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "collide_group",
            "title": "Collide",
            "target_types": ["portfolio"],
            "fields": [{ "key": "client_name", "label": "Trùng", "type": "text" }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(collide_fg.status(), StatusCode::CONFLICT);
    assert!(
        collide_fg
            .text()
            .await
            .unwrap()
            .contains("field_key_collision")
    );

    let collide_cpt_put = client
        .put(format!(
            "{}/api/admin/content-types/portfolio",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"},
                    "default": "Khách lẻ"
                },
                { "key": "budget", "label": "Ngân sách", "type": "number", "required": false },
                { "key": "internal_notes", "label": "Trùng với group", "type": "textarea" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(collide_cpt_put.status(), StatusCode::CONFLICT);
    assert!(
        collide_cpt_put
            .text()
            .await
            .unwrap()
            .contains("field_key_collision")
    );

    // AC-01b: User PUT trying to remove `budget` completely when entries exist -> 409 cannot_remove_field_with_data
    let remove_with_data = client
        .put(format!(
            "{}/api/admin/content-types/portfolio",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({
            "key": "portfolio",
            "label": "Dự án",
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"},
                    "default": "Khách lẻ"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(remove_with_data.status(), StatusCode::CONFLICT);
    assert!(
        remove_with_data
            .text()
            .await
            .unwrap()
            .contains("cannot_remove_field_with_data")
    );

    // 6. Restart round-trip verification: normalized public_exposure & groups persist cleanly
    drop(srv);
    let srv2 = RunningServer::start(site_root).await;
    let (admin_cookie2, admin_csrf2) =
        login(&client, &srv2.base_url, "admin", "Password123!").await;

    let cts2: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let portfolio2 = cts2
        .iter()
        .find(|c| c["descriptor"]["key"] == "portfolio")
        .unwrap();
    assert_eq!(portfolio2["version"], 2);
    assert_eq!(
        portfolio2["descriptor"]["fields"][0]["acl"]["public_exposure"],
        "public"
    );

    let fgs2: Vec<Value> = client
        .get(format!("{}/api/admin/content-field-groups", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fgs2.len(), 1);
    assert_eq!(fgs2[0]["fields"][0]["acl"]["public_exposure"], "private");
    assert_eq!(fgs2[0]["fields"][1]["acl"]["public_exposure"], "public");
    assert_eq!(fgs2[0]["fields"][2]["acl"]["public_exposure"], "public");

    // 7. Theme manifest field_groups & 3-level tombstone + collision check
    let theme_manifest = serde_json::to_vec(&json!({
        "manifest_version": 3,
        "theme_id": "studio-theme",
        "name": "Studio Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "field_groups": [
            {
                "local_key": "studio_seo_extra",
                "title": "Studio Extra",
                "target_types": ["post", "portfolio"],
                "fields": [
                    { "key": "studio_kicker", "label": "Kicker", "type": "text" },
                    { "key": "studio_public", "label": "PublicTag", "type": "text", "acl": {"public_exposure": "public"} }
                ]
            }
        ]
    }))
    .unwrap();
    let theme_zip = build_zip(&[
        ("theme.json", &theme_manifest),
        ("index.html", b"<main>Studio</main>"),
    ]);
    let inst_theme = client
        .post(format!("{}/api/admin/themes", srv2.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &admin_csrf2)
        .body(theme_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_theme.status(), StatusCode::CREATED);
    let rev1 = inst_theme.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();
    let act1 = client
        .put(format!("{}/api/admin/themes/active", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &admin_csrf2)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "revision_id": rev1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(act1.status(), StatusCode::OK);

    // Verify theme field_group normalized and persisted
    let fgs3: Vec<Value> = client
        .get(format!("{}/api/admin/content-field-groups", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let studio_fg = fgs3
        .iter()
        .find(|g| g["group_id"] == "theme:studio-theme:studio_seo_extra")
        .unwrap();
    assert_eq!(studio_fg["fields"][0]["acl"]["public_exposure"], "private");
    assert_eq!(studio_fg["fields"][1]["acl"]["public_exposure"], "public");

    // Theme v3 colliding with existing `client_name` on portfolio -> 409 field_key_collision
    let bad_theme_manifest = serde_json::to_vec(&json!({
        "manifest_version": 3,
        "theme_id": "studio-theme",
        "name": "Studio Theme",
        "version": "1.1.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "field_groups": [
            {
                "local_key": "studio_seo_extra",
                "title": "Studio Extra",
                "target_types": ["portfolio"],
                "fields": [
                    { "key": "client_name", "label": "Collide", "type": "text" }
                ]
            }
        ]
    }))
    .unwrap();
    let bad_theme_zip = build_zip(&[
        ("theme.json", &bad_theme_manifest),
        ("index.html", b"<main>Studio</main>"),
    ]);
    let inst_bad = client
        .post(format!("{}/api/admin/themes", srv2.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &admin_csrf2)
        .body(bad_theme_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(inst_bad.status(), StatusCode::CREATED);
    let rev_bad = inst_bad.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();
    let act_bad = client
        .put(format!("{}/api/admin/themes/active", srv2.base_url))
        .header("Cookie", &admin_cookie2)
        .header("Origin", &srv2.base_url)
        .header("X-CSRF-Token", &admin_csrf2)
        .header("If-Match", "\"v2\"")
        .json(&json!({ "revision_id": rev_bad }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_bad.status(), StatusCode::CONFLICT);
    assert!(
        act_bad
            .text()
            .await
            .unwrap()
            .contains("field_key_collision")
    );

    // 8. AC-01b: User DELETE withdraws group & targets without deleting rows, and
    //    remove-target when persisted values exist marks association_state = 'withdrawn'.
    drop(srv2);
    {
        let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .unwrap();
        // Insert a page entry holding a value for `hero_photo` from `user:project_meta`
        let fake_media_id = "a".repeat(64);
        sqlx::query(
            "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state, generation, version)
             VALUES (?, 'image', 'hero.png', 'image/png', 'png', 100, 10, 10, 10, 10, 'active', 1, 1)",
        )
        .bind(&fake_media_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_entries (content_type_key, slug, title, values_json, publication_status, version, locale, translation_group_id)
             VALUES ('page', 'about-us', 'About Us', ?, 'draft', 1, 'vi', 'eg-' || lower(hex(randomblob(8))))",
        )
        .bind(format!("{{\"hero_photo\":\"{fake_media_id}\"}}"))
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    let srv3 = RunningServer::start(site_root).await;
    let (admin_cookie3, admin_csrf3) =
        login(&client, &srv3.base_url, "admin", "Password123!").await;

    // Remove "page" from `user:project_meta` target_types -> `page` association becomes `withdrawn`
    let upd_fg = client
        .put(format!(
            "{}/api/admin/content-field-groups/user:project_meta",
            srv3.base_url
        ))
        .header("Cookie", &admin_cookie3)
        .header("Origin", &srv3.base_url)
        .header("X-CSRF-Token", &admin_csrf3)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "local_key": "project_meta",
            "title": "Thông tin mở rộng",
            "target_types": ["portfolio"],
            "fields": [
                { "key": "internal_notes", "label": "Ghi chú nội bộ", "type": "textarea", "acl": {"public_exposure": "private"} },
                { "key": "public_badge", "label": "Nhãn công khai", "type": "text", "acl": {"public_exposure": "public"} },
                { "key": "hero_photo", "label": "Ảnh Hero", "type": "media" }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd_fg.status(), StatusCode::OK);
    let upd_fg_val: Value = upd_fg.json().await.unwrap();
    let page_target = upd_fg_val["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["type_key"] == "page")
        .expect("page target tombstone must be preserved");
    assert_eq!(page_target["association_state"], "withdrawn");

    // DELETE withdraws group and all remaining targets
    let del_fg = client
        .delete(format!(
            "{}/api/admin/content-field-groups/user:project_meta",
            srv3.base_url
        ))
        .header("Cookie", &admin_cookie3)
        .header("Origin", &srv3.base_url)
        .header("X-CSRF-Token", &admin_csrf3)
        .header("If-Match", "\"v2\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_fg.status(), StatusCode::OK);
    let del_fg_val: Value = del_fg.json().await.unwrap();
    assert_eq!(del_fg_val["status"], "withdrawn");
    assert!(
        del_fg_val["targets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["association_state"] == "withdrawn")
    );

    // Media reconciliation / delete guard still pins `hero_photo` (not orphan -> 409 Conflict on DELETE)
    let fake_media_id = "a".repeat(64);
    let del_media = client
        .delete(format!("{}/api/admin/media/{fake_media_id}", srv3.base_url))
        .header("Cookie", &admin_cookie3)
        .header("Origin", &srv3.base_url)
        .header("X-CSRF-Token", &admin_csrf3)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(
        del_media.status(),
        StatusCode::CONFLICT,
        "media referenced in withdrawn target/group must remain pinned"
    );
    drop(srv3);
}

#[tokio::test]
async fn migration_0034_upgrades_legacy_0001_fixture_preserving_existing_data() {
    let dir = TempDirGuard::new("upgrade-fixture");
    let site_root = dir.path();
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir
        .join("..")
        .join("..")
        .join("fixtures")
        .join("baseline_0001.sqlite");
    fs::copy(&fixture, &db_path).expect("copy baseline_0001.sqlite");

    // Start server — runs preflight_builtin_key_collisions + migrations 0002..0034
    let srv = RunningServer::start(site_root).await;
    let client = Client::new();
    let (admin_cookie, _) = setup_and_login_admin(&client, &srv.base_url).await;

    let cts: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_core_builtins(&cts);
    drop(srv);

    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    let mig34: i64 = sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version = 34")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mig34, 1);
    pool.close().await;
}
