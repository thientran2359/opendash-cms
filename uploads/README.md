# Uploads của website

Thư mục cấp gốc lưu file media của website, nằm ngoài mã nguồn và phải được đưa vào backup cùng database SQLite. M3.5 hiện lưu ảnh đã upload trong `uploads/media/<2 ký tự đầu SHA-256>/<id>.<ext>` và thumbnail tại `<id>.thumb.png`.

API upload ảnh `POST /api/admin/media` và đọc thumbnail/file gốc chỉ cho phép tài khoản Admin/Editor đăng nhập (upload cần CSRF). File gốc được re-encode để bỏ metadata; hỗ trợ JPEG/PNG/WebP, không hỗ trợ video/audio ở M3.5. Nội dung đã loại khỏi Git, còn tính năng backup/restore vẫn chưa triển khai; xem [kế hoạch M3.5](../docs/plans/2026-11-09-m3-5-media-library.md).

Không dùng thư mục này để lưu code theme/plugin hay dữ liệu SQLite (database mặc định ở `database/opendash.sqlite`).
