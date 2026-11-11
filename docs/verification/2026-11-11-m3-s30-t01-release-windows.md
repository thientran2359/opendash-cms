# M3-S30-T01 · Windows release P19: pre-merge smoke và G5 final parity

**Task:** [#54](https://github.com/thientran2359/opendash-cms/issues/54) · OPS-02.2 · Forge · [ADR-0006 § P17–P19](../decisions/ADR-0006-m3-admin-and-release-contracts.md)

**Trạng thái cuối ngày 2026-11-11: M3 G5 VERIFIED / Milestone #4 CLOSED.** Bảng test bên dưới ghi **lượt pre-merge ban đầu** từ nhánh Forge. Bằng chứng **post-merge Windows**, kết quả macOS cùng release source, đối chiếu G5, việc đóng [Issue #54](https://github.com/thientran2359/opendash-cms/issues/54) và [milestone #4](https://github.com/thientran2359/opendash-cms/milestone/4) được ghi ở mục cuối. Không suy kết quả cùng source hoặc kết luận release từ một lần smoke pre-merge.

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

## Các gate từng còn mở trước khi nghiệm thu release

- Merge harness/test-only PR and re-run the release verification against the **new integrated main**. The initial `326ef65` source is a pre-candidate reference, **not** a pinned final release SHA.
- Harbor #53 must run its own complete macOS P17 E2E/upgrade/snapshot evidence using the **same exact merged release candidate SHA**. If SHA changes, reassess and rerun affected acceptance on both OS.
- Check full G5 security/data, linked Project #3 status and blocking PRs before declaring release-wide `VERIFIED`. If macOS evidence is missing, keep #54 `In progress`/`In review`, M3 open.
- Linux early smoke: explicitly `NOT_RUN` without runner; no false Linux acceptance. Linux V1 release/backup engine are M8.

**Kết luận của lượt pre-merge (lịch sử):** Windows P19 và A0 PASS trên pre-candidate; lúc đó M3 G5 chưa có chứng cứ macOS. Mục dưới đây thay thế trạng thái đó.

## G5 cuối cùng — Windows/macOS parity và M3 closure

**Pinned release source cho M3:** `d2b6bfe1e7b6c2f6895802b8ed609292747d3062` (sau khi [PR #67](https://github.com/thientran2359/opendash-cms/pull/67) merge). Đây là **cùng commit nguồn M3** mà cả Forge Windows và Harbor macOS dùng cho nghiệm thu độc lập; hash binary hai hệ điều hành khác nhau là bình thường. Không dùng SHA của nhánh test-only thay cho mã sản phẩm.

### Windows — Forge, kiểm chứng thực tế sau merge

- Windows 11 Pro 10.0.26200, Chrome 157.0.8103.50; `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1 -Release` **PASS**, Admin React (Vite 60 modules) và Wasmi worker nhúng vào executable release.
- `cms/target/release/opendash.exe`: 14,969,344 bytes; **SHA256 `85be89f684a57c49dc8f45905ff87c4006c53ffc1b251f192d650d5db34300e9`** tại đúng `main@d2b6bfe`. Old binary thật `6d6f4b5` SHA256 `8ed2e0f8da046f858c219b05f45898434149fbc75900ba206ef3887fdda1f77d`.
- `cargo test --locked --release -q`: **67/67 PASS**; `windows_m3_release_e2e.py`: **PASS** Chrome Admin/SQLite/Wasmi/auth/roles/CAS/CSRF/Origins, migration 0001–0011, fresh và pre-0008/post-0008 upgrade, restore bằng old binary thật, homepage intact.
- `windows_distribution_smoke.py` và `windows_admin_browser.py`: **PASS**. Bốn A0 P18 (`content_types_smoke.py`, `content_entries_smoke.py`, `media_smoke.py`, `public_content_smoke.py`): **4/4 PASS**. Năm Chrome regressions (Dashboard/Users/Settings/Themes/Plugins): **5/5 PASS**. Bốn backend regressions (Auth/Plugin/Themes/Kernel): **4/4 PASS**. Dùng site root và SQLite/assets tạm; không đụng website thực.
- [PR #67 required governance CI run 43001734590](https://github.com/thientran2359/opendash-cms/actions/runs/43001734590): **Windows SUCCESS, Ubuntu SUCCESS** trên exact PR head. [Báo cáo Windows sau merge tại Issue #54](https://github.com/thientran2359/opendash-cms/issues/54#issuecomment-6793174349).

### macOS — Harbor, kiểm chứng độc lập cùng source

- [Issue #53](https://github.com/thientran2359/opendash-cms/issues/53) đã **G5 VERIFIED**, Project Done, closed completed. macOS Intel 15.8.2, Chrome 157.0.8103.51, native release Rust **67/67 PASS**, full Chrome/SQLite E2E + **11/11 regression groups PASS**; snapshot fingerprint SQLite và media file original/thumbnail SHA256, upgrades pre/post-0008 và rollback bằng binary lịch sử thật **PASS**.
- Mac task harness/evidence [PR #68](https://github.com/thientran2359/opendash-cms/pull/68) merge tại `46f58e696283d6aef09c46f3ecaa4525b6823086`; Ubuntu/Windows [CI run 43004733652](https://github.com/thientran2359/opendash-cms/actions/runs/43004733652) **SUCCESS**. Post-merge macOS tiếp tục PASS; mã production M3 không thay đổi từ release candidate `d2b6bfe`. Chi tiết tại [macOS release G5 evidence](2026-11-11-m3-s29-t01-release-macos.md) và [Harbor signed comment](https://github.com/thientran2359/opendash-cms/issues/53#issuecomment-6793609366).

### Đối chiếu nguồn và kết luận

- Tại thời điểm kiểm gate, `origin/main@de24e4286bb4c2b6433595f297edd82603269fd0` bổ sung macOS harness/evidence và tài liệu M4 G1–G3/Task Index **không có thay đổi production M3**: `git diff --name-only d2b6bfe..de24e42 -- cms/admin/src cms/crates cms/migrations cms/plugins cms/Cargo.lock` trả rỗng. Vì vậy G5 gắn với **release source đã được kiểm thật trên cả hai OS**, không tuyên bố kiểm M4. `check-plans.py`, `--self-test` (12 fixtures), `check-standards.py`, `git diff --check` **PASS** trên `main@de24e42`.
- Sau xác minh 3 acceptance checklist và Project #3, [Issue #54](https://github.com/thientran2359/opendash-cms/issues/54) **Done / closed completed**; [GitHub Milestone M3 #4](https://github.com/thientran2359/opendash-cms/milestone/4) **CLOSED, 100%**; chi tiết sign-off ở [Forge G5 comment](https://github.com/thientran2359/opendash-cms/issues/54#issuecomment-6793877113).
- Linux early smoke được ghi **NOT_RUN** (không có runner trong phiên Windows), không được tính là PASS. Linux V1 distribution/backup engine nằm trong **M8**, không phải blocker của phạm vi M3 đã duyệt.

**Final verdict: M3 G5 VERIFIED / CLOSED, 2026-11-11. Agent: Forge (CoS Windows).**
