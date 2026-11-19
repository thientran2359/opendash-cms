# Aster thử giao nhận CoS sau review — 19/11/2026

Owner yêu cầu thử trước rồi ghi quy trình. Aster thực hiện bằng adapter hiện hữu,
không sửa code adapter, không tạo watcher hoặc thay task Harbor.

## Kết quả thực chạy

- Lệnh thông thường `probe harbor` từ Mac báo MAC_SSH_HOST_NOT_CONFIGURED vì
  mặc định là đường Atlas Windows qua SSH.
- Chạy cùng adapter với `--local probe harbor`: CoS 2.1.53, authenticated health,
  actions enabled, security OK, đúng project/worktree và branch agent/harbor,
  can_send=true. Không thay config hoặc mang credential khỏi host.
- Đọc hết phân trang `/v1/sessions?limit=50`, lọc project đã pin: 47 sessions,
  không có active/unclear turn. Chỉ dùng metadata, không đọc transcript.
- Live GitHub cho thấy #376 Closed/Project Done sau #398 và #400; #377 In progress,
  PR #401 Draft đang chờ coordinator ACK hai file backend. Không gửi lại #376
  hoặc yêu cầu implementation vượt ownership chưa duyệt của #377.
- Gửi một PONG qua hàm send hiện hữu của adapter, cùng semantics send-pong.
  Receipt ban đầu ACCEPTED, duplicate=false. Sau đó `--local status harbor`
  với cùng request xác nhận outbox_delivery=sent, assistant_turn_ended=true,
  pong_verified=true, unexpected_tool_calls=false. Harbor trả đúng PONG.
- Runtime request/input/project IDs và credential giữ trong private local receipt,
  không ghi vào repository. Không có production edit hoặc implementation pickup
  trong phép thử này.

## Phạm vi đã chứng minh và giới hạn

PASS giao nhận main chat mới trên chính Mac và phản hồi PONG hoàn tất.
NOT_RUN gửi yêu cầu sửa implementation sau review; không có bằng chứng task
được tiếp tục hoặc hoàn thành từ phép thử này.

`status-task` của #376 đã có ACCEPTED/sent/turn ended từ lần dispatch trước.
Adapter hiện khóa target/Issue/Task qua nhiều SHA, nên không hỗ trợ gửi tiếp
cùng task mỗi lần review. Không xóa lock hoặc tạo request mới để né giới hạn.
Quy trình Aster phải phân biệt đường gửi lần đầu đủ gate với resume chưa hỗ trợ.
Cơ chế resume/re-dispatch cần thiết kế và kiểm chứng riêng trước khi dùng.

Quy trình được ghi tại [Operating Standard](../AGENT_OPERATING_STANDARD.md#4-aster-bàn-giao-sau-review-qua-chat-on-steroids).

Agent: Aster
