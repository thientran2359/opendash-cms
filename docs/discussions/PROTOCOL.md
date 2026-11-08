# Giao thức phối hợp các agent — OpenDash CMS

Năm agent Atlas, Nova, Forge, Harbor và Orion dùng các thư mục code cục bộ
riêng, cùng repository GitHub `opendash-cms` và branch `agent/<name>`.
Đây là quy ước vận hành; danh sách branch trong cấu hình không chứng minh
chúng đã được tạo trên GitHub hoặc phiên AI đã được kết nối.

## Phân quyền nguồn tài liệu và task

- Nghiên cứu, phương án G0–G2, lượt phản biện chưa duyệt: `.agents/research/`
  và `.agents/discussions/`. Mỗi lượt có tác giả, thời gian và bằng chứng,
  không ghi đè phản biện trước đó.
- Bản nháp giao việc: `.agents/tasks/`. Khi đủ gate/acceptance, **GitHub Issue**
  trở thành nguồn trạng thái duy nhất cho task, assignee và dependency.
- **GitHub PR** chứa diff, commit, kiểm thử, review và quyết định merge.
- `docs/` giữ yêu cầu, hợp đồng, kế hoạch đã được duyệt và báo cáo kiểm chứng
  có lệnh/môi trường/kết quả thực. Đồng thuận AI không tự cấp quyền thay đổi
  PRD, contract hoặc đánh dấu `VERIFIED`.

Xem [quy trình vận hành GitHub](../../.agents/GITHUB_WORKFLOW.md) và
[đối chiếu trạng thái M3](../plans/M3_STATUS_2026-11-09.md) trước khi giao
task mới.

## Lịch sử và an toàn

Các thread trong `docs/discussions/threads/` được giữ để truy vết nguồn
tài liệu cũ, không nhận thêm lượt thảo luận mới. Các lựa chọn `PROPOSED`
trong thread không trở thành quyết định được duyệt chỉ vì code đã triển khai.

Không tự động merge/deploy, force push, phát động agent loop vô hạn hoặc
gửi trùng task khi chưa biết kết quả gửi trước đó. Không lưu khóa/secret
hay dữ liệu riêng tư trong GitHub Issue hoặc repository public.
