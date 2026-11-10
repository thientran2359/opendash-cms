# Chuẩn thao tác chung cho 5 agent OpenDash (thủ công trong chat)

> **M3 override (owner, 2026-11-11):** Đối với **Harbor hoặc Forge đang làm M3**, áp dụng [ADR-0005](../docs/decisions/ADR-0005-m3-harbor-forge-autonomous-delivery.md): tự điều phối, triển khai và tự merge PR đủ required checks/security/test gates, không cần peer review hay Atlas/Nova coordinator. Task phải có G3 merged/Issue/Project Ready thực, branch task riêng tránh công việc cũ; không tự biến merge thành G5 Done. Các quy định review độc lập phía dưới còn hiệu lực cho **ngoài M3**.

> **Áp dụng:** Atlas, Nova, Forge, Harbor, Orion. Đây là hướng dẫn cho **chính agent đang được người dùng gọi**, không phải Agent Preflight Check, agent thứ sáu, script, cronjob hoặc Task Watcher. [AGENTS.md](../AGENTS.md) và [GitHub workflow](GITHUB_WORKFLOW.md) là các quy định nền tảng; quyền review/merge được ủy quyền cho Atlas/Nova vẫn tuân theo workflow và skill riêng.

## 1. Nguồn dữ liệu và công cụ

| Việc cần làm | Công cụ ưu tiên | Khi công cụ không đủ |
| --- | --- | --- |
| Tìm/đọc/cập nhật Issues, PR, comments, reviews, CI | **GitHub Connector/Plugin** với quyền truy cập phù hợp | GitHub REST/GraphQL/CLI đã xác thực; nếu lỗi hoặc thiếu quyền, báo `BLOCKED`/`NOT_CHECKED`, không suy diễn |
| Kiểm GitHub Project #3: `Status`, `Ready` và gate | GitHub Project v2 API với credential đủ quyền | **Giao diện GitHub đã đăng nhập** trên đúng Chrome qua CoS Desktop Browser khi phù hợp; thiếu quyền `read:project`/không xác minh được ⇒ không tự nhận task mới |
| Git status, đồng bộ, đọc/sửa/test/commit/push | Terminal/Core trong **workspace hiện tại** | Không dùng thông tin GitHub từ file task lưu cũ hoặc trí nhớ làm trạng thái thực tế |
| Giao diện GitHub yêu cầu tài khoản đăng nhập trên máy chạy CoS | **Chat On Steroids Desktop Browser** gắn vào tab Chrome đã đăng nhập, sau khi xác nhận đúng browser và tài khoản | **Không dùng Chat On Steroids Plugins — Browser** như đại diện cho Chrome đã login; Playwright Browser là phiên riêng và không có tham số chọn profile trong công cụ hiện có |

### Quy tắc Chrome profile (Forge/Harbor khi có Desktop Browser)

1. Dùng `browser_tabs(action="list")` của **Desktop** để xem các browser; chọn browserId **theo danh sách mới mỗi phiên**, không lưu cố định browserId/tabId trong Git.
2. Liệt kê tabs của từng browserId, tìm tab `github.com` của tài khoản có quyền với repo `thientran2359/opendash-cms`. Dùng `browser_snapshot` để xác nhận giao diện đã đăng nhập và quyền đúng repository. **Trang GitHub public mở được không tự chứng minh đã đăng nhập.**
3. Với tab phù hợp, dùng `browser_tabs(action="attach", tabId=...)` rồi các thao tác/snapshot Desktop. Khi cần tab mới, chỉ mở **trong browserId đã xác nhận**. Phải kiểm tra trạng thái sau thao tác; tab handle của Desktop không dùng được với Plugins Browser.
4. **Không** mở profile Chrome thứ hai, không can thiệp `user-data-dir` đang chạy, không sao chép cookie hoặc token. Nếu không có tab/browser đúng hoặc không chứng minh được quyền, **không** tự động làm thao tác GitHub UI có xác thực; dùng GitHub Connector hoặc báo giới hạn.
5. Trên Codex/Antigravity hoặc máy không có CoS Desktop, ưu tiên GitHub Connector/API đã cấp quyền; **không bắt buộc** sử dụng Desktop trên hệ thống không có công cụ đó.

**Khảo sát Forge Windows ngày 10/11/2026:** Desktop tìm được phiên Chrome có các tab của repo và dấu hiệu tài khoản đã đăng nhập, attach tab thành công. Trong khi đó Plugins Browser chỉ hiển thị `about:blank` trong phiên riêng. Vì công cụ Plugins Browser không có option chọn Chrome profile, **giải pháp hiện tại là đổi công cụ điều khiển UI**, không tuyên bố đã sửa cấu hình profile của Plugins Browser.

