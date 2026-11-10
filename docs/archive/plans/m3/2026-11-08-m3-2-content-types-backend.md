# M3.2 — Lưu và truy cập Custom Post Types qua HTTP

- **Trạng thái:** `VERIFIED` trong phạm vi backend trên macOS. Xem [báo cáo chạy thực tế](../../../verification/2026-11-08-m3-2-content-types-backend.md).
- **Dựa trên:** [M3.1 CPT & Custom Fields](2026-11-08-m3-content-types-and-fields.md) và [bộ khung lõi](../../../contracts/CORE_FOUNDATION.md).
- **Hợp đồng sản phẩm:** [PRODUCT_REQUIREMENTS](../../../product/PRODUCT_REQUIREMENTS.md) — Admin tạo CPT, Editor quản lý nội dung, dữ liệu/schema được giữ khi đổi theme hoặc tắt plugin.

## Phạm vi đã triển khai

1. Migration bổ sung `cms/migrations/0004_content_types.sql` tạo bảng `content_types`: khóa duy nhất, `definition_json` (gồm custom fields), `owner_kind`/`owner_id` và `provider_status`; giữ nguyên 0001–0003 và fixture. Không đặt quan hệ xóa dây chuyền tới plugin/theme.
2. `opendash_server::storage::content` xử lý toàn bộ SQL, lưu định nghĩa người dùng với `ResourceOwner::User`/`ProviderStatus::Active`, phân biệt lỗi trùng (`409`) và khôi phục `OwnedDescriptor<ContentTypeDefinition>` khi đọc. Dữ liệu `provider_inactive` vẫn được trả về để tiếp tục quản lý.
3. API `GET /api/admin/content-types` dành cho phiên Admin hoặc Editor hợp lệ; `POST /api/admin/content-types` dành riêng cho Admin, bắt buộc `X-CSRF-Token`, body JSON tối đa 512 KiB. Tạo thành công trả `201`, invalid schema trả `400` và khóa trùng trả `409`.
4. `cms/scripts/content_types_smoke.py` kiểm chứng một server thực với database tạm, dữ liệu giữ qua restart và nâng cấp từ fixture lịch sử.

API nhận JSON `ContentTypeDefinition` từ M3.1:

```json
{
  "key": "portfolio_item",
  "label": "Dự án",
  "fields": [
    { "key": "summary", "label": "Tóm tắt", "type": "long_text" },
    { "key": "status", "label": "Trạng thái", "type": "select", "options": [
      { "value": "new", "label": "Mới" },
      { "value": "done", "label": "Hoàn tất" }
    ] }
  ]
}
```

Kết quả GET là mảng `OwnedDescriptor` gồm `owner`, `status`, `descriptor` và được sắp theo `descriptor.key` tăng dần. Không tự động chuyển quyền sở hữu khi plugin/theme bị tắt; quản lý vòng đời provider là một bước riêng.

## Tiêu chí nghiệm thu và phạm vi tiếp theo

- Tạo CPT hợp lệ và đọc thấy sau restart; hai yêu cầu trùng khóa đồng thời chỉ một yêu cầu thành công.
- Bảo vệ route theo phiên đăng nhập, RBAC và CSRF; giới hạn JSON, xác thực đầy đủ schema field theo M3.1.
- Migration có thể nâng cấp database SQLite cũ, giữ nội dung trang cũ và không thay checksum migration lịch sử.
- `cargo fmt --check`, `cargo build --locked`, `cargo test --workspace --locked`, smoke test M3.2 và auth regression đạt.

**Ngoài phạm vi:** cập nhật/xóa CPT; plugin/theme tự đăng ký CPT; bài viết/entry và giá trị custom field; taxonomy; media; giao diện Admin; render theo template; backup/restore. Các phần này phải triển khai theo slice riêng và tiếp tục tái sử dụng `registry`, `storage`, `auth` và `HookDispatcher`.
