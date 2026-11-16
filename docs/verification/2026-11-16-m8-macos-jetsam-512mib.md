# M8 — Native macOS operations-worker 512 MiB cap (Harbor)

**Date:** 2026-11-16 · **Platform:** macOS 15.8.2 (24H232), Intel `x86_64` · **Task context:** [#188](https://github.com/thientran2359/opendash-cms/issues/188), [#246](https://github.com/thientran2359/opendash-cms/issues/246), M8-S03-T01 / M8 operations security contract.

**Source baseline:** `main@ed85c9bd2c786379d7caf51f0fd4b7a5a1d2bd10`; task branch `fix/harbor-m8-macos-native-memory-cap`. This patch does not change the immutable Windows/Mac M6 source pin `4841a0b8c49d868ac452a538728ad463f11eae0b`; changes require a new common-source re-verification before any cross-OS G5 closure. Project owner directly requested the M8 memory limit fix. Do not claim release VERIFIED from a branch or documentation merge.

## Root cause and security decision

`backup_worker.rs` previously applied `setrlimit(RLIMIT_AS, 512 MiB)` in its worker process. On this Mac, both `RLIMIT_AS` and `RLIMIT_DATA` limits fail with `EINVAL (22)`; this blocked M8 snapshot/restore and the full Rust workspace, **correctly fail-closed** under [M8 operations contract](../contracts/M8_OPERATIONS_RELEASE.md). Mac Darwin aliases `RLIMIT_AS` with `RLIMIT_RSS`, unlike Linux. Using an allocator heap limit, dropping the check, or merely skipping tests would not satisfy the contract.

Native proof: a local `clang`-compiled isolated C probe called the macOS `posix_spawnattr_setjetsam_ext` function (fatal active and inactive flags `0x04 | 0x08`, both limits `512` MiB). The function returned success; the spawned child touching 600 MiB of allocated physical pages exited by **SIGKILL (9)** before completion, rather than surviving past the declared limit. The probe had no access to a live site or database.

Production patch: `operations_worker_process.rs` now uses macOS `posix_spawn` with fatal Jetsam limits set **before exec** and three scoped UnixStream channels (stdin request, stdout result, bootstrap proof). On other platforms it retains `std::process::Command`, Windows Job Object and Linux `RLIMIT_AS`. All parent-side snapshot/archive/restore paths use the same helper; macOS unguarded CLI worker entry refuses to process a request. Spawn attribute is resolved dynamically; if the private macOS symbol is unavailable or setting it fails, child launch is **rejected** rather than running without the cap. Existing cancellation still kills and joins the exact OS PID.

**Important limits:** `posix_spawnattr_setjetsam_ext` is an Apple *private* API, so its continued behavior is not guaranteed by a public SDK contract. The bootstrap FD proof distinguishes the trusted launch path but is not a cryptographic/kernel attestation of limits. The native **stress test**, not the bootstrap proof, establishes actual enforcement on this OS. Jetsam constrains physical footprint rather than Linux virtual address-space reservation; it is an OS-native allocation bound, not byte-for-byte the same resource accounting. Native Apple Silicon and other macOS releases: **NOT_RUN**. They must pass a stress fixture before G5 acceptance; otherwise block operations on those platforms. Do not treat CI on Ubuntu/Windows as a Mac native test.

## Native validation

All commands run on the task branch using a user-owned, `0700` `TMPDIR` under `opendash-tasks/.harbor-m8-native-test-tmp` (outside `cms/target` and macOS `/var` symlink) and a shared ignored `CARGO_TARGET_DIR`. Fixtures and SQLite databases are isolated from user site files.

| Check | Actual result |
| --- | --- |
| `npm --prefix cms/admin ci --no-audit --no-fund && npm --prefix cms/admin run build` | **PASS**, Vite 89 modules |
| `cargo check --manifest-path cms/Cargo.toml -p opendash-server --bin opendash` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_macos_jetsam_stress --locked -- --nocapture` | **PASS 2/2**: production spawn helper, 300 MiB pages survive, 600 MiB pages get native SIGKILL; direct unguarded worker rejected |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_s03_t01_snapshot --locked -- --nocapture` | **PASS 1/1**: snapshot, external DB, manifest, missing-file fail-closed |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_review_fixes --locked -- --nocapture` | **PASS 2/2**: cancellation/recovery and proxy origin |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m6_s08_t01_plugin_e2e --locked -- --nocapture` | **PASS 1/1**: plugin ZIP, content, builder, scoped egress, restart, matching binary snapshot/restore |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_s04_t01_backup_codec --locked --quiet` | **PASS 1/1**: archive/verify codec |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_s04_t02_backup_admin --locked --quiet` | **PASS 1/1**: HTTP Admin backup, download, verify |
| `cargo test --manifest-path cms/Cargo.toml -p opendash-server --test m8_s05_t01_restore --locked --quiet` | **PASS 1/1**: isolated restore and recovery validation |
| **`cargo test --manifest-path cms/Cargo.toml --workspace --locked --quiet`** | **PASS exit 0**, **no skipped/filter tests**, core 38/38, server 88/88, all integration suites (including the three M8 worker-mode tests and native Jetsam stress) 0 failures |
| Python 3.11.18: `cms/scripts/check-standards.py`, `--self-test`, `cms/scripts/check-plans.py`, `--self-test`, `.agents/scripts/check-agent-artifacts.py`; `git diff --check` | **PASS**, exit 0 |

After final Mac unit/E2E reruns on the stabilized implementation, the debug single-binary `opendash` is Mach-O `x86_64` with SHA-256 **`cbc4e6d0affe591d70df5b2d53a63aa60feb2d368cb42e5ea0e28d1270b0cba9`**. Rust/Cargo: 1.99.1, Node 24.22.2; the same `TMPDIR` and `CARGO_TARGET_DIR` were used for full-workspace and final focused checks. All Mac native successes are confined to **this Intel host and this source patch**, not automatically to release packaging or Apple Silicon. Re-run M8 native stress and Windows/macOS cross-OS G5 on an immutable merged source SHA before updating #188/#246 to Done.