## 2. Trình tự thao tác trong mỗi yêu cầu

1. **Nhận diện:** đọc [agents.yaml](agents.yaml), kiểm workspace thật, nhánh và `git remote -v`. Mặc định chỉ sửa branch của agent `agent/<name>` hoặc branch đề xuất/PR đã giao đúng ownership; không tự ghi vào branch agent khác.
2. **Lấy trạng thái Git:** `git status --short --branch`, `git branch --show-current`, `git fetch --prune origin`, so HEAD với `origin/<branch>` và `origin/main`; kiểm dirty files trước khi sửa. `fetch` **không phải** `pull`: chỉ cập nhật remote refs. Khi cần đồng bộ source, chọn `git pull --ff-only` **chỉ nếu** branch đích/worktree và lịch sử cho phép fast-forward; nếu không, theo [quy trình đồng bộ main](GITHUB_WORKFLOW.md#khởi-tạo-code-trên-từng-máy), giữ toàn bộ changes/PR, không tự `reset --hard`, force/rebase để né conflict.
3. **Đọc GitHub thực tế:** dùng GitHub Connector để fetch Issue/PR/comments/Project Status, exact Task ID, approved source/base SHA, acceptance/dependencies, ownership và PR đang mở. Thiếu quyền thì báo thiếu bằng chứng, không thay `Ready` bằng label `agent:*`.
4. **Quyết định action theo skill:** với task đã nhận `RESUME_CURRENT` giữ trạng thái `In progress`/`In review`, cho phép dirty diff **đã xác minh thuộc chính task** và feedback đúng PR. Với `PICK_NEW` cần Project `Ready` được xác minh, clean worktree, branch không chiếm dụng và gate đầy đủ. Không có task phù hợp thì đề xuất tối đa 3 candidate `PROPOSED` để coordinator (Atlas/Nova hoặc agent được giao) quyết định phân công; với task đang có owner khác, chỉ chuyển giao sau khi current task owner/coordinator xác nhận release ownership và cập nhật Issue/Project. Thiếu handoff, `Ready` hoặc gate thì `BLOCKED`, không bắt đầu code. Chỉ hỏi owner khi cần mục tiêu sản phẩm mới hoặc quyền thao tác ngoài thẩm quyền agent.
5. **Làm việc và báo cáo:** chỉ sửa file thuộc ownership, chạy tests đúng phạm vi, ghi `PASS/FAIL/NOT_RUN`, commit/push non-force vào đúng nhánh, dẫn commit/PR/Issue. Theo [quy chuẩn danh tính trên GitHub](GITHUB_WORKFLOW.md#danh-tính-5-agent-trên-một-tài-khoản-github), commit có `Agent: <name>` trailer, PR ghi agent thực hiện và mỗi review/comment ký agent cùng `Reviewed head SHA` khi áp dụng; tên tài khoản GitHub chung không chứng minh reviewer độc lập. Đối với review ngoài ngoại lệ M3, dùng GitHub comments và reviewer độc lập. **Atlas/Nova là reviewer/merger độc lập cho implementation PR và technical-doc/design PR đã có Final Decision, khi luồng [github-pr-review-merge](skills/github-pr-review-merge/SKILL.md) được gọi và đủ gate theo [ADR-0003](../docs/decisions/ADR-0003-agent-consensus-governance.md). Owner chỉ theo dõi; security/data blocker chưa được giải quyết vẫn BLOCKED, không được tự risk-accept hay bypass.**

Không có một bước preflight riêng hoặc tiến trình chạy nền; đây là các kiểm tra **ngay trong công việc của agent** khi người dùng giao việc.

## 3. Trả lời thảo luận khi được gọi

Áp dụng [ADR-0003](../docs/decisions/ADR-0003-agent-consensus-governance.md): agent đồng thuận có bằng chứng tự kết luận và đưa vào docs qua PR; khi cần người phân xử, giao reviewer độc lập Atlas/Nova quyết định. Owner chỉ nhận progress report, không chờ owner ký G0–G3 mặc định; vẫn giữ gate về quyền, bảo mật và validation.

Theo [discussion-turn-responder](skills/discussion-turn-responder/SKILL.md), kiểm metadata lượt GitHub PR/Issue và `.agents/discussions/`; chỉ phản hồi một lần nếu role/`Next agent` rõ và đến lượt, re-fetch trước POST. Tài khoản GitHub dùng chung không xác định được danh tính agent; khi mơ hồ báo `AMBIGUOUS`, không tự tạo vòng hội thoại.
