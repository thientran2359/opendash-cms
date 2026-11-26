//! Slice S02 plugin lifecycle parity, host confirmation protocol, trusted commit hooks,
//! HTTP exclusion (manifest + defense-in-depth host denial), maintenance epoch invalidation,
//! session/role/plugin revocation, and old-host `0.1.0` rejection of `0.1.1` manifests.

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use zip::{ZipWriter, write::SimpleFileOptions};

struct Site {
    root: PathBuf,
    child: Child,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    async fn start_binary(prefix: &str, binary: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("opendash-{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let log_path = root.join("server.log");
        let log = fs::File::create(&log_path).unwrap();
        let child = Command::new(binary)
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_HOST", "127.0.0.1")
            .env("OPENDASH_PORT", port.to_string())
            .env("OPENDASH_PLUGIN_ALLOW_LOOPBACK", "1")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..200 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self { root, child, base };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "server failed to become healthy; log={}",
            fs::read_to_string(&log_path).unwrap_or_default()
        );
    }

    async fn start(prefix: &str) -> Self {
        Self::start_binary(prefix, Path::new(env!("CARGO_BIN_EXE_opendash"))).await
    }

    async fn ro_db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(self.root.join("database/opendash.sqlite"))
                .read_only(true),
        )
        .await
        .unwrap()
    }
}

fn wat_escape(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect()
}

fn build_plugin_zip(manifest: &Value, wasm: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(wasm).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

/// Calls a single fixed host_call request and writes the raw host response as plugin output.
fn host_call_echo_wasm(req_json: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 1024) "{}")
            (func (export "opendash_run") (result i32)
                (local $n i32)
                (local.set $n (call $call (i32.const 1024) (i32.const {}) (i32.const 8192) (i32.const 16384)))
                (if (i32.lt_s (local.get $n) (i32.const 1)) (then (return (i32.const 1))))
                (drop (call $out (i32.const 8192) (local.get $n)))
                (i32.const 0)
            )
        )"#,
        wat_escape(req_json),
        req_json.len()
    ))
    .unwrap()
}

/// Executes 2 host calls in sequence (ignoring whether call #1 succeeded), writes output from call #2.
fn two_host_calls_wasm(req1_json: &str, req2_json: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 1024) "{}")
            (data (i32.const 4096) "{}")
            (func (export "opendash_run") (result i32)
                (local $n i32)
                (drop (call $call (i32.const 1024) (i32.const {}) (i32.const 8192) (i32.const 4096)))
                (local.set $n (call $call (i32.const 4096) (i32.const {}) (i32.const 16384) (i32.const 4096)))
                (if (i32.lt_s (local.get $n) (i32.const 1)) (then (return (i32.const 1))))
                (drop (call $out (i32.const 16384) (local.get $n)))
                (i32.const 0)
            )
        )"#,
        wat_escape(req1_json),
        wat_escape(req2_json),
        req1_json.len(),
        req2_json.len()
    ))
    .unwrap()
}

fn static_output_wasm(out_json: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{}")
            (func (export "opendash_run") (result i32)
                (drop (call $out (i32.const 1024) (i32.const {})))
                (i32.const 0)
            )
        )"#,
        wat_escape(out_json),
        out_json.len()
    ))
    .unwrap()
}

fn kv_set_wasm(key: &str, value: &str) -> Vec<u8> {
    let req = json!({ "op": "kv_set", "key": key, "value": value }).to_string();
    host_call_echo_wasm(&req)
}

