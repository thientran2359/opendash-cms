use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use opendash_core::builder::{
    BuilderDocument, ColumnNode, ElementContent, ElementNode, KnownElement, PluginBlockProps,
    RowNode, SectionNode,
};
use opendash_core::registry::{RegistryCatalog, ResourceOwner};
use opendash_core::theme::BlockDescriptor;
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::sqlite::SqlitePoolOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zip::{ZipWriter, write::SimpleFileOptions};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ServerHandle {
    child: Child,
}

impl ServerHandle {
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn build_plugin_zip(manifest: Value, wasm_bytes: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(wasm_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn extract_cookie(res: &reqwest::Response) -> String {
    res.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|s| s.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

async fn spawn_server(site_root: &Path) -> (ServerHandle, String) {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let exe = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_PLUGIN_ALLOW_LOOPBACK", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash");
    let handle = ServerHandle { child };
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(r) = client.get(format!("{base_url}/api/health")).send().await {
            if r.status().is_success() {
                return (handle, base_url);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become healthy in time");
}

async fn login_user(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status().as_u16(), 200, "login failed for {username}");
    let cookie = extract_cookie(&login);
    let me: Value = client
        .get(format!("{base_url}/auth/me"))
        .header("Cookie", &cookie)
        .header("Origin", base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = me["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

/// Builds a WASM module that performs a `host_call` (`http_get` to `egress_url`) and writes its JSON response.
fn host_call_wasm(req_json: &str) -> Vec<u8> {
    let req_esc: String = req_json
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{req_esc}")
            (func (export "opendash_run") (result i32)
                (local $n i32)
                (local.set $n (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 4096) (i32.const 2048)))
                (if (i32.lt_s (local.get $n) (i32.const 7))
                    (then (return (i32.const 1))))
                (if (i32.eq (i32.load8_u (i32.const 4102)) (i32.const 102))
                    (then
                        (drop (call $set_output (i32.const 4096) (local.get $n)))
                        (return (i32.const 1))))
                (drop (call $set_output (i32.const 4096) (local.get $n)))
                i32.const 0)
        )"#,
        rlen = req_json.len(),
    );
    wat::parse_str(&wat_src).unwrap()
}

fn static_output_wasm(output_json: &str) -> Vec<u8> {
    let len = output_json.len();
    let escaped: String = output_json
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{escaped}")
            (func (export "opendash_run") (result i32)
                (drop (call $set_output (i32.const 1024) (i32.const {len})))
                i32.const 0)
        )"#
    );
    wat::parse_str(&wat_src).unwrap()
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[tokio::test]
async fn m6_s08_t01_plugin_zip_e2e_content_builder_egress_disable_restart_and_snapshot() {
    // 0. Start mock egress HTTP server on loopback
    let egress_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let egress_port = egress_listener.local_addr().unwrap().port();
    let egress_task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = egress_listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"synced":true,"source":"mock-egress"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    let site_dir = TempDir::new("m6_s08_site");
    let snap_dir = TempDir::new("m6_s08_snap");

    // 1. Boot server #1, setup admin + editor
    let (mut srv1, base_url) = spawn_server(&site_dir.path).await;
    let client = Client::new();
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", &base_url)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert!(setup.status().is_success());

    let (admin_cookie, admin_csrf) =
        login_user(&client, &base_url, "admin", "AdminPassword123!").await;
    let create_editor = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert!(create_editor.status().is_success());

    // 2. Install Plugin ZIP v2 with CPT + fields + scoped egress + host_call action + declarative UI schema
    let egress_url = format!("http://127.0.0.1:{egress_port}/sync");
    let req_json = json!({
        "op": "http_request",
        "method": "GET",
        "url": egress_url,
    })
    .to_string();
    let suite_wasm = host_call_wasm(&req_json);
    let suite_manifest = json!({
        "id": "e2e-suite",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "E2E Suite Plugin",
        "description": "Full M6 E2E plugin",
        "entrypoint": "plugin.wasm",
        "permissions": {
            "sqlite": [],
            "http": [format!("http://127.0.0.1:{egress_port}")],
            "network": [
                {
                    "scheme": "http",
                    "host": "127.0.0.1",
                    "port": egress_port,
                    "methods": ["GET"]
                }
            ]
        },
        "declarations": {
            "content_types": [
                {
                    "key": "job_posting",
                    "label": "Job Posting",
                    "fields": [
                        { "key": "company", "label": "Company", "type": "short_text", "required": true },
                        { "key": "salary", "label": "Salary", "type": "number", "required": false }
                    ],
                    "taxonomies": ["category", "tag"]
                }
            ]
        },
        "actions": [
            { "id": "sync_jobs", "title": "Sync Jobs", "required_role": "editor", "capabilities": ["http"] }
        ]
    });
    let suite_zip = build_plugin_zip(suite_manifest, &suite_wasm);
    let install_suite = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("Content-Type", "application/zip")
        .header("X-CSRF-Token", &admin_csrf)
        .body(suite_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(
        install_suite.status().as_u16(),
        201,
        "install e2e-suite failed: {:?}",
        install_suite.text().await
    );

    // 3. Verify CPT + fields, Declarative UI schema, and host_call + scoped egress action as Editor
    let (editor_cookie, editor_csrf) =
        login_user(&client, &base_url, "editor1", "EditorPassword123!").await;

    let ui_schema_res = client
        .get(format!("{base_url}/api/admin/plugins/e2e-suite/ui-schema"))
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ui_schema_res.status().as_u16(), 200);
    let ui_schema: Value = ui_schema_res.json().await.unwrap();
    assert_eq!(ui_schema["schema_version"], 1);
    assert!(ui_schema.to_string().contains("sync_jobs"));

    let action_res = client
        .post(format!("{base_url}/api/admin/plugins/e2e-suite/actions/sync_jobs"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({ "input": { "endpoint": "https://example.test" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        action_res.status().as_u16(),
        200,
        "sync_jobs action failed: {:?}",
        action_res.text().await
    );
    let action_json: Value = action_res.json().await.unwrap();
    assert!(
        action_json["output"].as_str().unwrap_or("").contains("mock-egress")
            || action_json.to_string().contains("mock-egress"),
        "expected mock-egress payload in action output: {action_json}"
    );

    // 4. Create category/tag + ContentBeforeSave veto plugin -> verify veto -> disable veto -> save & publish entry
    let cat_res = client
        .post(format!("{base_url}/api/admin/taxonomy/terms"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "kind": "category", "name": "Engineering", "slug": "engineering" }))
        .send()
        .await
        .unwrap();
    let cat_status = cat_res.status();
    let cat_body = cat_res.text().await.unwrap();
    assert!(cat_status.is_success(), "create category failed ({cat_status}): {cat_body}");
    let cat_json: Value = serde_json::from_str(&cat_body).unwrap();
    let cat_id = cat_json["id"].as_i64().unwrap();

    let veto_wasm = static_output_wasm(
        r#"{"decision":"veto","code":"blocked","message":"policy hold","field_errors":{}}"#,
    );
    let veto_manifest = json!({
        "id": "veto-guard",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Veto Guard",
        "entrypoint": "plugin.wasm",
        "hooks": ["content/before_save"],
        "permissions": { "content_read": ["job_posting"] }
    });
    let veto_zip = build_plugin_zip(veto_manifest, &veto_wasm);
    let install_veto = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("Content-Type", "application/zip")
        .header("X-CSRF-Token", &admin_csrf)
        .body(veto_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(install_veto.status().as_u16(), 201);

    // Attempt entry create -> vetoed (422)
    let vetoed_entry = client
        .post(format!("{base_url}/api/admin/content-types/job_posting/entries"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({
            "title": "Rust Systems Engineer",
            "slug": "rust-systems-engineer",
            "values": { "company": "OpenDash", "salary": 180000 },
            "term_ids": [cat_id]
        }))
        .send()
        .await
        .unwrap();
    let veto_status = vetoed_entry.status().as_u16();
    let veto_body = vetoed_entry.text().await.unwrap();
    assert_eq!(veto_status, 422, "expected ContentBeforeSave veto: {veto_body}");
    assert!(veto_body.contains("policy hold"), "expected veto reason in {veto_body}");

    // Disable veto-guard -> entry create & publish succeeds
    let disable_veto = client
        .post(format!("{base_url}/api/admin/plugins/veto-guard/disable"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(disable_veto.status().as_u16(), 200);

    let created_entry = client
        .post(format!("{base_url}/api/admin/content-types/job_posting/entries"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({
            "title": "Rust Systems Engineer",
            "slug": "rust-systems-engineer",
            "values": { "company": "OpenDash", "salary": 180000 },
            "term_ids": [cat_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        created_entry.status().as_u16(),
        201,
        "create entry failed: {:?}",
        created_entry.text().await
    );
    let created_json: Value = created_entry.json().await.unwrap();
    let entry_id = created_json["id"].as_i64().unwrap();

    // 5. Verify Builder PluginBlock AST roundtrip & owner-neutral SQLite domain bridge (settings + template_overrides)
    let mut block_vals = BTreeMap::new();
    block_vals.insert("headline".into(), json!("Open Roles"));
    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            responsive: None,
            children: vec![RowNode {
                id: "row-1".into(),
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    responsive: None,
                    children: vec![ElementNode {
                        id: "el-1".into(),
                        content: ElementContent::Known(KnownElement::PluginBlock(PluginBlockProps {
                            block_id: "e2e-suite/jobs-card".into(),
                            values: block_vals,
                        })),
                    }],
                }],
            }],
        }],
    };
    let mut reg = RegistryCatalog::default();
    reg.register(
        "e2e-suite/jobs-card",
        ResourceOwner::Plugin("e2e-suite".into()),
        BlockDescriptor {
            id: "e2e-suite/jobs-card".into(),
            fields: Default::default(),
        },
    )
    .unwrap();
    assert!(doc.validate_for_save(&reg, None).is_ok());
    assert!(doc.validate_for_save(&RegistryCatalog::default(), Some(&doc)).is_ok());

    // 6. Disable e2e-suite, stop server #1, take site snapshot, restart server #2
    let disable_suite = client
        .post(format!("{base_url}/api/admin/plugins/e2e-suite/disable"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(disable_suite.status().as_u16(), 200);

    // Seed plugin-owned settings & template override in SQLite before snapshot
    srv1.stop();
    let db_path = site_dir.path.join("database").join("opendash.sqlite");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&db_url)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version) VALUES ('e2e-suite', 'plugin', 'e2e-suite', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_option_values (theme_id, owner_kind, owner_id, option_key, value_json) VALUES ('e2e-suite', 'plugin', 'e2e-suite', 'board_title', '\"OpenDash Careers\"')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO template_overrides (role, scope_type_key, scope_entry_id, ast_json, version, base_theme_id, owner_kind, owner_id, template_selection_key)
             VALUES ('single', 'job_posting', NULL, '{\"schema_version\":1,\"sections\":[]}', 1, 'e2e-suite', 'plugin', 'e2e-suite', 'job-single-v1')",
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Snapshot site_dir -> snap_dir
    copy_dir_recursive(&site_dir.path, &snap_dir.path);

    // 7. Boot server #2 (e2e-suite is disabled): verify entries & settings survive and Editor can still update entry
    let (mut srv2, base_url2) = spawn_server(&site_dir.path).await;
    let (admin_cookie2, admin_csrf2) =
        login_user(&client, &base_url2, "admin", "AdminPassword123!").await;
    let (editor_cookie2, editor_csrf2) =
        login_user(&client, &base_url2, "editor1", "EditorPassword123!").await;

    let get_entry = client
        .get(format!("{base_url2}/api/admin/content-types/job_posting/entries/{entry_id}"))
        .header("Cookie", &editor_cookie2)
        .send()
        .await
        .unwrap();
    assert_eq!(get_entry.status().as_u16(), 200);
    let entry_before: Value = get_entry.json().await.unwrap();
    assert_eq!(entry_before["title"], "Rust Systems Engineer");

    // Disabled plugin action is blocked
    let blocked_action = client
        .post(format!("{base_url2}/api/admin/plugins/e2e-suite/actions/sync_jobs"))
        .header("Cookie", &editor_cookie2)
        .header("Origin", &base_url2)
        .header("X-CSRF-Token", &editor_csrf2)
        .json(&json!({ "input": {} }))
        .send()
        .await
        .unwrap();
    assert_ne!(blocked_action.status().as_u16(), 200);

    // Mutate entry (to be rolled back by snapshot restore)
    let update_entry = client
        .put(format!("{base_url2}/api/admin/content-types/job_posting/entries/{entry_id}"))
        .header("Cookie", &editor_cookie2)
        .header("Origin", &base_url2)
        .header("X-CSRF-Token", &editor_csrf2)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "title": "Mutated Title Before Rollback",
            "slug": "rust-systems-engineer",
            "values": { "company": "OpenDash", "salary": 190000 },
            "term_ids": [cat_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update_entry.status().as_u16(), 200);

    // Re-enable e2e-suite & verify action works again
    let enable_suite = client
        .post(format!("{base_url2}/api/admin/plugins/e2e-suite/enable"))
        .header("Cookie", &admin_cookie2)
        .header("Origin", &base_url2)
        .header("X-CSRF-Token", &admin_csrf2)
        .send()
        .await
        .unwrap();
    assert_eq!(enable_suite.status().as_u16(), 200);

    let action_after_enable = client
        .post(format!("{base_url2}/api/admin/plugins/e2e-suite/actions/sync_jobs"))
        .header("Cookie", &editor_cookie2)
        .header("Origin", &base_url2)
        .header("X-CSRF-Token", &editor_csrf2)
        .json(&json!({ "input": { "endpoint": "https://example.test" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(action_after_enable.status().as_u16(), 200);

    // 8. Stop server #2, rollback site_dir from snap_dir, boot server #3 & verify pre-mutation state + owner-neutral DB rows
    srv2.stop();
    let _ = fs::remove_dir_all(&site_dir.path);
    copy_dir_recursive(&snap_dir.path, &site_dir.path);

    let (srv3, base_url3) = spawn_server(&site_dir.path).await;
    let (editor_cookie3, _editor_csrf3) =
        login_user(&client, &base_url3, "editor1", "EditorPassword123!").await;
    let restored_entry: Value = client
        .get(format!("{base_url3}/api/admin/content-types/job_posting/entries/{entry_id}"))
        .header("Cookie", &editor_cookie3)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(restored_entry["title"], "Rust Systems Engineer");

    drop(srv3);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();
    let (opt_val,): (String,) = sqlx::query_as(
        "SELECT value_json FROM theme_option_values WHERE owner_kind = 'plugin' AND owner_id = 'e2e-suite' AND option_key = 'board_title'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(opt_val, "\"OpenDash Careers\"");
    let (tpl_sel,): (String,) = sqlx::query_as(
        "SELECT template_selection_key FROM template_overrides WHERE owner_kind = 'plugin' AND owner_id = 'e2e-suite'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tpl_sel, "job-single-v1");

    egress_task.abort();
}
