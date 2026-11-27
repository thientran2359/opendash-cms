use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use age::secrecy::SecretString;
use opendash_core::backup::{
    BACKUP_FORMAT_VERSION, BackupFileKind, BackupFileRecord, BackupManifestV1,
    PortableRuntimeConfigV1, RESTORE_POLICY_VERSION,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection as _, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[path = "../../src/operations_worker_process.rs"]
mod operations_worker_process;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s05-t01-{label}-{}-{id}",
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

fn server_bin() -> &'static str {
    env!("CARGO_BIN_EXE_opendash")
}

fn run_worker(mode: &str, req: &Value) -> (i32, Value) {
    let mut child =
        operations_worker_process::spawn(Path::new(server_bin()), mode).expect("spawn worker");

    {
        let mut stdin = child.stdin.take().expect("stdin");
        let line = serde_json::to_string(req).expect("serialize req");
        stdin.write_all(line.as_bytes()).expect("write req");
        stdin.write_all(b"\n").expect("write newline");
        stdin.flush().expect("flush");
        let out = child.wait_with_output().expect("wait worker");
        drop(stdin);
        let code = out.status.code().unwrap_or(-1);
        let parsed: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid worker stdout JSON ({e}): stdout={}, stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (code, parsed)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn build_encrypted_backup_from_db_bytes(
    db_bytes: &[u8],
    schema_version: i64,
    migration_checksums: Vec<(i64, String)>,
    passphrase: &str,
    out_age: &Path,
) {
    let cfg = PortableRuntimeConfigV1::default();
    let cfg_bytes = serde_json::to_vec(&cfg).unwrap();
    let manifest = BackupManifestV1 {
        format_version: BACKUP_FORMAT_VERSION,
        app_version: "0.1.0".to_string(),
        source_sha: "dev".to_string(),
        schema_version,
        migration_checksums,
        created_at: "1731600000".to_string(),
        files: vec![
            BackupFileRecord {
                entry_name: "00000001.bin".to_string(),
                kind: BackupFileKind::Db,
                logical_key: "db/main.sqlite".to_string(),
                size: db_bytes.len() as u64,
                sha256: sha256_hex(db_bytes),
            },
            BackupFileRecord {
                entry_name: "00000002.bin".to_string(),
                kind: BackupFileKind::PortableConfig,
                logical_key: "config/site.json".to_string(),
                size: cfg_bytes.len() as u64,
                sha256: sha256_hex(&cfg_bytes),
            },
        ],
        restore_policy_version: RESTORE_POLICY_VERSION,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let mut zip_buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = ZipWriter::new(&mut zip_buf);
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(false);
        zw.start_file("00000001.bin", opts).unwrap();
        zw.write_all(db_bytes).unwrap();
        zw.start_file("00000002.bin", opts).unwrap();
        zw.write_all(&cfg_bytes).unwrap();
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(&manifest_bytes).unwrap();
        zw.finish().unwrap();
    }
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_string()));
    recipient.set_work_factor(10);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let f = File::create(out_age).unwrap();
    let mut w = encryptor.wrap_output(f).unwrap();
    w.write_all(&zip_buf.into_inner()).unwrap();
    w.finish().unwrap();
}

