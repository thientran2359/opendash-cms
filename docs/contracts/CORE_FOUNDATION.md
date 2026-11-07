# OpenDash — Hợp đồng Bộ khung Lõi (Core Kernel) & Quy chuẩn Code (v1)

- **Phiên bản:** `1.0`
- **Ngày hiệu lực:** `2026-11-07`
- **Trạng thái:** `AGREED` (Quy chuẩn bắt buộc cho M2.5 và toàn bộ các mốc M3–M6).
- **Tài liệu liên quan:** [Yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md) · [Kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) · [Plugin ABI v1](PLUGIN_ABI.md) · [ADR-0001](../decisions/ADR-0001-workspace-layout.md)

---

## 1. Mục đích

Tài liệu này định nghĩa **Bộ khung Lõi (Core Kernel)** của OpenDash — đóng vai trò tương đương hệ thống hàm gốc (`functions`), bộ đăng ký (`register_post_type`, `register_block_type`, `add_option`) và hệ thống sự kiện (`add_action` / `do_action`) của WordPress — đồng thời quy định **Quy chuẩn Code bắt buộc** để mọi giai đoạn phát triển (M2.5, M3, M4, M5, M6) dùng chung một chuẩn duy nhất, không viết lặp hàm hoặc phá vỡ ranh giới kiến trúc.

---

## 2. Danh mục Base APIs bắt buộc tái sử dụng (Single Source of Truth)

Trước khi thêm bất kỳ tính năng, route, bảng dữ liệu hay màn hình Admin nào, developer và AI bắt buộc tra bảng dưới đây và gọi đúng module gốc. **Nghiêm cấm viết lại các hàm tương đương ở module khác.**

