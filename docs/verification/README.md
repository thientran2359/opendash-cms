# Báo cáo kiểm chứng

Lưu kiểm chứng có bằng chứng theo ngày hoặc milestone (`YYYY-MM-DD-<chu-de>.md`, `M1_*.md`, ...).

Mỗi báo cáo ghi rõ: **môi trường**, **đường dẫn mã**, **lệnh/check đã thực hiện**, **kết quả (PASS / FAIL / NOT RUN)**, **giới hạn**, **các lỗi còn mở**. Phân biệt:

- `AGREED`: chủ dự án đã chốt thiết kế/contract.
- `implemented`: source code có mặt và chứa thay đổi tương ứng.
- `verified`: có bằng chứng lệnh, test hoặc kiểm tra thực tế của phiên bản đó.

Không suy ra trạng thái `verified` từ tài liệu chuyển đến hoặc báo cáo của repository khác.

- [Kiểm kê workspace trước khi tạo scaffold (2026-11-06)](2026-11-06-repository-inventory.md).
- [Báo cáo build và smoke test scaffold (2026-11-06)](2026-11-06-scaffold-build.md).
- [Chuyển cấu trúc `cms/`, `database/` và kiểm chứng (2026-11-07)](2026-11-07-layout-migration.md).
- [Chuyển `docs/` ra thư mục gốc (2026-11-07)](2026-11-07-docs-root-move.md).
- [SitePaths và lần khởi động đầu của binary trên Windows (2026-11-07)](2026-11-07-site-paths.md).
