# M3-S20-T01 — Post/Page publication backend · Windows G5 verification

- **Ngày thực chạy:** 2026-11-11; **Windows verifier:** Forge (CoS Windows), theo yêu cầu trực tiếp của owner tại [Issue #24](https://github.com/thientran2359/opendash-cms/issues/24).
- **Phạm vi:** kiểm chứng Windows cho backend do Harbor thực hiện tại [PR #29](https://github.com/thientran2359/opendash-cms/pull/29), bổ sung [macOS evidence](2026-11-11-m3-s20-t01-post-page-publication-macos.md). Hợp đồng: [ADR-0004](../decisions/ADR-0004-m3-post-page-publication-boundary.md), quyền thực thi M3: [ADR-0005](../decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md).
- **Backend source được test:** `origin/main@605acdef6979f49e0ee30069c74bbaf47579996e`, đã chứa Harbor G4 và Forge AdminShell sau PR #31/#32. **Không thay đổi Rust/backend/migrations** trong lượt xác minh này.
- **Windows harness commit:** `6896286adcd30d85765a010d0f348fe557d60198` trên branch `task/forge-m3-s20-win-g5`. Thay đổi duy nhất ở 4 Python smoke scripts: đóng SQLite connection bằng `contextlib.closing` ngoài transaction context. Các HTTP/SQL/security assertions giữ nguyên.
- **Môi trường thực:** Microsoft Windows 11 Pro 10.0.26200 (build 26200), rustc/Cargo 1.99.1, Python 3.13.4, SQLite Python module 3.49.1, Node.js 22.16.0, npm 10.9.2, Rust debug binary `cms/target/debug/opendash.exe` build ngay trên Windows.
- **Dữ liệu kiểm thử:** mọi smoke test dùng `TemporaryDirectory` với `OPENDASH_DB_PATH`, `OPENDASH_SITE_ROOT` riêng và `OPENDASH_PORT=0` (random localhost port). Không dùng DB/site người dùng hoặc server Admin preview `127.0.0.1:18766`.

## Lệnh thực chạy

Chạy từ repository root trong worktree Windows nêu trên. `npm ci` và `npm run build` **phải chạy trước Cargo** vì `opendash-server` nhúng `cms/admin/dist` qua RustEmbed.

| Lệnh / kiểm chứng | Windows result |
| --- | --- |
| `npm ci --prefix cms/admin` | **PASS** — 83 packages added, 0 vulnerabilities reported. |
| `npm run typecheck --prefix cms/admin` | **PASS**. |
| `npm run build --prefix cms/admin` | **PASS** — Vite 6.4.6, 46 modules, `dist` generated. |
| `cargo fmt --manifest-path cms/Cargo.toml --all -- --check` | **PASS**. |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS** — 19 Core + 40 Server = **59 tests**, 0 failed; Doc-tests 0 failed. |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** — debug executable from tested backend source. |
| `python -m py_compile cms/scripts/content_types_smoke.py cms/scripts/content_entries_smoke.py cms/scripts/media_smoke.py cms/scripts/public_content_smoke.py` | **PASS**. |
| `python -X utf8 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash.exe` | **PASS**, chạy **hai lượt** sau harness fix; cả M3.6 legacy/public regression và M3-S20 Post/Page publication. |
| `python -X utf8 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite` | **PASS**, chạy **hai lượt** sau harness fix; Core seeds/CPT ownership/restart/upgrade. |
| `python -X utf8 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite` | **PASS** — CRUD/RBAC/CSRF/version-CAS/restart/upgrade. |
| `python -X utf8 cms/scripts/media_smoke.py --binary cms/target/debug/opendash.exe` | **PASS** — raster upload, metadata/thumbnail, auth and persistence. |
| `python -X utf8 cms/scripts/windows_smoke.py --binary cms/target/debug/opendash.exe` | **PASS** — legacy homepage/Builder Save and related HTTP behavior. |
| `python -X utf8 cms/scripts/windows_auth_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite` | **PASS** — auth/RBAC/CSRF/expiry/race/Origin/fixture migration. |
| `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test` | **PASS**, self-test 12 fixtures. |
| `python .agents/scripts/check-agent-artifacts.py`, `git diff --check` | **PASS**. |

## Nghiệm thu bảo mật và dữ liệu (HTTP/SQLite thật)

Các assertions của `public_content_smoke.py::test_post_page_publication` đều thực chạy và PASS trên backend Windows, gồm:

1. Cài mới seed `post`/`page` Core-owned; takeover trả 409; tạo bởi Admin/Editor bắt đầu `draft`. Guest single/archive và file/thumbnail draft-only trả 404; title/slug draft không xuất hiện trong archive, kể cả khi có 21 drafts mới hơn published item; giới hạn/offset lọc theo published.
2. Draft preview phải đăng nhập; Editor được phép preview persisted draft, có `Cache-Control: no-store`, CSP cấm script, escape title/rich text và image URL Admin đã xác thực. Không tự publish lúc Save.
3. Publish/Unpublish với `If-Match: "vN"`: publish bởi Editor thành công, ETag tăng version và public single/media trở thành 200; unpublish làm GET mới 404. Guest 401, thiếu CSRF hoặc Origin sai 403, thiếu `If-Match` 428, malformed status/version 400, stale CAS 412 và id không tồn tại 404. Save published giữ nguyên trạng thái published.
4. Media dùng chung SHA256 vẫn public khi một entry legacy published còn tham chiếu; draft gallery không được vượt qua điều kiện `published AND (image OR gallery)`; xóa reference cuối làm file/thumb quay về 404.
5. INSERT trực tiếp vào SQLite Post thiếu explicit `publication_status` bị trigger từ chối; pre-0008 database **copy** chứa reserved `post` user-owned hoặc Core descriptor sai bị preflight chặn **trước migration**, không rewrite hoặc ghi marker 0008.
6. Pre-0008 database copy hợp lệ nâng cấp bằng SQLx thật lên 0008, giữ nội dung legacy public, seed Core và tạo Post mới vẫn draft; restart giữ draft và legacy Builder homepage; các CPT provider-inactive vẫn giữ dữ liệu.

## Lỗi Windows tìm được và cách khắc phục

- **Lượt chạy ban đầu:** `cargo test`/build bị chặn vì worktree sạch chưa có `cms/admin/dist` (RustEmbed yêu cầu đường dẫn tồn tại). Sau `npm ci`, typecheck và Vite build, Rust tests/build **PASS**.
- **Lượt smoke ban đầu:** `public_content_smoke.py` kết thúc với `PermissionError [WinError 32]` khi `TemporaryDirectory` xóa `opendash.sqlite`. Đây là lỗi cleanup của Python test harness: `with sqlite3.connect(...) as con` chỉ commit/rollback transaction, **không gọi `con.close()`**. Vì thế handle SQLite vẫn tồn tại ở thời điểm Windows dọn directory.
- **Sửa tối thiểu:** bao SQLite connections trong `closing(...)`, giữ lại connection transaction context (`..., con`) để không đổi rollback/commit semantics. Áp dụng vào các smoke files của Issue; không thay SQL, API hoặc assertions. Sau sửa, `public_content_smoke.py` và `content_types_smoke.py` mỗi test chạy lại hai lần đều **PASS**, các smoke tests khác đều **PASS**.

## Phạm vi kết luận / giới hạn

- Windows backend/HTTP/SQLite migration và security slice `M3-S20-T01` có **bằng chứng runtime PASS**, bổ sung macOS Harbor. Việc chuyển **G5 VERIFIED / Project Done / Issue Closed** chỉ thực hiện sau khi PR Windows harness/evidence được merge, CI theo đúng PR head PASS và trạng thái Project được kiểm chứng.
- Không nghiệm thu giao diện CRUD Posts/Pages hay release build: thuộc task/slice riêng. Không thử browser GUI Publish form vì Issue #24 là backend API; HTTP thực bao phủ mọi chuyển trạng thái. Không đo performance; không dùng database production.
- Migration 0008 **forward-only**: Git revert không phục hồi schema cũ; backup/restore trên bản copy vẫn là điều kiện khi triển khai vào website thật. Unpublish chỉ chặn request mới, không thu hồi nội dung đã cache từ trước hoặc asset còn được một entry public khác tham chiếu.
- Tách biệt với [Issue #25 AdminShell Windows](2026-11-11-m3-s21-adminshell-windows.md) và server preview local đang chạy; **task G5** không đồng nghĩa toàn bộ milestone M3 đã G5.
