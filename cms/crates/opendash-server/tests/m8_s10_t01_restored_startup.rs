//! M8-S10-T01: native Admin backup/restore with source Safe Mode disabled.
//! Always use a fresh loopback-only root and the actual opendash executable.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
const ADMIN_PASSWORD: &str = "local-restore-admin-pass-12345";
const EDITOR_PASSWORD: &str = "local-restore-editor-pass-12345";

struct IsolatedRoot(PathBuf);

impl IsolatedRoot {
    fn new() -> Self {
        // macOS std::env::temp_dir() may be /var; canonicalize to /private/var,
        // because the production path guard correctly rejects symlink ancestors.
        let temp = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = temp.join(format!(
            "opendash-m8-s10-t01-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("new isolated root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for IsolatedRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Server {
    child: Child,
    url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_opendash")
}

fn sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn free_port() -> u16 {
    let socket = TcpListener::bind("127.0.0.1:0").expect("free port");
    socket.local_addr().unwrap().port()
}

async fn start(root: &Path) -> Server {
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let child = Command::new(bin())
        .env("OPENDASH_SITE_ROOT", root)
        .env("OPENDASH_DB_PATH", root.join("database/opendash.sqlite"))
        .env("OPENDASH_PORT", port.to_string())
        .env_remove("OPENDASH_PUBLIC_ORIGIN")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start opendash executable");
    let client = Client::new();
    for _ in 0..100 {
        if let Ok(response) = client.get(format!("{url}/api/health")).send().await
            && response.status() == StatusCode::OK
        {
            return Server { child, url };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("fresh isolated server did not start on {url}");
}

async fn login(client: &Client, url: &str, username: &str, password: &str) -> (String, String) {
    let response = client
        .post(format!("{url}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login request");
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find_map(|h| h.split(';').next().map(str::to_owned))
        .expect("login cookie");
    let csrf = response.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .expect("csrf")
        .to_owned();
    (cookie, csrf)
}

async fn job(client: &Client, url: &str, cookie: &str, id: &str) -> Value {
    for _ in 0..240 {
        let response = client
            .get(format!("{url}/api/admin/operations/jobs/{id}"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .expect("poll job");
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = response.json().await.unwrap();
        if matches!(
            value["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("restore or backup job failed to complete");
}

fn on_disk_safe_mode(root: &Path) -> bool {
    let config: Value = serde_json::from_slice(
        &fs::read(root.join("config/site.json")).expect("restored config exists"),
    )
    .expect("valid restored config");
    config["recovery_safe_mode"]
        .as_bool()
        .expect("safe-mode flag")
}

async fn health_safe(client: &Client, url: &str, expected: bool) {
    let response: Value = client
        .get(format!("{url}/api/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["safe_mode"], expected);
}

async fn public_noindex(client: &Client, url: &str, path: &str, expected: bool) {
    let response = client.get(format!("{url}{path}")).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "public {path}");
    assert_eq!(
        response
            .headers()
            .get("x-robots-tag")
            .and_then(|h| h.to_str().ok()),
        expected.then_some("noindex, nofollow"),
        "public {path} index policy"
    );
}

#[tokio::test]
async fn native_false_config_restore_requires_activation_and_survives_restarts() {
    let isolated = IsolatedRoot::new();
    let source = isolated.path().join("source");
    fs::create_dir_all(source.join("config")).unwrap();
    fs::create_dir_all(source.join("database")).unwrap();
    let source_config = serde_json::to_vec_pretty(&json!({
        "version": 1, "listen_ip": "127.0.0.1", "port": 18766,
        "trusted_public_origin": null, "log_level": "info", "recovery_safe_mode": false
    }))
    .unwrap();
    fs::write(source.join("config/site.json"), &source_config).unwrap();

    let server = start(&source).await;
    let client = Client::new();
    let setup = client
        .post(format!("{}/auth/setup", server.url))
        .json(&json!({ "username": "admin", "password": ADMIN_PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (cookie, csrf) = login(&client, &server.url, "admin", ADMIN_PASSWORD).await;
    health_safe(&client, &server.url, false).await;

    let editor = client
        .post(format!("{}/api/admin/users", server.url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "username": "editor", "password": EDITOR_PASSWORD, "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(editor.status(), StatusCode::CREATED);

    let post = client
        .post(format!("{}/api/admin/content-types/post/entries", server.url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "title": "Restorable Public Post", "slug": "restore-safe-post", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(post.status(), StatusCode::CREATED);
    let post_id = post.json::<Value>().await.unwrap()["id"].as_i64().unwrap();
    let published = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            server.url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::OK);
    public_noindex(
        &client,
        &server.url,
        "/content/post/restore-safe-post",
        false,
    )
    .await;

    let form = client
        .post(format!("{}/api/admin/contact/forms", server.url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "restore-contact", "title": "Restore contact", "status": "active",
            "schema": { "version": 1, "fields": [
                { "key": "name", "label": "Name", "kind": "text", "required": true }
            ] }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(form.status(), StatusCode::CREATED);
    let form_id = form.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    let passphrase = "local-only-restore-passphrase-123456";
    let backup_response = client
        .post(format!("{}/api/admin/operations/backups", server.url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "passphrase": passphrase, "confirm_passphrase": passphrase, "reason": "M8-S10-T01 safe mode" }))
        .send()
        .await
        .unwrap();
    assert_eq!(backup_response.status(), StatusCode::ACCEPTED);
    let backup_id = backup_response.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let completed_backup = job(&client, &server.url, &cookie, &backup_id).await;
    assert_eq!(completed_backup["state"], "succeeded", "{completed_backup}");
    let archive_response = client
        .get(format!(
            "{}/api/admin/operations/backups/{backup_id}/file",
            server.url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(archive_response.status(), StatusCode::OK);
    let archive = archive_response.bytes().await.unwrap();
    let archive_hash = sha256(&archive);
    let upload_response = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            server.url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(archive)
        .send()
        .await
        .unwrap();
    assert_eq!(upload_response.status(), StatusCode::CREATED);
    let upload_id = upload_response.json::<Value>().await.unwrap()["upload_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let restore_response = client
        .post(format!("{}/api/admin/operations/restores", server.url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "upload_id": upload_id, "passphrase": passphrase, "reason": "M8-S10-T01 restore" }))
        .send()
        .await
        .unwrap();
    assert_eq!(restore_response.status(), StatusCode::ACCEPTED);
    let restore_id = restore_response.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let completed_restore = job(&client, &server.url, &cookie, &restore_id).await;
    assert_eq!(
        completed_restore["state"], "succeeded",
        "{completed_restore}"
    );
    let receipt: Value =
        serde_json::from_str(completed_restore["summary_json"].as_str().unwrap()).unwrap();
    let restored = PathBuf::from(receipt["target_root"].as_str().unwrap());
    assert!(restored.starts_with(&source));
    assert_eq!(
        fs::read(source.join("config/site.json")).unwrap(),
        source_config
    );
    assert!(!source.join("READY.json").exists());
    assert!(
        on_disk_safe_mode(&restored),
        "restored config must override copied source false"
    );
    let ready: Value =
        serde_json::from_slice(&fs::read(restored.join("READY.json")).unwrap()).unwrap();
    assert_eq!(ready["recovery_safe_mode"], true);
    assert_eq!(ready["job_id"], restore_id);
    assert_eq!(
        ready["db_sha256"],
        sha256(&fs::read(restored.join("database/opendash.sqlite")).unwrap())
    );

    // No source Admin cookie is valid in a sanitized restored database.
    let restore_server = start(&restored).await;
    health_safe(&client, &restore_server.url, true).await;
    public_noindex(&client, &restore_server.url, "/", true).await;
    public_noindex(
        &client,
        &restore_server.url,
        "/content/post/restore-safe-post",
        true,
    )
    .await;
    let contact = client
        .post(format!(
            "{}/api/public/contact/{form_id}",
            restore_server.url
        ))
        .header(header::ORIGIN, &restore_server.url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("form_version=1&nonce=abc&name=Visitor")
        .send()
        .await
        .unwrap();
    assert_eq!(contact.status(), StatusCode::SERVICE_UNAVAILABLE);
    let old_session = client
        .get(format!(
            "{}/api/admin/operations/recovery/status",
            restore_server.url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(old_session.status(), StatusCode::UNAUTHORIZED);

    // Restart before activation: both the runtime flag and noindex persist.
    drop(restore_server);
    let restore_server = start(&restored).await;
    health_safe(&client, &restore_server.url, true).await;
    public_noindex(&client, &restore_server.url, "/", true).await;
    assert!(on_disk_safe_mode(&restored));
    let (fresh_cookie, fresh_csrf) =
        login(&client, &restore_server.url, "admin", ADMIN_PASSWORD).await;
    let (editor_cookie, editor_csrf) =
        login(&client, &restore_server.url, "editor", EDITOR_PASSWORD).await;
    let status = client
        .get(format!(
            "{}/api/admin/operations/recovery/status",
            restore_server.url
        ))
        .header(header::COOKIE, &fresh_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let state = status.json::<Value>().await.unwrap();
    assert_eq!(state["safe_mode"], true);
    assert_eq!(state["smtp_enabled"], false);
    let generation = state["expected_generation"].as_str().unwrap();

    // The Safe Mode guard must run before plugin lookup/dispatch. No plugin
    // execution or plugin-provided action/egress can begin even for an unknown id.
    for path in [
        "/api/admin/plugins/safe-mode-probe/run",
        "/api/admin/plugins/safe-mode-probe/actions/egress-probe",
    ] {
        let denied = client
            .post(format!("{}{path}", restore_server.url))
            .header(header::COOKIE, &fresh_cookie)
            .header("X-CSRF-Token", &fresh_csrf)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(denied.json::<Value>().await.unwrap()["code"], "safe_mode");
    }

    let activate_url = format!(
        "{}/api/admin/operations/recovery/activate",
        restore_server.url
    );
    let editor_denied = client
        .post(&activate_url)
        .header(header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({ "password": EDITOR_PASSWORD, "expected_generation": generation }))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_denied.status(), StatusCode::FORBIDDEN);
    let csrf_denied = client
        .post(&activate_url)
        .header(header::COOKIE, &fresh_cookie)
        .json(&json!({ "password": ADMIN_PASSWORD, "expected_generation": generation }))
        .send()
        .await
        .unwrap();
    assert_eq!(csrf_denied.status(), StatusCode::FORBIDDEN);
    let origin_denied = client
        .post(&activate_url)
        .header(header::COOKIE, &fresh_cookie)
        .header("X-CSRF-Token", &fresh_csrf)
        .header(header::ORIGIN, "https://evil.example")
        .json(&json!({ "password": ADMIN_PASSWORD, "expected_generation": generation }))
        .send()
        .await
        .unwrap();
    assert_eq!(origin_denied.status(), StatusCode::FORBIDDEN);
    let password_denied = client
        .post(&activate_url)
        .header(header::COOKIE, &fresh_cookie)
        .header("X-CSRF-Token", &fresh_csrf)
        .json(&json!({ "password": "wrong", "expected_generation": generation }))
        .send()
        .await
        .unwrap();
    assert_eq!(password_denied.status(), StatusCode::UNAUTHORIZED);
    let generation_denied = client
        .post(&activate_url)
        .header(header::COOKIE, &fresh_cookie)
        .header("X-CSRF-Token", &fresh_csrf)
        .json(&json!({ "password": ADMIN_PASSWORD, "expected_generation": "bad-generation" }))
        .send()
        .await
        .unwrap();
    assert_eq!(generation_denied.status(), StatusCode::PRECONDITION_FAILED);
    assert!(
        on_disk_safe_mode(&restored),
        "invalid activations must not mutate durable state"
    );
    health_safe(&client, &restore_server.url, true).await;

    // An authenticated Admin may still edit drafts while recovery protections apply.
    let edit = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            restore_server.url
        ))
        .header(header::COOKIE, &fresh_cookie)
        .header("X-CSRF-Token", &fresh_csrf)
        .json(&json!({ "title": "Recovery Draft", "slug": "recovery-draft", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(edit.status(), StatusCode::CREATED);
    let smtp_denied = client
        .post(format!(
            "{}/api/admin/settings/smtp/test",
            restore_server.url
        ))
        .header(header::COOKIE, &fresh_cookie)
        .header("X-CSRF-Token", &fresh_csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(smtp_denied.status(), StatusCode::SERVICE_UNAVAILABLE);

    let activation = client
        .post(&activate_url)
        .header(header::COOKIE, &fresh_cookie)
        .header("X-CSRF-Token", &fresh_csrf)
        .json(&json!({ "password": ADMIN_PASSWORD, "expected_generation": generation }))
        .send()
        .await
        .unwrap();
    assert_eq!(activation.status(), StatusCode::OK);
    let activated = activation.json::<Value>().await.unwrap();
    assert_eq!(activated["safe_mode"], false);
    assert_eq!(activated["smtp_enabled"], false);
    assert!(!on_disk_safe_mode(&restored));
    public_noindex(&client, &restore_server.url, "/", false).await;
    drop(restore_server);

    let activated_server = start(&restored).await;
    health_safe(&client, &activated_server.url, false).await;
    public_noindex(
        &client,
        &activated_server.url,
        "/content/post/restore-safe-post",
        false,
    )
    .await;
    let (fresh_cookie, _) = login(&client, &activated_server.url, "admin", ADMIN_PASSWORD).await;
    let final_status = client
        .get(format!(
            "{}/api/admin/operations/recovery/status",
            activated_server.url
        ))
        .header(header::COOKIE, fresh_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(
        final_status.json::<Value>().await.unwrap()["smtp_enabled"],
        false
    );
    assert_eq!(
        fs::read(source.join("config/site.json")).unwrap(),
        source_config
    );
    println!(
        "M8-S10-T01 native binary_sha256={} encrypted_archive_sha256={} restored_db_sha256={} config_sha256={} restart_before_and_after_activation=PASS",
        sha256(&fs::read(bin()).unwrap()),
        archive_hash,
        sha256(&fs::read(restored.join("database/opendash.sqlite")).unwrap()),
        sha256(&fs::read(restored.join("config/site.json")).unwrap()),
    );
}

#[test]
fn config_and_ready_corruption_fail_closed_on_native_binary_start() {
    let isolated = IsolatedRoot::new();
    let root = isolated.path().join("invalid");
    fs::create_dir_all(root.join("config")).unwrap();
    let cfg = serde_json::to_vec(&json!({
        "version": 1, "listen_ip": "127.0.0.1", "port": 18766,
        "trusted_public_origin": null, "log_level": "info", "recovery_safe_mode": true
    }))
    .unwrap();
    let marker = serde_json::to_vec(&json!({
        "version": 1, "job_id": "a".repeat(64), "schema_version": 1,
        "db_sha256": "b".repeat(64), "file_count": 2, "recovery_safe_mode": true
    }))
    .unwrap();
    fs::write(root.join("config/site.json"), &cfg).unwrap();
    fs::write(root.join("READY.json"), &marker).unwrap();
    let check = || {
        Command::new(bin())
            .arg("--check-config")
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_DB_PATH", root.join("database/opendash.sqlite"))
            .env_remove("OPENDASH_PUBLIC_ORIGIN")
            .output()
            .unwrap()
    };

    let valid = check();
    assert!(valid.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&valid.stdout).unwrap()["recovery_safe_mode"],
        true
    );

    fs::remove_file(root.join("config/site.json")).unwrap();
    let missing_config = check();
    assert!(missing_config.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&missing_config.stdout).unwrap()["recovery_safe_mode"],
        true
    );
    fs::write(root.join("config/site.json"), &cfg).unwrap();

    for (target, contents) in [
        ("config/site.json", vec![b'{']),
        ("config/site.json", vec![b'x'; 65 * 1024]),
        ("READY.json", vec![b'{']),
        ("READY.json", vec![b'x'; 65 * 1024]),
    ] {
        let path = root.join(target);
        let original = if target == "READY.json" {
            &marker
        } else {
            &cfg
        };
        fs::write(&path, contents).unwrap();
        assert!(
            !check().status.success(),
            "invalid {target} must prevent startup"
        );
        fs::write(path, original).unwrap();
    }

    fs::remove_file(root.join("READY.json")).unwrap();
    let no_marker = check();
    assert!(no_marker.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&no_marker.stdout).unwrap()["recovery_safe_mode"],
        true
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::remove_file(root.join("config/site.json")).unwrap();
        symlink(
            isolated.path().join("untrusted.json"),
            root.join("config/site.json"),
        )
        .unwrap();
        assert!(!check().status.success(), "symlink config must not load");
        fs::remove_file(root.join("config/site.json")).unwrap();
        fs::write(root.join("config/site.json"), &cfg).unwrap();
        symlink(
            isolated.path().join("untrusted.json"),
            root.join("READY.json"),
        )
        .unwrap();
        assert!(!check().status.success(), "symlink READY must not load");
    }
}
