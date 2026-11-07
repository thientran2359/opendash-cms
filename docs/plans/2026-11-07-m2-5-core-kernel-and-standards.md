# Kế hoạch M2.5 — Bộ khung Lõi (Core Kernel) & Quy chuẩn Code

- **Ngày lập:** `2026-11-07`
- **Trạng thái:** `verified` — xem [báo cáo kiểm chứng M2.5](../verification/2026-11-07-m2-5-core-kernel.md).
- **Hợp đồng liên quan:** [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md) · [PLUGIN_ABI.md](../contracts/PLUGIN_ABI.md) · [CMS_ARCHITECTURE.md](../architecture/CMS_ARCHITECTURE.md) · [PRODUCT_REQUIREMENTS.md](../product/PRODUCT_REQUIREMENTS.md)

---

## 1. Bối cảnh và mục tiêu

Sau khi nhập M1 (Auth/Builder 1 block) và M2 (Plugin Wasmi) vào workspace 2 crate ([báo cáo kiểm chứng](../verification/2026-11-07-m1-m2-import.md)), mã nguồn đã chạy ổn định nhưng tồn tại các điểm lệch chuẩn giữa hai giai đoạn:
- Kiểm tra định danh (`id`, `key`, `semver`) viết riêng ở từng file (`content.rs`, `theme.rs`, `plugin.rs`).
- Truy vấn SQLite nằm rải rác trong `routes/auth.rs`, `auth.rs`, `plugin/manager.rs` và `storage/page.rs`.
- Giải nén và kiểm tra gói ZIP tách rời giữa `theme.rs` và `plugin/package.rs`.
- Xử lý lỗi HTTP (`ApiError`) chỉ nhận `&'static str`, khiến route plugin phải tự tạo response lỗi riêng.
- Chưa có cơ chế `Registry` gắn quyền sở hữu (`ResourceOwner`) và `HookDispatcher` trung tâm để chuẩn bị cho M3 (CPT/Media) và M4 (Builder nhiều tầng).

Mốc **M2.5** xây dựng bộ khung CMS cơ bản và áp dụng quy chuẩn trong [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md) lên toàn bộ mã M1/M2 trước khi mở M3.

---

## 2. Phạm vi và phần ngoài phạm vi

### Trong phạm vi
1. Ban hành [CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md) và cập nhật [AGENTS.md](../../AGENTS.md), [docs/README.md](../README.md).
2. Xây dựng 3 module nền trong `opendash-core`:
   - `id.rs`: chuẩn hóa `validate_snake_key`, `validate_kebab_id`, `validate_namespaced_id`, `validate_semver`.
   - `registry.rs`: định nghĩa `ResourceOwner`, `ProviderStatus`, `OwnedDescriptor<T>` và `RegistryCatalog<T>`.
   - `hooks.rs`: định nghĩa `HookEvent`, `HookEnvelope` và tên sự kiện chuẩn.
3. Chuẩn hóa `opendash-server` theo đúng phân tầng:
   - Nâng cấp `error::ApiError` dùng `Cow<'static, str>`.
   - Tạo `package::SafeZipArchive` dùng chung cho đọc/kiểm tra ZIP của Theme và Plugin.
   - Đưa toàn bộ câu lệnh `sqlx::query*` của Auth và Plugin về `storage/auth.rs` và `storage/plugin.rs`.
   - Bổ sung guard kết hợp `require_session_with_csrf` và `require_admin_with_csrf` trong `auth.rs`.
   - Tạo `hooks::HookDispatcher` trong `AppState` để điều phối sự kiện hệ thống và thực thi plugin.
4. Chuẩn hóa `cms/admin/src/`:
   - Bổ sung helper `requestJson` / `requestText` kèm CSRF trong `api/client.ts`.
   - Tạo `components/FormPrimitives.tsx` và áp dụng cho Builder/Plugin UI mà không đổi DOM selector.

### Ngoài phạm vi
- Không thay đổi schema SQLite hiện có (`0001`–`0003`) và không đổi byte của `fixtures/baseline_0001.sqlite`.
- Chưa thêm bảng hay màn hình quản trị CPT/Media (thuộc M3).
- Không thay đổi giao thức nhị phân Wasm trong [PLUGIN_ABI.md](../contracts/PLUGIN_ABI.md).

---

## 3. Các bước thực hiện (Slices)

1. **Slice 1 — Tài liệu & Quy chuẩn:** Tạo `docs/contracts/CORE_FOUNDATION.md`, `docs/plans/2026-11-07-m2-5-core-kernel-and-standards.md`, cập nhật `AGENTS.md` và `docs/README.md`.
2. **Slice 2 — Core Kernel (`opendash-core`):** Triển khai `id.rs`, `registry.rs`, `hooks.rs`; hợp nhất logic kiểm tra trong `content.rs`, `theme.rs`, `plugin.rs` và viết unit test đầy đủ.
3. **Slice 3 — Server Kernel (`opendash-server`):** Nâng cấp `error.rs`, tạo `package.rs`, `storage/auth.rs`, `storage/plugin.rs`, `hooks.rs`, cập nhật `auth.rs`, `theme.rs`, `plugin/*` và `routes/*`.
4. **Slice 4 — Admin Base (`cms/admin`):** Cập nhật `api/client.ts`, `api/{auth,builder,plugins}.ts`, tạo `components/FormPrimitives.tsx` và kết nối vào UI hiện tại.

---

## 4. Tiêu chí nghiệm thu

- Không còn `sqlx::query` hay `sqlx::query_as` trong `routes/`, `auth.rs` hay `plugin/manager.rs`.
- `cargo fmt --manifest-path cms/Cargo.toml --all --check` đạt `PASS`.
- `powershell -NoProfile -ExecutionPolicy Bypass -File cms/scripts/build.ps1 -Release` đạt `PASS`.
- `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` đạt `PASS` (kèm các unit test mới của Core Kernel và Package).
- Cả 5 bộ kiểm thử tự động trên Windows (`windows_smoke.py`, `windows_auth_smoke.py`, `windows_distribution_smoke.py`, `windows_plugin_smoke.py`, `windows_admin_browser.py`) đạt `PASS`.
- `git diff --check` sạch lỗi định dạng.

