# Uploads của website

Thư mục cấp gốc lưu file media do người dùng tải lên, ví dụ ảnh, video và các phiên bản ảnh đã xử lý. Cấu trúc con như `uploads/2026/11/` sẽ do Media module quy định khi được triển khai.

Scaffold hiện tại **chưa có API upload** và chưa ghi file vào thư mục này. Nội dung được loại trừ khỏi Git, nhưng vẫn cần được tính vào backup/restore website theo [yêu cầu V1](../docs/product/PRODUCT_REQUIREMENTS.md).

Không dùng thư mục này để lưu code theme/plugin hay dữ liệu SQLite (database mặc định ở `database/opendash.sqlite`).
