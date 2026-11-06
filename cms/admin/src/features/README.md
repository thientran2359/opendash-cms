# Feature modules

Mỗi chức năng thực tế dùng thư mục riêng dưới `src/features/` (và component dùng chung đặt tại `src/components/`) **chỉ khi bắt đầu triển khai mã thực tế**, không tạo sẵn thư mục hay component giữ chỗ:

- `system/` — Hiện minh họa cách gọi Health API qua `src/api/client.ts`.
- `users/` — Đăng nhập, phiên Admin/Editor và quản lý tài khoản (nhập từ M1/M2).
- `themes/` — Cài đặt theme ZIP, quản lý theme và theme options (kiểm tra hợp lệ phía server).
- `plugins/` — Cài đặt plugin ZIP, hiển thị quyền theo [Plugin ABI v1](../../../../docs/contracts/PLUGIN_ABI.md), bật/tắt và báo lỗi runtime Wasmi (nhập từ M2).
- `content/` — (Mở ở M3) Danh sách nội dung, form nhập liệu, Custom Post Types, custom fields và taxonomies.
- `media/` — (Mở ở M3) Thư viện ảnh, upload và media picker.
- `settings/` — Cài đặt website, SMTP, SEO và backup/restore theo phạm vi V1.

Các thao tác ghi (Save/Preview) phải tuân thủ quyền và contract trong [yêu cầu V1](../../../../docs/product/PRODUCT_REQUIREMENTS.md); UI không được tự coi kết quả local state là dữ liệu đã được lưu trên server.
