use super::*;

pub fn enforce_native_memory_limit_512mib() -> Result<(), String> {
    crate::operations_worker_process::enforce_native_memory_limit_bytes(
        WORKER_MEMORY_LIMIT_BYTES,
        crate::operations_worker_process::OPERATIONS_BOOTSTRAP_PROOF,
    )
}

pub fn run_operations_worker_from_stdio() -> i32 {
    if let Err(err) = enforce_native_memory_limit_512mib() {
        let resp = SnapshotWorkerResponse {
            ok: false,
            manifest_path: None,
            error: Some(format!("memory_limit_failed: {err}")),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    }

    let mode = std::env::args()
        .skip_while(|a| a != "--operations-worker")
        .nth(1)
        .unwrap_or_else(|| "snapshot".to_string());
    match mode.as_str() {
        "snapshot" => run_snapshot_worker_from_stdio(),
        "archive" => run_archive_worker_from_stdio(),
        "verify" => run_verify_worker_from_stdio(),
        "restore" => run_restore_worker_from_stdio(),
        _ => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some("unknown_operations_worker_mode".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            2
        }
    }
}

pub(crate) fn read_worker_line() -> Result<String, &'static str> {
    let mut req_line = String::new();
    let mut reader = io::BufReader::new(io::stdin().take((MAX_WORKER_REQUEST_BYTES + 1) as u64));
    if io::BufRead::read_line(&mut reader, &mut req_line).is_err()
        || req_line.is_empty()
        || req_line.len() > MAX_WORKER_REQUEST_BYTES
    {
        return Err("invalid_worker_request_size");
    }
    Ok(req_line)
}

pub(crate) fn spawn_stdin_lifeline() -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = Arc::clone(&done);
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        loop {
            match io::stdin().read(&mut byte) {
                Ok(0) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
            }
        }
    });
    done
}

