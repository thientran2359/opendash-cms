# M3.1 — Mô hình Custom Post Type và Custom Fields

- **Trạng thái:** `VERIFIED` trong phạm vi domain schema + validation trên macOS (xem [báo cáo kiểm chứng M3.1](../../../verification/2026-11-08-m3-1-content-domain.md)); chưa có API/SQLite/Admin.
- **Hợp đồng:** [Yêu cầu V1](../../../product/PRODUCT_REQUIREMENTS.md), [Core Foundation](../../../contracts/CORE_FOUNDATION.md), [CMS Architecture](../../../architecture/CMS_ARCHITECTURE.md).
- **Phần đã kiểm chứng làm nền:** [M2.5 Core Kernel](../../../verification/2026-11-07-m2-5-core-kernel.md).

## Phạm vi slice đầu tiên

Thêm mô hình `ContentTypeDefinition`, `ContentFieldDefinition`, `ContentFieldType` và `SelectOption` trong `cms/crates/opendash-core/src/content.rs` với JSON serialization và `validate()`. Bao phủ 9 kiểu field của V1: `short_text`, `long_text`, `rich_text`, `number`, `boolean`, `date`, `select`, `image`, `gallery`.

Áp dụng `opendash_core::id::valid_snake_key` cho khóa CPT/field. Cho phép CPT chưa khai báo custom field, từ chối khóa trùng, nhãn rỗng và cấu hình `select` thiếu hoặc trùng option. Giới hạn kỹ thuật ban đầu: 64 fields/CPT, 100 options/select, nhãn tối đa 100 ký tự, giá trị option tối đa 128 ký tự. Các ngưỡng này cần được rà lại khi triển khai Admin thực tế.

Định nghĩa schema độc lập với theme/plugin; lớp đăng ký sau này sẽ dùng `registry::OwnedDescriptor<ContentTypeDefinition>` và `ResourceOwner` để giữ schema khi provider ngừng hoạt động.

## Tiêu chí nghiệm thu

1. Có thể serialize/deserialize một CPT cùng custom fields bằng JSON và giữ nguyên dữ liệu.
2. Phân biệt 9 kiểu field V1, từ chối khóa không hợp lệ và field trùng khóa.
3. Field `select` phải có danh sách lựa chọn hợp lệ, không trùng giá trị; các kiểu field khác không chứa `options`.
4. `cargo fmt --manifest-path cms/Cargo.toml --all --check` và `cargo test --manifest-path cms/Cargo.toml -p opendash-core --locked` chạy thành công trước khi ghi `VERIFIED`.

## Ngoài phạm vi và phần tiếp theo

- **M3.2:** [Lưu schema CPT/field và API tạo/liệt kê CPT](2026-11-08-m3-2-content-types-backend.md) bằng `opendash_server::storage`, guard Auth/CSRF và `ApiError`, giữ owner/provider status trong SQLite.
- **M3.3:** [lưu entry và giá trị custom field](2026-11-08-m3-3-content-entry-crud.md), kiểm tra kiểu giá trị, CRUD với guard Auth/CSRF và hooks; phân quyền chỉnh sửa nội dung cho Editor.
- **M3.4:** Admin UI cho CPT/entry và từng field type bằng `FormPrimitives`; giữ form chưa lưu khi lỗi xác thực.
- **M3.5:** media storage/upload/gallery và liên kết dữ liệu ảnh tới entry.
- **Taxonomy:** danh mục cha–con và thẻ theo yêu cầu V1, có vòng đời độc lập theme.

Slice M3.1 chưa tạo DB, route hoặc UI; chưa được coi là chức năng quản lý CPT có thể dùng từ Admin.
