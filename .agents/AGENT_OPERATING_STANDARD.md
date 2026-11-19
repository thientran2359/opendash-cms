# Chuẩn thao tác chung cho 5 agent OpenDash (thủ công trong chat)

> **Ủy quyền của chủ dự án:** [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md) giao Atlas điều phối và merge mọi PR đủ gate. Nova, Harbor và Forge tự nhận, tự review và merge task đủ điều kiện. G3/Issue/Project Ready, acceptance, required CI và security/data cần bằng chứng. G5 Done cần kiểm chứng riêng.

> **Áp dụng:** Atlas, Nova, Forge, Harbor, Orion. Đây là hướng dẫn cho **chính agent đang được người dùng gọi**, không phải Agent Preflight Check, agent thứ sáu, script, cronjob hoặc Task Watcher. [AGENTS.md](../AGENTS.md) và [GitHub workflow](GITHUB_WORKFLOW.md) là các quy định nền tảng; quyền review/merge tuân theo ADR-0010 và workflow.

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

1. **Nhận diện:** đọc [agents.yaml](agents.yaml), kiểm workspace thật, nhánh và `git remote -v`. **Nhánh triển khai mặc định là `agent/<name>` cho mọi task mới** theo [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md); chỉ dùng `task/*` như ngoại lệ cô lập đã ghi lý do/base/ownership trên Issue/PR hoặc tiếp tục PR cũ đã giao. Không tự ghi vào branch agent khác. Trước commit, xác minh `git config user.name` và nguồn config theo [Git worktree identity setup](GIT_WORKTREE_IDENTITY.md); không thay `git global` hay shared `--local user.name` để mạo danh agent.
2. **Lấy trạng thái Git:** `git status --short --branch`, `git branch --show-current`, `git fetch --prune origin`, so HEAD với `origin/<branch>` và `origin/main`; kiểm dirty files, PR active và commit chưa merge. `fetch` **không phải** `pull`: chỉ cập nhật remote refs. Trước task mới, khi branch agent sạch/rảnh, ưu tiên `git merge --ff-only origin/main`; nếu phân kỳ thì đọc commit, review merge/conflict theo [quy trình đồng bộ main](GITHUB_WORKFLOW.md#khởi-tạo-code-trên-từng-máy) hoặc BLOCKED. Không tự `reset --hard`, force/rebase để né conflict, không chuyển sang `task/*` chỉ để tránh PR/dirty diff.
3. **Đọc GitHub thực tế:** dùng GitHub Connector để fetch Issue/PR/comments/Project Status, exact Task ID, approved source/base SHA, acceptance/dependencies, ownership và PR đang mở. Thiếu quyền thì báo thiếu bằng chứng, không thay `Ready` bằng label `agent:*`.
4. **Quyết định action theo skill:** với task đã nhận `RESUME_CURRENT` giữ trạng thái `In progress`/`In review`, cho phép dirty diff **đã xác minh thuộc chính task** và feedback đúng PR/branch (kể cả `task/*` lịch sử). Với `PICK_NEW` cần Project `Ready` được xác minh, clean worktree, nhánh agent không chiếm dụng, source đã sync từ main an toàn và gate đầy đủ. Atlas, Nova, Harbor và Forge được đồng bộ Backlog → Ready hoặc nhận task Ready chưa có owner sau khi kiểm đủ gate theo ADR-0010. Orion chuyển đề xuất cho Atlas. Task có owner khác chỉ chuyển giao sau khi current owner giải phóng ownership và Issue/Project được cập nhật. Thiếu handoff, `Ready` hoặc gate thì `BLOCKED`.
5. **Làm việc và báo cáo:** chỉ sửa file thuộc ownership, chạy tests đúng phạm vi, ghi `PASS/FAIL/NOT_RUN`, commit/push non-force vào đúng nhánh, dẫn commit/PR/Issue. Theo [quy chuẩn danh tính trên GitHub](GITHUB_WORKFLOW.md#danh-tính-5-agent-trên-một-tài-khoản-github), commit có `Agent: <name>` trailer, PR ghi agent thực hiện và mỗi review/comment ký agent cùng `Reviewed head SHA` khi áp dụng; tên tài khoản GitHub chung không chứng minh reviewer độc lập. **Atlas, Nova, Harbor và Forge được tự review và merge PR của mình khi đủ gate theo ADR-0010.** Atlas cũng có quyền review và merge PR của agent khác. Security/data blocker chưa được giải quyết vẫn BLOCKED, không được tự risk-accept hay bypass.

Không có một bước preflight riêng hoặc tiến trình chạy nền; đây là các kiểm tra **ngay trong công việc của agent** khi người dùng giao việc.

## 3. Trả lời thảo luận khi được gọi

Áp dụng [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md): ghi Final Decision dựa trên bằng chứng. Atlas chốt quyết định liên task và phân xử bất đồng khi cần. Agent tự chủ triển khai task đủ contract; các gate về quyền, bảo mật và validation vẫn áp dụng.

Theo [discussion-turn-responder](skills/discussion-turn-responder/SKILL.md), kiểm metadata lượt GitHub PR/Issue và `.agents/discussions/`; chỉ phản hồi một lần nếu role/`Next agent` rõ và đến lượt, re-fetch trước POST. Tài khoản GitHub dùng chung không xác định được danh tính agent; khi mơ hồ báo `AMBIGUOUS`, không tự tạo vòng hội thoại.


## 4. Aster bàn giao sau review qua Chat On Steroids

**Owner authorization, 19/11/2026:** sau khi Aster review Issue/PR của Harbor,
Aster chủ động gửi yêu cầu tiếp tục trong cùng lượt chat khi còn hành động hợp lệ,
để chủ dự án không phải chuyển lời. Áp dụng [skill RUN/STATUS](skills/atlas-task-dispatch/SKILL.md)
và [hợp đồng adapter](skills/atlas-task-dispatch/references/direct-adapter-contract.md).

1. Re-fetch Issue/Project, PR head/review/CI và nguồn G3 đã merge. Ghi kết luận,
   ownership ACK và việc cần làm trên GitHub trước khi gửi. Issue đã Done không
   được kích hoạt lại; task chờ quyền/contract chưa duyệt giữ blocker.
2. Kiểm đúng project/worktree/branch bằng probe và đọc hết các trang metadata
   sessions. Không gửi khi có active turn, trạng thái mơ hồ hoặc công việc xung đột.
   Chỉ đọc metadata để kiểm slot, không cần transcript.
3. Dùng adapter được xác minh trên đúng host. Aster chạy ngay trên Mac có thể dùng
   `python3.13 .agents/scripts/cos_dispatch.py --local probe harbor` và tương tự
   `--local status-task`, `send-issue`, `status`. Atlas Windows dùng SSH theo skill.
   Không sao chép endpoint/token, không sửa bảo vệ filesystem để làm probe xanh.
4. Kiểm receipt trước khi gửi. Chỉ gửi một lần khi NOT_SENT và các gate đủ;
   pin Issue/Task/source/base. Với implementation, dùng luồng Issue;
   luồng send-pr hiện chỉ dành cho sửa PR Markdown tài liệu đủ điều kiện.
5. Kiểm riêng ACCEPTED, outbox sent, assistant turn ended và GitHub kết quả thực.
   Timeout/UNKNOWN cần reconcile; không đổi UUID hoặc SHA để né lock.
   Báo đúng mức đã xác minh và blocker, không nhờ chủ dự án chuyển lời khi đường
   gửi được phép còn hoạt động.

**Resume sau review:** giữ nguyên khóa gửi lần đầu. Khi cùng task còn In progress/
In review và có ACK/feedback mới, dùng mục **RESUME Issue** trong skill dispatch.
Lệnh `resume-issue` yêu cầu structured coordinator ACK pin PR/head/base, lượt trước
ACCEPTED/sent và đã kết thúc, project không active turn; tạo một successor duy nhất.
Không xóa lock hoặc dùng UUID mới để né UNKNOWN. `status-task` theo receipt mới nhất.
Xem [thiết kế và cách kiểm chứng](research/2026-11-19-cos-resume.md).

**Thử trực tiếp 19/11/2026:** xem [bằng chứng Mac](research/2026-11-19-aster-cos-post-review.md).

## 5. Handoff Harbor Aster qua Codex macOS

**Chỉ đạo chủ dự án 2026-11-19 (đã làm rõ):** khi Harbor hoàn tất phần việc cần **Aster review hoặc quyết định điều phối/ownership**, Harbor chủ động dùng **Chat On Steroids Desktop** để gửi yêu cầu vào **cửa sổ chat Codex/ChatGPT đang mở** trên cùng Mac. Với chiều **CoS → Codex**, không cần guard xác minh đúng Aster/project/chat đích; guard chống gửi nhầm đó chỉ áp dụng chiều **Codex → CoS**. Đây là thao tác theo yêu cầu trong phiên Harbor hiện hành, không phải lịch tự chạy, hook, watcher hoặc peer review bắt buộc với mọi Issue. Nếu task đủ quyền self-review/self-merge theo ADR-0010 và không có lý do gọi Aster, tiếp tục workflow bình thường.

1. **Chuẩn bị handoff kiểm chứng được:** đọc lại Issue, Project #3, PR, review threads, branch, chính xác `head SHA`/`base SHA`, CI hiện hành, acceptance/test receipts và scope/file ownership. Đã push non-force và có PR khi yêu cầu review implementation; nếu chỉ xin duyệt mở rộng ownership trước khi code, nêu rõ *chưa được sửa* file nào và lý do cần ACK. Phân biệt yêu cầu `REVIEW_ONLY`, `REVIEW_AND_MERGE` (chỉ khi có quyền/yêu cầu phù hợp), hoặc `OWNERSHIP_DECISION`. Draft PR và gate còn thiếu phải được nêu thật; không gọi Draft/CI PASS là đủ điều kiện merge.
2. **Tránh gửi trùng:** kiểm comment/handoff mới nhất trên Issue/PR và các yêu cầu đã gửi có thể quan sát được, cùng số PR và `head SHA`/loại yêu cầu. Nếu đã có yêu cầu tương đương đang chờ Aster thì không gửi lại; nếu SHA thay đổi và cần re-review, chỉ gửi một cập nhật nêu SHA cũ/mới và findings đã xử lý. Khi trạng thái gửi trước không rõ, xác minh trước khi thử lại.
3. **Dùng cửa sổ đang mở trên Mac:** qua CoS Desktop `observe(what="windows")` hoặc `observe(what="active")`, chọn cửa sổ Codex/ChatGPT **đang mở**, gửi vào cuộc trò chuyện hiện tại trong cửa sổ đó. **Không yêu cầu** xác minh project `opendash-cms-mac-codex`, vai trò Aster, đúng PR, hay chuyển sang chat khác; không chặn chỉ vì tiêu đề chat hiện tại nói về Issue/PR khác. Không tạo phiên/agent thay thế để gửi. Quy tắc xác minh đích nghiêm ngặt chỉ dành cho chiều Codex → CoS, không dùng để từ chối handoff chiều này.
4. **Bảo toàn nội dung đang soạn:** trước khi nhập, kiểm tra ô composer. Nếu có bản nháp chưa gửi, tin nhắn đang được gõ hoặc có thao tác nhập đồng thời, không xóa/ghi đè nội dung đó; báo `CHAT_BLOCKED_DRAFT` và dùng comment Issue/PR làm handoff bền vững. Chỉ nhập khi ô composer trống; không đóng tab, xóa dữ liệu hoặc ghi đè clipboard nếu không cần thiết.
5. **Gửi yêu cầu giới hạn và kiểm chứng — dừng tại `SENT`:** tin nhắn ngắn có `Agent: Harbor`, repo/Issue/PR URL, `head SHA` đầy đủ, base, loại yêu cầu, scope file, kết quả CI/test thực chạy, blocker và verdict/ACK cụ thể cần nhận. Gửi một lần bằng thao tác UI được hỗ trợ; xác minh tin nhắn xuất hiện trong **cửa sổ chat đang mở** rồi báo `SENT` và **kết thúc ngay lượt handoff của Harbor**. **Không chờ Codex/Aster phản hồi, không polling, không tiếp tục kiểm tra kết quả review, giao việc hoặc thay đổi GitHub trong cùng lượt chỉ để đợi Aster xử lý.** `SENT` chỉ xác nhận đã gửi tin nhắn, không chứng minh Aster đã review, ACK hoặc giao task. Không copy secrets, token, log nội bộ hoặc dữ liệu site. Nếu lỗi hoặc trạng thái gửi không chắc chắn, báo `UNKNOWN`, không gửi lặp mù.
6. **Xử lý phản hồi trong lượt sau:** ghi link handoff/đề nghị review trên Issue/PR GitHub (nguồn chính thức) **trước khi gửi** nếu phù hợp; nếu không gửi được qua UI vẫn ghi blocker trong PR/Issue và báo người dùng. Codex/Aster tự tiếp tục phần việc sau `SENT`; Harbor chỉ kiểm lại review theo SHA hiện tại và xử lý findings **trong ownership** khi có lượt làm việc tiếp theo. Nếu ACK mở rộng file thì phải có quyết định ghi nhận rõ trên GitHub trước khi sửa. Aster ghi findings/verdict trên GitHub, không coi câu trả lời chat đơn lẻ là review/approval, không tự chuyển `Done`, đóng Issue hoặc merge khi còn gate. Giữ `In progress`/`In review` đúng trạng thái thực; G5 trên exact merged source sau merge vẫn bắt buộc.

**Mẫu nội dung:** `Aster — Agent: Harbor. Repo thientran2359/opendash-cms, Issue #N, PR #M (link), head <40-hex>, base <40-hex>. Request: REVIEW_ONLY | REVIEW_AND_MERGE | OWNERSHIP_DECISION. Scope: ...; CI: ...; native tests/receipts: ...; blockers: ... . Xin review đúng head hoặc ghi ACK/phạm vi ownership trên GitHub PR/Issue; phản hồi P1/P2 theo path:line. Không sửa branch Harbor.`