pub(crate) fn run_archive_worker_from_stdio() -> i32 {
    let Ok(req_line) = read_worker_line() else {
        let resp = ArchiveWorkerResponse {
            ok: false,
            archive_path: None,
            archive_size: None,
            archive_sha256: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    };
    let req: ArchiveWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };
    let done = spawn_stdin_lifeline();
    let manifest_bytes = match fs::read(&req.manifest_path) {
        Ok(b) => b,
        Err(_) => {
            done.store(true, Ordering::SeqCst);
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some("manifest_read_failed".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 1;
        }
    };
    let manifest: BackupManifestV1 = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(_) => {
            done.store(true, Ordering::SeqCst);
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some("manifest_parse_failed".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 1;
        }
    };
    let receipt = crate::backup::SnapshotReceipt {
        job_id: "worker".to_string(),
        snapshot_dir: req.snapshot_dir,
        manifest_path: req.manifest_path,
        manifest,
    };
    let secret = age::secrecy::SecretString::from(req.passphrase);
    let res = crate::backup_archive::write_encrypted_archive(&receipt, &secret, &req.out_path);
    done.store(true, Ordering::SeqCst);
    match res {
        Ok(art) => {
            let resp = ArchiveWorkerResponse {
                ok: true,
                archive_path: Some(art.archive_path),
                archive_size: Some(art.archive_size),
                archive_sha256: Some(art.archive_sha256),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            0
        }
        Err(err) => {
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            1
        }
    }
}

pub(crate) fn run_verify_worker_from_stdio() -> i32 {
    let Ok(req_line) = read_worker_line() else {
        let resp = VerifyArchiveWorkerResponse {
            ok: false,
            zip_path: None,
            payload_dir: None,
            file_count: None,
            schema_version: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    };
    let req: VerifyArchiveWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = VerifyArchiveWorkerResponse {
                ok: false,
                zip_path: None,
                payload_dir: None,
                file_count: None,
                schema_version: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };
    let done = spawn_stdin_lifeline();
    let secret = age::secrecy::SecretString::from(req.passphrase);
    let res = crate::backup_archive::decrypt_and_validate_archive(
        &req.input_path,
        &secret,
        &req.private_dir,
    );
    done.store(true, Ordering::SeqCst);
    match res {
        Ok(v) => {
            let resp = VerifyArchiveWorkerResponse {
                ok: true,
                zip_path: Some(v.zip_path),
                payload_dir: Some(v.payload_dir),
                file_count: Some(v.manifest.files.len()),
                schema_version: Some(v.manifest.schema_version),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            0
        }
        Err(err) => {
            let resp = VerifyArchiveWorkerResponse {
                ok: false,
                zip_path: None,
                payload_dir: None,
                file_count: None,
                schema_version: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            1
        }
    }
}

pub(crate) fn run_restore_worker_from_stdio() -> i32 {
    let Ok(req_line) = read_worker_line() else {
        let resp = RestoreWorkerResponse {
            ok: false,
            target_root: None,
            schema_version: None,
            db_sha256: None,
            file_count: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    };
    let req: RestoreWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = RestoreWorkerResponse {
                ok: false,
                target_root: None,
                schema_version: None,
                db_sha256: None,
                file_count: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };
    let done = spawn_stdin_lifeline();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            done.store(true, Ordering::SeqCst);
            let resp = RestoreWorkerResponse {
                ok: false,
                target_root: None,
                schema_version: None,
                db_sha256: None,
                file_count: None,
                error: Some(format!("runtime_init_failed: {err}")),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };

    let failpoint = match req.failpoint.as_deref() {
        Some("after_extract") => Some(crate::restore::RestoreFailpoint::AfterExtract),
        Some("after_db_sanitize") => Some(crate::restore::RestoreFailpoint::AfterDbSanitize),
        Some("after_ready_marker") => Some(crate::restore::RestoreFailpoint::AfterReadyMarker),
        _ => None,
    };

    let secret = age::secrecy::SecretString::from(req.passphrase);
    let res = rt.block_on(async {
        let validated = crate::backup_archive::decrypt_and_validate_archive(
            &req.input_path,
            &secret,
            &req.private_dir,
        )?;
        let policy = crate::restore::RestorePolicy {
            job_id: req.job_id,
            source_site_root: req.source_site_root,
            source_db_path: req.source_db_path,
            failpoint,
        };
        crate::restore::restore_to_new_root(&validated, &policy, &req.generated_target).await
    });
    done.store(true, Ordering::SeqCst);
    match res {
        Ok(r) => {
            let resp = RestoreWorkerResponse {
                ok: true,
                target_root: Some(r.target_root),
                schema_version: Some(r.schema_version),
                db_sha256: Some(r.db_sha256),
                file_count: Some(r.file_count),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            0
        }
        Err(err) => {
            let resp = RestoreWorkerResponse {
                ok: false,
                target_root: None,
                schema_version: None,
                db_sha256: None,
                file_count: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            1
        }
    }
}

pub(crate) fn run_snapshot_worker_from_stdio() -> i32 {
    let mut req_line = String::new();
    let mut reader = io::BufReader::new(io::stdin().take((MAX_WORKER_REQUEST_BYTES + 1) as u64));
    if io::BufRead::read_line(&mut reader, &mut req_line).is_err()
        || req_line.is_empty()
        || req_line.len() > MAX_WORKER_REQUEST_BYTES
    {
        let resp = SnapshotWorkerResponse {
            ok: false,
            manifest_path: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    }

    let req: SnapshotWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };

    // Lifeline watcher: if parent closes stdin pipe unexpectedly, terminate.
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = Arc::clone(&done);
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        loop {
            match io::stdin().read(&mut byte) {
                Ok(0) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
            }
        }
    });

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some(format!("runtime_init_failed: {err}")),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };

    let result = rt.block_on(execute_snapshot_worker(&req));
    done.store(true, Ordering::SeqCst);

    match result {
        Ok(manifest_path) => {
            let resp = SnapshotWorkerResponse {
                ok: true,
                manifest_path: Some(manifest_path),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            let _ = io::stdout().flush();
            0
        }
        Err(err) => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            let _ = io::stdout().flush();
            1
        }
    }
}
