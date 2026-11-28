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
            "opendash-m7-s10-t01-{label}-{}-{id}",
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
async fn panel_registry_partition_auto_append_validation_and_rbac_projection() {
    let dir = TempDirGuard::new("panel-layout");
    let srv = RunningServer::start(dir.path()).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    // Create Editor user
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

    // 1. Create CPT `project` + Field Groups covering required+editor, admin-edit, and admin-view
    let create_cpt = client
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
    let create_cpt_status = create_cpt.status();
    let create_cpt_text = create_cpt.text().await.unwrap();
    assert_eq!(
        create_cpt_status,
        StatusCode::CREATED,
        "create_cpt failed: {create_cpt_text}"
    );
    for fg_payload in [
        json!({
            "local_key": "project_info",
            "title": "Thông tin dự án",
            "target_types": ["project"],
            "default_region": "main_bottom",
            "default_order": 50,
            "fields": [
                {
                    "key": "client_name",
                    "label": "Khách hàng",
                    "type": "text",
                    "required": true,
                    "acl": {"min_view_role": "editor", "min_edit_role": "editor"}
                },
                {
                    "key": "budget",
                    "label": "Ngân sách",
                    "type": "number",
                    "required": false,
                    "acl": {"min_view_role": "editor", "min_edit_role": "admin"}
                }
            ]
        }),
        json!({
            "local_key": "project_status",
            "title": "Tiến độ",
            "target_types": ["project"],
            "default_region": "main_bottom",
            "default_order": 55,
            "fields": [
                {
                    "key": "completion_date",
                    "label": "Ngày hoàn thành",
                    "type": "date",
                    "required": false
                }
            ]
        }),
        json!({
            "local_key": "project_internal",
            "title": "Nội bộ Admin",
            "location": [[
                { "param": "content_type", "op": "eq", "value": "project" },
                { "param": "user_role", "op": "eq", "value": "admin" }
            ]],
            "default_region": "sidebar",
            "default_order": 90,
            "fields": [
                {
                    "key": "internal_note",
                    "label": "Ghi chú nội bộ",
                    "type": "text",
                    "required": false,
                    "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
                }
            ]
        }),
    ] {
        let r = client
            .post(format!("{}/api/admin/content-field-groups", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&fg_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
    }
    let cts_init: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let initial_schema_token = cts_init
        .iter()
        .find(|c| c["descriptor"]["key"] == "project")
        .unwrap()["schema_token"]
        .as_str()
        .unwrap()
        .to_string();

    // 2. Editor gets 403 on GET & PUT /editor-layout
    let ed_get_layout = client
        .get(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &ed_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ed_get_layout.status(), StatusCode::FORBIDDEN);

    let ed_put_layout = client
        .put(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .header("If-Match", "\"v0\"")
        .json(&json!({
            "schema_version": 1,
            "type_key": "project",
            "version": 0,
            "regions": {
                "main_top": [],
                "main_primary": [],
                "main_bottom": [],
                "sidebar": []
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_put_layout.status(), StatusCode::FORBIDDEN);

    // 3. Admin gets default layout (version 0, ETag "v0")
    let get_v0 = client
        .get(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get_v0.status(), StatusCode::OK);
    assert_eq!(
        get_v0.headers().get("etag").unwrap().to_str().unwrap(),
        "\"v0\""
    );
    let v0_body: Value = get_v0.json().await.unwrap();
    assert_eq!(v0_body["version"], 0);

    // 4. PUT /editor-layout moving `user:project_info` to main_top (order 25) and `user:project_status` to sidebar (order 35)
    let mut layout_put_payload = v0_body["layout"].clone();
    for reg in ["main_top", "main_primary", "main_bottom", "sidebar"] {
        layout_put_payload["regions"][reg]
            .as_array_mut()
            .unwrap()
            .retain(|p| {
                p["panel_id"] != "user:project_info" && p["panel_id"] != "user:project_status"
            });
    }
    layout_put_payload["regions"]["main_top"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "panel_id": "user:project_info",
            "order": 25,
            "enabled": true,
            "collapsed_by_default": false,
            "subgroups": [
                {
                    "subgroup_id": "commercial",
                    "title": "Thương mại",
                    "field_keys": ["client_name", "budget"]
                }
            ]
        }));
    layout_put_payload["regions"]["sidebar"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "panel_id": "user:project_status",
            "order": 35,
            "enabled": true,
            "collapsed_by_default": false,
            "subgroups": []
        }));

    let put_v1 = client
        .put(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v0\"")
        .json(&layout_put_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(put_v1.status(), StatusCode::OK);
    assert_eq!(
        put_v1.headers().get("etag").unwrap().to_str().unwrap(),
        "\"v1\""
    );
    let v1_body: Value = put_v1.json().await.unwrap();
    assert_eq!(v1_body["version"], 1);

    // Verify CPT schema_token was NOT altered by layout save
    let cts_after: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let project_after = cts_after
        .iter()
        .find(|c| c["descriptor"]["key"] == "project")
        .unwrap();
    assert_eq!(project_after["schema_token"], initial_schema_token);

    // Verify stale If-Match "v0" returns 409 Conflict
    let stale_put = client
        .put(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v0\"")
        .json(&layout_put_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(stale_put.status(), StatusCode::CONFLICT);

    // 5. Reject invalid layout mutations (AC-04c):
    // (a) Placing core:header_bar into sidebar (outside allowed_regions = [header_bar])
    let mut bad_region = v1_body["layout"].clone();
    bad_region["regions"]["sidebar"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "panel_id": "core:header_bar",
            "order": 99,
            "enabled": true,
            "collapsed_by_default": false,
            "subgroups": []
        }));
    let bad_region_resp = client
        .put(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&bad_region)
        .send()
        .await
        .unwrap();
    assert_eq!(bad_region_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // (b) Disabling `user:project_info` which contains `client_name` (`required: true, min_edit_role: Editor`)
    let mut hide_required = v1_body["layout"].clone();
    for p in hide_required["regions"]["main_top"].as_array_mut().unwrap() {
        if p["panel_id"] == "user:project_info" {
            p["enabled"] = json!(false);
        }
    }
    let hide_req_resp = client
        .put(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&hide_required)
        .send()
        .await
        .unwrap();
    assert_eq!(hide_req_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let hide_req_err = hide_req_resp.text().await.unwrap();
    assert!(hide_req_err.contains("required_field_hidden_or_inaccessible"));

    // (c) Duplicate panel_id across two regions
    let mut dup_field = v1_body["layout"].clone();
    dup_field["regions"]["sidebar"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "panel_id": "user:project_info",
            "order": 99,
            "enabled": true,
            "collapsed_by_default": false,
            "subgroups": []
        }));
    let dup_resp = client
        .put(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&dup_field)
        .send()
        .await
        .unwrap();
    assert_eq!(dup_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 6. Auto-append newly registered User FieldGroup & Plugin Action/FieldGroup panels (Rule LAY-6)
    let fg_create = client
        .post(format!("{}/api/admin/content-field-groups", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "local_key": "project_links",
            "title": "Liên kết dự án",
            "target_types": ["project"],
            "fields": [
                { "key": "repo_url", "label": "Repo URL", "type": "text", "required": false }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fg_create.status(), StatusCode::CREATED);

    let wasm_bytes = wat::parse_str(
        "(module (memory (export \"memory\") 1) (func (export \"opendash_run\") (result i32) (i32.const 0)))",
    )
    .unwrap();
    let plugin_manifest = serde_json::to_vec(&json!({
        "id": "seo-panel-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "min_host_version": "0.1.0",
        "name": "SEO Panel Plugin",
        "entrypoint": "plugin.wasm",
        "actions": [{
            "id": "analyze_seo",
            "title": "Analyze SEO",
            "required_role": "editor",
            "capabilities": ["kv_read"]
        }],
        "declarations": {
            "field_groups": [{
                "local_key": "seo_fields",
                "title": "SEO Fields",
                "target_types": ["project"],
                "fields": [
                    { "key": "seo_title", "label": "SEO Title", "type": "text", "required": false }
                ]
            }]
        },
        "editor_panels": [{
            "local_key": "seo_action",
            "title": "SEO Check",
            "action_id": "analyze_seo",
            "button_label": "Run SEO",
            "applies_to": ["project"],
            "default_region": "sidebar",
            "default_order": 85
        }]
    }))
    .unwrap();
    let plugin_zip = build_zip(&[
        ("manifest.json", &plugin_manifest),
        ("plugin.wasm", &wasm_bytes),
    ]);
    let plugin_install = client
        .post(format!("{}/api/admin/plugins", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(plugin_zip)
        .send()
        .await
        .unwrap();
    let plugin_install_status = plugin_install.status();
    let plugin_install_text = plugin_install.text().await.unwrap();
    assert_eq!(
        plugin_install_status,
        StatusCode::CREATED,
        "plugin_install failed: {plugin_install_text}"
    );

    // Reject invalid plugin manifest with reserved `core:` prefix in editor_panels
    let bad_plugin_manifest = serde_json::to_vec(&json!({
        "id": "evil-panel-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Evil",
        "entrypoint": "plugin.wasm",
        "actions": [{
            "id": "act",
            "title": "Act",
            "required_role": "editor",
            "capabilities": ["kv_read"]
        }],
        "editor_panels": [{
            "local_key": "core:hijack",
            "title": "Hijack",
            "action_id": "act",
            "button_label": "Go",
            "applies_to": ["project"]
        }]
    }))
    .unwrap();
    let bad_plugin_zip = build_zip(&[
        ("manifest.json", &bad_plugin_manifest),
        ("plugin.wasm", &wasm_bytes),
    ]);
    let bad_plugin_resp = client
        .post(format!("{}/api/admin/plugins", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(bad_plugin_zip)
        .send()
        .await
        .unwrap();
    assert!(
        bad_plugin_resp.status() == StatusCode::BAD_REQUEST
            || bad_plugin_resp.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    // Reject theme declaring `editor_panels` (Theme cannot register Action panels)
    let bad_theme_manifest = serde_json::to_vec(&json!({
        "manifest_version": 3,
        "theme_id": "bad-action-theme",
        "name": "Bad Action Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "editor_panels": [{
            "local_key": "evil",
            "title": "Evil",
            "action_id": "act",
            "button_label": "Go",
            "applies_to": ["project"]
        }]
    }))
    .unwrap();
    let bad_theme_zip = build_zip(&[
        ("theme.json", &bad_theme_manifest),
        ("index.html", b"<main>Home</main>"),
    ]);
    let bad_theme_resp = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(bad_theme_zip)
        .send()
        .await
        .unwrap();
    assert!(
        bad_theme_resp.status() == StatusCode::BAD_REQUEST
            || bad_theme_resp.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    // Verify GET /editor-layout keeps old placements & auto-appends new user/plugin panels
    let get_after_append: Value = client
        .get(format!(
            "{}/api/admin/content-types/project/editor-layout",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let main_top = get_after_append["layout"]["regions"]["main_top"]
        .as_array()
        .unwrap();
    let main_bottom = get_after_append["layout"]["regions"]["main_bottom"]
        .as_array()
        .unwrap();
    let sidebar = get_after_append["layout"]["regions"]["sidebar"]
        .as_array()
        .unwrap();
    assert!(
        main_top
            .iter()
            .any(|p| p["panel_id"] == "user:project_info" && p["order"] == 25)
    );
    assert!(
        sidebar
            .iter()
            .any(|p| p["panel_id"] == "user:project_status" && p["order"] == 35)
    );
    assert!(
        main_bottom
            .iter()
            .chain(sidebar.iter())
            .chain(main_top.iter())
            .any(|p| p["panel_id"] == "user:project_links")
    );
    assert!(
        main_bottom
            .iter()
            .chain(sidebar.iter())
            .chain(main_top.iter())
            .any(|p| p["panel_id"] == "plugin:seo-panel-plugin:seo_fields")
    );
    assert!(
        sidebar
            .iter()
            .any(|p| p["panel_id"] == "plugin:seo-panel-plugin:seo_action")
    );

    // 7. Editor calls GET /effective-editor-config -> 200 OK with ACL filtering (AC-09b)
    let ed_cfg_resp = client
        .get(format!(
            "{}/api/admin/content-types/project/effective-editor-config",
            srv.base_url
        ))
        .header("Cookie", &ed_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ed_cfg_resp.status(), StatusCode::OK);
    let ed_cfg: Value = ed_cfg_resp.json().await.unwrap();
    assert_eq!(ed_cfg["layout_token"], "v1");
    assert_ne!(ed_cfg["schema_token"], initial_schema_token); // schema_token changed when field_groups were added

    let ed_fields = ed_cfg["effective_fields"].as_object().unwrap();
    assert!(ed_fields.get("internal_note").is_none()); // hidden for Editor
    assert_eq!(ed_fields["budget"]["editable"], false); // Admin-only edit -> read-only for Editor
    assert_eq!(ed_fields["client_name"]["editable"], true);
    assert_eq!(ed_fields["repo_url"]["editable"], true);
    assert_eq!(ed_fields["seo_title"]["editable"], true);

    // Admin sees `internal_note` and has `editable: true` on `budget`
    let adm_cfg: Value = client
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
    let adm_fields = adm_cfg["effective_fields"].as_object().unwrap();
    assert!(adm_fields.get("internal_note").is_some());
    assert_eq!(adm_fields["budget"]["editable"], true);
    let adm_internal_panel = adm_cfg["regions"]["sidebar"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["panel"]["panel_id"] == "user:project_internal")
        .unwrap();
    assert_eq!(adm_internal_panel["field_keys"], json!(["internal_note"]));
}
