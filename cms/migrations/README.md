# SQLite migrations

Thư mục dành cho migration thực sự của OpenDash, sử dụng SQLx. Khi nhập source lịch sử, bảo toàn các file đã phát hành như `0001_builder_slice.sql`, `0002_auth.sql` và `0003_plugins.sql` cùng thứ tự và nội dung ban đầu.

**Chưa có migration schema trong scaffold.** Server chỉ tạo file SQLite và thực hiện `SELECT 1` để xác nhận kết nối. Không tạo migration `0001` giả vì sẽ xung đột với dữ liệu nâng cấp của repository nguồn.
