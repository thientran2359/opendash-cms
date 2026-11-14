# M8 — G1 operations/release source inventory

**Agent:** Atlas · **Date:** 2026-11-14 · **Baseline:** `main@8c5b90072c231c1f0813eb593d7be4447fb0a11d` (migration0031).
**Scope:** all8 M8 candidates; source/official documentation inspection, not an implementation benchmark or native test.
Promotion: [ADR-0015](../../docs/decisions/ADR-0015-m8-operations-release.md), [normative contract](../../docs/contracts/M8_OPERATIONS_RELEASE.md), [full G3](../../docs/plans/milestones/M8/slices/2026-11-14-m8-operations-release-g3.md). Canonical authority takes effect on merged docs PR.

## Actual source evidence

CodeGraph MediaStore symbol query returned0 nodes/unindexed workspace; used rg and actual source. No absence claim inferred solely from the index.

| Candidate | Source inspected | Finding / required work |
| --- | --- | --- |
| M8-P08 | `media.rs::MediaStore`, `storage/media.rs::find/save/require_existing_references_in_tx`, `storage/public_content.rs::media_is_referenced`, migrations0006/0027/0030/0031 | Image/video IDs, canonical paths, public ACL projections exist. Public visibility is NOT complete deletion reachability: drafts, retained provider values, hidden AST/overrides and opaque plugin values need conservative pins. No delete journal/reconcile API. |
| M8-P01 | `paths.rs::SitePaths`, `storage/mod.rs::init_db`, `storage/import.rs::execute_sample_import/recover_orphaned_import_runs`, `storage/theme.rs`, `storage/plugin.rs` | DB can be outside site root; WAL enabled; immutable package revisions and separate stage/commit phases. No instance root/DB lease or shared snapshot barrier. SQLite snapshot alone does not snapshot filesystem writes. |
| M8-P01/P02 | `secrets.rs::SecretStore`, `storage/smtp.rs::get_smtp_settings/create_initial_attempt_in_conn`, media/package stores | SMTP secrets are private plaintext files with DB references, including attempt snapshots. Full backup must include those files but never expose plaintext artifacts or passwords in manifest/log/API. |
| M8-P02/P03 | `package.rs::ensure_safe_flat_entry/open_bounded_zip`, `app.rs`, Admin App/api modules | Current ZIP reader consumes bounded package bytes and flat entries. Reuse flat-path validation with a file-backed bounded backup profile; do not raise Theme/Plugin package caps. No Admin backup/restore UI or safe restore engine. |
| M8-P04 | `main.rs`, `app.rs` Host/Origin layer, `paths.rs`, workspace/server Cargo manifests | Embedded Admin already exists. Listener currently IPv4 loopback/port; guard trusts listener host and plain-http listener Origin. Linux reverse-proxy authority, graceful shutdown, instance locking and safe upgrade need explicit contract. No current release pipeline/install unit. |
| M8-P05/P06 | M7 E2E harness/browser runner, Windows report, current governance workflow | M7#202–#213 closed, Windows report pinned `a633ad84d2f03023eba66305cc884f288df2ad28`; M7 final/macOS #214/#215 open. Governance checks do not build/run CMS on Linux. No Linux/performance or backup/restore proof. |
| M8-P07 | PRD, M8 plan, repo root file inventory | Product license remains owner decision; no root license file found in inspected tracked source. Do not choose legal/commercial terms. Release must wait explicit owner license decision and dependency notice review. |

## Alternatives

1. **Selected:** cooperative instance leases + explicit shared writer barrier; immutable private snapshot; encrypted file-backed archive; Admin restores a separate site copy; Linux release with loopback behind configured proxy. Costs a bounded writer pause but keeps rollback understandable.
2. Copy live SQLite/WAL/files independently: rejected; filesystem/package/secret state can mismatch DB. Holding SQLite transaction while running workers/network also rejected.
3. Hot-overwrite active root/external DB: rejected for V1; Windows open handles and multi-volume DB/files cannot be replaced atomically by one rename. Restore-to-new-root is already the M8-P03 acceptance, with explicit operator activation.
4. Plain ZIP/manual secret omission: rejected; omission breaks recovery, plaintext downloads leak private state. External age/sqlite3/tar executable requirement rejected; use embedded Rust libraries.

## Primary documentation inspected

- [SQLite snapshot backup](https://www.sqlite.org/backup.html) and [VACUUM](https://www.sqlite.org/lang_vacuum.html): database-consistent copies do not guarantee external file consistency. Select VACUUM INTO via existing SQLx0.9.0 under the application barrier, not live main-file copy. SQLx [connection docs](https://docs.rs/sqlx/0.9.0/sqlx/struct.SqliteConnection.html) distinguish close and allocation-heavy serialization; do not use whole-DB serialize for streaming backup.
- [age0.11.2](https://docs.rs/age/0.11.2/age/) implements the age file format. [Streaming encryptor](https://docs.rs/age/0.11.2/age/struct.Encryptor.html) requires finish; [scrypt recipient](https://docs.rs/age/0.11.2/age/scrypt/struct.Recipient.html) and [identity](https://docs.rs/age/0.11.2/age/scrypt/struct.Identity.html) expose work-factor controls. Select exact0.11.2, mandatory passphrase encryption, fixed bounded KDF. New dependency compilation/security corpus belongs implementation; no cryptographic protocol invented here. Upstream pre1.0 status is not a production audit claim.
- [Rust File locking](https://doc.rust-lang.org/stable/std/fs/struct.File.html): advisory platform locking, process/handle lifetime matter. Local rustc inspected1.99.1; implementation pins that toolchain and verifies Windows/Linux/macOS semantics before relying on locks. Exact1.99.1 documentation URL unavailable through web tool; do not infer its behavior from latest docs alone.
- [systemd execution](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html): explicit working/state directories, user and sandbox parameters. Use directives supported by the selected Ubuntu24.04 systemd, not latest-only directives without verification. Linux execution remains NOT_RUN.

## Resolved G2 questions

Contract fixes all technical decisions: writer admission/lease rules; conservative media pins and journal recovery; managed-file manifest and external DB rebasing; encrypted archive limits/passphrase lifecycle; safe new-root restore with session invalidation/no external effects; proxy/bind trust; pinned Linux packaging, finite measured acceptance and common-source native release evidence. No scheduled backup/GC, public deployment, existing milestone closure or runtime PASS follows from this research.

Worker bounds references: [Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) and [Linux resource limits](https://man7.org/linux/man-pages/man2/getrlimit.2.html). Selected existing libc0.2.191/native windows-sys0.61.2 APIs for worker memory/termination guards; native enforcement still needs stress tests. Local Node reports22.16.0; release build pins it with Rust1.99.1 and dependency audit/manifest, no deployed Node runtime.
