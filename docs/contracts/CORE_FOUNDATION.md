# OpenDash — Hợp đồng Bộ khung Lõi (Core Kernel) & Quy chuẩn Code (v1)

- **Phiên bản:** `1.1`
- **Ngày hiệu lực:** `2026-11-07`
- **Trạng thái:** `AGREED` (Quy chuẩn bắt buộc cho M2.5 và toàn bộ các mốc M3–M6); các phần thuộc phạm vi M2.5 đã `implemented` và `verified` theo [báo cáo kiểm chứng M2.5](../verification/2026-11-07-m2-5-core-kernel.md).
- **Tài liệu liên quan:** [Yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md) · [Kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) · [Plugin ABI v1](PLUGIN_ABI.md) · [ADR-0001](../decisions/ADR-0001-workspace-layout.md) · [ADR-0002](../decisions/ADR-0002-core-extension-contract.md) · [Kế hoạch M2.5](../plans/milestones/M2.5-core-foundation.md)

---

## 1. Mục đích

Tài liệu này định nghĩa **Bộ khung Lõi (Core Kernel)** của OpenDash — đóng vai trò tương đương hệ thống hàm gốc (`functions`), bộ đăng ký (`register_post_type`, `register_block_type`, `add_option`) và hệ thống sự kiện (`add_action` / `do_action`) của WordPress — đồng thời quy định **Quy chuẩn Code bắt buộc** để mọi giai đoạn phát triển (M2.5, M3, M4, M5, M6) dùng chung một chuẩn duy nhất, không viết lặp hàm hoặc phá vỡ ranh giới kiến trúc.

---

## 2. Danh mục Base APIs bắt buộc tái sử dụng (Single Source of Truth)

Trước khi thêm bất kỳ tính năng, route, bảng dữ liệu hay màn hình Admin nào, developer và AI bắt buộc tra bảng dưới đây (kèm CodeGraph và mã nguồn thực tế) và gọi đúng module gốc. **Nghiêm cấm viết lại các hàm tương đương ở module khác.** Nếu API hiện có chưa đủ, ưu tiên mở rộng tại module sở hữu; chỉ tạo module mới khi có trách nhiệm khác biệt rõ ràng và được ghi nhận trong ADR/contract.