#[tokio::test]
async fn m8_s05_t01_restore_rebase_fingerprint_and_failpoints() {
    let tmp = TempDirGuard::new("restore");
    let site_root = tmp.path().join("site");
    let external_db_path = tmp.path().join("external-db").join("custom.sqlite");
    fs::create_dir_all(&site_root).unwrap();
    fs::create_dir_all(external_db_path.parent().unwrap()).unwrap();

    // 1. Create a real migrated SQLite DB at an EXTERNAL path with an Admin user, session, SMTP enabled, and active job.
    let opts = SqliteConnectOptions::new()
        .filename(&external_db_path)
        .create_if_missing(true);
    let mut conn = SqliteConnection::connect_with(&opts).await.unwrap();
    let migrator = sqlx::migrate!("../../migrations");
    migrator.run(&mut conn).await.unwrap();

    sqlx::query("INSERT INTO users (username, password_hash, role, version) VALUES ('admin', '$argon2id$v=19$m=19456,t=2,p=1$dummy$hash', 'admin', 1)")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at) VALUES ('tokhash123', 1, 'csrf123', '2099-01-01T00:00:00Z')")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE site_smtp_settings SET enabled = 1, host = 'smtp.example.com' WHERE id = 1",
    )
    .execute(&mut conn)
    .await
    .unwrap();
    let stale_job_id = "aa".repeat(32);
    sqlx::query(
        "INSERT INTO operation_jobs (id, kind, state, version) VALUES (?, 'backup', 'running', 1)",
    )
    .bind(&stale_job_id)
    .execute(&mut conn)
    .await
    .unwrap();

    let mig_rows = sqlx::query(
        "SELECT version, lower(hex(checksum)) AS hex FROM _sqlx_migrations ORDER BY version ASC",
    )
    .fetch_all(&mut conn)
    .await
    .unwrap();
    let mut mig_checksums = Vec::new();
    let mut max_schema_ver = 0i64;
    for r in mig_rows {
        let v: i64 = r.try_get("version").unwrap();
        let h: String = r.try_get("hex").unwrap();
        max_schema_ver = max_schema_ver.max(v);
        mig_checksums.push((v, h));
    }
    conn.close().await.unwrap();

    let source_db_sha_before = sha256_hex(&fs::read(&external_db_path).unwrap());

    // 2. Snapshot & encrypt via isolated workers.
    let snapshot_dir = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot_dir).unwrap();
    let (code, snap_resp) = run_worker(
        "snapshot",
        &json!({
            "site_root": site_root,
            "db_path": external_db_path,
            "snapshot_dir": snapshot_dir,
            "app_version": "0.1.0",
            "source_sha": "testsha",
            "created_at": "1731600000"
        }),
    );
    assert_eq!(code, 0, "snapshot failed: {snap_resp}");

    let passphrase = "restore-passphrase-2026-secure!";
    let out_age = tmp.path().join("backup.odbackup.age");
    let (code, arch_resp) = run_worker(
        "archive",
        &json!({
            "snapshot_dir": snapshot_dir,
            "manifest_path": snap_resp["manifest_path"],
            "out_path": out_age,
            "passphrase": passphrase
        }),
    );
    assert_eq!(code, 0, "archive failed: {arch_resp}");

    // 3. Failpoints at every phase must NOT publish target and must leave source DB untouched.
    for (idx, fp) in ["after_extract", "after_db_sanitize", "after_ready_marker"]
        .iter()
        .enumerate()
    {
        let job_id = format!("{:064x}", idx + 10);
        let target = site_root.join("backups").join("restored").join(&job_id);
        let (code, fp_resp) = run_worker(
            "restore",
            &json!({
                "input_path": out_age,
                "private_dir": tmp.path().join(format!("priv-fp-{idx}")),
                "generated_target": target,
                "source_site_root": site_root,
                "source_db_path": external_db_path,
                "job_id": job_id,
                "passphrase": passphrase,
                "failpoint": fp
            }),
        );
        assert_ne!(code, 0, "failpoint {fp} must fail: {fp_resp}");
        assert!(!target.exists(), "failpoint {fp} must never publish target");
    }

    // 4. Successful restore to separate root, rebasing external DB -> database/opendash.sqlite.
    let job_ok = "bb".repeat(32);
    let target_ok = site_root.join("backups").join("restored").join(&job_ok);
    let (code, ok_resp) = run_worker(
        "restore",
        &json!({
            "input_path": out_age,
            "private_dir": tmp.path().join("priv-ok"),
            "generated_target": target_ok,
            "source_site_root": site_root,
            "source_db_path": external_db_path,
            "job_id": job_ok,
            "passphrase": passphrase,
            "failpoint": null
        }),
    );
    assert_eq!(code, 0, "restore failed: {ok_resp}");
    assert!(target_ok.join("READY.json").is_file());
    let rebased_db = target_ok.join("database").join("opendash.sqlite");
    assert!(rebased_db.is_file());

    // Verify safe mode config
    let cfg_raw = fs::read_to_string(target_ok.join("config").join("site.json")).unwrap();
    let cfg_json: Value = serde_json::from_str(&cfg_raw).unwrap();
    assert_eq!(cfg_json["recovery_safe_mode"], true);

    // Verify sanitized restored DB state + preserved Admin user + legitimate triggers
    let r_opts = SqliteConnectOptions::new()
        .filename(&rebased_db)
        .read_only(true);
    let mut r_conn = SqliteConnection::connect_with(&r_opts).await.unwrap();
    let sess_cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&mut r_conn)
        .await
        .unwrap();
    assert_eq!(sess_cnt, 0, "sessions must be cleared");
    let smtp_enabled: i64 =
        sqlx::query_scalar("SELECT enabled FROM site_smtp_settings WHERE id = 1")
            .fetch_one(&mut r_conn)
            .await
            .unwrap();
    assert_eq!(smtp_enabled, 0, "SMTP must be disabled on restore");
    let job_state: String = sqlx::query_scalar("SELECT state FROM operation_jobs WHERE id = ?")
        .bind(&stale_job_id)
        .fetch_one(&mut r_conn)
        .await
        .unwrap();
    assert_eq!(job_state, "interrupted");
    let admin_cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE username = 'admin' AND role = 'admin'",
    )
    .fetch_one(&mut r_conn)
    .await
    .unwrap();
    assert_eq!(admin_cnt, 1);
    r_conn.close().await.unwrap();

    // Verify source DB unchanged byte-for-byte
    assert_eq!(
        sha256_hex(&fs::read(&external_db_path).unwrap()),
        source_db_sha_before
    );

    // 5. Negative: Forged-valid migration ledger with an extra malicious trigger in sqlite_schema MUST be rejected!
    let evil_db_path = tmp.path().join("evil.sqlite");
    fs::copy(&external_db_path, &evil_db_path).unwrap();
    let evil_opts = SqliteConnectOptions::new().filename(&evil_db_path);
    let mut evil_conn = SqliteConnection::connect_with(&evil_opts).await.unwrap();
    sqlx::query("CREATE TRIGGER evil_trigger AFTER INSERT ON users BEGIN SELECT 1; END;")
        .execute(&mut evil_conn)
        .await
        .unwrap();
    evil_conn.close().await.unwrap();

    let evil_age = tmp.path().join("evil.odbackup.age");
    build_encrypted_backup_from_db_bytes(
        &fs::read(&evil_db_path).unwrap(),
        max_schema_ver,
        mig_checksums,
        passphrase,
        &evil_age,
    );
    let evil_job = "cc".repeat(32);
    let evil_target = site_root.join("backups").join("restored").join(&evil_job);
    let (code, evil_resp) = run_worker(
        "restore",
        &json!({
            "input_path": evil_age,
            "private_dir": tmp.path().join("priv-evil"),
            "generated_target": evil_target,
            "source_site_root": site_root,
            "source_db_path": external_db_path,
            "job_id": evil_job,
            "passphrase": passphrase,
            "failpoint": null
        }),
    );
    assert_ne!(
        code, 0,
        "malicious extra trigger must be rejected: {evil_resp}"
    );
    assert!(!evil_target.exists());
}
