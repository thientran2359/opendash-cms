# M3-S24-T01 · Users Admin — Windows verification

**Ngày:** 2026-11-11 · **Agent:** Forge (CoS Windows) · **Issue:** [#48](https://github.com/thientran2359/opendash-cms/issues/48) · **Task:** M3-S24-T01.

**Trạng thái lúc triển khai:** \`IMPLEMENTED\`. Các nhóm test ban đầu ghi bên dưới không tự chứng minh G5 từ việc merge. **Task-level G5 đã VERIFIED sau khi thực hiện hậu kiểm riêng trên Windows**, xem phần cuối. Nghiệm thu release toàn M3 vẫn độc lập.

## Baseline và môi trường

- Source/G3 đã phát hành: \`d5d206afa75256e51ad52fd60ba8d9f966873e4e\`; base code: \`6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0\`, branch \`task/forge-m3-s24-t01\`.
- Windows 11 Pro; Rust/Cargo 1.99.1; Node 22.16.0; npm 10.9.2; Python 3.13.4; Chrome 157.0.8103.50.
- Compiler profile \`dev\`; Admin \`vite build\` được nhúng bằng RustEmbed trong \`target/debug/opendash.exe\`.
- SQLite thật qua file \`site.sqlite\` tạm, WAL/foreign keys theo runtime; server chạy \`OPENDASH_PORT=0\`, \`OPENDASH_SITE_ROOT\`/DB riêng từng fixture.
- Contract: [ADR-0006](../decisions/ADR-0006-m3-admin-and-release-contracts.md#p12--users-fnd-02fnd-05-full-track); [G3 Users](../plans/milestones/M3/slices/2026-11-11-m3-users-settings-g3.md); migration tăng tiến \`0009_users_version.sql\`.

## Hành vi được triển khai

- \`GET /api/admin/users?limit&offset\` phân trang, đọc \`items,total\` trong một snapshot; \`POST /api/admin/users\` tạo Admin hoặc Editor (mặc định Editor cho client cũ).
- \`PATCH /api/admin/users/{id}\`, \`POST /api/admin/users/{id}/password\`, \`DELETE /api/admin/users/{id}\` dùng \`If-Match: "vN"\` và mã 428/400/412 tương ứng. Summary chỉ gồm \`id,username,role,version\`; private responses \`no-store\`.
- Transaction SQLite lấy write reservation trước khi kiểm tra actor/session, target/version và số lượng Admin. Khi thay đổi username, role, mật khẩu hoặc xóa user, tất cả session của target được thu hồi trong cùng transaction. Bảo vệ Admin cuối cùng với 409.
- Login hash/verify bên ngoài lock; tạo session bằng lệnh \`INSERT ... SELECT\` có điều kiện đúng \`users.id/version\`. Login bằng hash cũ khi reset đồng thời không thể để lại session hợp lệ sau reset.
- Màn Users dành riêng cho Admin có danh sách, phân trang không ghi đè trang mới bằng response cũ, thao tác tạo/sửa/reset/xóa, confirmation, stale-form error/retry giữ nguyên input, bàn phím và layout 390 px. Editor không thực hiện Users API ở nền.

## Lệnh thực chạy và kết quả

Chạy tại repository root:

| Lệnh | Kết quả |
| --- | --- |
| \`npm ci --prefix cms/admin\` | **PASS**, 83 packages, audit 0 vulnerabilities |
| \`npm run build --prefix cms/admin\` | **PASS**, TypeScript và Vite production |
| \`cargo check --manifest-path cms/Cargo.toml --locked\` | **PASS** sau khi Admin dist đã có |
| \`cargo build --manifest-path cms/Cargo.toml --locked\` | **PASS**, Windows debug binary |
| \`cargo fmt --manifest-path cms/Cargo.toml --all --check\` | **PASS** |
| \`cargo test --manifest-path cms/Cargo.toml --locked\` | **PASS**, opendash_core **19/19**, opendash-server **42/42**, doc tests 0; thêm kiểm tra SQLite extended-lock codes và stale-login conditional insert sau reset commit |
| \`python cms/scripts/check-standards.py\` | **PASS** |
| \`python cms/scripts/check-plans.py\` | **PASS** |
| \`python cms/scripts/check-plans.py --self-test\` | **PASS**, 12 fixtures |
| \`git diff --check\` | **PASS** |

Chạy từ thư mục \`cms/\` (đường dẫn binary là tương đối):

| Lệnh | Kết quả |
| --- | --- |
| \`python scripts/windows_auth_smoke.py --binary target/debug/opendash.exe --baseline-binary ../../opendash-cms-m3-s24-rollback-base/cms/target/debug/opendash.exe\` | **PASS**: bootstrap, Argon2 concurrency, Origin/CSRF/RBAC, legacy Editor POST, Users CRUD, CAS, ID invalid 400, last-Admin, session revocation, concurrent demotions/deletes, login/reset race, SQLite write lock 503, restart, upgrade/rollback |
| \`python scripts/windows_users_browser.py --binary target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"\` | **PASS**: Chrome thật, desktop và viewport 390×844, CRUD/role/delete, stale 412 reload/retry giữ draft và giữ draft sau 401/re-login cùng tài khoản, reset, keyboard/focus, Editor không gọi Users API, self-change/logout |
| \`python scripts/windows_admin_browser.py --binary target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"\` | **PASS**: UI hồi quy Builder (preview draft/Save/session 401 recovery) và Plugins ZIP sau khi điều hướng đúng màn Admin |
| \`python scripts/content_types_smoke.py --binary target/debug/opendash.exe\` | **PASS**: auth/CSRF, validation, persistence, upgrade |
| \`python scripts/content_entries_smoke.py --binary target/debug/opendash.exe\` | **PASS**: CRUD, RBAC, CSRF, CAS, restart, upgrade |
| \`python scripts/public_content_smoke.py --binary target/debug/opendash.exe\` | **PASS**: archive/single, theme, XSS, media ACL, pagination, restart |

### Kiểm tra schema upgrade và rollback

Fixture giả lập DB **trước 0009** bằng chính schema và journal \`0008\`, chứa hai người dùng Admin/Editor với Argon2 hash và hai session thật; snapshot SQLite bằng online backup. Binary mới áp dụng migration 0009 một lần và giữ nguyên \`id\`, \`username\`, \`role\`, \`password_hash\`, các session/token đã có; \`PRAGMA foreign_key_check\` sạch; đăng nhập bằng mật khẩu cũ vẫn hoạt động.

Một worktree tạm ở \`6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0\` được build độc lập với \`npm ci\`, \`npm run build\`, \`cargo build --locked\`. Mở **snapshot pre-0009** bằng binary baseline này, kiểm \`/auth/me\` của hai session và login của hai role: **PASS**. Snapshot không chứa version và không có migration \`0009\`. Không thực hiện migration ngược lên database đã nâng cấp.

## Giới hạn và kiểm chứng ngoài phạm vi

- Lần chạy \`windows_admin_browser.py\` trước khi sửa harness **FAIL** vì kiểm tra \`HTTP: ok\` trên Dashboard thay vì Settings. Forge ghi [Design Delta trên Issue #48](https://github.com/thientran2359/opendash-cms/issues/48) trước khi mở rộng ownership duy nhất cho \`cms/scripts/windows_admin_browser.py\`: sửa bước điều hướng Settings/Builder/Plugins, không đổi runtime. Sau sửa, chính script này **PASS** trên cùng binary Windows.
- Không khẳng định macOS, Linux hoặc Windows release build đã được kiểm trong task này. CI của PR được theo dõi riêng sau khi push, trước khi chuyển G5.
- Test concurrent Admin demotion và concurrent Admin delete chạy qua hai request/connection cùng lúc; không đại diện cho mọi thứ tự scheduler của hệ điều hành. Test Rust buộc thứ tự \`login đọc version cũ → reset commit → conditional insert\` và xác nhận INSERT thất bại. Test write-lock giữ \`BEGIN IMMEDIATE\` từ SQLite connection khác cho đến khi API trả 503, sau đó kiểm dữ liệu không bị ghi muộn.

## Hậu kiểm G5 trên Windows — 2026-11-11

- **Phạm vi:** riêng task Users `M3-S24-T01` / [Issue #48](https://github.com/thientran2359/opendash-cms/issues/48), không phải release G5 của toàn M3.
- **Code PR:** [#56](https://github.com/thientran2359/opendash-cms/pull/56), head `93d1f59439c0ffd78fcf2185b5823a3434b0b7d2`, merge SHA `cef71f3bc534dbe28ff6719d00f30cf8327f0f33`. [GitHub Governance run 42967716364](https://github.com/thientran2359/opendash-cms/actions/runs/42967716364) Ubuntu/Windows **SUCCESS** tại head code PR.
- **Mã sản phẩm hậu kiểm:** worktree sạch tích hợp `origin/main@51aae56ed232e4d7be881b7baab01977c5881d11` (Users, Settings, Plugins, Dashboard). Các merge tài liệu G5 #61/#62 sau đó không thay bất kỳ file `cms/` nào; `git diff --name-only 51aae56..081e08d -- cms` rỗng. Binary Windows `cms/target/debug/opendash.exe` được build lại từ source đúng SHA đó với Admin Vite đã nhúng.
- **Môi trường:** Windows 11 Pro, Rust/Cargo 1.99.1, Node 22.16.0, Python 3.13.4, Chrome 157.0.8103.50; site root, SQLite, port loopback riêng theo từng test; không chạm dữ liệu thật.

| Lệnh / test hậu kiểm thực chạy | Kết quả |
| --- | --- |
| `npm ci --prefix cms/admin`; `npm run build --prefix cms/admin` | **PASS** — 83 packages, Vite 58 modules |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check`; `cargo build --manifest-path cms/Cargo.toml --locked`; `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS 65/65 Rust** — 19 core + 46 server; các race/session/timeout tests |
| `cd cms; python scripts/windows_users_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** — Chrome desktop/390px, user CRUD, role, reset, stale 412, 401 draft recovery, Editor guard, keyboard |
| `cd cms; python scripts/windows_admin_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** — Builder/session recovery và Plugins regression |
| `cd cms; python scripts/content_types_smoke.py --binary target/debug/opendash.exe` | **PASS** — schema/CSRF/validation, legacy upgrade |
| `cd cms; python scripts/content_entries_smoke.py --binary target/debug/opendash.exe`; `public_content_smoke.py`; `general_settings_smoke.py --baseline fixtures/baseline_0001.sqlite` | **PASS** — auth/RBAC, CRUD, publication/public ACL, Settings CAS, upgrade và restart |
| `cd cms; python scripts/windows_auth_smoke.py --binary <integrated-binary> --baseline-binary <actual-pre-0009-binary>` | **PASS** sau sửa fixture test-only — Auth/CSRF/Origin, race, last Admin, session revoke, SQLite lock 503, pre-0009 upgrade và old-binary snapshot restore |
| `cd cms; python scripts/windows_auth_smoke.py --binary <integrated-binary>` | **PASS** — đường fixture tổng hợp không có baseline binary, chỉ cho phép migration 0009/0010 đã biết |
| Python `py_compile`, `check-plans.py`, `check-plans.py --self-test`, `check-standards.py`, `git diff --check` | **PASS** — gồm 12 planning fixtures |

### Sửa sai lệch fixture kiểm thử, không sửa CMS

Trong lượt hậu kiểm đầu tiên, `windows_auth_smoke.py` tạo database giả lập pre-0009 từ binary mới (đã có Settings migration 0010) nhưng chỉ xóa `users.version` và journal 0009. Bản snapshot vẫn có journal 0010, khiến binary trước 0009 trả `VersionMissing(10)` một cách **đúng và an toàn**; đây không phải lỗi migration sản phẩm. Chi tiết đã báo trên [Issue #48](https://github.com/thientran2359/opendash-cms/issues/48).

Sửa duy nhất harness `cms/scripts/windows_auth_smoke.py`: khi được cung cấp genuine baseline binary tại `main@6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0`, tạo fixture và các session ngay bằng binary cũ (schema thật trước 0009), snapshot SQLite, nâng cấp bằng binary hiện hành, so hash/session dữ liệu nguyên vẹn, rồi **mở snapshot lại bằng chính binary cũ**. Khi không có baseline binary, đường giả lập chỉ được xóa hai migration 0009/0010 trên DB **tạm**, kiểm không còn `site_general_settings` và từ chối schema mới chưa biết thay vì âm thầm tạo backup không thể khôi phục. Hai đường smoke đều **PASS** sau chỉnh sửa; không sửa migration trong repo, code runtime hay database thật.

**Verdict: task G5 VERIFIED trên Windows cho M3-S24-T01 Users.** Các trường hợp last-Admin, revoked session và rollback đã có evidence chạy thật sau merge. **Không được suy ra** M3 release G5, macOS parity, hay đóng Issue #53/#54; các gate này phải chạy release candidate chung sau khi Themes tích hợp.