| Nhu cầu kỹ thuật | Module gốc sở hữu | API công khai & Caller thực tế | Trạng thái (`AGREED` / `implemented` / `verified`) |
|---|---|---|---|
| Kiểm tra định danh, khóa schema, block ID, mã gói, SemVer | `opendash_core::id` | `valid_snake_key`, `valid_snake_key_with_limit`, `valid_kebab_segment_with_limit`, `valid_namespaced_block_id`, `valid_namespaced_snake_key`, `validate_package_id`, `valid_package_id`, `validate_semver` (re-export tại `plugin.rs` thành `valid_plugin_id`, `validate_plugin_id`). Caller: `content.rs`, `theme.rs`, `plugin.rs`, `hooks.rs`, `storage/page.rs`. | `AGREED` · `implemented` · `verified` |
| Quản lý tài nguyên có chủ sở hữu (Block, CPT, Field, Template, Setting) | `opendash_core::registry` | `ResourceOwner` (`Core`, `User`, `Theme(id)`, `Plugin(id)`), `ProviderStatus` (`Active`, `ProviderInactive`), `OwnedDescriptor<T>`, `RegistryCatalog<T>` (`register`, `update`, `get`, `iter`, `deactivate_owner`, `activate_owner`), `RegistryError`. Caller: `storage/page.rs` (`load_block_catalog`, `insert_theme_block`, `load_theme_block`). | `AGREED` · `implemented` · `verified` cho Block M1 (CPT/Media thuộc M3) |
| Định nghĩa & điều phối sự kiện vòng đời / thao tác hệ thống | `opendash_core::hooks` & `opendash_server::hooks` | `HookEvent` (`PageAfterSave`, `ContentBeforeSave`, `ContentAfterSave`, `PluginManualRun`), `HookEnvelope` (`opendash-core`); `HookDispatcher` (`register_handler`, `dispatch`, `dispatch_plugin_run`), `DispatchReport`, `HookOutcome` (`opendash-server`). Caller: `page.rs::save_page`, `routes/plugin.rs::plugin_run`. | `AGREED` · `implemented` · `verified` cho `PageAfterSave` & `PluginManualRun` (`Content*` thuộc M3) |
| Đọc & kiểm tra an toàn gói ZIP (Theme, Plugin) | `opendash_server::package` | `open_bounded_zip`, `ensure_safe_flat_entry`, `read_bounded_entry`, `is_valid_zip_content_type` (mỗi loại gói giữ giới hạn kích thước riêng: Theme `64 KiB`, Plugin `5 MiB`). Caller: `theme.rs`, `plugin/package.rs`, `routes/builder.rs`, `routes/plugin.rs`. | `AGREED` · `implemented` · `verified` |
| Quản lý đường dẫn runtime của website | `opendash_server::paths::SitePaths` | Mọi đường dẫn `database/`, `themes/`, `plugins/`, `uploads/`, `backups/`, `runtime/staging/` lấy từ `SitePaths`. Caller: `main.rs`, `storage/mod.rs`. | `AGREED` · `implemented` · `verified` |
| Xác thực phiên, kiểm tra quyền (`Admin`/`Editor`) và CSRF | `opendash_server::auth` | Route đọc dữ liệu dùng `require_session` hoặc `require_admin`. Route thay đổi dữ liệu đã có phiên (`POST`/`PUT`/`DELETE`/`PATCH`) bắt buộc dùng `require_session_with_csrf` hoặc `require_admin_with_csrf`. Ngoại lệ bootstrap/đăng nhập (`auth_setup`, `auth_login`) chưa có phiên nên dùng kiểm tra `Host`/`Origin`/`Referer` ở middleware `app.rs` kèm kiểm tra thông tin xác thực. | `AGREED` · `implemented` · `verified` |
| Trả lỗi HTTP từ server | `opendash_server::error::ApiError` | Dùng `ApiError` (`Cow<'static, str>`, mặc định `text/plain; charset=utf-8`) và `ApiResult<T>`. Ngoại lệ tương thích có chủ đích: `POST /api/admin/plugins/{id}/run` trả JSON `PluginRunResponse` (`{success, data, error, code, correlation_id}`). | `AGREED` · `implemented` · `verified` |
| Truy vấn cơ sở dữ liệu SQLite | `opendash_server::storage::*` | **100% câu lệnh `sqlx::query*` và `sqlx::raw_sql` của code ứng dụng phải nằm trong `crates/opendash-server/src/storage/`** (`mod.rs`, `auth.rs`, `page.rs`, `plugin.rs`, `plugin_kv.rs`). | `AGREED` · `implemented` · `verified` |
| Gọi HTTP API từ React Admin | `cms/admin/src/api/client.ts` | Dùng `requestJson`, `requestText`, `ApiClientError`, `getHealth`. Mọi lời gọi `fetch` trong Admin đi qua `api/client.ts`; `App.tsx` bảo toàn dữ liệu đang nhập (`blockData`) khi gặp mã `401`. | `AGREED` · `implemented` · `verified` |
| Thành phần giao diện Form & điều khiển cơ bản | `cms/admin/src/components/FormPrimitives.tsx` | Tái sử dụng `TextInput`, `FormGroup`, `TextField`, `SelectField`, `ZipUploadField` trên `BuilderWorkspace.tsx`, `PluginsPanel.tsx`, `AuthPanel.tsx`, `UsersPanel.tsx`. | `AGREED` · `implemented` · `verified` |

---

## 3. Quy chuẩn Định danh (Identifier Standards)

OpenDash phân biệt rõ các nhóm định danh trong `opendash_core::id` (chi tiết quyết định xem [ADR-0002](../decisions/ADR-0002-core-extension-contract.md)):

1. **`SnakeKey` (`valid_snake_key`, `valid_snake_key_with_limit` — `[a-z][a-z0-9_]*`):**
   - Dùng cho: định danh CPT (`post`, `portfolio_item`), tên trường dữ liệu (`field_key` như `title`, `hero_image` giới hạn `1..=32`), tên thuộc tính nội bộ (mặc định `1..=64`).
2. **`PackageId` (`validate_package_id`, `valid_package_id` — `[a-z0-9-]`, `3..=64` ký tự):**
   - Dùng cho: `plugin_id` (`sample-plugin`) và `theme_id` theo đúng hợp đồng ABI; không bắt đầu hoặc kết thúc bằng `-`.
3. **`KebabSegment` & `NamespacedBlockId` (`valid_kebab_segment_with_limit`, `valid_namespaced_block_id`):**
   - Dùng cho: định danh block `<namespace>/<name>` (mỗi đoạn `1..=32` ký tự, bắt đầu bằng `a-z`, gồm `a-z`, `0-9`, `-`, giữ tương thích ngược với dữ liệu block M1). Slug nội dung cho các module tương lai không mặc định dùng chung validator block M1.
