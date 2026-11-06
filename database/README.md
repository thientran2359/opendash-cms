# Database của website

Thư mục `database/` được đổi tên từ `data/` ngày 07/11/2026. Hai database SQLite ban đầu (`opendash.sqlite` và `scaffold-smoke.sqlite`) được giữ nguyên byte khi di chuyển và được kiểm tra bằng `PRAGMA quick_check`.

Đường dẫn mặc định của server mẫu: `database/opendash.sqlite`, tính từ thư mục website `opendash-cms/`. Có thể dùng `OPENDASH_DB_PATH` để chọn tệp khác và `OPENDASH_SITE_ROOT` để xác định vị trí thư mục website.

Các file SQLite và file WAL/SHM trong thư mục này được `.gitignore` bỏ qua; `README.md` vẫn được lưu cùng mã nguồn. Backup/restore CMS V1 cần xử lý SQLite nhất quán cùng `themes/`, `plugins/` và `uploads/`.
