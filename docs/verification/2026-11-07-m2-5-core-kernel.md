# Báo cáo kiểm chứng M2.5 — Bộ khung Lõi (Core Kernel) & Quy chuẩn Code

- **Ngày kiểm chứng:** `2026-11-07`
- **Môi trường:** Windows, Rust `1.99.1`, Node `22.16.0`, Python `3.13.4`, Chromium headless (`ms-playwright`).
- **Trạng thái:** `verified` cho phạm vi M2.5 theo [kế hoạch M2.5](../plans/2026-11-07-m2-5-core-kernel-and-standards.md) và hợp đồng [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md).

---

## 1. Thay đổi đã thực hiện

1. **Hợp đồng & Quy chuẩn Code (`docs/contracts/CORE_FOUNDATION.md`, `AGENTS.md`, `docs/README.md`):**
   - Ban hành danh mục Base APIs bắt buộc tái sử dụng (`opendash_core::{id, registry, hooks}`, `opendash_server::{package, auth, error, storage, hooks}`, `cms/admin/src/api/client.ts`, `cms/admin/src/components/FormPrimitives.tsx`).
   - Quy định 100% câu lệnh `sqlx::query*` của các luồng ứng dụng phải nằm trong `crates/opendash-server/src/storage/`.
2. **Core Kernel trong `crates/opendash-core`:**
   - Thêm `src/id.rs`: hợp nhất kiểm tra `SnakeKey`, `KebabId`, `NamespacedId`, `PackageId` và `SemVer`; cập nhật `content.rs`, `theme.rs`, `plugin.rs` gọi về `id.rs`.
   - Thêm `src/registry.rs`: triển khai `ResourceOwner` (`Core`, `User`, `Theme`, `Plugin`), `ProviderStatus`, `OwnedDescriptor<T>` và `RegistryCatalog<T>` bảo toàn schema/dữ liệu khi tắt plugin hoặc đổi theme.
   - Thêm `src/hooks.rs`: định nghĩa `HookEvent` (`PageAfterSave`, `ContentBeforeSave`, `ContentAfterSave`, `PluginManualRun`) và `HookEnvelope`.
3. **Server Kernel trong `crates/opendash-server`:**
   - Nâng cấp `src/error.rs` dùng `Cow<'static, str>` và các helper trả lỗi thống nhất.
   - Thêm `src/package.rs`: hợp nhất kiểm tra ZIP (`open_bounded_zip`, `ensure_safe_flat_entry`, `read_bounded_entry`, `is_valid_zip_content_type`) dùng chung cho `theme.rs`, `plugin/package.rs` và `routes/{builder,plugin}.rs`.
   - Thêm `src/storage/auth.rs` và `src/storage/plugin.rs`: chuyển toàn bộ truy vấn SQL từ `routes/auth.rs`, `auth.rs`, `plugin/manager.rs` và `routes/health.rs` về `storage/`.
   - Bổ sung guard kết hợp `require_session_with_csrf` và `require_admin_with_csrf` trong `src/auth.rs`; xóa file cầu nối dư `src/plugin/manifest.rs`.
   - Thêm `src/hooks.rs` (`HookDispatcher`) trong `AppState` để phát sự kiện vòng đời khi Lưu trang và điều phối chạy plugin.
4. **Admin Base trong `cms/admin/src/`:**
   - Nâng cấp `api/client.ts` với `requestText` và `requestJson` tự động đính kèm `X-CSRF-Token` và chuẩn hóa lỗi; cập nhật `api/{auth,builder,plugins}.ts`.
   - Tạo `components/FormPrimitives.tsx` (`FormGroup`, `ZipUploadField`, `SelectField`, `TextField`) và áp dụng cho `BuilderWorkspace.tsx` và `PluginsPanel.tsx` mà giữ nguyên các DOM selector.

---

## 2. Lệnh và kết quả kiểm chứng thực tế

| Lệnh / Kiểm tra | Kết quả | Chi tiết |
|---|---|---|
| Kiểm tra không còn `sqlx::query` trong `routes/`, `auth.rs`, `theme.rs`, `plugin/manager.rs` | `PASS` | Toàn bộ truy vấn SQL của các luồng trên đã chuyển vào `storage/{mod,page,auth,plugin}.rs` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | `PASS` | Không có chênh lệch định dạng |
| `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release` | `PASS` | Build Admin (`tsc && vite build`), plugin mẫu AssemblyScript và nhúng vào `opendash.exe` |
| `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | `PASS` | `10 passed` (`opendash-core`) + `33 passed` (`opendash-server`), gồm cả unit test mới cho `id`, `registry`, `hooks`, `package` |
| `python scripts/windows_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Cài ZIP theme, cách ly preview, Lưu và giữ dữ liệu qua restart |
| `python scripts/windows_auth_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Race setup, Argon2 burst, Origin, Auth/RBAC/CSRF, hết hạn phiên và nâng cấp từ `baseline_0001.sqlite` |
| `python scripts/windows_plugin_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | RBAC/CSRF/Origin, kiểm tra gói ZIP, chạy sample plugin, KV persistence qua restart và giới hạn kích thước body |
| `python scripts/windows_distribution_smoke.py --binary target/release/opendash.exe` (từ `cms/`) | `PASS` | Binary độc lập trong thư mục tạm, phục vụ asset nhúng, nâng cấp DB baseline và fail-fast khi thiếu lockfile |
| `python scripts/windows_admin_browser.py --browser <chrome-headless-shell.exe>` (từ `cms/`) | `PASS` | Kiểm thử trình duyệt: thiết lập Admin, đăng nhập, tải theme ZIP, preview chưa lưu, phục hồi dữ liệu đang nhập sau lỗi `401`, Lưu và chạy plugin |

---

## 3. Giới hạn và bước tiếp theo

- Chuỗi migrations `0001`–`0003` và fixture `baseline_0001.sqlite` được giữ nguyên hoàn toàn (chưa mở bảng mới cho CPT/Media).
- Trong tiến trình con thực thi Wasm (`plugin/host_ops.rs` và `plugin/runtime.rs`), truy vấn đọc/commit bảng `plugin_kv` theo deadline mili-giây của worker vẫn giữ nguyên tại chỗ để không làm thay đổi cơ chế `tokio::select!` hủy tức thời của M2.
- **Bước tiếp theo:** Mở kế hoạch và triển khai **M3 (Mô hình Nội dung CPT, Custom Fields, Taxonomy & Media)** dựa trực tiếp trên `opendash_core::{id, registry, hooks}`, `opendash_server::{storage, auth, error, package}` và `FormPrimitives` đã thiết lập ở M2.5.

