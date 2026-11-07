# Báo cáo kiểm chứng M2.5 — Bộ khung Lõi (Core Kernel) & Quy chuẩn Code

- **Ngày kiểm chứng:** `2026-11-07`
- **Môi trường:** Windows, Rust `1.99.1`, Node `22.16.0`, Python `3.13.4`, Chromium headless (`ms-playwright`).
- **Đường dẫn mã:** `C:\Users\Thientran\Local Sites\opendash-cms\cms`
- **Trạng thái:** `verified` cho toàn bộ phạm vi M2.5 (gồm cả 8 mục hoàn thiện trong [kế hoạch M2.5](../plans/2026-11-07-m2-5-core-kernel-and-standards.md), quyết định [ADR-0002](../decisions/ADR-0002-core-extension-contract.md) và hợp đồng [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md)).

---

## 1. Hoàn thiện 8 mục M2.5 (Đợt 2 — Khép kín bộ khung lõi & kiểm tra quy chuẩn)

Sau đợt triển khai đầu (giữ nguyên bằng chứng lịch sử ở Mục 3 bên dưới), toàn bộ 8 mục hoàn thiện trong kế hoạch M2.5 đã được triển khai và kiểm chứng trên mã thực:

| Mục kế hoạch | Nội dung thay đổi chính | Trạng thái | Bằng chứng kiểm chứng |
|---|---|---|---|
| **4.1. Đồng bộ contract & Base APIs thật** | Đồng bộ bảng Base APIs trong `CORE_FOUNDATION.md` với chữ ký thực tế (`valid_*`, `validate_package_id`, `validate_semver` trong `opendash_core::id`; `open_bounded_zip`, `ensure_safe_flat_entry`, `read_bounded_entry`, `is_valid_zip_content_type` trong `opendash_server::package`; `TextInput` trong `FormPrimitives.tsx`); thắt chặt `validate_semver` từ chối đoạn rỗng/số `0` đứng đầu ở pre-release/build. | `verified` | `id::tests::validates_package_id_and_semver`, `check-standards.py` |
| **4.2. Chốt quy chuẩn tổ chức code** | Ban hành [ADR-0002](../decisions/ADR-0002-core-extension-contract.md) và Mục 4 `CORE_FOUNDATION.md`; tách tầng nghiệp vụ `opendash_server::page` (`render_block`, `save_page`) khỏi `routes/builder.rs` và `routes/public.rs`. | `verified` | `page::tests::save_commits_before_hooks_and_keeps_data_after_handler_failure`, `check-standards.py` |
| **4.3. Hoàn thiện Registry có quyền sở hữu** | Bổ sung `RegistryError` (`AlreadyRegistered`, `OwnerConflict`, `NotFound`), tách `RegistryCatalog::register` và `RegistryCatalog::update` (giữ nguyên `ProviderStatus`); tích hợp `storage::page::load_block_catalog` dựng `RegistryCatalog<BlockDefinition>` từ SQLite (`theme_blocks`) độc lập với dữ liệu trang (`home_page`). | `verified` | `registry::tests::rejects_owner_takeover_and_keeps_inactive_status_on_update`, `windows_kernel_smoke.py` |
| **4.4. Hoàn thiện Hook/Action chạy thực tế** | Nâng cấp `HookEnvelope` (`occurred_at_ms`, serde rename canonical); bổ sung `PluginManifest.hooks` tùy chọn (`page/after_save`); triển khai `HookDispatcher::register_handler`, `dispatch` (thứ tự `(priority, id)`, timeout core `250ms`, bắt panic đồng bộ/bất đồng bộ, chặn gọi lồng `IN_DISPATCH`, ghi `plugin_logs`/`last_error`) và `dispatch_plugin_run`. | `verified` | `hooks::tests::ordered_handlers_continue_after_failure_and_timeout`, `plugin::hook_tests`, `windows_kernel_smoke.py` |
| **4.5. Đưa luồng hiện có qua bộ lõi** | Kết nối `routes/builder.rs::save` → `page::save_page` → `storage::page` → `HookDispatcher::dispatch(HookEvent::PageAfterSave, ...)`; kết nối `routes/plugin.rs::plugin_run` → `HookDispatcher::dispatch_plugin_run`. | `verified` | `page::tests`, `windows_kernel_smoke.py`, `windows_smoke.py`, `windows_admin_browser.py` |
| **4.6. Khép các điểm chưa thống nhất của Base APIs** | Chuyển toàn bộ SQL/PRAGMA/transaction của `plugin/host_ops.rs` và `plugin/runtime.rs` về `storage/plugin_kv.rs` mà giữ nguyên deadline, cancellation và `StagedCommitGuard`; áp dụng `ensure_safe_flat_entry` (chặn symlink/path traversal/dir) cho `theme.rs`; dùng `requestJson` cho `getHealth` và `TextInput` cho `AuthPanel`/`UsersPanel`. | `verified` | `plugin::review_regressions::*`, `plugin::tests::*`, `windows_kernel_smoke.py` |
| **4.7. Mẫu mở rộng & kiểm tra quy chuẩn** | Tạo `cms/scripts/check-standards.py` (kèm `--self-test` tạo repo tạm cố ý vi phạm SQL, I/O core, dependency core, CSRF guard và `fetch` ngoài `api/client.ts`) và tài liệu hóa mẫu mở rộng trong `CORE_FOUNDATION.md`. | `verified` | `python cms/scripts/check-standards.py` & `--self-test` (`PASS`) |
| **4.8. Kiểm chứng & chốt M2.5** | Chạy toàn bộ unit test, `check-standards.py`, `windows_kernel_smoke.py` và 5 bộ smoke test hồi quy trên Windows; cập nhật chỉ mục docs CodeGraph. | `verified` | Bảng kết quả Mục 2 bên dưới |

