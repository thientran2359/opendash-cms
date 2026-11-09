# Test matrix (PROPOSED): Muse → CoS Windows → GitHub

- **Agent / ngày:** Forge, 2026-11-09
- **Trạng thái:** **PROPOSED / toàn bộ test chưa chạy**
- **Source SHA:** `8833fc0f9f13903373ace8cdb4e67ee29b8cfb16`
- **Thiết kế:** [Muse Orchestrator POC](muse-cos-win-orchestrator-poc.md)

## Nguyên tắc nghiệm thu

Không chuyển trạng thái test sang PASS nếu chỉ nhìn thấy `HTTP 202`, URL DNS, một file script, một câu trả lời agent tự khẳng định hoặc PR đã merge. Mỗi case có evidence thực chạy và **expected vs actual**. Chỉ ghi `PASS`, `FAIL`, `BLOCKED` hoặc `NOT_RUN` theo quan sát. Không tạo GitHub Issue implementation giả để thử. Với P0–P4 dùng `TEST-PONG-01` không đụng source code.

### Preflight: xác minh trước khi tương tác

- [ ] Đối chiếu CoS version đã cài và docs Local Control API tương ứng.
- [ ] Kiểm tra Local API enabled và `Allow actions` do chủ máy cho phép; không tự bật/đổi quyền ngầm.
- [ ] Xác minh CoS Project ID thật + root/worktree, và ChatGPT Project `opendash-cms` đích; không log ID nhạy cảm ở nơi công khai.
- [ ] Xác minh hành vi main chat, không worker/helper; không làm thay đổi chat/cửa sổ đang làm việc ngoài chat mới của POC.
- [ ] Xác minh tính năng mạng Muse thực sự có và cách giữ service token; không giả định connector có quyền arbitrary HTTP.
- [ ] Xác định vị trí lưu receipt/secret local với ACL; xác định kill switch và thời điểm offline.

## P0 — Discovery và contract

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P0-01 | Đọc `GET /v1/health` qua loopback với token local hợp lệ | 200, có route/version đúng; không in bearer ra terminal/log |
| P0-02 | Local API disabled hoặc `Allow actions` disabled | Không tạo được chat; 403/không kết nối được theo build; không tự nâng quyền |
| P0-03 | So sánh CoS Project ID/root và ChatGPT Project | Có bằng chứng cả hai cùng đúng hoặc **BLOCKED** để quyết định mapping |
| P0-04 | Kiểm tra Muse có thể thực hiện request có auth tới connector/relay | Được gọi thực tế hoặc **BLOCKED** có lỗi cụ thể; không giả định Muse có HTTP tool |

## P1 — Local Control API / một chat chính

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P1-01 | `POST /v1/inputs` với UUID mới, `projectId` approved, `text` chỉ PONG, **không có `sessionId`** | 202 queued và chỉ một main chat mới; không worker/helper, đúng project |
| P1-02 | Kiểm tra `GET /v1/inputs` sau P1-01 | Quan sát delivery rõ `sent/pending/unconfirmed/not_sent`, không gán completed từ 202 |
| P1-03 | Theo dõi `GET /v1/sessions/{id}/events` hoặc SSE | Có phản hồi PONG quan sát được và turn end; không nhầm một event acknowledgement là answer |
| P1-04 | Sai `projectId` | 404 `project_not_found`; không fallback vào chat chung |
| P1-05 | Gửi lại cùng UUID trong cửa sổ outbox còn lưu | Không có chat/tin nhắn thứ hai |
| P1-06 | `projectId` cùng `sessionId` hoặc model override sai | Request bị từ chối, không đổi model/chat đang mở |
| P1-07 | `Allow actions` bị thu hồi giữa quá trình | Ngừng nhận việc mới; thể hiện BLOCKED/UNKNOWN chính xác |

