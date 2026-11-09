# TW-01 — Read-only evidence GitHub Project / CoS idle (09/11/2026)

> **Trạng thái:** PARTIAL / BLOCKED cho Project-v2 read permission và full-idle proof. Đây là **metadata discovery** trên Windows, không phải chứng minh Watcher đã chạy hoặc CoS live dispatch. **Nguồn thiết kế:** [TW-01 contract đề xuất](../plans/muse-cos-win-task-watcher-tw01.md), [G2/G3 merged](../plans/muse-cos-win-phase1-g2.md) tại `141e560bd423de479cdeae865bbcf689cf015105`.

## Môi trường, phương pháp và kết quả

| Kiểm tra read-only thực hiện | Kết quả | Phạm vi / giới hạn |
| --- | --- | --- |
| `git fetch --prune origin`, `git rev-parse origin/main` | **PASS** | `origin/main = 141e560bd423de479cdeae865bbcf689cf015105`, PR #8 đã merge; nhánh tài liệu riêng từ main |
| GitHub Plugin `search_issues`, repo + `agent:forge` | **PASS** | Trả Issue #6; chứng minh Issue search được, **không chứng minh được Project Status** |
| GitHub Project #3 view trong browser | **PASS (UI only)** | Mở được trang “Backlog · Opendash cms” ở chế độ chưa đăng nhập; **không xác nhận ID Status/Ready option qua API** |
| Windows Git Credential Manager + GitHub GraphQL `user.projectV2(number:3)` | **BLOCKED — permission** | Credential hiện có không có scope `read:project`; GraphQL báo required scope. **Không có** field ID/option ID được xác minh, **không** xác nhận Issue nào Ready. Không in/lưu token |
| CoS endpoint/token file presence, PID process check | **PASS (snapshot)** | Hai file local tồn tại, PID còn process; không đưa port/PID/token vào PR |
| CoS `GET /v1/health` (bearer, localhost) | **PASS (snapshot)** | Bản chạy `2.1.51`, `ok=true`, `actions.enabled=true`; chỉ xác minh health/auth, không chứng minh idle |
| CoS `GET /v1/agents` | **PASS (snapshot)** | `enabled=true`, `running=false`, không có active agent records tại lúc đọc; **không đủ kết luận Forge idle** |
| CoS `GET /v1/status`, `GET /v1/sessions` | **PASS (schema only)** | Quan sát `toolCalls.running/settling/inFlight`; danh sách sessions có phân trang và metadata như `activeTurnId`, `activityExpiresAt`, `lastTurnEndAt`, `endedAt`. **Chưa xác minh toàn bộ sessions/queued input/approval**; không trích xuất transcript/titles/IDs |
| CoS `state/projects.json` canonical-root match | **PASS (snapshot)** | Đúng một mapping tới current Forge workspace, ID dạng UUID hợp lệ, không ghi ID hoặc path riêng tư |
| GitHub [PR #7](https://github.com/thientran2359/opendash-cms/pull/7), branch `agent/forge` | **BLOCKED for dispatch** | PR `open / not merged` ⇒ branch occupied; không giao việc implementation mới chỉ vì CoS `running=false` |
| W01 security | **BLOCKED_SECURITY** | `W01-SEC-01 HIGH/open`. Không sửa ACL/rotate token/risk acceptance thay chủ dự án |

**Cách kiểm:** GitHub connector read-only issue/PR, browser UI, GitHub GraphQL POST chỉ chứa **query** (không mutation) qua HTTPS với credential Git Credential Manager giữ trong memory; PowerShell đọc local file **metadata** và GET CoS loopback bearer. Output đã redacted, không lưu bearer/GraphQL raw response, CoS private ID/path, titles/chat history hoặc nội dung session.

## Phân biệt kết luận

- **PASS:** GitHub Issue search, CoS health/API metadata, khả năng tìm canonical project mapping.
- **BLOCKED:** Windows credential chưa có Project v2 `read:project`; chưa thể pin `Status`/`Ready` IDs hoặc xác minh `Ready` của Issue. Chưa chứng minh CoS idle bao phủ mọi session/queued input/approval.
- **BUSY guard:** PR #7 còn mở ở `agent/forge`. Dù API trả `running=false`, workflow vẫn cấm dispatch task implementation khác vào branch này.
- **NOT_RUN:** CoS `POST /v1/inputs`, PONG Issue #5, watcher/runtime test, idle→POST end-to-end, scope upgrade, ACL hardening. Không coi thiết kế đã VERIFIED.

## Hành động cần để đóng blocker (sau review)

1. Chủ máy quyết định cấp quyền **chỉ đọc Project v2**, ví dụ bổ sung `read:project` cho credential Windows phù hợp ([GitHub Docs](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project/using-the-api-to-manage-projects)); **không gửi PAT vào chat/Issue/log**. Chạy lại GraphQL read-only để pin Project owner/field/option ID và test phân trang, giữ dữ liệu nhạy cảm ở local config.
2. Chứng minh CoS idle bằng metadata đủ freshness, pagination, queued/pending approvals/helper/compaction; nếu route thiếu evidence thì giữ `UNKNOWN/0 send`. Không truy cập lịch sử chat để lấp thiếu metadata.
3. PR #7 và `W01-SEC-01` cần xử lý theo workflow riêng; khi chưa xong, status TW-01 không tự cấp dispatch. TW-02 cần W01 security disposition + approved recheck.
4. Atlas review TW-01 contract/evidence; sau review/merge mới chuẩn bị Issue triển khai riêng theo approved gate. **Không mở TW-02 runtime hoặc arm Issue #5 từ báo cáo này.**
