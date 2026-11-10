# M3.5 — Media Library và Image/Gallery Picker

- **Verification theo slice (không phải task `Done`):** `VERIFIED` trên macOS cho backend/API và giao diện chọn ảnh đã kiểm thử; xem [báo cáo M3.5](../../../../verification/2026-11-09-m3-5-media-library.md). Trạng thái task phải tra [GitHub Project #3](https://github.com/users/thientran2359/projects/3).
- **Hợp đồng:** [PRODUCT_REQUIREMENTS](../../../../product/PRODUCT_REQUIREMENTS.md), [CORE_FOUNDATION](../../../../contracts/CORE_FOUNDATION.md), [M3.3 Content Entry CRUD](2026-11-08-m3-3-content-entry-crud.md), [M3.4 Admin Content UI](2026-11-08-m3-4-admin-content-ui.md).
- **Mục tiêu:** Admin và Editor có thể upload ảnh, duyệt thumbnail và chọn ảnh/gallery cho CPT entry, không dùng URL/path do người dùng nhập để phục vụ file; giữ media độc lập theme/plugin.

## Phạm vi triển khai

1. `0006_media_assets.sql` thêm bảng `media_assets` (SHA-256 ID, display filename, MIME, extension, size, width/height và thumbnail dimensions, created_at). **Không sửa migrations 0001–0005**.
2. `SitePaths::uploads_dir()/media/<2 ký tự đầu>/<id>.<ext>` lưu raster đã được giải mã, chuẩn hóa và re-encode; `<id>.thumb.png` là thumbnail, tối đa 320×320. Không dùng filename do client cung cấp để tạo đường dẫn. File được ghi trong cùng filesystem qua staging tạm rồi rename nguyên tử trước khi thêm metadata SQLite; duplicate uploads được deduplicate theo nội dung ảnh đã chuẩn hóa.
3. Chỉ hỗ trợ JPEG, PNG và WebP. Giới hạn upload đầu vào và file đã chuẩn hóa là **8 MiB**, chiều rộng/cao tối đa **6000 px**, tổng tối đa **20 triệu pixel**, decoder allocation <=128 MiB, tối đa hai decode chạy đồng thời. Thực hiện decode/re-encode trong blocking worker, lọc định dạng thực/mismatch MIME, loại metadata EXIF. SVG, HTML và các format scriptable không được hỗ trợ.
4. `storage/media.rs` chứa toàn bộ SQL, hỗ trợ list/find/save và xác minh image/gallery IDs tồn tại. `routes/entries.rs` gọi verifier sau domain validation, trước save; ảnh giả không thể lưu vào entry mới/sửa. Entry cũ có ID giả vẫn đọc được nhưng cần thay ảnh hợp lệ trước khi lưu lại.
5. `POST /api/admin/media`: request body là **bytes gốc ảnh**, `Content-Type` phải khớp định dạng thực, tùy chọn header `X-File-Name` chứa tên file được URI-encode, cần session Admin/Editor + CSRF; trả `201` JSON metadata. `GET /api/admin/media?limit&offset`, `GET /api/admin/media/{id}/file`, `GET /api/admin/media/{id}/thumbnail`: cần session hợp lệ, trả metadata/ảnh, giới hạn phân trang 1–100 và header `Cache-Control: private, no-store`, `X-Content-Type-Options: nosniff`.
6. `MediaPicker` được mở từ nút Media Library trong tab **Nội dung** hoặc từ field `image`/`gallery`. Chọn 1/nhiều ảnh, upload file, tìm trong trang hiện tại, sắp xếp/xóa ảnh khỏi **field** gallery (không xóa file thư viện). Draft React và ETag/If-Match từ M3.4 tiếp tục được giữ.

## Nghiệm thu

- Rust fmt/build/workspace tests, TypeScript typecheck/Vite build thành công với `--locked`.
- HTTP smoke thật trên site+SQLite tạm: RBAC/CSRF/Origin, 400/413/415, list/serve/thumbnail/restart, SHA ID, thumbnail, field image/gallery hợp lệ và thiếu media 400; không có file phụ cạnh fixture.
- Browser: upload từ Admin, thumbnail tải thực, chọn ảnh cho single/gallery, lưu entry và xem lại. Không được chạy nội dung HTML/SVG do upload.
- Regression M3.2/M3.3, Auth/Builder; không ghi vào database/ uploads website hiện tại.

## Ngoài phạm vi

- **Chưa có xóa media khỏi thư viện** (cần khóa tham chiếu giữa entry và media, transaction an toàn và chính sách phục hồi), bulk edit, crop/rotate, caption/alt metadata, phân quyền media theo từng user, xử lý video/audio.
- Chưa phục vụ file media công khai: route `/api/admin/media` chỉ dành cho phiên Admin/Editor. Renderer CPT/publication và URL ảnh công khai phải thiết kế ở slice sau.
- Không tự động xóa media nếu bản nháp entry bị bỏ; media là tài sản riêng của site. Backup/restore bao gồm `uploads/` sẽ được triển khai ở mốc khác.