## P2 — Relay an toàn (ưu tiên mock trước live)

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P2-01 | POST `/dispatch` thiếu auth, auth sai hoặc body vượt giới hạn | 401/403/400/413 theo contract, **0 CoS send** |
| P2-02 | Request tới project/agent/mode không allowlist, có prompt/shell tự do | 400/403, không forward sang CoS |
| P2-03 | Cùng request ID + cùng payload sau retry | Trả cùng receipt và session ID, chỉ một send |
| P2-04 | Cùng request ID + payload khác | 409, không ghi đè job cũ |
| P2-05 | Kill process giữa ghi ledger và gửi CoS; khởi động lại | `UNKNOWN` và reconcile input/session trước khi replay; không tự tạo UUID mới |
| P2-06 | CoS timeout/429/503, outbox đã cắt mất dữ liệu | BLOCKED/UNKNOWN và cần xử lý thủ công, không phỏng đoán task đã nhận |
| P2-07 | Hai task active cùng agent; yêu cầu quá dày | Chỉ một active, trả busy/rate limit, không chen ngang tin user |
| P2-08 | Kiểm tra log, HTTP errors, GET `/jobs`, temp/ledger files | Không có bearer, CF secret, raw lịch sử chat, dữ liệu private trong output |

## P3 — Cloudflare, on/off cùng CoS

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P3-01 | Từ ngoài máy, request chưa có Cloudflare Service Auth | Access từ chối, không tới relay |
| P3-02 | Service token hợp lệ / sai / đã revoke | Chỉ caller được phép truy cập endpoint có whitelist |
| P3-03 | Tunnel đang hoạt động và CoS Local API có quyền actions | `/health` báo online, PONG từ relay đi vào đúng main chat |
| P3-04 | Đóng CoS hoàn toàn | Không thể dispatch, trả offline/không kết nối được, tunnel/relay xử lý shutdown; không giả định DNS biến mất |
| P3-05 | Bật CoS trở lại, token local được rotate | Relay khôi phục với token mới và đúng permissions; không dùng token cũ |
| P3-06 | Crash CoS, launcher/watchdog crash, restart Windows, sleep/wake | Fail-closed, phục hồi có giới hạn, không tự resend yêu cầu UNKNOWN |
| P3-07 | Access auth bị revoke hoặc tunnel mất mạng | Không lộ CoS API; Muse có thể phân biệt OFFLINE/BLOCKED/UNKNOWN |

## P4 — Muse thật gọi remote relay

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P4-01 | Muse gửi một `TEST-PONG-01` qua công cụ có auth được cấp | Relay nhận đúng một lần, receipt có request ID, chat mới đúng Project |
| P4-02 | Muse đọc `/jobs/{id}` khi queued/running/responded | Không xem `202` là xong; hiển thị trạng thái theo evidence, phản hồi PONG |
| P4-03 | Muse retry cùng ID sau timeout hoặc network break | Không tạo thêm main chat, kết quả có idempotency ổn định |
| P4-04 | Muse bị thiếu quyền/không có connector thật | Dừng tại BLOCKED và báo chính xác thiếu quyền/capability, không đổi sang đường không được duyệt |

## P5 — GitHub integration (sau khi P0–P4 được duyệt)

| ID | Bài kiểm thử | Expected |
| --- | --- | --- |
| P5-01 | Issue G1/G2 hoặc task G3 không READY | Không dispatch code, không đổi Project Ready/In progress giả |
| P5-02 | Issue thật đã duyệt, có source SHA, base SHA, file owner và dependency | Muse kiểm tra gate + repo HEAD + Issue tồn tại, ghi receipt Issue↔Agent↔Session |
| P5-03 | Trùng Issue ID, đã có PR hoặc branch busy | Chặn giao trùng, kiểm tra current state trước khi retry |
| P5-04 | Chat gửi xong nhưng agent chưa commit/PR | Không chuyển `In review` hoặc `Done` |
| P5-05 | Agent tạo PR, review/CI, merge, verification | Chỉ đổi status theo bằng chứng, không auto-merge, `Done` chỉ sau G5 được duyệt |

## Evidence form cho mỗi case

```text
Test ID:
Date/time (timezone):
Host / OS / CoS version / relay build SHA:
Git baseline SHA / Issue / PR (nếu có):
Điều kiện tiền đề, mức quyền:
Steps đã thực hiện:
Expected:
Actual (đã redacted):
Result: PASS / FAIL / BLOCKED / NOT_RUN
Evidence link hoặc hash (không chứa secret/raw chat):
Risk/next action:
```

**Tổng trạng thái lúc tạo tài liệu: `NOT_RUN` cho toàn bộ test.** Các lệnh kiểm tra Markdown/CI của repository (nếu chạy) chỉ kiểm tra **chất lượng tài liệu**, không được đánh dấu thành POC runtime PASS.