4. **`NamespacedSnakeKey` (`valid_namespaced_snake_key`):**
   - Dùng cho: khóa schema `<namespace>/<name>` (`core/heading`) và ID của core hook handler (`core/page_saved`), mỗi phần là một `SnakeKey` hợp lệ.
5. **`SemVer` (`validate_semver`):**
   - Kiểm tra phiên bản SemVer 2.0.0 (`MAJOR.MINOR.PATCH` kèm pre-release/build metadata hợp lệ, từ chối số `0` đứng đầu hoặc đoạn rỗng).

---

## 4. Quy chuẩn Phân tầng & Tổ chức Code

1. **Ranh giới trách nhiệm giữa các tầng:**
   - **`routes/*`:** chỉ thực hiện guard xác thực/CSRF (`require_*`), kiểm tra header/Content-Type, giải mã input HTTP và ánh xạ kết quả sang HTTP response. Không đặt truy vấn SQL, không tự ghép nhiều bước nghiệp vụ và không gọi trực tiếp `PluginManager::run_plugin*` từ route.
   - **Tầng nghiệp vụ (`opendash_server::page` và tương đương ở các mốc sau):** điều phối luồng nhiều bước (validate/render → ghi SQLite qua `storage` → phát sự kiện sau commit qua `HookDispatcher`). Dependency (`&SqlitePool`, `&HookDispatcher`) được truyền tường minh qua tham số hàm hoặc `AppState`, không dùng biến toàn cục hay service locator ngầm.
   - **`opendash_core`:** thuần Rust, chỉ phụ thuộc `serde`, `serde_json`, `url`. Tuyệt đối không dùng `axum`, `sqlx`, `wasmi`, `tokio`, `reqwest` hoặc `std::{fs, io, net, process, thread}`.
   - **`opendash_server::storage::*`:** sở hữu toàn bộ câu lệnh SQL, kết nối/giao dịch SQLite và việc dựng snapshot `RegistryCatalog` từ cơ sở dữ liệu. Các hàm trong `storage::plugin_kv` chỉ mượn `&SqlitePool` hoặc `&mut SqliteConnection` của caller mà không tự spawn task hay áp đặt timeout riêng, bảo toàn cơ chế deadline, hủy tức thời và `StagedCommitGuard` của M2.
2. **Mẫu mở rộng chuẩn (Reference Flow):**
   - Luồng Lưu trang là mẫu chuẩn cho các tính năng ở M3–M6:
     `routes/builder.rs::save` (`require_session_with_csrf`) → `page.rs::save_page` → `page.rs::render_block` + `storage/page.rs::load_theme_block` (kiểm tra `RegistryCatalog<BlockDefinition>` và `ProviderStatus::Active`) → `storage/page.rs::save_home_page` (commit SQLite) → `hooks.rs::HookDispatcher::dispatch(HookEvent::PageAfterSave, ...)`.
   - Kiểm chứng thực tế của luồng này nằm tại unit test trong `crates/opendash-server/src/page.rs`, `crates/opendash-server/src/hooks.rs` và smoke test đầu-cuối `cms/scripts/windows_kernel_smoke.py`.

---

## 5. Hợp đồng Registry & Quyền sở hữu Dữ liệu (`opendash_core::registry`)

Để đảm bảo yêu cầu cứng trong [PRODUCT_REQUIREMENTS.md](../product/PRODUCT_REQUIREMENTS.md) (*"Đổi theme hoặc tắt plugin giữ các CPT, fields và nội dung đã nhập để tiếp tục quản lý; xóa dữ liệu là thao tác riêng"*):

1. **Chủ sở hữu tài nguyên (`ResourceOwner`) và tính bất biến quyền sở hữu:**
   - `Core`, `User`, `Theme(String)`, `Plugin(String)`.
   - `RegistryCatalog::register` chỉ đăng ký khóa mới; từ chối khóa trùng cùng owner bằng `RegistryError::AlreadyRegistered` và khác owner bằng `RegistryError::OwnerConflict`.
   - `RegistryCatalog::update` chỉ cho phép cùng `ResourceOwner` cập nhật định nghĩa (`descriptor`) và **giữ nguyên `ProviderStatus` hiện tại** (không tự động kích hoạt lại provider đang tắt).
