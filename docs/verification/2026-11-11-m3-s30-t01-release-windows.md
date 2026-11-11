# M3-S30-T01 · Windows release P19: harness chuẩn bị và smoke trước khi chốt SHA

**Task:** [#54](https://github.com/thientran2359/opendash-cms/issues/54) · OPS-02.2 · Forge · [ADR-0006 § P17–P19](../decisions/ADR-0006-m3-admin-and-release-contracts.md)

**Trạng thái ngày 2026-11-11:** Windows pre-candidate **PASS** theo phạm vi các lệnh dưới đây. **M3 release-wide G5 vẫn `PENDING`**: harness/evidence phải được merge trước khi pin cùng SHA với [macOS #53](https://github.com/thientran2359/opendash-cms/issues/53); #53 chưa có G5 trên cùng release SHA. Không dùng báo cáo này để đóng #54/M3 khi thiếu bằng chứng macOS hoặc khi SHA thay đổi.

## Môi trường và provenance

| Thuộc tính | Giá trị thực chạy |
| --- | --- |
| OS | Microsoft Windows 11 Pro 10.0.26200 (build 26200) |
| Browser | Google Chrome `157.0.8103.50`, `C:/Program Files/Google/Chrome/Application/chrome.exe` |
| Source lúc build và kiểm thử | `origin/main@326ef65a0f8fa482c99d7d8f5883023bfbc907f0` + các sửa đổi **test-only** trong branch `task/forge-m3-s30-t01` |
| Release profile | `cms/scripts/build.ps1 -Release` (npm ci/build Admin + sample Wasmi plugin, `cargo build --locked --release -p opendash-server`) |
| Executable | `cms/target/release/opendash.exe`, 14,969,344 bytes (14.28 MiB) |
| Release executable SHA256 | `15db47648e8cdaf8b5a8ffccb36e155f4b776378d7ef26f0da2e596a5e8e3655` |
| Historical binary | Unmodified binary from source `6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0`, post-0008/pre-0009, SHA256 `8ed2e0f8da046f858c219b05f45898434149fbc75900ba206ef3887fdda1f77d` |
| Fixture | `cms/fixtures/baseline_0001.sqlite`, copied using SQLite online backup; disposable `TemporaryDirectory` only |
| Release SQLx migration versions | `[1,2,3,4,5,6,7,8,9,10,11]` from `cms/migrations/*.sql` |
| Linux early smoke | `NOT_RUN` — no Linux runner for this Windows session; V1 Linux release belongs to M8 |

No live-site database, uploads or credentials were used. Browser instances, server/worker processes and SQLite handles were closed before the temporary sites were removed. The root `agent/forge` worktree has unrelated edits and was **not** changed by this task.

## Test-only changes

1. `cms/scripts/windows_admin_browser.py`: legacy Settings/`HTTP: ok` selector no longer exists on the M3 Dashboard. Assert the real heading and four empty counts instead; verify `/api/health` independently via HTTP. Existing Builder/401 recovery/Plugins assertions remain.
2. `cms/scripts/windows_distribution_smoke.py`: replace old hard-coded migration count `3` with exact successful SQLx versions read from `cms/migrations` of the release source, plus integrity/FK and failed-migration-record checks. Use `closing(...)` for SQLite even on assertion failures; reap server and close stdout/stderr pipes in `finally`.
3. `cms/scripts/windows_m3_release_e2e.py`: new release-only integrated Chrome/SQLite acceptance. One fresh isolated site covers login/bootstrap, Dashboard, Post/Page drafts and publication/412, media upload, Builder preview-vs-Save, Themes activation, Users, Settings 403/412/Origin/CSRF, Wasmi plugin, Editor restrictions, 390px, logout and real process restart. Separate pre-0008 and genuine post-0008 snapshots are upgraded to current release; originals are restored with **actual old binary**, not synthetic migration deletion. Exact migration versions, homepage row and health checked.

No Rust/runtime, production UI, SQLx migration, fixture or existing P18 harness was edited.

## Commands executed and results

All commands below were executed from `cms/` in the isolated task worktree (unless otherwise stated). `<OLD_BIN>` denotes the real historical executable above and `<CHROME>` denotes the full installed Chrome executable path.

| Test/command | Result |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1 -Release` | **PASS**. Embedded Admin: Vite 60 modules; sample Wasmi plugin ZIP built; Rust release built. |
| `cargo test --locked --release -q` | **PASS 67/67** (19 core + 48 server; 0 doctests). |
| `python -m py_compile scripts/windows_admin_browser.py scripts/windows_distribution_smoke.py scripts/windows_m3_release_e2e.py` | **PASS**. |
| `python scripts/windows_admin_browser.py --binary target/release/opendash.exe --browser <CHROME>` | **PASS**. Admin, Dashboard/health, Builder preview/save, unsaved 401 recovery, plugin worker. |
| `python scripts/windows_distribution_smoke.py --binary target/release/opendash.exe --baseline-db fixtures/baseline_0001.sqlite` | **PASS**. Standalone clean working directory without Node/Rust paths, embedded Admin JS/CSS and MIME, strict 404, RBAC/plugin, startup site root, exact SQLx 0001–0011, upgrade/restart, fail-fast build script. |
| `python scripts/windows_m3_release_e2e.py --binary target/release/opendash.exe --baseline-binary <OLD_BIN> --browser <CHROME>` | **PASS**. Real Chrome integrated site + historical SQLite upgrade/restore on pre-0008 and post-0008 copies. |
| `python scripts/content_types_smoke.py --binary target/release/opendash.exe --baseline-db fixtures/baseline_0001.sqlite` | **PASS** (P18 A0 1/4). |
| `python scripts/content_entries_smoke.py --binary target/release/opendash.exe --baseline-db fixtures/baseline_0001.sqlite` | **PASS** (P18 A0 2/4). |
| `python scripts/media_smoke.py --binary target/release/opendash.exe` | **PASS** (P18 A0 3/4). |
| `python scripts/public_content_smoke.py --binary target/release/opendash.exe` | **PASS** (P18 A0 4/4). |
| `python scripts/dashboard_browser.py --binary target/release/opendash.exe --browser <CHROME>` | **PASS**. Loading/zero/error/retry, role, keyboard, 390px. |
| `python scripts/windows_users_browser.py --binary target/release/opendash.exe --browser <CHROME>` | **PASS**. CRUD, password/role, stale 412, unsaved, 390px. |
| `python scripts/general_settings_browser.py --binary target/release/opendash.exe --browser <CHROME>` | **PASS**. Browser 412, keyboard, 390px, restart. |
| `python scripts/themes_browser.py --binary target/release/opendash.exe --browser <CHROME>` | **PASS**, with `PYTHONIOENCODING=utf-8`. A→B, A→C data retain, C→A restore, stale tab, 390px. |
| `python scripts/windows_plugins_browser.py --binary target/release/opendash.exe --browser <CHROME>` | **PASS**, with `PYTHONIOENCODING=utf-8`. UI ZIP, worker recovery, roles. |
| `python scripts/windows_auth_smoke.py --binary target/release/opendash.exe --baseline-binary <OLD_BIN>` | **PASS**. CSRF/Origin, RBAC, races, session revoke, last-admin, 503, real old-binary rollback. |
| `python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe` | **PASS**. Wasmi KV/restart, security, body size, package validation. |
| `python scripts/themes_smoke.py --binary target/release/opendash.exe --baseline-db fixtures/baseline_0001.sqlite` | **PASS**, with `PYTHONIOENCODING=utf-8`. Theme compatibility/rollback, 0011, invalid ZIP. |
| `python scripts/windows_kernel_smoke.py --binary target/release/opendash.exe` | **PASS**. Worker trap/recovery checks emit **expected** error logs. |

Every P18 harness returned exit code 0 and completed disposable directory cleanup. Some first-run *invocations* needed correction: `media_smoke.py` does not accept `--baseline-db`, so the final 4/4 run used its supported flags; `themes_browser.py` initially finished assertions but failed printing Unicode arrows under the default Windows `cp1252` stream, then returned exit 0 with `PYTHONIOENCODING=utf-8`. Neither case is a CMS behavior regression; no product assertion was relaxed.

## Các gate còn mở

- Merge harness/test-only PR and re-run the release verification against the **new integrated main**. The initial `326ef65` source is a pre-candidate reference, **not** a pinned final release SHA.
- Harbor #53 must run its own complete macOS P17 E2E/upgrade/snapshot evidence using the **same exact merged release candidate SHA**. If SHA changes, reassess and rerun affected acceptance on both OS.
- Check full G5 security/data, linked Project #3 status and blocking PRs before declaring release-wide `VERIFIED`. If macOS evidence is missing, keep #54 `In progress`/`In review`, M3 open.
- Linux early smoke: explicitly `NOT_RUN` without runner; no false Linux acceptance. Linux V1 release/backup engine are M8.

**Conclusion:** Windows release smoke P19 and four A0 regressions **PASS** on the pre-candidate binary. **Milestone M3 closure: NOT YET VERIFIED.**