async fn setup_admin(client: &Client, base: &str) -> (String, String) {
    assert_eq!(
        client
            .post(format!("{base}/auth/setup"))
            .header("Origin", base)
            .json(&json!({"username":"admin","password":"AdminPassword123!"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    login(client, base, "admin", "AdminPassword123!").await
}

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .header("Origin", base)
        .json(&json!({"username":username,"password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|h| h.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn install_plugin(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    manifest: Value,
    wasm: &[u8],
) -> (StatusCode, String) {
    let resp = client
        .post(format!("{base}/api/admin/plugins"))
        .header(header::COOKIE, cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("Content-Type", "application/zip")
        .body(build_plugin_zip(&manifest, wasm))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    (status, text)
}

async fn wait_for_kv(db: &SqlitePool, plugin_id: &str, key: &str, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        let row: Option<String> =
            sqlx::query_scalar("SELECT value_json FROM plugin_kv WHERE plugin_id = ? AND key = ?")
                .bind(plugin_id)
                .bind(key)
                .fetch_optional(db)
                .await
                .unwrap_or(None);
        if row.as_deref() == Some(expected) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    panic!("timed out waiting for plugin_kv {plugin_id}/{key} == {expected}");
}

#[tokio::test]
async fn s02_plugin_trash_read_restore_purge_confirmation_and_security_fences() {
    let site = Site::start("s02-plugin-life").await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &site.base).await;

    // Create a second Admin and an Editor for session/role boundary checks
    for (u, role) in [("admin2", "admin"), ("editor1", "editor")] {
        assert_eq!(
            client
                .post(format!("{}/api/admin/users", site.base))
                .header(header::COOKIE, &admin_cookie)
                .header("Origin", &site.base)
                .header("X-CSRF-Token", &admin_csrf)
                .json(&json!({"username":u,"password":"UserPassword123!","role":role}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::CREATED
        );
    }
    let (admin2_cookie, admin2_csrf) =
        login(&client, &site.base, "admin2", "UserPassword123!").await;
    let (editor_cookie, editor_csrf) =
        login(&client, &site.base, "editor1", "UserPassword123!").await;

    // Install async hook observers (after_delete, after_publish)
    for (pid, event, key) in [
        ("obs-del", "content/after_delete", "del_fired"),
        ("obs-pub", "content/after_publish", "pub_fired"),
    ] {
        let (st, txt) = install_plugin(
            &client,
            &site.base,
            &admin_cookie,
            &admin_csrf,
            json!({
                "id": pid,
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 3,
                "min_host_version": "0.1.0",
                "name": pid,
                "hooks": [event],
                "permissions": { "content_read": ["post", "item"], "sqlite": ["kv_write"] }
            }),
            &kv_set_wasm(key, "1"),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{txt}");
    }

    // Create a post and an immediate CPT `item`
    let post_id = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"plug-post","title":"Plug Post","values":{"body":"secret body"}}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    assert_eq!(
        client
            .post(format!("{}/api/admin/content-types", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "key": "item",
                "label": "Item",
                "fields": [],
                "settings": {"public": true, "has_archive": false, "publication": "immediate", "supports": ["editor"]}
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    let item_id = client
        .post(format!(
            "{}/api/admin/content-types/item/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"plug-item","title":"Plug Item","values":{}}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    // 1. Forged guest JSON must NEVER trigger hooks or emit host confirmation
    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "forger",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": "Forger",
            "actions": [{ "id": "forge", "title": "Forge", "required_role": "admin", "capabilities": [] }]
        }),
        &static_output_wasm(
            r#"{"deleted_entry_id":999,"deleted_type_key":"post","entry":{"id":999,"publication_status":"published"},"confirmation":{"confirmation_id":"fake","operation":"purge"}}"#,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{txt}");
    let forge_resp = client
        .post(format!(
            "{}/api/admin/plugins/forger/actions/forge",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(forge_resp.status(), StatusCode::OK);
    let forge_json: Value = forge_resp.json().await.unwrap();
    assert!(forge_json.get("confirmation").is_none());
    tokio::time::sleep(Duration::from_millis(200)).await;
    let ro_db = site.ro_db().await;
    let forged_del: Option<String> = sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE plugin_id = 'obs-del' AND key = 'del_fired'",
    )
    .fetch_optional(&ro_db)
    .await
    .unwrap();
    assert!(
        forged_del.is_none(),
        "forged JSON must not fire after_delete"
    );

    // 2. Trash post via `content_delete` action (Editor allowed), verify retained in Trash + NO after_delete
    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "p-trash",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.1",
            "name": "P Trash",
            "permissions": {
                "content_delete": ["post", "item"],
                "content_trash_read": ["post", "item"]
            },
            "actions": [
                { "id": "trash_post", "title": "Trash Post", "required_role": "editor", "capabilities": ["content_delete"] },
                { "id": "read_trash", "title": "Read Trash", "required_role": "editor", "capabilities": ["content_trash_read"] }
            ]
        }),
        &host_call_echo_wasm(
            &json!({"op":"content_delete","type_key":"post","entry_id":post_id,"expected_version":1})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{txt}");
    let tr_resp = client
        .post(format!(
            "{}/api/admin/plugins/p-trash/actions/trash_post",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(tr_resp.status(), StatusCode::OK);
    let tr_json: Value = tr_resp.json().await.unwrap();
    assert_eq!(tr_json["data"]["lifecycle"]["operation"], json!("trash"));
    assert_eq!(tr_json["data"]["deleted_entry_id"], json!(post_id));

    // 3. Read Trash metadata via `content_trash_get` and `content_trash_list` (metadata-only, no body/values)
    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "p-tread",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.1",
            "name": "P Trash Read",
            "permissions": { "content_trash_read": ["post"] },
            "actions": [
                { "id": "get_one", "title": "Get One", "required_role": "editor", "capabilities": ["content_trash_read"] },
                { "id": "no_cap", "title": "No Cap", "required_role": "editor", "capabilities": [] }
            ]
        }),
        &host_call_echo_wasm(
            &json!({"op":"content_trash_get","type_key":"post","entry_id":post_id}).to_string(),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{txt}");
    let got_trash = client
        .post(format!(
            "{}/api/admin/plugins/p-tread/actions/get_one",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(got_trash.status(), StatusCode::OK);
    let got_json: Value = got_trash.json().await.unwrap();
    let entry_meta = &got_json["data"]["data"]["entry"];
    assert_eq!(entry_meta["id"], json!(post_id));
    assert_eq!(entry_meta["slug"], json!("plug-post"));
    assert!(entry_meta.get("values").is_none(), "must be metadata-only");
    assert!(entry_meta.get("public_url").is_none());

    // Action missing `content_trash_read` capability is denied, and `/run` is denied
    let no_cap_res: Value = client
        .post(format!(
            "{}/api/admin/plugins/p-tread/actions/no_cap",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(no_cap_res["data"]["ok"], json!(false));

    let run_res: Value = client
        .post(format!("{}/api/admin/plugins/p-tread/run", site.base))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(run_res["data"]["ok"], json!(false));

    // 4. Draft-publish restore via plugin commits directly (200) to draft;
    // immediate CPT restore requires Host Confirmation (409 -> cancel -> re-start -> confirm -> 200 + after_publish)!
    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "p-restore-post",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.1",
            "name": "P Restore Post",
            "permissions": { "content_restore": ["post"] },
            "actions": [
                { "id": "restore", "title": "Restore", "required_role": "editor", "capabilities": ["content_restore"] }
            ]
        }),
        &host_call_echo_wasm(
            &json!({"op":"content_restore","type_key":"post","entry_id":post_id,"expected_version":2})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{txt}");
    let r_post = client
        .post(format!(
            "{}/api/admin/plugins/p-restore-post/actions/restore",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(r_post.status(), StatusCode::OK);
    let r_post_json: Value = r_post.json().await.unwrap();
    assert_eq!(
        r_post_json["data"]["lifecycle"]["operation"],
        json!("restore")
    );
    assert_eq!(r_post_json["data"]["lifecycle"]["version"], json!(3));

    // Trash immediate `item` via HTTP, then restore via plugin -> 409 confirmation_required
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/item/entries/{item_id}/trash",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "p-restore-item",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.1",
            "name": "P Restore Item",
            "permissions": { "content_restore": ["item"] },
            "actions": [
                { "id": "restore", "title": "Restore", "required_role": "editor", "capabilities": ["content_restore"] }
            ]
        }),
        &host_call_echo_wasm(
            &json!({"op":"content_restore","type_key":"item","entry_id":item_id,"expected_version":2})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{txt}");

    let pending_restore = client
        .post(format!(
            "{}/api/admin/plugins/p-restore-item/actions/restore",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pending_restore.status(), StatusCode::CONFLICT);
    assert_eq!(
        pending_restore
            .headers()
            .get("X-OpenDash-Error-Code")
            .unwrap(),
        "confirmation_required"
    );
    let pr_json: Value = pending_restore.json().await.unwrap();
    assert_eq!(pr_json["code"], json!("confirmation_required"));
    assert_eq!(pr_json["confirmation"]["operation"], json!("restore"));
    let cid1 = pr_json["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Cancel confirmation -> 200 confirmation_cancelled, entry remains trashed
    let cancel_res = client
        .post(format!(
            "{}/api/admin/plugins/p-restore-item/actions/restore/confirm-lifecycle",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({"confirmation_id": cid1, "decision": "cancel"}))
        .send()
        .await
        .unwrap();
    assert_eq!(cancel_res.status(), StatusCode::OK);
    assert_eq!(
        cancel_res.json::<Value>().await.unwrap()["code"],
        json!("confirmation_cancelled")
    );

    // Re-invoke and confirm -> 200 restored + after_publish hook fires!
    let pr2: Value = client
        .post(format!(
            "{}/api/admin/plugins/p-restore-item/actions/restore",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid2 = pr2["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let confirm_res = client
        .post(format!(
            "{}/api/admin/plugins/p-restore-item/actions/restore/confirm-lifecycle",
            site.base
        ))
        .header(header::COOKIE, &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({"confirmation_id": cid2, "decision": "confirm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(confirm_res.status(), StatusCode::OK);
    wait_for_kv(&ro_db, "obs-pub", "pub_fired", "\"1\"").await;

    // 5. Plugin Purge (Admin-only, one-use confirmation, cross-session & disable & replay fences)
    // Trash post again (now v3 -> v4)
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{post_id}/trash",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v3\"")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "p-purge",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.1",
            "name": "P Purge",
            "permissions": { "content_purge": ["post"] },
            "actions": [
                { "id": "purge", "title": "Purge", "required_role": "admin", "capabilities": ["content_purge"] }
            ]
        }),
        &host_call_echo_wasm(
            &json!({"op":"content_purge","type_key":"post","entry_id":post_id,"expected_version":4})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{txt}");

    // Editor gets 403
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/plugins/p-purge/actions/purge",
                site.base
            ))
            .header(header::COOKIE, &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // Admin2 starts purge confirmation, then logs out -> confirmation must fail (401)
    let p_adm2: Value = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge",
            site.base
        ))
        .header(header::COOKIE, &admin2_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin2_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid_adm2 = p_adm2["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Admin1 cannot steal Admin2's confirmation_id -> 409 confirmation_invalid with NO metadata leak
    let stolen = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge/confirm-lifecycle",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"confirmation_id": cid_adm2, "decision": "confirm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(stolen.status(), StatusCode::CONFLICT);
    let stolen_txt = stolen.text().await.unwrap();
    assert!(
        !stolen_txt.contains("plug-post"),
        "must not disclose target"
    );

    // Admin2 logs out -> using old cookie fails 401
    assert_eq!(
        client
            .post(format!("{}/auth/logout", site.base))
            .header(header::COOKIE, &admin2_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin2_csrf)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/plugins/p-purge/actions/purge/confirm-lifecycle",
                site.base
            ))
            .header(header::COOKIE, &admin2_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin2_csrf)
            .json(&json!({"confirmation_id": cid_adm2, "decision": "confirm"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );

    // Admin1 starts purge confirmation, then disables plugin -> confirm rejected!
    let p_dis: Value = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid_dis = p_dis["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        client
            .post(format!("{}/api/admin/plugins/p-purge/disable", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let after_disable = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge/confirm-lifecycle",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"confirmation_id": cid_dis, "decision": "confirm"}))
        .send()
        .await
        .unwrap();
    assert_ne!(after_disable.status(), StatusCode::OK);

    // Re-enable plugin -> fresh start + confirm succeeds, deletes row, fires after_delete once, replay fails!
    assert_eq!(
        client
            .post(format!("{}/api/admin/plugins/p-purge/enable", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let p_final: Value = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cid_final = p_final["confirmation"]["confirmation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let confirmed_purge = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge/confirm-lifecycle",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"confirmation_id": cid_final, "decision": "confirm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(confirmed_purge.status(), StatusCode::OK);
    let cp_json: Value = confirmed_purge.json().await.unwrap();
    assert_eq!(cp_json["data"]["lifecycle"]["operation"], json!("purge"));
    assert_eq!(cp_json["data"]["lifecycle"]["version"], json!(4));

    // Replay of same confirmation_id -> 409 confirmation_invalid
    let replay = client
        .post(format!(
            "{}/api/admin/plugins/p-purge/actions/purge/confirm-lifecycle",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"confirmation_id": cid_final, "decision": "confirm"}))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CONFLICT);
    assert_eq!(
        replay.json::<Value>().await.unwrap()["code"],
        json!("confirmation_invalid")
    );

    // Physical deletion + after_delete hook fired
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM content_entries WHERE id = ?")
        .bind(post_id)
        .fetch_one(&ro_db)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    wait_for_kv(&ro_db, "obs-del", "del_fired", "\"1\"").await;
}

#[tokio::test]
async fn s02_plugin_http_exclusion_v011_manifest_gate_and_old_host_rejection() {
    let site = Site::start("s02-plugin-gates").await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_admin(&client, &site.base).await;

    // 1. Manifest combining `http` + `content_purge` or `content_restore` rejected at install
    for cap in ["content_purge", "content_restore"] {
        let (st, txt) = install_plugin(
            &client,
            &site.base,
            &admin_cookie,
            &admin_csrf,
            json!({
                "id": format!("bad-http-{cap}"),
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 3,
                "min_host_version": "0.1.1",
                "name": "Bad HTTP Mix",
                "permissions": {
                    cap: ["post"],
                    "http": ["https://example.com/*"]
                },
                "actions": [{
                    "id": "mix",
                    "title": "Mix",
                    "required_role": "admin",
                    "capabilities": [cap, "http"]
                }]
            }),
            &static_output_wasm("{}"),
        )
        .await;
        assert_eq!(
            st,
            StatusCode::BAD_REQUEST,
            "http + {cap} must be rejected: {txt}"
        );
    }

    // 2. Manifest requesting new grants with min_host_version 0.1.0 rejected
    let (st, txt) = install_plugin(
        &client,
        &site.base,
        &admin_cookie,
        &admin_csrf,
        json!({
            "id": "bad-min-ver",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": "Bad Min Ver",
            "permissions": { "content_trash_read": ["post"] },
            "actions": [{ "id": "r", "title": "R", "required_role": "admin", "capabilities": ["content_trash_read"] }]
        }),
        &static_output_wasm("{}"),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "0.1.0 must fail: {txt}");

    // 3. Defense-in-depth host HTTP denial in a content_purge action (before & after staging):
    // local TCP listener must receive ZERO outbound connections!
    let trap_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let trap_port = trap_listener.local_addr().unwrap().port();
    trap_listener.set_nonblocking(true).unwrap();
    let hit_count = Arc::new(AtomicUsize::new(0));
    let hit_bg = Arc::clone(&hit_count);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_bg = Arc::clone(&stop);
    let listener_thread = std::thread::spawn(move || {
        while !stop_bg.load(Ordering::Relaxed) {
            if trap_listener.accept().is_ok() {
                hit_bg.fetch_add(1, Ordering::SeqCst);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    // Create & trash a post so staging succeeds after the denied HTTP call
    let pid = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"slug":"http-guard","title":"Http Guard","values":{}}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{pid}/trash",
                site.base
            ))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let http_req = json!({
        "op": "http_request",
        "method": "GET",
        "url": format!("http://127.0.0.1:{trap_port}/exfil")
    })
    .to_string();
    let purge_req = json!({
        "op": "content_purge",
        "type_key": "post",
        "entry_id": pid,
        "expected_version": 2
    })
    .to_string();

    for (name, wasm) in [
        (
            "http-before-stage",
            two_host_calls_wasm(&http_req, &purge_req),
        ),
        (
            "http-after-stage",
            two_host_calls_wasm(&purge_req, &http_req),
        ),
    ] {
        let (st, txt) = install_plugin(
            &client,
            &site.base,
            &admin_cookie,
            &admin_csrf,
            json!({
                "id": name,
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 3,
                "min_host_version": "0.1.1",
                "name": name,
                "permissions": {
                    "content_purge": ["post"],
                    "http": [format!("http://127.0.0.1:{trap_port}/*")]
                },
                "actions": [{
                    "id": "go",
                    "title": "Go",
                    "required_role": "admin",
                    "capabilities": ["content_purge"]
                }]
            }),
            &wasm,
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{txt}");
        let _ = client
            .post(format!("{}/api/admin/plugins/{name}/actions/go", site.base))
            .header(header::COOKIE, &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    let _ = listener_thread.join();
    assert_eq!(
        hit_count.load(Ordering::SeqCst),
        0,
        "HTTP requests in content_purge action must be blocked before network I/O"
    );

    // 4. Real old host `0.1.0` rejects valid `0.1.1` manifest when OPENDASH_BASELINE_BINARY is provided
    if let Ok(baseline_bin) = std::env::var("OPENDASH_BASELINE_BINARY") {
        let baseline_path = PathBuf::from(baseline_bin);
        assert!(baseline_path.exists(), "OPENDASH_BASELINE_BINARY missing");
        let old_site = Site::start_binary("s02-old-host", &baseline_path).await;
        let (old_cookie, old_csrf) = setup_admin(&client, &old_site.base).await;
        let valid_v011 = json!({
            "id": "needs-v011",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.1",
            "name": "Needs 0.1.1",
            "permissions": { "content_trash_read": ["post"] },
            "actions": [{ "id": "r", "title": "R", "required_role": "admin", "capabilities": ["content_trash_read"] }]
        });
        let (old_status, old_txt) = install_plugin(
            &client,
            &old_site.base,
            &old_cookie,
            &old_csrf,
            valid_v011,
            &static_output_wasm("{}"),
        )
        .await;
        assert!(
            old_status == StatusCode::BAD_REQUEST || old_status == StatusCode::UNPROCESSABLE_ENTITY,
            "old 0.1.0 host must reject min_host_version 0.1.1; got {old_status}: {old_txt}"
        );
    }
}
