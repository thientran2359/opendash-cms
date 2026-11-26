# Uploads của website

Thư mục cấp gốc lưu toàn bộ media do người dùng tải lên của website, tách biệt hoàn toàn với mã nguồn trong [`cms/`](../cms/README.md) và được đưa vào bản sao lưu mã hóa cùng cơ sở dữ liệu SQLite (`database/opendash.sqlite`).

- **Ảnh & Video:** Lưu file gốc trong `uploads/media/<2 ký tự đầu SHA-256>/<id>.<ext>` (ảnh JPEG/PNG/WebP đã strip EXIF kèm thumbnail `<id>.thumb.png`; video MP4 qua `POST /api/admin/media/video`, hỗ trợ HTTP Range `206`).
- **Vòng đời & Đối soát (M8):** Xóa media kiểm tra tham chiếu (`storage::media_refs`), chuyển file sang `uploads/.quarantine/<job_id>/` trước khi dọn sạch và hỗ trợ đối soát tại `#/tools/media-reconcile`.
- **Quyền truy cập & Git:** Nội dung trong thư mục này bị `.gitignore` bỏ qua (chỉ giữ `README.md`). Media chưa được tham chiếu từ nội dung đã xuất bản / công khai sẽ trả `404` trên route công khai `/media/{id}/file`. Xem chi tiết tại [`docs/contracts/M7_SITE_FEATURES.md`](../docs/contracts/M7_SITE_FEATURES.md) và [`docs/contracts/M8_OPERATIONS_RELEASE.md`](../docs/contracts/M8_OPERATIONS_RELEASE.md).
