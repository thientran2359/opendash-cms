use std::{
    fs,
    io::{Cursor, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use opendash_core::backup::{BackupFileKind, BackupManifestV1, validate_manifest_v1};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection as _, SqliteConnection, SqlitePool, sqlite::SqliteConnectOptions};

#[path = "../src/operations_worker_process.rs"]
mod operations_worker_process;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s03-t01-{label}-{}-{id}",
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
    async fn start(site_root: &Path, db_path: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let bin = env!("CARGO_BIN_EXE_opendash");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_DB_PATH", db_path)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..80 {
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

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn tiny_png(r: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([r, 120, 180, 255])));
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("encode png");
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn invoke_snapshot_worker(
    site_root: &Path,
    db_path: &Path,
    snapshot_dir: &Path,
) -> (bool, Value) {
    let bin = env!("CARGO_BIN_EXE_opendash");
    let mut child = operations_worker_process::spawn(Path::new(bin), "snapshot")
        .expect("spawn snapshot worker");

    let req = json!({
        "site_root": site_root,
        "db_path": db_path,
        "snapshot_dir": snapshot_dir,
        "app_version": "0.1.0",
        "source_sha": "test-sha",
        "created_at": "1700000000"
    });
    let mut bytes = serde_json::to_vec(&req).expect("serialize req");
    bytes.push(b'\n');

    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(&bytes).expect("write req");
    stdin.flush().expect("flush req");

    let output = child.wait_with_output().expect("wait worker");
    drop(stdin);
    let resp: Value = serde_json::from_slice(&output.stdout).expect("worker stdout json");
    (output.status.success(), resp)
}

#[tokio::test]
async fn snapshot_worker_vacuums_external_db_verifies_manifest_and_fails_closed_on_missing_refs() {
    let dir = TempDirGuard::new("main");
    let site_root = dir.path().join("site");
    let ext_db_dir = dir.path().join("external-db");
    fs::create_dir_all(&site_root).expect("create site root");
    fs::create_dir_all(&ext_db_dir).expect("create ext db dir");
    let ext_db_path = ext_db_dir.join("custom-opendash.sqlite");

    let mut server = RunningServer::start(&site_root, &ext_db_path).await;
    let client = Client::new();

    // Setup admin, login, upload media
    let setup = client
        .post(format!("{}/auth/setup", server.base_url))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .expect("setup");
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let login_json: Value = login.json().await.expect("login json");
    let csrf = login_json["csrf_token"].as_str().expect("csrf").to_string();

    let up = client
        .post(format!("{}/api/admin/media", server.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("X-File-Name", "snap.png")
        .header(reqwest::header::CONTENT_TYPE, "image/png")
        .body(tiny_png(55))
        .send()
        .await
        .expect("upload");
    assert_eq!(up.status(), StatusCode::CREATED);
    let up_json: Value = up.json().await.expect("up json");
    let media_id = up_json["id"].as_str().expect("media id").to_string();

    server.stop();

    // Seed an SMTP secret blob & reference in external DB
    let secret_ref = format!("{:064x}", 0x9999u64);
    let secrets_dir = site_root.join("database").join("secrets").join("smtp");
    fs::create_dir_all(&secrets_dir).expect("create secrets dir");
    let secret_file = secrets_dir.join(format!("{secret_ref}.json"));
    fs::write(&secret_file, b"{\"v\":1,\"password\":\"secret\"}").expect("write secret");

    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&ext_db_path))
        .await
        .expect("connect ext db");
    sqlx::query("UPDATE site_smtp_settings SET secret_ref = ? WHERE id = 1")
        .bind(&secret_ref)
        .execute(&pool)
        .await
        .expect("seed secret_ref");
    pool.close().await;

    // Run isolated --operations-worker snapshot
    let snap_dir = site_root.join("backups").join(".staging").join("job1").join("snapshot");
    let (ok, resp) = invoke_snapshot_worker(&site_root, &ext_db_path, &snap_dir);
    assert!(ok, "snapshot worker should succeed: {resp:?}");
    assert_eq!(resp["ok"].as_bool(), Some(true));

    let manifest_bytes = fs::read(snap_dir.join("manifest.json")).expect("read manifest");
    let manifest: BackupManifestV1 =
        serde_json::from_slice(&manifest_bytes).expect("parse manifest");
    validate_manifest_v1(&manifest).expect("manifest must validate");

    // Verify DB, media original, media thumbnail, secret, and portable config are all present with exact hashes
    let mut kinds = Vec::new();
    for rec in &manifest.files {
        kinds.push(rec.kind);
        let payload = fs::read(snap_dir.join(&rec.entry_name)).expect("read payload");
        assert_eq!(payload.len() as u64, rec.size);
        assert_eq!(hex_encode(&Sha256::digest(&payload)), rec.sha256);
    }
    assert!(kinds.contains(&BackupFileKind::Db));
    assert!(kinds.contains(&BackupFileKind::PortableConfig));
    assert!(kinds.contains(&BackupFileKind::MediaOriginal));
    assert!(kinds.contains(&BackupFileKind::MediaThumbnail));
    assert!(kinds.contains(&BackupFileKind::SmtpSecret));

    // Verify rebased DB snapshot passes integrity_check and preserves media_id
    let db_entry = manifest
        .files
        .iter()
        .find(|f| f.kind == BackupFileKind::Db)
        .expect("db record");
    let snap_db_path = snap_dir.join(&db_entry.entry_name);
    let mut snap_conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&snap_db_path)
            .read_only(true),
    )
    .await
    .expect("open snap db");
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut snap_conn)
        .await
        .expect("integrity_check");
    assert_eq!(integrity.to_ascii_lowercase(), "ok");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE id = ?")
        .bind(&media_id)
        .fetch_one(&mut snap_conn)
        .await
        .expect("query media in snap");
    assert_eq!(count, 1);
    let _: () = snap_conn.close().await.unwrap();

    // Negative 1: Missing referenced secret fails closed
    fs::remove_file(&secret_file).expect("remove secret");
    let snap_dir2 = site_root.join("backups").join(".staging").join("job2").join("snapshot");
    let (ok2, resp2) = invoke_snapshot_worker(&site_root, &ext_db_path, &snap_dir2);
    assert!(!ok2);
    assert_eq!(resp2["ok"].as_bool(), Some(false));

    // Restore secret, Negative 2: Missing referenced media file fails closed
    fs::write(&secret_file, b"{\"v\":1,\"password\":\"secret\"}").expect("restore secret");
    let orig_media = site_root
        .join("uploads")
        .join("media")
        .join(&media_id[..2])
        .join(format!("{media_id}.png"));
    fs::remove_file(&orig_media).expect("remove media");
    let snap_dir3 = site_root.join("backups").join(".staging").join("job3").join("snapshot");
    let (ok3, resp3) = invoke_snapshot_worker(&site_root, &ext_db_path, &snap_dir3);
    assert!(!ok3);
    assert_eq!(resp3["ok"].as_bool(), Some(false));
}
