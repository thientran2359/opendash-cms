//! Slice S04 E2E & regression tests for Spec B:
//! 1. Theme Functions settings/secrets (`GET`/`PUT` redact `{"configured": bool}`, `{"clear": true}`),
//!    Theme Actions (Admin/Editor RBAC + Safe Mode `503`), Theme Functions logs (`GET /logs`),
//!    and Backup/Restore `ExtensionSecret` roundtrip into `database/secrets/{plugins,themes}/`.
//! 2. Dev Live-Edit (`--dev-theme`, `--dev-plugin`): startup guards (`recovery_safe_mode`,
//!    `trusted_public_origin`), `GET /api/health` (`dev_mode`), live hot-reload of HTML, CSS (`no-store`),
//!    declarative block, `functions.js`, `plugin.js`, `.env` / symlink blocking, `POST /api/admin/dev/sync`,
//!    clean backup snapshot with `dev_mode_active: true`, and `--check-theme`/`--check-plugin` on examples.

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let path = root.join(format!("{prefix}_{}_{nanos}", std::process::id()));
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
    log_path: PathBuf,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn spawn_server_with_args(
    site_root: &Path,
    port: u16,
    extra_args: &[String],
) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let base_url = format!("http://127.0.0.1:{port}");
    let log_path = site_root.join(format!("server_{port}.log"));
    let stderr = fs::File::create(&log_path).unwrap();
    let mut cmd = Command::new(exe);
    cmd.args(extra_args)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr));
    let child = cmd.spawn().expect("spawn opendash server");

    let handle = ServerHandle { child, log_path };
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return handle;
        }
        if Instant::now() >= deadline {
            let log = fs::read_to_string(&handle.log_path).unwrap_or_default();
            panic!("server failed to become healthy on port {port}: {log}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(
        setup.status().is_success(),
        "setup status={}",
        setup.status()
    );
    login_as(client, base_url, "admin", "AdminPassword123!").await
}

async fn login_as(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        login.status(),
        StatusCode::OK,
        "login failed for {username}"
    );
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_owned();
    (cookie, csrf)
}

fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

