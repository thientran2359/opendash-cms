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
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m7-s11-t01-{label}-{}-{id}",
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
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..100 {
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
        panic!("server failed to become ready at {base_url}");
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

#[tokio::test]
async fn composite_entry_save_patch_semantics_acl_fence_and_profile_api() {
    let dir = TempDirGuard::new("composite-profile");
    let srv = RunningServer::start(dir.path()).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    // Create Editor user
    let ed_create = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_create.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) = login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;

    // Create CPT `project` + FieldGroup with default_value, admin-edit, and admin-view fields
    let cpt_resp = client
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "project",
            "label": "Dự án"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cpt_resp.status(), StatusCode::CREATED);
    let fg_resp = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "project_fields",
            "title": "Project Fields",
            "target_types": ["project"],
            "fields": [
                {
                    "key": "summary",
                    "label": "Tóm tắt",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "editor"}
                },
                {
                    "key": "optional_tag",
                    "label": "Nhãn tùy chọn",
                    "type": "text",
                    "required": false,
                    "default": "Standard",
                    "acl": {"min_view_role": "editor", "min_edit_role": "editor"}
                },
                {
                    "key": "approved_budget",
                    "label": "Ngân sách duyệt",
                    "type": "number",
                    "required": false,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"}
                },
                {
                    "key": "internal_audit",
                    "label": "Kiểm toán nội bộ",
                    "type": "text",
                    "required": false,
                    "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fg_resp.status(), StatusCode::CREATED);
    let cfg_json: Value = client
        .get(format!(
            "{}/api/admin/content-types/project/effective-editor-config",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let schema_token = cfg_json["schema_token"].as_str().unwrap().to_string();

    // 1. Missing expected_schema_token -> 428; stale expected_schema_token -> 409
    let missing_tok = client
        .post(format!(
            "{}/api/admin/content-types/project/entries/composite",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Alpha",
            "slug": "alpha",
            "values": { "summary": "Hello" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_tok.status(), StatusCode::PRECONDITION_REQUIRED);

    let stale_tok = client
        .post(format!("{}/api/admin/content-types/project/entries/composite", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "title": "Alpha",
            "slug": "alpha",
            "values": { "summary": "Hello" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_tok.status(), StatusCode::CONFLICT);

    // 2. Atomic composite POST (entry + custom_path + seo) -> version 1
    let create_comp = client
        .post(format!(
            "{}/api/admin/content-types/project/entries/composite",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_schema_token": &schema_token,
            "title": "Alpha Project",
            "slug": "alpha-project",
            "values": {
                "summary": "Initial summary",
                "approved_budget": 1000,
                "internal_audit": "Top Secret"
            },
            "custom_path": "/du-an-alpha",
            "seo": {
                "title": "Alpha SEO",
                "description": "Alpha Desc",
                "noindex": false
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_comp.status(), StatusCode::CREATED);
    let created: Value = create_comp.json().await.unwrap();
    let entry_id = created["entry"]["id"].as_i64().unwrap();
    assert_eq!(created["entry"]["version"], 1);
    assert_eq!(created["entry"]["values"]["optional_tag"], "Standard");
    assert_eq!(created["custom_path"], "/du-an-alpha");
    assert_eq!(created["seo"]["title"], "Alpha SEO");

    // 3. Rollback guarantee (AC-07a): PUT with invalid og_image_id rolls back title/values/version completely
    let bad_seo_put = client
        .put(format!(
            "{}/api/admin/content-types/project/entries/{}/composite",
            srv.base_url, entry_id
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_version": 1,
            "expected_schema_token": &schema_token,
            "title": "Should Rollback",
            "slug": "alpha-project",
            "values": { "summary": "Should not persist" },
            "seo": {
                "title": "Bad SEO",
                "og_image_id": "nonexistent-media-id"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_seo_put.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Verify entry remained untouched at version 1
    let admin_get: Value = client
        .get(format!(
            "{}/api/admin/content-types/project/entries/{}",
            srv.base_url, entry_id
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(admin_get["version"], 1);
    assert_eq!(admin_get["title"], "Alpha Project");
    assert_eq!(admin_get["values"]["summary"], "Initial summary");
    assert_eq!(admin_get["values"]["internal_audit"], "Top Secret");

    // Verify Editor GET hides `internal_audit` (`min_view_role: admin`)
    let ed_get: Value = client
        .get(format!(
            "{}/api/admin/content-types/project/entries/{}",
            srv.base_url, entry_id
        ))
        .header("Cookie", &ed_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(ed_get["values"].get("internal_audit").is_none());
    assert_eq!(ed_get["values"]["approved_budget"], 1000);

    // 4. Editor sending `approved_budget` (even with SAME value 1000 or null) -> 403 field_edit_forbidden (AC-06d)
    let ed_forbidden_same = client
        .put(format!(
            "{}/api/admin/content-types/project/entries/{}/composite",
            srv.base_url, entry_id
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({
            "expected_version": 1,
            "expected_schema_token": &schema_token,
            "title": "Alpha Project",
            "slug": "alpha-project",
            "values": { "approved_budget": 1000 }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_forbidden_same.status(), StatusCode::FORBIDDEN);

    // 5. Admin clears `optional_tag: null`, `custom_path: null`, `seo: null` (Clear -> version 2)
    let admin_clear = client
        .put(format!(
            "{}/api/admin/content-types/project/entries/{}/composite",
            srv.base_url, entry_id
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "expected_version": 1,
            "expected_schema_token": &schema_token,
            "title": "Alpha Project v2",
            "slug": "alpha-project",
            "values": { "optional_tag": null },
            "custom_path": null,
            "seo": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_clear.status(), StatusCode::OK);
    let cleared: Value = admin_clear.json().await.unwrap();
    assert_eq!(cleared["entry"]["version"], 2);
    assert!(cleared["entry"]["values"].get("optional_tag").is_none());
    assert!(cleared["custom_path"].is_null());
    assert!(cleared["seo"].is_null());

    // 6. Editor updates title while omitting `optional_tag` -> `optional_tag` stays ABSENT (Omitted = Preserve, never re-injects default_value)
    //    and `approved_budget` + `internal_audit` remain preserved -> version 3
    let ed_omit_put = client
        .put(format!(
            "{}/api/admin/content-types/project/entries/{}/composite",
            srv.base_url, entry_id
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({
            "expected_version": 2,
            "expected_schema_token": &schema_token,
            "title": "Alpha Project v3",
            "slug": "alpha-project",
            "values": { "summary": "Editor updated summary" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_omit_put.status(), StatusCode::OK);
    let v3: Value = ed_omit_put.json().await.unwrap();
    assert_eq!(v3["entry"]["version"], 3);
    assert!(v3["entry"]["values"].get("optional_tag").is_none());
    assert_eq!(v3["entry"]["values"]["approved_budget"], 1000);

    // Admin still sees `internal_audit: "Top Secret"` preserved
    let admin_check: Value = client
        .get(format!(
            "{}/api/admin/content-types/project/entries/{}",
            srv.base_url, entry_id
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(admin_check["values"]["internal_audit"], "Top Secret");

    // 7. Profile API (AC-09c): Editor reads and updates own profile/password
    let ed_prof: Value = client
        .get(format!("{}/api/admin/profile", srv.base_url))
        .header("Cookie", &ed_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(ed_prof["username"], "editor1");
    let prof_ver = ed_prof["version"].as_i64().unwrap();

    // Wrong current_password -> 403
    let bad_pw = client
        .put(format!("{}/api/admin/profile", srv.base_url))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .header("If-Match", format!("\"v{prof_ver}\""))
        .json(&json!({
            "current_password": "WrongPassword123!",
            "new_password": "NewEditorPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_pw.status(), StatusCode::FORBIDDEN);

    // Attempting to send `role` -> rejected (400/422)
    let role_escalate = client
        .put(format!("{}/api/admin/profile", srv.base_url))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .header("If-Match", format!("\"v{prof_ver}\""))
        .json(&json!({
            "username": "editor1_renamed",
            "role": "admin"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        role_escalate.status() == StatusCode::BAD_REQUEST
            || role_escalate.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    // Valid password change -> 200 OK & login with new password succeeds
    let good_pw = client
        .put(format!("{}/api/admin/profile", srv.base_url))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .header("If-Match", format!("\"v{prof_ver}\""))
        .json(&json!({
            "current_password": "EditorPassword123!",
            "new_password": "NewEditorPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(good_pw.status(), StatusCode::OK);
    let (_new_cookie, _) = login(&client, &srv.base_url, "editor1", "NewEditorPassword123!").await;
}
