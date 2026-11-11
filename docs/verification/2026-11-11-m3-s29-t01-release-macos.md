# M3-S29-T01 · Nghiệm thu M3 release trên macOS (P17)

**Task:** [#53](https://github.com/thientran2359/opendash-cms/issues/53) · Harbor (CoS macOS) · OPS-02.1 · [ADR-0006 P17/P19](../decisions/ADR-0006-m3-admin-and-release-contracts.md)

**Kết quả P17 trên macOS:** **PASS** trên cùng source release candidate với kết quả Windows của Forge: `d2b6bfe1e7b6c2f6895802b8ed609292747d3062`. Báo cáo này chỉ xác nhận các phép thử thực chạy theo phạm vi P17. Nghiệm thu milestone M3/Project G5 cần đối chiếu [P19 #54](https://github.com/thientran2359/opendash-cms/issues/54) và trạng thái chính thức riêng; merge harness không tự động đóng milestone.

## Nguồn và môi trường

| Mục | Giá trị thực chạy |
| --- | --- |
| Source release SHA | `d2b6bfe1e7b6c2f6895802b8ed609292747d3062` (merge Windows harness [PR #67](https://github.com/thientran2359/opendash-cms/pull/67)) |
| Task worktree | `task/harbor-m3-s29-t01`, checkout từ đúng source SHA trên; chỉ thêm harness và evidence |
| macOS | 15.8.2, Intel x86_64, Darwin |
| Browser thực | Google Chrome `157.0.8103.51` tại `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` |
| Python / Playwright | CPython `3.13.18` (uv venv riêng), Playwright `1.55.0` |
| Admin build | `npm ci --prefix cms/admin`; `npm run build --prefix cms/admin`: 60 modules PASS |
| Release build | `cargo build --manifest-path cms/Cargo.toml --locked --release`, nhúng Admin đã build và Wasmi plugin |
| Release binary | `cms/target/release/opendash-m3-d2b6bfe`; SHA256 `0917c856829063a07108dc83478ab6271a402a834db0975220e6feeb497d9a9a` |
| Binary rollback thật | Build `cargo --locked --release -p opendash-server` từ `6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0` (post-0008/pre-0009); SHA256 `063854a7e484e0b09a8b1fe1ad909b7cadbfb4971119fc7ed60d90749c904f9d` |
| Historical fixture | Bản sao SQLite online từ `cms/fixtures/baseline_0001.sqlite`; bản sau migration 0008 được tạo bằng binary lịch sử thật |
| Site/DB dùng để thử | `TemporaryDirectory(prefix="opendash-m3-macos-release-")` và các site tạm độc lập của smoke/browser scripts; `OPENDASH_PORT=0` |
| Linux | **NOT_RUN**; V1 Linux distribution thuộc M8 |

Tài khoản, media, database và thư mục `uploads` của website đang chạy không được dùng. Lệnh kiểm thử dùng temporary site roots; tất cả subprocess/browsers và kết nối SQLite được đóng trước khi dọn dữ liệu thử.

## Lệnh và kết quả

Các lệnh dưới đây chạy từ root của worktree Harbor; `<RELEASE>` là file `cms/target/release/opendash-m3-d2b6bfe`, `<OLD>` là file `cms/target/release/opendash-m3-pre0009`, `<CHROME>` là Chrome path ghi ở trên. Node `v24.22.2` và Rust/Cargo được thêm vào `PATH` của lệnh build riêng, không sửa PATH hệ thống.

| Lệnh / nhóm | Kết quả | Bằng chứng |
| --- | --- | --- |
| `npm ci --prefix cms/admin` và `npm run build --prefix cms/admin` | **PASS** | TypeScript/Vite, 60 modules |
| `cargo build --manifest-path cms/Cargo.toml --locked --release` | **PASS** | Binary macOS release, Admin embedded |
| `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked -q` | **PASS 67/67** | 19 `opendash-core`, 48 `opendash-server`; 0 doctests |
| `.venv/bin/python cms/scripts/macos_m3_e2e.py --binary <RELEASE> --baseline-binary <OLD> --browser <CHROME>` | **PASS, exit 0** | Fresh integrated Chrome + SQLite/assets integrity; historical upgrade/rollback; 11 regression groups |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** | Rust formatting |
| `.venv/bin/python cms/scripts/check-plans.py` và `--self-test` | **PASS** | Links/IDs/evidence; 12 fixtures |
| `.venv/bin/python cms/scripts/check-standards.py` | **PASS** | Architecture rule checker |
| `.venv/bin/python -m py_compile cms/scripts/macos_m3_e2e.py`; `git diff --check` | **PASS** | Python syntax và whitespace |

### Một site thực: browser, publication và dữ liệu

Hàm `browser_flow` được tái sử dụng từ harness Windows [P19](../../cms/scripts/windows_m3_release_e2e.py), chạy **trên Mac bằng binary được build tại Mac và Chrome cài trên Mac**, cùng SQLite mới. Kết quả **PASS** cho bootstrap/login; Dashboard zero/published/draft counts; tạo, xuất bản Post/Page và CAS 412; upload media và Media page; Builder preview chưa lưu so với public Save; Themes activation; tạo Editor/Users; Settings 403 (CSRF/Origin), 412 (stale); Wasmi plugin ZIP/run; 390px Editor 403 và UI ẩn chức năng Admin; logout 401 và restart.

Sau khi server dừng, harness macOS kiểm tra SQLite `PRAGMA integrity_check=ok`, `PRAGMA foreign_key_check=[]`, và migration thành công `[1..11]`. Dữ liệu persisted gồm: `home_page=1`, `content_entries=2` (Post và Page ở `published`), `media_assets=1`, `users=2`, `plugins=1`, `plugin_kv=1`, `site_general_settings=1`, `site_active_theme=1`; active theme `sample/banner`.

SQLite online snapshot + bản sao **cả media gốc lẫn thumbnail** được thực hiện trên site thử. Snapshot khôi phục có cùng row-count và SHA256 fingerprint của tất cả hàng thuộc `home_page`, `theme_blocks`, `content_types`, `content_entries`, `media_assets`, `users`, `plugins`, `plugin_kv`, `site_general_settings`, `site_active_theme`. Hai file ảnh được đối chiếu bằng SHA256 và `foreign_key_check` vẫn sạch. Đây là kiểm tra bản sao đồng bộ cả dữ liệu lẫn asset, không phải triển khai backup engine V1.

### Upgrade và restore với binary lịch sử

Hàm `historical_sqlite_upgrade` dùng hai nguồn: fixture pre-0008 từ baseline `0001`, và bản post-0008 được **chính binary lịch sử `6d6f4b5`** nâng lên `0008`. Mỗi nguồn được SQLite online backup sang bản riêng trước khi chạy binary release `d2b6bfe`. Cả hai nâng lên đủ migration `0001..0011`, health 200, `home_page` giữ nguyên từng field. Snapshot gốc được restore sang **một database khác** rồi mở bằng binary lịch sử thật; cả hai đều PASS với nội dung homepage `Baseline Title` và health 200. Không mở file đã nâng lên `0011` bằng binary cũ.

### Hồi quy bảo mật và UX riêng biệt

Hàm `run_regressions` trong [macOS harness](../../cms/scripts/macos_m3_e2e.py) gọi 11 script hiện hữu trên các site tạm và mọi lệnh trả exit 0:

| Suite | Kết quả |
| --- | --- |
| `windows_auth_smoke.py` với `--baseline-binary` thật | **PASS** — RBAC, session revoke, race login/reset, last Admin, concurrent setup, CSRF/Origin, 503, old-binary rollback |
| `public_content_smoke.py` | **PASS** — draft/publication, private media ACL, XSS/CSP, template escaping, restart |
| `windows_plugin_smoke.py` | **PASS** — ZIP/permission/body boundary, CSRF/RBAC, Wasmi invocation, KV và restart |
| `themes_smoke.py` | **PASS** — A→B compatible, A→C ẩn/giữ dữ liệu, C→A khôi phục, CAS và migration 0011 |
| `windows_users_browser.py` | **PASS** — Chrome Users desktop/390px, stale draft, keyboard, role và password/session |
| `general_settings_browser.py` | **PASS** — Chrome Settings 412, draft preservation, responsive, restart |
| `themes_browser.py` | **PASS** — Chrome A/B/C, preview chưa lưu, 390px, restart |
| `windows_plugins_browser.py` | **PASS** — Chrome plugin permissions, replace, retry, worker error UI, Editor restriction, 390px |
| `dashboard_browser.py` | **PASS** — Chrome aggregates/loading/503/retry, shortcuts, role, 390px |
| `media_smoke.py` | **PASS** — file validation, thumbnail, auth, references, restart |
| `windows_kernel_smoke.py` | **PASS** — lỗi Wasmi trap được cố tình gây ra, hook isolation, disable/re-enable, recovery |

Plugin browser suite giả lập `last_error` để thử điều khiển UI; nó không tự gây timeout worker thật. Wasmi runtime lỗi/trap và worker isolation được xác minh riêng trong Rust/kernel tests. Các log `Guest trap` từ kernel smoke là expected negative case.

## Rủi ro, giới hạn và handoff

- Evidence này là nghiệm thu **macOS P17** tại source `d2b6bfe`. Windows P19 đã được Forge test lại tại cùng SHA (xem [comment #54](https://github.com/thientran2359/opendash-cms/issues/54#issuecomment-6793174349)). Hash binary Windows và macOS khác nhau là bình thường vì target khác OS.
- Bản rollback chứng minh trên hai fixture SQLite lịch sử và bản sao assets tạm. Chưa kiểm tra dữ liệu website thực, backup engine tổng quát hay Linux distribution.
- Mọi source production (`cms/admin`, Rust, migrations, Wasmi ABI) giữ nguyên; thay đổi của PR #53 chỉ là macOS harness + báo cáo này. Nếu release source đổi, phải repin và chạy lại acceptance cần thiết trên cả hai OS.
- **Post-merge G5 trên macOS:** [PR #68](https://github.com/thientran2359/opendash-cms/pull/68) với head `d669eb59761012930f680779a1024667bb4af657` đã qua [GitHub Actions run 43004733652](https://github.com/thientran2359/opendash-cms/actions/runs/43004733652) (**Ubuntu/Windows SUCCESS**) và merge tại `46f58e696283d6aef09c46f3ecaa4525b6823086`. Harbor tạo clean checkout đúng `main@46f58e6`, build lại Admin (60 modules) và Rust release, chạy Rust **67/67 PASS**, full `macos_m3_e2e.py` **exit 0 PASS** gồm 11 suites, SQLite + media snapshot và pre/post-0008 genuine old-binary restore. Binary post-merge macOS SHA256 `00556521d6675291cd5f44d04b68580ba5d7c61ad63c188bc10c302bf7378798`; mã sản phẩm vẫn đúng release source `d2b6bfe`. Cargo fmt, check-plans, 12 self-tests, standards và diff check PASS. Kết quả đã đăng trong [Issue #53 G5 comment](https://github.com/thientran2359/opendash-cms/issues/53#issuecomment-6793609366), ba checklist hoàn thành, Project #3 Status **Done**, Issue đóng **completed**. **P17 task G5 VERIFIED**; quyết định **M3 milestone G5** và Forge #54 được quản lý riêng.

Agent: Harbor (CoS macOS).
