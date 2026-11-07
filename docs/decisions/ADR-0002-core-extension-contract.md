# ADR-0002 — Cơ chế mở rộng lõi M2.5

- Ngày: 2026-11-07
- Trạng thái thiết kế: AGREED trong phạm vi triển khai M2.5 chủ dự án giao; kiểm chứng ghi riêng.
- Liên quan: [kế hoạch](../plans/2026-11-07-m2-5-core-kernel-and-standards.md), [contract](../contracts/CORE_FOUNDATION.md).

## Quyết định

1. Giữ API validation `valid_*` hiện có và helper ZIP nhỏ; không tạo wrapper chỉ để khớp tên dự kiến. Quy tắc block M1 giữ tương thích (đoạn bắt đầu bằng chữ, có thể kết thúc bằng dấu gạch); package ID giữ ABI. Slug cho module tương lai không mặc định dùng validator block.
2. Registry từ chối khác owner cùng khóa. `register` chỉ thêm; `update` chỉ sửa descriptor cùng owner và giữ status. Bật/tắt là API riêng. Catalog không chứa nội dung hoặc tùy chỉnh người dùng. Với M1, storage dựng catalog block từ `theme_blocks`, owner là namespace trước `/`; tất cả block đã cài vẫn khả dụng như hành vi M1. `home_page` giữ giá trị người dùng độc lập. Chưa có UI đổi theme hoặc cập nhật block; ZIP trùng ID vẫn trả 409. Không thêm migration.
3. Server có module nghiệp vụ `page` để render/save; route chỉ guard, decode và gọi nghiệp vụ. Storage sở hữu SQL, transaction SQLite và snapshot registry. Core không I/O. Dependency truyền tường minh qua tham số/AppState, không global service locator.
4. Hook M2.5 là thông báo chỉ đọc sau commit, không sửa/chặn lưu. `ContentBeforeSave` còn dành M3, dispatcher hiện từ chối event đó. Handler nhận envelope gồm tên canonical, timestamp Unix milliseconds do server cấp, correlation ID và payload. Handler không nhận dispatcher; gọi lồng trong cùng async task bị từ chối. Không cam kết durable delivery/retry khi server tắt hoặc request bị hủy.
5. Core handler đăng ký lúc khởi động với ID duy nhất và priority. Plugin đăng ký qua trường manifest tùy chọn `hooks`, chỉ nhận `page/after_save`; manifest cũ mặc định không đăng ký. Mỗi plugin một handler/event, priority mặc định 100, thứ tự `(priority, id)` ổn định. Đọc đăng ký từ SQLite mỗi lần dispatch, kiểm tra enabled lại khi chạy; không cache trạng thái tắt plugin. Core handler trusted phải async/cooperative, không blocking I/O; timeout 250 ms. Plugin luôn qua manager/worker M2, không outer timeout bao quanh commit để tránh đổi ngữ nghĩa commit handover.
6. Mỗi lượt tối đa 32 handler, ngân sách nhận handler mới 5 giây; handler đang chạy được hoàn tất theo giới hạn riêng (plugin runtime 5 giây cộng truy vấn/log có timeout). Handler bị bỏ qua có outcome rõ; plugin bỏ qua/lỗi được ghi log và last_error nếu DB khả dụng, stderr nếu ghi log thất bại. Không có queue nền hoặc task tách rời. Dispatch sau lưu luôn trả report, không trả lỗi HTTP làm người dùng hiểu nhầm giao dịch lưu thất bại.
7. Manual run giữ nguyên input/response M2. Lifecycle plugin nhận `{event, occurred_at_ms, correlation_id, payload}` qua input JSON hiện có; ABI Wasm giữ nguyên. Correlation dùng chung envelope, response và log.
8. SQL runtime đưa vào `storage/plugin_kv`; điều phối worker, hủy và commit guard vẫn giữ hành vi M2. Storage helper mượn đúng connection, không spawn tác vụ DB. Auth bootstrap/login dùng Origin và kiểm tra riêng; mutation có session dùng guard CSRF. API lỗi mặc định `ApiError` text; response JSON `PluginRunResponse` của manual run là ngoại lệ tương thích có chủ đích.

## Hệ quả và kiểm chứng

Không xây CPT/Media, settings, theme switching hay before-save pipeline trong M2.5. Test registry xung đột/cập nhật/lifecycle, restart block và dữ liệu người dùng; hook thứ tự, trùng đăng ký, lỗi/timeout, plugin tắt, event sau commit; hồi quy plugin cancellation/commit. Script standards kiểm tra ranh giới SQL (kể cả raw SQL), dependency/I/O core, route guards và fetch tập trung; test cố ý vi phạm xác nhận exit code. Review vẫn kiểm tra trùng trách nhiệm và lời gọi động mà công cụ tĩnh bỏ sót.
