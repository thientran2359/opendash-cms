# Verification Report: M8 Post-Merge Review Fixes (`#262`)

- **Task:** `#262` (`[M8 review] Sửa activation barrier, hủy restore, shutdown và proxy origin`)
- **Branch:** `agent/nova`
- **Date:** 2026-11-15
- **Status:** VERIFIED ON WINDOWS (`PASS`); native Linux `SIGTERM` drain test `NOT_RUN` on Windows host.

## Changes Implemented

1. **R1 — Activation barrier & restore admission (`cms/crates/opendash-server/src/routes/operations.rs`, `cms/crates/opendash-server/tests/m8_s01_t02_writer_barrier.rs`):**
   - Added `AdmittedMutation(_lease)` extractor to `activate_recovery` and `upload_restore_archive` so exclusive maintenance windows reject activation/upload with `503 Service Unavailable` + `Retry-After` without modifying `config/site.json` or runtime `safe_mode`, and admitted activations complete before snapshot entry.
   - Kept `create_restore` using migration `0032` partial unique index (`409 Conflict` when another operation job is active) and kept `cancel_operation_job` terminal bookkeeping accessible during maintenance.
   - Added `operations.rs` (`upload_restore_archive`, `activate_recovery`) to `audited_writer_inventory_covers_every_mutating_route_handler`.
2. **R2 — Restore worker cancellation (`cms/crates/opendash-server/src/routes/operations.rs`):**
   - Registered `cancel_flag` (`BackupRuntimeRegistry::register_cancel_flag`) in `create_restore` and passed it to `spawn_restore_worker_process`.
   - On cancellation, `spawn_restore_worker_process` immediately kills and waits on the OS child worker, removes the cancelled job's `target_root` (preventing late `READY.json` / `succeeded` transitions), transitions the job to `cancelled`, and leaves the source site and any previous completed restore directories intact.
3. **R3 — Bounded shutdown drain & upload timeout (`cms/crates/opendash-server/src/main.rs`, `operations.rs`, `backup.rs`, `routes/operations.rs`):**
   - Bounded `axum::serve` HTTP drain from `shutdown_signal()` with a 10s timeout.
   - Enforced a 5-minute (`300s`) deadline in `upload_restore_archive` (`upload_restore_archive_with_timeout`), cleaning up `.part` staging files on timeout.
   - Added `SiteCoordinator::drain_for_shutdown` (blocking new mutations and awaiting active `MutationLease` writers including background SMTP tasks) and `BackupRuntimeRegistry::cancel_and_drain_all` returning `DrainReport`, propagating `cancel_flag` into `create_snapshot_with_cancel`, and returning a non-zero error if shutdown drain fails to reach quiescence before closing DB/locks.
4. **R4 — Proxy origin canonicalization (`cms/crates/opendash-core/src/backup.rs`, `cms/crates/opendash-server/src/config.rs`, `app.rs`, `routes/contact.rs`):**
   - Added `opendash_core::backup::canonicalize_trusted_origin` using `url::Url` to accept a single optional trailing `/`, normalize scheme/host casing and default `:80`/`:443` ports, and reject credentials, non-root paths, query strings, and fragments.
   - Derived `RuntimeConfigV1::trusted_origin_normalized()`, `trusted_origin_is_https()`, and `trusted_host()` from the same canonical helper, and rejected non-UTF8 `Origin` headers in `app.rs`.

## Verification Commands & Results

- `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-core --lib backup::tests` -> `PASS` (`2 passed; 0 failed`).
- `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash routes::operations::tests` -> `PASS` (`2 passed; 0 failed`).
- `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash operations::tests` -> `PASS` (`9 passed; 0 failed`).
- `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m8_review_fixes --test m8_s01_t02_writer_barrier --test m8_s05_t02_restore_admin` -> `PASS` (`m8_review_fixes`: `2 passed; 0 failed`; `m8_s01_t02_writer_barrier`: `2 passed; 0 failed`; `m8_s05_t02_restore_admin`: `1 passed; 0 failed`).
- `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m8_s06_t01_runtime` -> `PASS` (`3 passed; 0 failed`).
- `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test m8_s04_t02_backup_admin --test m7_s06_t01_smtp` -> `PASS` (`m7_s06_t01_smtp`: `1 passed; 0 failed`; `m8_s04_t02_backup_admin`: `1 passed; 0 failed`).
- Headless Chrome Playwright `#239` browser E2E (`verify_239_browser.py`: source login → backup → UI `RestorePanel` upload & restore → operator start on restored root → `X-Robots-Tag: noindex, nofollow` check → browser login & `Mở khóa Safe Mode` → Builder Section/Row/Column/Heading edit & `#save` → public `/` check → source health intact) -> `PASS` (exit code `0`).
- `python cms/scripts/check-standards.py` -> `PASS: core, storage, route and Admin boundaries`.
- Native Linux `SIGTERM` drain while backup/SMTP active: `NOT_RUN` (Windows host).