async fn wait_job_terminal(client: &Client, base_url: &str, cookie: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let resp = client
            .get(format!("{base_url}/api/admin/operations/jobs/{job_id}"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let job: Value = resp.json().await.unwrap();
        let state = job["state"].as_str().unwrap_or("");
        if matches!(state, "succeeded" | "failed" | "cancelled" | "interrupted") {
            return job;
        }
        assert!(Instant::now() < deadline, "job {job_id} timed out: {job}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn s04_theme_functions_secrets_actions_logs_and_extension_secret_backup_restore() {
    let tmp = TempDir::new("s04_theme_fn_backup");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let _server = spawn_server_with_args(&tmp.path, port, &[]).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login(&client, &base_url).await;

    // Create an Editor account to test RBAC on Theme Actions
    let create_editor = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login_as(&client, &base_url, "editor1", "EditorPassword123!").await;

    // Before activating a theme with functions.js, GET settings returns has_functions: false
    let initial_fn = client
        .get(format!(
            "{base_url}/api/admin/themes/active/functions/settings"
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(initial_fn.status(), StatusCode::OK);
    let initial_json: Value = initial_fn.json().await.unwrap();
    assert_eq!(initial_json["has_functions"], json!(false));

    // Install & activate Theme v3 with functions.js, secret setting, and admin/editor actions
    let theme_manifest = json!({
        "manifest_version": 1,
        "theme_id": "s04-secret-theme",
        "name": "S04 Secret Theme",
        "version": "1.0.0",
        "templates": [
            { "kind": "index", "file": "index.html" }
        ],
        "assets": [],
        "functions": {
            "min_host_version": "0.1.0",
            "permissions": {
                "plugin_secrets_read": ["api_token"]
            },
            "settings_schema": [
                { "key": "api_token", "label": "API Token", "type": "secret", "required": false },
                { "key": "label", "label": "Label", "type": "text", "default": "init" }
            ],
            "actions": [
                {
                    "id": "inspect_secret",
                    "title": "Inspect Secret",
                    "required_role": "admin",
                    "capabilities": ["plugin_secrets_read", "plugin_settings_read", "kv_write", "log"]
                },
                {
                    "id": "editor_ping",
                    "title": "Editor Ping",
                    "required_role": "editor",
                    "capabilities": ["log"]
                }
            ]
        }
    })
    .to_string();
    let functions_js = r#"
od.addAction("inspect_secret", () => {
  const secResp = od.secret.get("api_token") || {};
  const sec = secResp.value;
  const settingsResp = od.settings.get() || {};
  const settings = settingsResp.settings || {};
  od.kv.set("last_label", settings.label || "");
  od.log.info("inspect_secret ran for label=" + (settings.label || ""));
  return { secret_len: sec ? sec.length : 0, matches: sec === "theme-secret-xyz-999", label: settings.label };
});
od.addAction("editor_ping", () => {
  od.log.info("editor_ping ok");
  return { pong: "editor" };
});
"#;
    let theme_zip = build_zip(&[
        ("theme.json", theme_manifest.as_bytes()),
        ("index.html", b"<main><h1>{{ site.title }}</h1></main>"),
        ("functions.js", functions_js.as_bytes()),
    ]);
    let install_theme = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(theme_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(install_theme.status(), StatusCode::CREATED);
    let rev_id = install_theme.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let themes_list: Value = client
        .get(format!("{base_url}/api/admin/themes"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sel_ver = themes_list["selection"]["version"].as_i64().unwrap();
    let act = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{sel_ver}\""))
        .json(&json!({ "revision_id": rev_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), StatusCode::OK);

    // Also install a JS plugin with a secret setting so Backup/Restore covers BOTH plugins & themes secrets!
    let plugin_manifest = json!({
        "id": "s04-secret-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "S04 Secret Plugin",
        "entrypoint": "plugin.js",
        "permissions": {
            "plugin_secrets_read": ["hook_key"]
        },
        "settings_schema": [
            { "key": "hook_key", "label": "Hook Key", "type": "secret", "required": false }
        ]
    })
    .to_string();
    let plugin_zip = build_zip(&[
        ("manifest.json", plugin_manifest.as_bytes()),
        ("plugin.js", b"od.on('manual', () => ({ ok: true }));"),
    ]);
    let install_plugin = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(plugin_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(install_plugin.status(), StatusCode::CREATED);

    let save_plugin_sec = client
        .put(format!(
            "{base_url}/api/admin/plugins/s04-secret-plugin/settings"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "values": { "hook_key": "plugin-secret-abc-777" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_plugin_sec.status(), StatusCode::OK);

    // 1. GET Theme Functions settings
    let get_fn = client
        .get(format!(
            "{base_url}/api/admin/themes/active/functions/settings"
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get_fn.status(), StatusCode::OK);
    assert_eq!(get_fn.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(get_fn.headers()[header::ETAG], "\"v1\"");
    let fn_json: Value = get_fn.json().await.unwrap();
    assert_eq!(fn_json["has_functions"], json!(true));
    assert_eq!(
        fn_json["values"]["api_token"],
        json!({ "configured": false })
    );
    assert_eq!(fn_json["actions"].as_array().unwrap().len(), 2);

    // 2. PUT Theme Functions settings (write secret + text)
    let put_fn = client
        .put(format!(
            "{base_url}/api/admin/themes/active/functions/settings"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "values": {
                "api_token": "theme-secret-xyz-999",
                "label": "prod-banner"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_fn.status(), StatusCode::OK);
    assert_eq!(put_fn.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(put_fn.headers()[header::ETAG], "\"v2\"");
    let put_text = put_fn.text().await.unwrap();
    assert!(
        !put_text.contains("theme-secret-xyz-999"),
        "secret plaintext must never leak"
    );
    assert!(
        !put_text.contains("secret_ref"),
        "secret_ref must never leak"
    );
    let put_json: Value = serde_json::from_str(&put_text).unwrap();
    assert_eq!(
        put_json["values"]["api_token"],
        json!({ "configured": true })
    );
    assert_eq!(put_json["values"]["label"], json!("prod-banner"));

    // Verify secret file exists on disk under database/secrets/themes/s04-secret-theme/
    let theme_sec_dir = tmp.path.join("database/secrets/themes/s04-secret-theme");
    let theme_sec_files: Vec<_> = fs::read_dir(&theme_sec_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".json"))
        .collect();
    assert_eq!(theme_sec_files.len(), 1);

    // 3. Theme Actions + RBAC + Logs
    let editor_denied = client
        .post(format!(
            "{base_url}/api/admin/themes/active/actions/inspect_secret"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_denied.status(), StatusCode::FORBIDDEN);

    let editor_ok = client
        .post(format!(
            "{base_url}/api/admin/themes/active/actions/editor_ping"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_ok.status(), StatusCode::OK);
    let editor_body: Value = editor_ok.json().await.unwrap();
    assert_eq!(editor_body["data"]["pong"], json!("editor"));

    let admin_action = client
        .post(format!(
            "{base_url}/api/admin/themes/active/actions/inspect_secret"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_action.status(), StatusCode::OK);
    let admin_action_body: Value = admin_action.json().await.unwrap();
    assert_eq!(admin_action_body["success"], json!(true));
    assert_eq!(admin_action_body["data"]["matches"], json!(true));
    assert_eq!(admin_action_body["data"]["label"], json!("prod-banner"));

    let missing_action = client
        .post(format!(
            "{base_url}/api/admin/themes/active/actions/no_such_action"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_action.status(), StatusCode::NOT_FOUND);

    // 4. GET Theme Functions logs
    let logs_resp = client
        .get(format!(
            "{base_url}/api/admin/themes/active/functions/logs?limit=20"
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(logs_resp.status(), StatusCode::OK);
    assert_eq!(logs_resp.headers()[header::CACHE_CONTROL], "no-store");
    let logs_json: Value = logs_resp.json().await.unwrap();
    assert_eq!(logs_json["theme_id"], json!("s04-secret-theme"));
    let items = logs_json["items"].as_array().unwrap();
    assert!(
        items.iter().any(|it| it["message"]
            .as_str()
            .unwrap_or("")
            .contains("inspect_secret ran")),
        "expected theme log entry, got: {logs_json}"
    );

    // 5. Backup & Restore ExtensionSecret roundtrip (both plugins & themes secrets!)
    let passphrase = "StrongBackupPassphrase-S04-2026!";
    let create_bkp = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "reason": "s04-extension-secrets-roundtrip"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_bkp.status(), StatusCode::ACCEPTED);
    let bkp_job_id = create_bkp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bkp_done = wait_job_terminal(&client, &base_url, &admin_cookie, &bkp_job_id).await;
    assert_eq!(
        bkp_done["state"],
        json!("succeeded"),
        "backup failed: {bkp_done}"
    );
    let bkp_summary: Value =
        serde_json::from_str(bkp_done["summary_json"].as_str().unwrap()).unwrap();
    assert!(bkp_summary.get("dev_mode_active").is_none());

    let archive_bytes = client
        .get(format!(
            "{base_url}/api/admin/operations/backups/{bkp_job_id}/file"
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert!(!archive_bytes.is_empty());

    let up_restore = client
        .post(format!("{base_url}/api/admin/operations/restore-uploads"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(archive_bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(up_restore.status(), StatusCode::CREATED);
    let upload_id = up_restore.json::<Value>().await.unwrap()["upload_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let start_restore = client
        .post(format!("{base_url}/api/admin/operations/restores"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "upload_id": upload_id,
            "passphrase": passphrase,
            "reason": "verify-extension-secrets-restore"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(start_restore.status(), StatusCode::ACCEPTED);
    let rst_job_id = start_restore.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let rst_done = wait_job_terminal(&client, &base_url, &admin_cookie, &rst_job_id).await;
    assert_eq!(
        rst_done["state"],
        json!("succeeded"),
        "restore failed: {rst_done}"
    );
    let rst_summary: Value =
        serde_json::from_str(rst_done["summary_json"].as_str().unwrap()).unwrap();
    let target_root = PathBuf::from(rst_summary["target_root"].as_str().unwrap());
    assert!(target_root.join("READY.json").is_file());

    // Assert BOTH plugin and theme secret files were restored to database/secrets/{plugins,themes}/!
    let restored_theme_sec_dir = target_root.join("database/secrets/themes/s04-secret-theme");
    let restored_plugin_sec_dir = target_root.join("database/secrets/plugins/s04-secret-plugin");
    let restored_theme_files: Vec<_> = fs::read_dir(&restored_theme_sec_dir)
        .expect("restored theme secrets dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
        .collect();
    let restored_plugin_files: Vec<_> = fs::read_dir(&restored_plugin_sec_dir)
        .expect("restored plugin secrets dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
        .collect();
    assert_eq!(restored_theme_files.len(), 1);
    assert_eq!(restored_plugin_files.len(), 1);
    let restored_theme_sec_txt = fs::read_to_string(restored_theme_files[0].path()).unwrap();
    assert!(restored_theme_sec_txt.contains("theme-secret-xyz-999"));
    let restored_plugin_sec_txt = fs::read_to_string(restored_plugin_files[0].path()).unwrap();
    assert!(restored_plugin_sec_txt.contains("plugin-secret-abc-777"));

    // 6. Boot restored site in Safe Mode and verify:
    //    - Theme Action returns 503 safe_mode
    //    - PUT Theme Functions settings {"clear": true} clears secret even in Safe Mode
    let safe_port = free_port();
    let safe_url = format!("http://127.0.0.1:{safe_port}");
    let _safe_server = spawn_server_with_args(&target_root, safe_port, &[]).await;
    let (safe_cookie, safe_csrf) = login_as(&client, &safe_url, "admin", "AdminPassword123!").await;

    let safe_action = client
        .post(format!(
            "{safe_url}/api/admin/themes/active/actions/inspect_secret"
        ))
        .header("Origin", &safe_url)
        .header(header::COOKIE, &safe_cookie)
        .header("X-CSRF-Token", &safe_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(safe_action.status(), StatusCode::SERVICE_UNAVAILABLE);

    let clear_sec = client
        .put(format!(
            "{safe_url}/api/admin/themes/active/functions/settings"
        ))
        .header("Origin", &safe_url)
        .header(header::COOKIE, &safe_cookie)
        .header("X-CSRF-Token", &safe_csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({ "values": { "api_token": { "clear": true } } }))
        .send()
        .await
        .unwrap();
    assert_eq!(clear_sec.status(), StatusCode::OK);
    let cleared_json: Value = clear_sec.json().await.unwrap();
    assert_eq!(
        cleared_json["values"]["api_token"],
        json!({ "configured": false })
    );
}

#[tokio::test]
async fn s04_dev_mode_live_edit_guards_sync_backup_and_examples_cli() {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let workspace_cms_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    // 1. CLI offline checks on reference JS examples
    let theme_example_dir = workspace_cms_dir.join("examples/themes/theme-js-starter");
    let theme_check = Command::new(exe)
        .args(["--check-theme", theme_example_dir.to_str().unwrap()])
        .output()
        .expect("run --check-theme on theme-js-starter");
    assert!(
        theme_check.status.success(),
        "--check-theme failed: {}",
        String::from_utf8_lossy(&theme_check.stdout)
    );
    let theme_summary: Value = serde_json::from_slice(&theme_check.stdout).unwrap();
    assert_eq!(theme_summary["ok"], json!(true));
    assert_eq!(theme_summary["theme_id"], json!("theme-js-starter"));
    assert_eq!(theme_summary["has_functions"], json!(true));

    let plugin_example_dir = workspace_cms_dir.join("examples/plugins/plugin-js-example");
    let plugin_check = Command::new(exe)
        .args(["--check-plugin", plugin_example_dir.to_str().unwrap()])
        .output()
        .expect("run --check-plugin on plugin-js-example");
    assert!(
        plugin_check.status.success(),
        "--check-plugin failed: {}",
        String::from_utf8_lossy(&plugin_check.stdout)
    );
    let plugin_summary: Value = serde_json::from_slice(&plugin_check.stdout).unwrap();
    assert_eq!(plugin_summary["ok"], json!(true));
    assert_eq!(plugin_summary["plugin_id"], json!("plugin-js-example"));

    // 2. Prepare local Dev Theme & Dev Plugin directories (including .git/.DS_Store/.env in root)
    let tmp = TempDir::new("s04_dev_mode_e2e");
    let site_root = tmp.path.join("site");
    let dev_theme_dir = tmp.path.join("dev-live-theme");
    let dev_plugin_dir = tmp.path.join("dev-live-plugin");
    fs::create_dir_all(&site_root).unwrap();
    fs::create_dir_all(dev_theme_dir.join("assets")).unwrap();
    fs::create_dir_all(dev_theme_dir.join("blocks/promo")).unwrap();
    fs::create_dir_all(dev_theme_dir.join("patterns")).unwrap();
    fs::create_dir_all(dev_theme_dir.join(".git")).unwrap();
    fs::create_dir_all(&dev_plugin_dir).unwrap();

    // Ignored dev repo artifacts + secret .env that must NEVER be exposed
    fs::write(dev_theme_dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(dev_theme_dir.join(".DS_Store"), b"junk").unwrap();
    fs::write(dev_theme_dir.join(".env"), "DB_PASS=never-expose\n").unwrap();
    fs::write(
        dev_theme_dir.join("patterns/dev-hero.json"),
        json!({
            "schema_version": 1,
            "blocks": [{
                "id": "sec-pat-1",
                "block_id": "core/section",
                "children": [{
                    "id": "row-pat-1",
                    "block_id": "core/row",
                    "children": [{
                        "id": "col-pat-1",
                        "block_id": "core/column",
                        "children": []
                    }]
                }]
            }]
        })
        .to_string(),
    )
    .unwrap();

    let dev_theme_manifest = json!({
        "manifest_version": 1,
        "theme_id": "dev-live-theme",
        "name": "Dev Live Theme",
        "version": "0.1.0",
        "templates": [
            { "kind": "index", "file": "index.html" },
            { "kind": "page", "file": "page.html" }
        ],
        "patterns": [
            { "key": "dev_hero", "title": "Dev Hero Pattern", "category": "hero", "file": "patterns/dev-hero.json" }
        ],
        "options": [
            { "key": "dev_tagline", "label": "Dev Tagline", "type": "text", "default": "DevOptDefault", "public": true }
        ],
        "assets": ["assets/style.css"],
        "styles": [
            { "handle": "dev-main", "src": "assets/style.css", "conditions": ["all"] }
        ],
        "functions": {
            "min_host_version": "0.1.0",
            "filters": ["filter:render/slot"],
            "render_cache": "by_route"
        }
    });
    fs::write(
        dev_theme_dir.join("theme.json"),
        serde_json::to_string_pretty(&dev_theme_manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("index.html"),
        "<main><h1>INDEX-V1</h1>{{ od_hook(\"hero_banner\") }}</main>",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("page.html"),
        "<article><h1>PAGE-V1 {{ entry.title }}</h1><p class=\"opt\">{{ options.dev_tagline }}</p>{{ od_hook(\"hero_banner\") }}<div>{{ od_content() }}</div></article>",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("assets/style.css"),
        "body { color: #111111; } /* CSS-V1 */",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("blocks/promo/block.json"),
        json!({
            "schema_version": 1,
            "id": "dev-live-theme/promo",
            "title": "Dev Promo",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "key": "heading", "label": "Heading", "type": "text", "default": "Promo" }
            ]
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("blocks/promo/render.html"),
        "<div class=\"promo-v1\">BLOCK-V1: {{ heading }}</div>",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("functions.js"),
        r#"od.addFilter("filter:render/slot", (env) => env.payload && env.payload.slot === "hero_banner" ? "<p>FUNC-V1</p>" : "");"#,
    )
    .unwrap();

    let dev_plugin_manifest = json!({
        "id": "dev-live-plugin",
        "version": "0.1.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Dev Live Plugin",
        "entrypoint": "plugin.js",
        "filters": ["filter:render/footer"],
        "render_cache": "global",
        "permissions": {
            "render_trusted_html": true
        }
    });
    fs::write(
        dev_plugin_dir.join("manifest.json"),
        serde_json::to_string_pretty(&dev_plugin_manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        dev_plugin_dir.join("plugin.js"),
        r#"od.addFilter("filter:render/footer", () => "<footer>PLUGIN-V1</footer>");"#,
    )
    .unwrap();

    // 3. Startup Guard Tests: refuse startup in recovery_safe_mode and with trusted_public_origin
    let guard_site = tmp.path.join("guard-site");
    fs::create_dir_all(guard_site.join("config")).unwrap();
    fs::write(
        guard_site.join("config/site.json"),
        json!({
            "version": 1,
            "listen_ip": "127.0.0.1",
            "port": free_port(),
            "trusted_public_origin": null,
            "log_level": "info",
            "recovery_safe_mode": true
        })
        .to_string(),
    )
    .unwrap();
    let safe_guard_out = Command::new(exe)
        .args(["--dev-theme", dev_theme_dir.to_str().unwrap()])
        .env("OPENDASH_SITE_ROOT", &guard_site)
        .output()
        .expect("run guarded startup safe_mode");
    assert!(
        !safe_guard_out.status.success(),
        "Dev Mode must refuse startup when recovery_safe_mode=true"
    );

    fs::write(
        guard_site.join("config/site.json"),
        json!({
            "version": 1,
            "listen_ip": "127.0.0.1",
            "port": free_port(),
            "trusted_public_origin": "https://prod.example.com",
            "log_level": "info",
            "recovery_safe_mode": false
        })
        .to_string(),
    )
    .unwrap();
    let remote_guard_out = Command::new(exe)
        .args(["--dev-theme", dev_theme_dir.to_str().unwrap()])
        .env("OPENDASH_SITE_ROOT", &guard_site)
        .output()
        .expect("run guarded startup trusted_public_origin");
    assert!(
        !remote_guard_out.status.success(),
        "Dev Mode must refuse startup when trusted_public_origin is set without --allow-dev-remote"
    );

    // 4. Launch server with --dev-theme & --dev-plugin
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let _server = spawn_server_with_args(
        &site_root,
        port,
        &[
            "--dev-theme".to_owned(),
            dev_theme_dir.to_string_lossy().to_string(),
            "--dev-plugin".to_owned(),
            dev_plugin_dir.to_string_lossy().to_string(),
        ],
    )
    .await;
    let client = Client::new();

    // Verify /api/health advertises dev_mode, dev_theme, dev_plugins
    let health: Value = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["dev_mode"], json!(true));
    assert_eq!(health["dev_theme"], json!("dev-live-theme"));
    assert_eq!(health["dev_plugins"], json!(["dev-live-plugin"]));

    let (admin_cookie, admin_csrf) = setup_and_login(&client, &base_url).await;

    // Sync Dev Manifest into SQLite (`POST /api/admin/dev/sync`) so dev blocks & plugin metadata are synced
    let sync_resp = client
        .post(format!("{base_url}/api/admin/dev/sync"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(sync_resp.status(), StatusCode::OK);
    let sync_json: Value = sync_resp.json().await.unwrap();
    assert_eq!(sync_json["synced_theme"], json!("dev-live-theme"));
    assert_eq!(sync_json["synced_plugins"], json!(["dev-live-plugin"]));

    // Verify Dev Theme Pattern & Dev Theme Options APIs work in Dev Mode (F-09)
    let dev_pat_resp = client
        .get(format!(
            "{base_url}/api/admin/patterns/library/theme/dev_hero"
        ))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(dev_pat_resp.status(), StatusCode::OK);
    let dev_pat_json: Value = dev_pat_resp.json().await.unwrap();
    assert_eq!(dev_pat_json["key"], json!("dev_hero"));

    let dev_opts_resp = client
        .get(format!("{base_url}/api/themes/dev-live-theme/options"))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(dev_opts_resp.status(), StatusCode::OK);
    let dev_opts_json: Value = dev_opts_resp.json().await.unwrap();
    assert_eq!(
        dev_opts_json["values"]["dev_tagline"],
        json!("DevOptDefault")
    );

    // Verify sync_dev_plugin_in_db did NOT persist raw dev code bytes into SQLite plugins.wasm_blob (F-10)
    let db_opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(site_root.join("database/opendash.sqlite"))
        .read_only(true);
    let db_pool = sqlx::SqlitePool::connect_with(db_opts).await.unwrap();
    let (blob_len, active_rev): (i64, Option<i64>) = sqlx::query_as(
        "SELECT length(wasm_blob), active_revision_id FROM plugins WHERE id = 'dev-live-plugin'",
    )
    .fetch_one(&db_pool)
    .await
    .unwrap();
    assert_eq!(
        blob_len, 0,
        "dev-only plugin must not persist code bytes in SQLite"
    );
    assert_eq!(active_rev, None);
    db_pool.close().await;

    // Create & publish a page with a Builder document containing the dev block `dev-live-theme/promo`
    let create_page = client
        .post(format!("{base_url}/api/admin/content-types/page/entries"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Dev Page",
            "slug": "dev-page",
            "values": { "body": "<p>Body text</p>" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_page.status(), StatusCode::CREATED);
    let page_json: Value = create_page.json().await.unwrap();
    let page_id = page_json["id"].as_i64().unwrap();
    let page_ver = page_json["version"].as_i64().unwrap();

    // Save Builder AST referencing `dev-live-theme/promo`
    let save_builder = client
        .put(format!(
            "{base_url}/api/admin/entries/page/{page_id}/builder"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{page_ver}\""))
        .json(&json!({
            "schema_version": 1,
            "blocks": [{
                "id": "sec-1",
                "block_id": "core/section",
                "children": [{
                    "id": "row-1",
                    "block_id": "core/row",
                    "children": [{
                        "id": "col-1",
                        "block_id": "core/column",
                        "children": [{
                            "id": "el-1",
                            "block_id": "dev-live-theme/promo",
                            "props": { "heading": "HelloLive" }
                        }]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert!(
        save_builder.status().is_success(),
        "save builder failed: {} {}",
        save_builder.status(),
        save_builder.text().await.unwrap()
    );
    let page_ver_after_builder = save_builder.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let pub_page = client
        .post(format!(
            "{base_url}/api/admin/content-types/page/entries/{page_id}/publication"
        ))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{page_ver_after_builder}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_page.status(), StatusCode::OK);

    // 5. Initial public render — verify V1 across template, block, functions.js, plugin.js, and dev-* CSS asset
    let html_v1 = client
        .get(format!("{base_url}/dev-page"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html_v1.contains("PAGE-V1"),
        "missing V1 template: {html_v1}"
    );
    assert!(
        html_v1.contains("FUNC-V1"),
        "missing V1 functions.js hook: {html_v1}"
    );
    assert!(
        html_v1.contains("PLUGIN-V1"),
        "missing V1 plugin.js footer: {html_v1}"
    );
    assert!(
        html_v1.contains("BLOCK-V1: HelloLive"),
        "missing V1 block: {html_v1}"
    );
    let css_marker = "/theme/dev-";
    let css_start = html_v1.find(css_marker).expect("missing dev-* asset URL");
    let css_end = html_v1[css_start..].find('"').unwrap() + css_start;
    let css_url_v1 = &html_v1[css_start..css_end];
    assert!(css_url_v1.ends_with("/assets/style.css"));

    let css_resp_v1 = client
        .get(format!("{base_url}{css_url_v1}"))
        .send()
        .await
        .unwrap();
    assert_eq!(css_resp_v1.status(), StatusCode::OK);
    assert_eq!(css_resp_v1.headers()[header::CACHE_CONTROL], "no-store");
    let css_body_v1 = css_resp_v1.text().await.unwrap();
    assert!(css_body_v1.contains("CSS-V1"));

    // Hidden `.env` file must return 404 even via valid dev-* digest prefix
    let dev_prefix = css_url_v1.strip_suffix("/assets/style.css").unwrap();
    let env_probe = client
        .get(format!("{base_url}{dev_prefix}/.env"))
        .send()
        .await
        .unwrap();
    assert_eq!(env_probe.status(), StatusCode::NOT_FOUND);

    // 6. HOT-RELOAD: Edit template, CSS, block template, functions.js, and plugin.js on disk (NO restart!)
    tokio::time::sleep(Duration::from_millis(25)).await;
    fs::write(
        dev_theme_dir.join("page.html"),
        "<article><h1>PAGE-V2-HOT {{ entry.title }}</h1>{{ od_hook(\"hero_banner\") }}<div>{{ od_content() }}</div></article>",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("assets/style.css"),
        "body { color: #222222; background: #fff; } /* CSS-V2-HOT */",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("blocks/promo/render.html"),
        "<div class=\"promo-v2\">BLOCK-V2-HOT: {{ heading }}</div>",
    )
    .unwrap();
    fs::write(
        dev_theme_dir.join("functions.js"),
        r#"od.addFilter("filter:render/slot", (env) => env.payload && env.payload.slot === "hero_banner" ? "<p>FUNC-V2-HOT</p>" : "");"#,
    )
    .unwrap();
    fs::write(
        dev_plugin_dir.join("plugin.js"),
        r#"od.addFilter("filter:render/footer", () => "<footer>PLUGIN-V2-HOT</footer>");"#,
    )
    .unwrap();

    let html_v2 = client
        .get(format!("{base_url}/dev-page"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html_v2.contains("PAGE-V2-HOT"),
        "hot template failed: {html_v2}"
    );
    assert!(
        html_v2.contains("FUNC-V2-HOT"),
        "hot functions.js failed: {html_v2}"
    );
    assert!(
        html_v2.contains("PLUGIN-V2-HOT"),
        "hot plugin.js failed: {html_v2}"
    );
    assert!(
        html_v2.contains("BLOCK-V2-HOT: HelloLive"),
        "hot block failed: {html_v2}"
    );

    let css_start2 = html_v2
        .find(css_marker)
        .expect("missing dev-* asset URL v2");
    let css_end2 = html_v2[css_start2..].find('"').unwrap() + css_start2;
    let css_url_v2 = &html_v2[css_start2..css_end2];
    assert_ne!(
        css_url_v1, css_url_v2,
        "dev digest should change after edits"
    );
    let css_resp_v2 = client
        .get(format!("{base_url}{css_url_v2}"))
        .send()
        .await
        .unwrap();
    assert_eq!(css_resp_v2.status(), StatusCode::OK);
    assert_eq!(css_resp_v2.headers()[header::CACHE_CONTROL], "no-store");
    assert!(css_resp_v2.text().await.unwrap().contains("CSS-V2-HOT"));

    // 7. Backup while Dev Mode is active: clean committed snapshot + dev_mode_active: true in metadata
    let bkp_resp = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Origin", &base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "passphrase": "StrongDevBackupPassphrase2026!",
            "confirm_passphrase": "StrongDevBackupPassphrase2026!",
            "reason": "dev-mode-warning-check"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bkp_resp.status(), StatusCode::ACCEPTED);
    let bkp_id = bkp_resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bkp_done = wait_job_terminal(&client, &base_url, &admin_cookie, &bkp_id).await;
    assert_eq!(
        bkp_done["state"],
        json!("succeeded"),
        "dev backup failed: {bkp_done}"
    );
    let summary: Value = serde_json::from_str(bkp_done["summary_json"].as_str().unwrap()).unwrap();
    assert_eq!(summary["dev_mode_active"], json!(true));
}
