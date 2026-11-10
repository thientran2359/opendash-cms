# M3.3 — Content Entry CRUD và kiểm tra giá trị custom fields

- **Verification theo slice (không phải task `Done`):** `VERIFIED` trong phạm vi backend trên macOS; xem [báo cáo kiểm chứng M3.3](../../../../verification/2026-11-08-m3-3-content-entry-crud.md). Trạng thái task phải tra [GitHub Project #3](https://github.com/users/thientran2359/projects/3).
- **Hợp đồng:** [PRODUCT_REQUIREMENTS](../../../../product/PRODUCT_REQUIREMENTS.md), [CORE_FOUNDATION](../../../../contracts/CORE_FOUNDATION.md), [M3.1](2026-11-08-m3-content-types-and-fields.md), [M3.2](2026-11-08-m3-2-content-types-backend.md).
- **Mục tiêu:** Admin/Editor quản lý nội dung thuộc CPT đã khai báo, lưu dữ liệu qua restart và khi theme/plugin cung cấp CPT ngừng hoạt động; không thay đổi dữ liệu/chuỗi migration cũ.

## Phạm vi

1. Thêm migration `0005_content_entries.sql` chứa `content_type_key`, `slug` (duy nhất trong CPT), `title`, `values_json`, `version`, timestamps. FK `RESTRICT` trỏ tới `content_types`, **không có liên kết cascade tới plugin/theme**.
2. Core `content.rs` xác thực các giá trị theo schema 9 loại field: chuỗi, rich text, số JSON, boolean, ngày `YYYY-MM-DD` thật, select theo options, image ID và gallery ID. Từ chối field ngoài khai báo và trường `required` thiếu hoặc null.
3. `storage/entries.rs` chứa truy vấn SQLite parameterized; `routes/entries.rs` chỉ gọi storage, Auth/CSRF và `HookDispatcher`.
4. Mọi thao tác cập nhật/xóa dùng phiên bản nguyên tử bằng điều kiện SQL `version = ?`. Header HTTP `If-Match: "vN"` phải khớp bản đọc gần nhất, tránh mất dữ liệu khi hai biên tập viên ghi cùng lúc.
5. Phát `ContentBeforeSave` cho yêu cầu tạo/sửa đã qua xác thực/validation; phát `ContentAfterSave` **chỉ sau khi SQLite ghi thành công**. Không phát hook xóa, vì Core chưa định nghĩa sự kiện xóa. Dispatcher hiện chỉ ghi sự kiện vào log in-memory, chưa kích hoạt plugin hook listener.

## REST API

Tất cả route yêu cầu phiên đăng nhập. Admin và Editor đều có quyền xem và chỉnh sửa entry; chỉ Admin được tạo CPT schema tại M3.2.

| Phương thức | Endpoint | Kết quả |
| --- | --- | --- |
| `GET` | `/api/admin/content-types/{type_key}/entries?limit=50&offset=0` | `200` danh sách theo ID giảm dần, limit 1–100 |
| `POST` | `/api/admin/content-types/{type_key}/entries` | `201` entry mới, `Location`, `ETag: "v1"`; cần CSRF |
| `GET` | `/api/admin/content-types/{type_key}/entries/{id}` | `200`, `ETag: "vN"` |
| `PUT` | `/api/admin/content-types/{type_key}/entries/{id}` | `200` và ETag tăng; thay thế toàn bộ title/slug/values, cần CSRF và If-Match |
| `DELETE` | `/api/admin/content-types/{type_key}/entries/{id}` | `204`, cần CSRF và If-Match |

Body POST/PUT ví dụ:

```json
{
  "title": "Dự án mẫu",
  "slug": "du-an-mau",
  "values": {
    "caption": "Giới thiệu",
    "price": 0,
    "active": false,
    "date": "2024-02-29",
    "state": "open"
  }
}
```

Trường `values` phải được khai báo trong CPT tương ứng. Lỗi phổ biến: `400` sai dữ liệu, `401` chưa đăng nhập, `403` CSRF/Origin, `404` không tìm thấy CPT/entry, `409` slug trùng, `412` phiên bản cũ, `428` thiếu If-Match, `413` vượt kích thước body 512 KiB. `ApiError` hiện trả plain text, không phải JSON.

## Ranh giới và bước tiếp theo

- `short_text` tối đa 512 ký tự, `long_text` tối đa 100.000, `rich_text` tối đa 200.000; trường chọn chỉ nhận giá trị option khai báo. Ngày kiểm tra cả năm nhuận; gallery tối đa 50 ID, không trùng. Giá trị ảnh/gallery **mới chỉ là ID chuỗi opaque**, chưa có cơ chế xác minh file/owner vì Media Library chưa triển khai.
- Rich text **là dữ liệu không tin cậy**, được lưu thô và **không được đưa trực tiếp vào HTML**; khi xây dựng renderer và Admin preview phải có escape/sanitization theo ngữ cảnh.
- Chưa thêm revision/draft hay cơ chế xuất bản tách biệt. Lưu entry vào SQLite **chưa làm nội dung xuất hiện trên website công khai**, vì template và renderer CPT chưa nối ở slice này.
- **M3.4:** Admin UI để tạo/quản lý CPT và entry với `FormPrimitives`, giữ form chưa lưu khi session hết hạn.
- **M3.5:** Media Library, xác minh tham chiếu ảnh, rồi nối nội dung CPT với template/public renderer và thêm taxonomy theo kế hoạch riêng.

## Nghiệm thu

Rust fmt/build/workspace tests; smoke HTTP/SQLite trong `cms/scripts/content_entries_smoke.py` (vai trò Admin/Editor, CSRF, CRUD, lỗi schema, ETag cạnh tranh, restart, nâng cấp từ fixture) và regression M3.2/Auth/Builder. Không thay đổi trực tiếp DB website thật hoặc migration 0001–0004.