| Nhu cầu kỹ thuật | Module gốc bắt buộc dùng | Quy định chi tiết |
|---|---|---|
| Kiểm tra định danh, slug, khóa schema, mã gói, SemVer | `opendash_core::id` | Dùng `validate_snake_key`, `validate_kebab_id`, `validate_namespaced_id`, `validate_semver`. Không tự viết hàm `valid_*` riêng lẻ trong từng module. |
| Quản lý tài nguyên có chủ sở hữu (CPT, Field, Block, Template, Setting) | `opendash_core::registry` | Gắn `ResourceOwner` (`Core`, `User`, `Theme(id)`, `Plugin(id)`) và dùng `OwnedDescriptor<T>`. Khi đổi theme hoặc tắt plugin, giữ nguyên schema và dữ liệu theo quy tắc `ResourceOwner`. |
| Định nghĩa & phát sự kiện vòng đời / thao tác hệ thống | `opendash_core::hooks` & `opendash_server::hooks` | Khai báo sự kiện trong `HookEvent` / `HookEnvelope` (`opendash-core`) và phát qua `HookDispatcher` (`opendash-server`). Không gọi rời rạc từ route vào từng plugin ngoài luồng dispatcher. |
| Đọc & kiểm tra an toàn gói ZIP (Theme, Plugin, Backup) | `opendash_server::package` | Dùng `SafeZipArchive` để kiểm tra kích thước nén/giải nén, số lượng mục, chặn path traversal (`..`, `/`, `\`), symlink và thư mục con; kết hợp `SitePaths::staging_dir` khi cần hoán đổi trên đĩa. |
| Quản lý đường dẫn runtime của website | `opendash_server::paths::SitePaths` | Mọi đường dẫn `database/`, `themes/`, `plugins/`, `uploads/`, `backups/`, `runtime/staging/` phải lấy từ `SitePaths`. |
| Xác thực phiên, kiểm tra quyền (`Admin`/`Editor`) và CSRF | `opendash_server::auth` | Route đọc dữ liệu dùng `require_session` hoặc `require_admin`. Route thay đổi dữ liệu (`POST`/`PUT`/`DELETE`/`PATCH`) bắt buộc dùng `require_session_with_csrf` hoặc `require_admin_with_csrf`. |
| Trả lỗi HTTP từ server | `opendash_server::error::ApiError` | Dùng `ApiError` (`Cow<'static, str>`) và `ApiResult<T>` cho cả thông báo lỗi tĩnh lẫn động. |
| Truy vấn cơ sở dữ liệu SQLite | `opendash_server::storage::*` | **100% câu lệnh `sqlx::query*` của ứng dụng phải nằm trong `crates/opendash-server/src/storage/`.** Cấm viết SQL trực tiếp trong `routes/`, `auth.rs`, `theme.rs` hoặc `plugin/manager.rs`. |
| Gọi HTTP API từ React Admin | `cms/admin/src/api/client.ts` | Dùng `requestJson`, `requestText` và `ApiClientError`. Bảo toàn dữ liệu đang nhập trên form khi gặp mã `401`. |
| Thành phần giao diện Form & điều khiển cơ bản | `cms/admin/src/components/FormPrimitives.tsx` | Tái sử dụng `FormGroup`, `TextField`, `SelectField`, `ZipUploadField` trước khi tạo component mới. |

---

## 3. Quy chuẩn Định danh (Identifier Standards)

OpenDash phân biệt rõ 3 nhóm định danh trong `opendash_core::id`:

1. **`SnakeKey` (`[a-z][a-z0-9_]*`):**
   - Dùng cho: định danh CPT (`post`, `portfolio_item`), tên trường dữ liệu (`field_key` như `title`, `hero_image`), tên thuộc tính nội bộ.
   - Quy tắc: bắt đầu bằng chữ cái thường ASCII (`a-z`), chỉ chứa `a-z`, `0-9`, `_`; độ dài mặc định `1..=64` (hoặc giới hạn con như `1..=32` cho field key của block M1).
2. **`KebabId` (`[a-z0-9-]`):**
   - Dùng cho: `plugin_id` (`sample-plugin`), `theme_id` (`sample-theme`), tên ngắn của block (`hero-banner`), URL slug.
   - Quy tắc: chỉ chứa `a-z`, `0-9`, `-`, không bắt đầu hoặc kết thúc bằng `-`.
3. **`NamespacedId` (`<namespace>/<name>`):**
   - Dùng cho: định danh block (`sample-theme/hero-banner`, `core/heading`) và tên hook/action (`page/after_save`, `content/before_save`).
   - Quy tắc: gồm đúng 2 phần ngăn cách bởi `/`; mỗi phần tuân theo quy tắc `KebabId` hoặc `SnakeKey` tương ứng với loại tài nguyên.

---

## 4. Hợp đồng Registry & Quyền sở hữu Dữ liệu (`opendash_core::registry`)

Để đảm bảo yêu cầu cứng trong [PRODUCT_REQUIREMENTS.md](../product/PRODUCT_REQUIREMENTS.md) (*"Đổi theme hoặc tắt plugin giữ các CPT, fields và nội dung đã nhập để tiếp tục quản lý; xóa dữ liệu là thao tác riêng"*):

1. **Chủ sở hữu tài nguyên (`ResourceOwner`):**
   - `Core`: thành phần mặc định của CMS (ví dụ: trang, bài viết cơ bản).
   - `User`: CPT, custom fields, danh mục do quản trị viên tạo qua giao diện Admin.
   - `Theme(String)`: block, template, Theme Options hoặc CPT/fields do gói theme khai báo.
   - `Plugin(String)`: CPT, fields, block, setting hoặc hook do plugin khai báo.
2. **Trạng thái vòng đời (`ProviderStatus`):**
   - `Active`: nguồn cung cấp đang hoạt động bình thường.
   - `ProviderInactive`: theme đã đổi hoặc plugin đã tắt — **dữ liệu và định nghĩa schema vẫn được giữ nguyên và cho phép quản trị viên/biên tập viên đọc/sửa**, đồng thời Admin hiển thị cảnh báo nguồn cung cấp đang tắt hoặc thiếu template/block.

---

## 5. Hợp đồng Hook & Action (`opendash_core::hooks` & `opendash_server::hooks`)

1. **Phân loại sự kiện (`HookEvent`):**
   - `PageAfterSave`: kích hoạt sau khi trang/bố cục được lưu thành công vào SQLite.
   - `ContentBeforeSave` / `ContentAfterSave`: kích hoạt trước/sau khi lưu mục nội dung (M3).
   - `PluginManualRun`: kích hoạt khi quản trị viên chủ động chạy hành động plugin từ Admin.
2. **Nguyên tắc cô lập lỗi:**
   - Mọi sự kiện phát qua `HookDispatcher` đi kèm `HookEnvelope` (gồm tên sự kiện, thời điểm, `correlation_id` và `payload` JSON).
   - Lỗi hoặc quá thời gian (timeout/out-of-fuel) từ một plugin khi xử lý hook được ghi vào `plugin_logs` và `last_error`, **không làm hỏng giao dịch lưu nội dung chính của CMS** và không chặn đường phục hồi trong Admin.

---

## 6. Checklist bắt buộc trước và sau mỗi Slice

Mọi kế hoạch trong `docs/plans/` và mọi lượt viết code phải kiểm tra:

- [ ] **Trước khi code:** Đã liệt kê các hàm/kiểu trong `CORE_FOUNDATION.md` sẽ tái sử dụng chưa?
- [ ] **Ranh giới crate:** `opendash-core` có giữ thuần Rust (không `axum`, `sqlx`, `wasmi`, `std::fs`) không?
- [ ] **Ranh giới SQL:** Có bất kỳ câu lệnh `sqlx::query*` nào nằm ngoài `crates/opendash-server/src/storage/` không? (Phải bằng `0` trong code ứng dụng).
- [ ] **Ranh giới Auth/CSRF:** Mọi route thay đổi dữ liệu đã gọi guard kèm CSRF chưa?
- [ ] **Tương thích ngược:** Toàn bộ unit test và 5 bộ smoke test trên Windows còn PASS 100% không?

