use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use age::secrecy::SecretString;
use opendash_core::backup::{
    BACKUP_FORMAT_VERSION, BackupFileKind, BackupFileRecord, BackupManifestV1,
    RESTORE_POLICY_VERSION,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
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
            "opendash-m8-s04-t01-{label}-{}-{id}",
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

fn encrypt_zip_bytes_with_age(zip_bytes: &[u8], passphrase: &str, out_path: &Path) {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_string()));
    recipient.set_work_factor(10); // fast for negative fixture tests; verifier allows <= 16
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .expect("encryptor");
    let f = File::create(out_path).expect("create out");
    let mut w = encryptor.wrap_output(f).expect("wrap");
    w.write_all(zip_bytes).expect("write");
    w.finish().expect("finish");
}

fn build_zip_bytes(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = ZipWriter::new(&mut buf);
        for (name, data, method) in entries {
            let opts = SimpleFileOptions::default()
                .compression_method(*method)
                .large_file(false);
            zw.start_file(*name, opts).expect("start_file");
            zw.write_all(data).expect("write_all");
        }
        zw.finish().expect("finish zip");
    }
    buf.into_inner()
}

#[tokio::test]
async fn m8_s04_t01_encrypted_backup_codec_and_bounded_verifier() {
    let tmp = TempDirGuard::new("codec");
    let site_root = tmp.path().join("site");
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(db_path.parent().unwrap()).expect("mkdir db");

    // 1. Create a real migrated SQLite DB.
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let mut conn = SqliteConnection::connect_with(&opts)
        .await
        .expect("connect");
    let migrator = sqlx::migrate!("../../migrations");
    migrator.run(&mut conn).await.expect("migrate");
    conn.close().await.expect("close");

    // 2. Run snapshot worker.
    let snapshot_dir = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot_dir).expect("mkdir snapshot");
    let (code, snap_resp) = run_worker(
        "snapshot",
        &json!({
            "site_root": site_root,
            "db_path": db_path,
            "snapshot_dir": snapshot_dir,
            "app_version": "0.1.0",
            "source_sha": "testsha",
            "created_at": "1731600000"
        }),
    );
    assert_eq!(code, 0, "snapshot failed: {snap_resp}");
    assert_eq!(snap_resp["ok"], true);
    let manifest_path = PathBuf::from(snap_resp["manifest_path"].as_str().unwrap());

    // 3. Encrypt snapshot with valid 20-char passphrase via isolated archive worker.
    let passphrase = "correct-horse-battery-staple-2026!";
    let out_age = tmp.path().join("artifacts").join("b1.odbackup.age");
    let (code, arch_resp) = run_worker(
        "archive",
        &json!({
            "snapshot_dir": snapshot_dir,
            "manifest_path": manifest_path,
            "out_path": out_age,
            "passphrase": passphrase
        }),
    );
    assert_eq!(code, 0, "archive failed: {arch_resp}");
    assert_eq!(arch_resp["ok"], true);
    assert!(out_age.exists(), "encrypted artifact must exist");
    assert!(
        !snapshot_dir.join(".archive-staging.zip").exists(),
        "staging zip must be removed after encryption"
    );
    assert!(
        !out_age.with_file_name(".b1.odbackup.age.part").exists(),
        "partial file must not remain"
    );

    // 4. Verify valid roundtrip via isolated verify worker.
    let verify_dir = tmp.path().join("verify-ok");
    let (code, ver_resp) = run_worker(
        "verify",
        &json!({
            "input_path": out_age,
            "private_dir": verify_dir,
            "passphrase": passphrase
        }),
    );
    assert_eq!(code, 0, "verify failed: {ver_resp}");
    assert_eq!(ver_resp["ok"], true);
    assert!(ver_resp["file_count"].as_u64().unwrap() >= 2);

    // 5. Short passphrase rejected.
    let (code, short_resp) = run_worker(
        "archive",
        &json!({
            "snapshot_dir": snapshot_dir,
            "manifest_path": manifest_path,
            "out_path": tmp.path().join("artifacts").join("short.odbackup.age"),
            "passphrase": "too-short"
        }),
    );
    assert_ne!(code, 0);
    assert_eq!(short_resp["ok"], false);

    // 6. Wrong passphrase rejected and leaves no plaintext in private_dir.
    let verify_wrong = tmp.path().join("verify-wrong");
    let (code, wrong_resp) = run_worker(
        "verify",
        &json!({
            "input_path": out_age,
            "private_dir": verify_wrong,
            "passphrase": "wrong-horse-battery-staple-2026!"
        }),
    );
    assert_ne!(code, 0);
    assert_eq!(wrong_resp["ok"], false);
    assert!(!verify_wrong.join("archive.zip").exists());
    assert!(!verify_wrong.join("payloads").exists());

    // 7. Truncated artifact rejected.
    let mut raw_age = fs::read(&out_age).expect("read age");
    let trunc_path = tmp.path().join("trunc.odbackup.age");
    fs::write(&trunc_path, &raw_age[..raw_age.len() - 16]).expect("write trunc");
    let (code, trunc_resp) = run_worker(
        "verify",
        &json!({
            "input_path": trunc_path,
            "private_dir": tmp.path().join("verify-trunc"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "truncated archive must fail: {trunc_resp}");

    // 8. Tampered ciphertext byte rejected.
    let tamper_path = tmp.path().join("tamper.odbackup.age");
    let last_idx = raw_age.len() - 1;
    raw_age[last_idx] ^= 0xFF;
    fs::write(&tamper_path, &raw_age).expect("write tamper");
    let (code, tamper_resp) = run_worker(
        "verify",
        &json!({
            "input_path": tamper_path,
            "private_dir": tmp.path().join("verify-tamper"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "tampered archive must fail: {tamper_resp}");

    // 9. High scrypt work factor (logN=22) rejected in preflight (< 2s, without running scrypt N=2^22).
    let high_wf_path = tmp.path().join("high_wf.odbackup.age");
    fs::write(
        &high_wf_path,
        b"age-encryption.org/v1\n-> scrypt AAAAAAAAAAAAAAAAAAAAAA 22\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n--- AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nbinarypayload",
    )
    .expect("write high wf");
    let t0 = Instant::now();
    let (code, wf_resp) = run_worker(
        "verify",
        &json!({
            "input_path": high_wf_path,
            "private_dir": tmp.path().join("verify-wf"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "high work factor must be rejected: {wf_resp}");
    assert!(
        t0.elapsed().as_secs() < 5,
        "preflight must reject before expensive KDF"
    );

    // 10. Extra entry in ZIP rejected.
    let db_bytes = b"sqlite-dummy";
    let config_bytes = br#"{"version":1,"listen_ip":"127.0.0.1","port":18766,"trusted_public_origin":null,"log_level":"info","recovery_safe_mode":false}"#;
    let valid_manifest = BackupManifestV1 {
        format_version: BACKUP_FORMAT_VERSION,
        app_version: "0.1.0".to_string(),
        source_sha: "dev".to_string(),
        schema_version: 1,
        migration_checksums: vec![(1, "00".repeat(32))],
        created_at: "1731600000".to_string(),
        files: vec![
            BackupFileRecord {
                entry_name: "00000001.bin".to_string(),
                kind: BackupFileKind::Db,
                logical_key: "database/opendash.sqlite".to_string(),
                size: db_bytes.len() as u64,
                sha256: sha256_hex(db_bytes),
            },
            BackupFileRecord {
                entry_name: "00000002.bin".to_string(),
                kind: BackupFileKind::PortableConfig,
                logical_key: "config/site.json".to_string(),
                size: config_bytes.len() as u64,
                sha256: sha256_hex(config_bytes),
            },
        ],
        restore_policy_version: RESTORE_POLICY_VERSION,
    };
    let manifest_bytes = serde_json::to_vec(&valid_manifest).unwrap();
    let extra_zip = build_zip_bytes(&[
        ("00000001.bin", db_bytes, CompressionMethod::Stored),
        ("00000002.bin", config_bytes, CompressionMethod::Stored),
        ("00000003.bin", b"extra", CompressionMethod::Stored),
        ("manifest.json", &manifest_bytes, CompressionMethod::Stored),
    ]);
    let extra_age = tmp.path().join("extra.odbackup.age");
    encrypt_zip_bytes_with_age(&extra_zip, passphrase, &extra_age);
    let (code, _) = run_worker(
        "verify",
        &json!({
            "input_path": extra_age,
            "private_dir": tmp.path().join("verify-extra"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "extra ZIP entry must be rejected");

    // 11. Deflate-compressed ZIP rejected by format1 profile.
    let deflate_zip = build_zip_bytes(&[
        ("00000001.bin", db_bytes, CompressionMethod::Deflated),
        ("00000002.bin", config_bytes, CompressionMethod::Stored),
        ("manifest.json", &manifest_bytes, CompressionMethod::Stored),
    ]);
    let deflate_age = tmp.path().join("deflate.odbackup.age");
    encrypt_zip_bytes_with_age(&deflate_zip, passphrase, &deflate_age);
    let (code, _) = run_worker(
        "verify",
        &json!({
            "input_path": deflate_age,
            "private_dir": tmp.path().join("verify-deflate"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "deflate ZIP must be rejected");

    // 12. Case collision & malicious logical key in manifest rejected.
    let mut bad_manifest = valid_manifest.clone();
    bad_manifest.files.push(BackupFileRecord {
        entry_name: "00000003.bin".to_string(),
        kind: BackupFileKind::MediaOriginal,
        logical_key: "uploads/media/Abc.png".to_string(),
        size: 3,
        sha256: sha256_hex(b"abc"),
    });
    bad_manifest.files.push(BackupFileRecord {
        entry_name: "00000004.bin".to_string(),
        kind: BackupFileKind::MediaOriginal,
        logical_key: "uploads/media/abc.png".to_string(),
        size: 3,
        sha256: sha256_hex(b"abc"),
    });
    let bad_manifest_bytes = serde_json::to_vec(&bad_manifest).unwrap();
    let case_zip = build_zip_bytes(&[
        ("00000001.bin", db_bytes, CompressionMethod::Stored),
        ("00000002.bin", config_bytes, CompressionMethod::Stored),
        ("00000003.bin", b"abc", CompressionMethod::Stored),
        ("00000004.bin", b"abc", CompressionMethod::Stored),
        (
            "manifest.json",
            &bad_manifest_bytes,
            CompressionMethod::Stored,
        ),
    ]);
    let case_age = tmp.path().join("case.odbackup.age");
    encrypt_zip_bytes_with_age(&case_zip, passphrase, &case_age);
    let (code, _) = run_worker(
        "verify",
        &json!({
            "input_path": case_age,
            "private_dir": tmp.path().join("verify-case"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "casefold collision must be rejected");

    // Malicious Windows reserved name / path traversal
    let mut trav_manifest = valid_manifest;
    trav_manifest.files.push(BackupFileRecord {
        entry_name: "00000003.bin".to_string(),
        kind: BackupFileKind::MediaOriginal,
        logical_key: "uploads/../CON.png".to_string(),
        size: 3,
        sha256: sha256_hex(b"abc"),
    });
    let trav_manifest_bytes = serde_json::to_vec(&trav_manifest).unwrap();
    let trav_zip = build_zip_bytes(&[
        ("00000001.bin", db_bytes, CompressionMethod::Stored),
        ("00000002.bin", config_bytes, CompressionMethod::Stored),
        ("00000003.bin", b"abc", CompressionMethod::Stored),
        (
            "manifest.json",
            &trav_manifest_bytes,
            CompressionMethod::Stored,
        ),
    ]);
    let trav_age = tmp.path().join("trav.odbackup.age");
    encrypt_zip_bytes_with_age(&trav_zip, passphrase, &trav_age);
    let (code, _) = run_worker(
        "verify",
        &json!({
            "input_path": trav_age,
            "private_dir": tmp.path().join("verify-trav"),
            "passphrase": passphrase
        }),
    );
    assert_ne!(code, 0, "malicious logical key must be rejected");
}