2. **Trạng thái vòng đời (`ProviderStatus`) và tách biệt dữ liệu người dùng:**
   - `Active`: nguồn cung cấp đang hoạt động bình thường.
   - `ProviderInactive`: nguồn cung cấp đã tắt (`deactivate_owner`) — định nghĩa trong catalog và dữ liệu người dùng (như `home_page`) vẫn được bảo toàn, nhưng không cho phép thực thi/render block từ provider đang tắt cho tới khi `activate_owner`.
   - Với phạm vi M1/M2.5, SQLite (`theme_blocks`, `home_page`) là nguồn sự thật; `storage::page::load_block_catalog` dựng `RegistryCatalog<BlockDefinition>` từ SQLite nên giữ nguyên trạng thái qua các lần khởi động lại server.

---

## 6. Hợp đồng Hook & Action (`opendash_core::hooks` & `opendash_server::hooks`)

1. **Phân loại sự kiện (`HookEvent`) và thời điểm chạy:**
   - `PageAfterSave` (`page/after_save`): thông báo chỉ đọc phát **sau khi** SQLite đã commit dữ liệu trang. Handler không thể sửa hoặc hủy kết quả lưu đã hoàn tất.
   - `PluginManualRun` (`plugin/manual_run`): điều phối lượt chạy plugin thủ công từ Admin qua `HookDispatcher::dispatch_plugin_run`, giữ nguyên định dạng input/output của M2.
   - `ContentBeforeSave` (`content/before_save`) và `ContentAfterSave` (`content/after_save`): đã định nghĩa tên chuẩn trong `HookEvent` cho M3; ở M2.5, `HookDispatcher::dispatch` từ chối mọi sự kiện khác `PageAfterSave` và chặn gọi lồng (`IN_DISPATCH`) trong cùng async task.
2. **Đăng ký, thứ tự thực thi và cô lập lỗi:**
   - Core handler đăng ký lúc khởi động qua `HookDispatcher::register_handler` với ID `core/<snake_key>` duy nhất và `priority: i32`.
   - Plugin đăng ký qua trường tùy chọn `"hooks": ["page/after_save"]` trong `manifest.json` (manifest cũ không có trường `hooks` mặc định không đăng ký). Mỗi lượt dispatch truy vấn danh sách plugin đang `enabled = 1` từ SQLite (mặc định `priority = 100`) và sắp xếp tất cả handler theo `(priority, id)`.
   - Khi chạy qua `PageAfterSave`, plugin nhận JSON `HookEnvelope` (`{event, occurred_at_ms, correlation_id, payload}`) qua ABI v1 hiện có.
   - Giới hạn bảo vệ: tối đa `32` handler mỗi lượt, ngân sách nhận handler mới `5s`, timeout cho mỗi core handler `250ms` (bắt cả panic đồng bộ lẫn bất đồng bộ). Plugin chạy qua `PluginManager::run_plugin_correlated` với đầy đủ giới hạn Wasmi/worker của M2. Lỗi hoặc timeout của bất kỳ handler nào được ghi vào `DispatchReport`, `plugin_logs` và `plugins.last_error`, **không làm hỏng giao dịch lưu trang và không chặn các handler kế tiếp**.

---

## 7. Kiểm tra Quy chuẩn Tự động & Checklist mỗi Slice

Trước và sau mỗi slice, bắt buộc chạy lệnh kiểm tra ranh giới kiến trúc (kèm tự kiểm thử bằng các mẫu cố ý vi phạm trong thư mục tạm):

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
```

Checklist bắt buộc cho mọi kế hoạch trong `docs/plans/` và mọi lượt viết code:

- [x] **Trước khi code:** Đã tra CodeGraph, mã nguồn và bảng Base APIs ở Mục 2 để tái sử dụng chưa?
- [x] **Ranh giới crate:** `opendash-core` chỉ dùng `serde`, `serde_json`, `url` và không chứa framework/I/O (`check-standards.py`)?
- [x] **Ranh giới SQL:** 100% câu lệnh `sqlx::query*` và `sqlx::raw_sql` của code ứng dụng nằm trong `crates/opendash-server/src/storage/` (`check-standards.py`)?
- [x] **Ranh giới Route & Auth/CSRF:** Mọi route thay đổi dữ liệu có phiên đã gọi `require_session_with_csrf` / `require_admin_with_csrf` và không gọi trực tiếp plugin ngoài `HookDispatcher` (`check-standards.py`)?
- [x] **Ranh giới Admin HTTP:** Mọi lời gọi `fetch` trong `cms/admin/src/` đều đi qua `api/client.ts` (`check-standards.py`)?
- [x] **Tương thích ngược & Hồi quy:** Unit test, `windows_kernel_smoke.py` và 5 bộ smoke test trên Windows đều PASS 100%?
