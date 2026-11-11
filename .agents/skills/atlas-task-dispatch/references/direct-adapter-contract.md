# Hợp đồng adapter Atlas → Chat On Steroids (chưa kích hoạt)

Tài liệu này mô tả **điều kiện để cho phép `SEND`**, không là chứng cứ đã có kênh remote hoạt động. Trạng thái runtime và thông tin xác thực của hai host không được đưa vào Git.

## Hai đích độc lập

| Đích | Host | Điều phải xác minh tại host |
| --- | --- | --- |
| `forge` | Windows / CoS | Local Control API có authentication/authorization an toàn, đúng CoS Project `opendash-cms`, worktree Forge và đường tạo **main chat**; không dùng worker |
| `harbor` | macOS / CoS | Cùng các điều kiện trên tại máy Mac; Atlas chỉ gọi qua một kênh quản trị bảo vệ riêng được owner chấp thuận, ví dụ SSH được cấu hình và giới hạn quyền; không mở Local API ra Internet |

Không sao chép cookie, token hoặc secret từ CoS chat/GitHub. Kiểm tra security blocker CoS Windows (W01), quyền đọc Project, quyền gọi API và trạng thái app trước mỗi thử nghiệm. **Không dùng watcher/Poller, Issue #5 PONG cũ hay archived dispatch scripts làm adapter mặc định.**

## Giao diện logic tối thiểu (transport cụ thể phải được chứng minh)

- `probe(target)` → định danh host, phiên đã xác thực, đúng project ID ↔ canonical worktree, capability tạo chat chính, phiên bản API; không lộ token.
- `deliver(target, dispatch_key, payload)` → receipt durable `ACCEPTED | REJECTED | UNKNOWN`, input/chat ID nằm **cục bộ** trong nơi lưu được bảo vệ. `dispatch_key` xác định duy nhất `(Issue, Task ID, target, source_sha, lần giao được phê duyệt)`; không dùng chuỗi tự do từ Issue làm lệnh shell.
- `reconcile(target, dispatch_key)` → đọc tình trạng input/chat theo ID và event nguồn trước khi đánh giá lại `UNKNOWN`; không gửi trùng nếu không chứng minh lần trước đã thất bại.
- `status(target, dispatch_key)` → chứng cứ `delivered / assistant turn ended` của giao vận; **không tự suy** `CLAIMED`, PR merged hay `G5_VERIFIED` (các trạng thái đó đối chiếu GitHub).

Payload có giới hạn: `{repository, issue_number, task_id, target, source_sha, base_sha, approved_scope}`. Mỗi bản ghi dispatch lưu timestamp, idempotency key, trạng thái, lỗi đã redact và kết quả reconcile; **không** lưu raw chat hay secrets trong repo hoặc GitHub comments.

## Gate bật `SEND` cho từng host

1. Owner cấp quyền adapter và target project/worktree cụ thể; chứng minh auth/access control, token scope, audit, giới hạn host. Với CoS Windows phải giải quyết các security blocker liên quan trước khi kích hoạt.
2. Dùng task thử nghiệm không có quyền sửa sản phẩm: một yêu cầu PONG một lần tới chat chính mới, xác minh chính xác target agent/project, event assistant hoàn tất, không bị gửi vào worker hoặc chat cũ.
3. Thử duplicate/timeout/crash: input không được gửi lại mù, `UNKNOWN` được reconcile; app offline/credential sai/host sai ⇒ từ chối, không mở endpoint inbound công khai.
4. Lưu evidence đủ host, OS, phiên bản/API, request ID đã redact, result và giới hạn qua PR tài liệu được review. Chỉ khi một host PASS mới bật `SEND` cho **host đó**; host kia vẫn `BLOCKED_ADAPTER`.

Cho đến khi cả hai host có evidence, chế độ vận hành là `ASSIGN` qua GitHub và Forge/Harbor tự gọi `$github-task-pickup` khi nhận thông báo.
