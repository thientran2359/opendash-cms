# SQLite migrations

SQLx giữ nguyên ba migration gốc: 0001 lưu trang và theme block, 0002 thêm users/sessions/bootstrap, 0003 thêm plugin, KV và log. 0004 bổ sung bảng `content_types` lưu định nghĩa CPT, custom fields và trạng thái provider; 0005 bổ sung `content_entries` lưu giá trị field, slug và phiên bản cập nhật. Không xóa dây chuyền khi theme/plugin bị tắt. Giữ nguyên nội dung và LF của migrations lịch sử để bảo toàn checksum. Dùng migration mới cho thay đổi schema tiếp theo.