---

## 2. Lệnh và kết quả kiểm chứng thực tế (Sau khi hoàn thiện 8 mục M2.5)

| Lệnh / Kiểm tra | Kết quả | Chi tiết |
|---|---|---|
| `python cms/scripts/check-standards.py` | `PASS` | `PASS: core, storage, route and Admin boundaries` (0 câu lệnh `sqlx::query*`/`raw_sql` ngoài `storage/`; `opendash-core` không chứa I/O hay framework; mọi route mutation có session dùng CSRF guard; mọi `fetch` nằm trong `api/client.ts`) |
| `python cms/scripts/check-standards.py --self-test` | `PASS` | `PASS: standards check rejects SQL, I/O, dependency, guard and fetch violations` (xác nhận exit code `1` cho từng vi phạm giả lập trong thư mục tạm) |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | `PASS` | Không có chênh lệch định dạng |
| `cargo build --manifest-path cms/Cargo.toml --locked` | `PASS` | Build dev workspace khớp `Cargo.lock` |
| `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release` | `PASS` | Build Admin (`tsc && vite build`), plugin mẫu AssemblyScript và nhúng vào `cms/target/release/opendash.exe` |
| `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | `PASS` | `12 passed` (`opendash-core`) + `34 passed` (`opendash-server`), bao gồm unit test cho `id`, `registry`, `hooks`, `page`, `package` và toàn bộ hồi quy cancellation/zero-late-writes của M2 |
| `python scripts/windows_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Cài ZIP theme, cách ly preview, Lưu và giữ dữ liệu qua restart |
| `python scripts/windows_auth_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Race setup, Argon2 burst, Origin, Auth/RBAC/CSRF, hết hạn phiên và nâng cấp từ `baseline_0001.sqlite` |
| `python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | RBAC/CSRF/Origin, kiểm tra gói ZIP, chạy sample plugin, KV persistence qua restart và giới hạn kích thước body |
| `python scripts/windows_kernel_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Từ chối ZIP theme chứa symlink (`400`), phát hook `page/after_save` tới plugin Wasm đã đăng ký sau khi Lưu (không phát khi Preview), cô lập lỗi khi plugin trước trap (`a-failing` ghi `last_error` và `plugin_logs` cùng `correlation_id`, `b-listener` vẫn chạy), tắt/bật lại plugin, giữ tương thích chạy thủ công (`c-manual`) và bảo toàn `RegistryCatalog`/trang qua restart |
| `python scripts/windows_distribution_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Binary độc lập trong thư mục tạm, phục vụ asset nhúng, nâng cấp DB baseline và fail-fast khi thiếu lockfile |
| `python scripts/windows_admin_browser.py --binary target/release/opendash.exe --browser <chrome-headless-shell.exe>` (từ `cms/`) | `PASS` | Kiểm thử trình duyệt: thiết lập Admin, đăng nhập, tải theme ZIP, preview chưa lưu, phục hồi dữ liệu đang nhập sau lỗi `401`, Lưu và chạy plugin |
| `node cms/scripts/codegraph-index-docs.mjs` | `PASS` | Cập nhật chỉ mục tài liệu của CodeGraph sau khi sửa `CORE_FOUNDATION.md` |
| `git diff --check` | `PASS` | Sạch lỗi khoảng trắng và định dạng |

---

## 3. Bằng chứng lịch sử của đợt triển khai đầu (07/11/2026)

Phần dưới đây giữ nguyên báo cáo thực chạy của đợt triển khai đầu (`1062fe8`) trước khi bổ sung 8 mục hoàn thiện ở Mục 1–2:

1. **Thay đổi đợt đầu:**
   - Ban hành phiên bản đầu của `docs/contracts/CORE_FOUNDATION.md`, `opendash_core::{id, registry, hooks}`, `opendash_server::{package, error, hooks}` và `storage/{auth,plugin}.rs`.
   - Bổ sung `require_session_with_csrf`, `require_admin_with_csrf` trong `src/auth.rs`; xóa file cầu nối dư `src/plugin/manifest.rs`.
   - Nâng cấp `cms/admin/src/api/client.ts` (`requestText`, `requestJson`) và tạo `components/FormPrimitives.tsx`.
2. **Kết quả chạy đợt đầu:**
   - `cargo fmt --manifest-path cms/Cargo.toml --all --check`: `PASS`.
   - `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release`: `PASS`.
   - `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked`: `PASS` (`10 passed` ở `opendash-core` + `33 passed` ở `opendash-server`).
   - 5 bộ smoke test (`windows_smoke.py`, `windows_auth_smoke.py`, `windows_plugin_smoke.py`, `windows_distribution_smoke.py`, `windows_admin_browser.py`): `PASS`.

---

## 4. Giới hạn hiện tại và bước tiếp theo

- Chuỗi migrations `0001`–`0003` và fixture `baseline_0001.sqlite` được giữ nguyên hoàn toàn (không mở bảng mới trong M2.5).
- Hook ở M2.5 chỉ thực thi thông báo chỉ đọc sau commit (`PageAfterSave`) và điều phối chạy thủ công (`PluginManualRun`). Các sự kiện `ContentBeforeSave` và `ContentAfterSave` đã được định nghĩa trong `HookEvent` nhưng được `HookDispatcher` từ chối cho tới khi triển khai mô hình nội dung ở M3.
- Chưa có lỗi mở trong phạm vi M2.5.
- **Bước tiếp theo:** Mở kế hoạch và triển khai **M3 (Mô hình Nội dung CPT, Custom Fields, Taxonomy & Media)** tuân thủ trực tiếp `CORE_FOUNDATION.md`, `ADR-0002` và kiểm tra tự động `check-standards.py`.
