# M8 Operations & Release E2E Fixtures (`m8`)

## Overview

Deterministic schema-fingerprint fixture and E2E contract baseline for the M8 backup, restore, and recovery harnesses ([m8_s08_t01_e2e.rs](../../crates/opendash-server/tests/ops/backup_restore_e2e.rs) and [operations_backup_restore_e2e.py](../../scripts/operations_backup_restore_e2e.py)). Static `.odbackup.age` archives and passphrases are never stored in Git; tests synthesize encrypted backups dynamically in disposable temporary site roots.

## Interface

- **Schema Fingerprint Fixture ([schema_fingerprints.json](schema_fingerprints.json)):**
  - Records `schema_version: 33`, `trusted_schema: "OFF"`, `max_schema_objects: 256`, `max_sql_per_object_bytes: 262144`, `max_total_sql_bytes: 2097152`, and `verified_by: "runtime_compiled_migrator_memory_db"`, aligning with `MAX_SCHEMA_OBJECTS`, `MAX_SCHEMA_SQL_PER_OBJECT_BYTES`, and `MAX_SCHEMA_TOTAL_SQL_BYTES` in `[cms/crates/opendash-server/src/storage/restore.rs:validate_restored_db_read_only]`.
- **V1 Functional Scope (`M8-S08-T01`):**
  - Covers initial `/auth/setup`, custom content types (`published` vs `draft` visibility isolation), pinned media assets, menus/theme ZIPs, plugins + `plugin_kv`, SEO routes/settings/contact forms, and builder homepage layout.
- **OPS-01 Intentional Restore Resets:**
  1. Source `sessions` and login tokens are cleared; admins must re-authenticate on the restored target.
  2. Restored targets boot in `safe_mode = true` (`X-Robots-Tag: noindex, nofollow` on public routes; anonymous contact submission returns `503`) until password-verified activation via `/api/admin/operations/recovery/activate`.
  3. `site_smtp_settings.enabled` is reset to `0` (`false`), while `BackupFileKind::SmtpSecret` restores referenced SMTP secret blobs (`secrets/smtp/{sref}.*`) and `validate_restored_db_read_only` fails restore if any referenced secret file is missing.
  4. Active/queued `operation_jobs` at snapshot time transition to `interrupted` on restart without silent retry.
  5. External `OPENDASH_DB_PATH` values rebase to `<target_root>/database/opendash.sqlite`, applying private ACLs to `target_parent`, `stage_root`, and restored subdirectories (`database/`, `config/`, `uploads/`, `themes/`, `secrets/`).
- **Negative Corpus & Recovery Tests (`cms/crates/opendash-server/tests/ops/backup_restore_e2e.rs`):**
  - `[m8_s08_t01_e2e.rs:negative_corpus_preserves_source_and_never_publishes_target]` verifies fail-closed behavior (source intact, no `READY.json` published) on tampered `.odbackup.age` ciphertext, wrong passphrase, newer `schema_version` (`9999`), and `507 Insufficient Storage` at `MAX_BACKUP_ARTIFACTS = 10` (`[cms/crates/opendash-server/src/backup.rs:MAX_BACKUP_ARTIFACTS]`).
  - `[m8_s08_t01_e2e.rs:upgrade_0031_to_0033_and_interrupted_job_recovery]` verifies `'running'` `operation_jobs` transition to `'interrupted'` on restart.

## Invariants

- **Fail-Closed Read-Only DB Inspection:** `[cms/crates/opendash-server/src/storage/restore.rs:validate_restored_db_read_only]` enforces `PRAGMA trusted_schema = OFF`, `PRAGMA integrity_check`, `PRAGMA foreign_key_check`, bounded `sqlite_schema` metadata (`256` objects / `256 KiB` per object / `2 MiB` total), and `_sqlx_migrations` verification before any write or migration occurs.
- **State Equivalence:** Every non-exempt record (content IDs, slugs, `values`, `plugin_kv`, media assets, menus, themes, settings, contact forms, and builder documents) must match 100% after restore.
- **No Static Secrets:** No `.odbackup.age` payloads or passphrases are committed under `cms/fixtures/m8/`.

## Side Effects

- None from static files; running `m8_s08_t01_e2e.rs` or `operations_backup_restore_e2e.py` spawns isolated loopback server instances and cleans up temporary `site_root` directories and `.odbackup.age` artifacts upon completion.

## Verification

Run the M8 Rust integration/E2E suite and Playwright browser E2E runner:

```powershell
cargo test --manifest-path cms/Cargo.toml --locked --test m8_s08_t01_e2e
python cms/scripts/operations_backup_restore_e2e.py --binary cms/target/release/opendash.exe
```
