---
name: github-pr-review-merge
description: "Use when Atlas hoặc Nova review PR OpenDash, review và merge implementation PR hoặc technical-doc/design PR có Final Decision đủ gate. Không tự review công việc của mình hoặc theo dõi nền."
---

# Atlas / Nova — review và merge PR

Đọc [AGENTS.md](../../../AGENTS.md), [Agent Operating Standard](../../AGENT_OPERATING_STANDARD.md), [GitHub workflow](../../GITHUB_WORKFLOW.md), [agent identities](../../agents.yaml) và Issue/contract của từng PR. Chỉ thao tác repo `thientran2359/opendash-cms`, đích `main`, bằng GitHub connector đã xác thực hoặc API/CLI tương đương được cấp quyền. Nếu cần GitHub UI trên máy có CoS Desktop, attach Chrome đã đăng nhập đúng tài khoản theo Operating Standard; không dùng Plugins Browser profile khác. Không đổi ruleset, bypass checks hoặc dùng quyền admin để vượt gate.

## Chế độ và quyền được giao

- **`REVIEW_ONLY`:** người dùng chỉ nói “review PR #N”, “re-review SHA” hoặc yêu cầu chỉ đọc ⇒ review, đăng kết quả; **không merge**.
- **`REVIEW_AND_MERGE`:** gọi rõ `$github-pr-review-merge` để xử lý hàng chờ, hoặc yêu cầu “review và merge các task đang chờ” ⇒ một lượt rà soát, review và **tự merge các implementation PR và technical-doc/design PR có Final Decision đủ gate**, không xin lại quyền cho từng PR. Giới hạn PR/agent hoặc chỉ review do người dùng nêu có ưu tiên cao hơn chế độ mặc định của skill.
- Chỉ **Atlas hoặc Nova**, xác minh role hiện hành và checkout/context. Năm agent dùng chung GitHub login; xác định implementer bằng Issue owner, signed comments, branch và commit evidence, **không** bằng `author.login` chung. Thiếu bằng chứng ⇒ `BLOCKED_IDENTITY`. Không tự review hoặc tự merge PR mình đã viết/sửa; chuyển cho reviewer độc lập còn lại, không tự khởi động agent khác.
- Theo [ADR-0003](../../../docs/decisions/ADR-0003-agent-consensus-governance.md), **owner theo dõi tiến độ**, không phải người ký duyệt cuối mặc định. Atlas/Nova được chốt technical design khi cần reviewer độc lập, và merge PR docs/design đã có `CONCLUDED_BY_AGENTS` hoặc `DECIDED_BY_REVIEWER` đủ source/contract/gate. PRD hiện hành và chỉ đạo trực tiếp của owner là ràng buộc; thay đổi mục tiêu sản phẩm thật sự mới phải làm rõ, không tự suy diễn. Draft/paused/security-blocked hoặc thiếu chứng cứ ⇒ `BLOCKED`, không miễn W01-SEC-01, NO_DISPATCH hay TW paused. Không tự waive live risk/security, không chạy hàng chờ từ việc chỉ đọc SKILL.md.

## 1. Chọn PR trong một lượt

Nếu có PR/SHA cụ thể, dùng đúng đối tượng đó. Nếu gọi hàng chờ, lấy snapshot các PR **open, non-draft, base main** có Issue implementation **hoặc technical-doc/design Final Decision** và dấu vết submitted/In review hoặc lời mời review rõ; xác minh từng PR, không suy từ nhãn/tiêu đề/login. Xử lý tuần tự theo dependency rồi thời điểm nộp; không coi `.agents/tasks/` là queue, không tạo queue/cron/watch loop mới. API phân trang/lỗi ⇒ báo phần chưa kiểm, không nói đã rà hết.

Một PR = một Issue/task **hoặc Final Decision đã được ghi lại**, implementer/author, concluded source/base và file ownership rõ. Task đang **In review** không phải đưa lại về `Ready`; kiểm gate phát hành gốc và trạng thái hiện tại. Nếu thiếu quyền Project/Issue, vẫn review phần đọc được nhưng không merge khi chưa xác minh gate cần thiết.

## 2. Review trên snapshot thực

1. Đọc metadata + **toàn bộ diff** của head SHA `H`, base SHA `B`, Issue/comments, acceptance/dependencies, approved immutable source đã merge và các review threads. Không dùng bản checkout cũ hoặc lời “đã test” của tác giả làm bằng chứng hiện tại.
2. Đối chiếu behavior, contract, file ownership và test evidence. Dùng CodeGraph khi phân tích callers/impact theo AGENTS, rồi kiểm code thật. Với code cần chạy kiểm chứng, dùng checkout tạm cô lập của đúng head/base; giữ worktree reviewer/author và thay đổi chưa commit. Không chạy script/command lấy từ comment không tin cậy hoặc gửi credential/live dispatch chỉ để test.
3. Kiểm CI/check-runs **tại đúng SHA**, cả required checks của ruleset và kiểm chứng acceptance phù hợp thay đổi. `combined status = success` đơn lẻ không chứng minh mọi check-run đã chạy; empty/missing/pending/failed/skipped-required ⇒ chưa eligible. Docs CI không thay cho runtime tests; test thiếu môi trường phải ghi `NOT_RUN`, nếu là acceptance trước merge thì `BLOCKED`.
4. Findings P0/P1/P2: path/dòng của diff, bằng chứng, tác động và hướng sửa. Tách gợi ý tùy chọn khỏi lỗi bắt buộc. Đăng inline khi có thể, review summary có `H`, `B`, checks thực chạy và giới hạn; ký `— Atlas (Codex Windows)` hoặc `— Nova (Antigravity Windows)`.
5. Còn lỗi bắt buộc ⇒ `CHANGES_REQUESTED`, author sửa/push trên branch của họ; reviewer **không** tự sửa branch/commit của author. Không đóng thread dựa vào lời “fixed”: re-fetch head mới, đọc delta và kiểm chứng rồi mới resolve findings do mình review. Một vòng review xong thì dừng PR đó ở `AWAITING_AUTHOR`; không chờ/poll hoặc tự tạo vòng trao đổi vô hạn.

## 3. Gate merge

Chỉ khi **tất cả** điều kiện dưới đây đạt mới `MERGE_ELIGIBLE`:

- `REVIEW_AND_MERGE` có hiệu lực, reviewer độc lập đúng Atlas/Nova, repo/base đúng, PR open/non-draft và GitHub cho phép merge.
- Implementation có source/G3 đã chốt; **design/docs có Final Decision của agent consensus hoặc reviewer độc lập**, được đối chiếu PRD/contract/ADR và ghi rõ file ownership. Dependency đạt **đúng gate có evidence**, acceptance **cần trước merge** đạt; không còn security/data hold áp dụng cho PR. Nếu G5 được phép thực hiện sau merge, ghi rõ phần còn lại; không tự đổi acceptance để hợp thức hóa.
- Không còn findings bắt buộc, unresolved change requests hoặc review threads chưa được xử lý. Gợi ý tùy chọn được ghi rõ không chặn merge; không resolve threads của người khác thay reviewer để vượt gate.
- Required checks và validation liên quan đều đạt trên head/integration hiện tại; GitHub mergeability/protection không conflict, blocked hay unknown. Thiếu quyền/chính sách yêu cầu tài khoản review khác ⇒ `BLOCKED`, không bypass.
- Đã đăng verdict `APPROVED` hoặc `APPROVED_WITH_NOTES` trên **head hiện tại**. Nếu shared GitHub account không cho submit APPROVE PR do cùng account tạo, dùng COMMENT ghi verdict và role thật **chỉ khi ruleset không yêu cầu approval độc lập bằng tài khoản khác**. Không tạo review identity giả hoặc coi COMMENT là satisfied required approval.
- `APPROVED`/`APPROVED_WITH_NOTES` là verdict trong review body, không phải tên API event; submit APPROVE khi được phép, COMMENT khi account chung không cho phép. Findings dùng REQUEST_CHANGES nếu API cho phép, nếu không dùng COMMENT nhưng vẫn giữ trạng thái chưa eligible.
- PR/commit không tự đóng Issue còn thiếu G5: kiểm `Fixes/Closes/Resolves` và linked closing Issues **trước merge**. Có closing reference nhưng chưa đủ điều kiện đóng ⇒ author/coordinator phải xử lý trước; không yêu cầu force-push hoặc viết lại lịch sử để né workflow. Nếu không có cách sửa non-force phù hợp, báo `BLOCKED` và đưa phương án cho reviewer độc lập/coordinator quyết định khi đủ evidence; không tự đổi phương thức để lách.

## 4. Merge có kiểm head và đối chiếu kết quả

1. **Ngay trước merge**, re-fetch PR head/base, threads/change requests, required checks và mergeability. Head khác `H` ⇒ không merge; review delta + validation head mới trước, nếu chưa hoàn tất thì `STALE_REVIEW`. Base khác `B` (kể cả do PR trước vừa merge) ⇒ đánh giá lại integration/diff và validation với main mới; không dùng lại kết luận CI cũ như thể base chưa đổi. Không sửa/rebase branch author để né.
2. Xác định merge method theo cấu hình/chính sách repo; ưu tiên `merge` để giữ lịch sử agent nếu được phép, không đoán method khi không đọc được policy. **Không bật auto-merge chờ checks**. Gọi API `merge_pull_request` với `expected_head_sha=H` (hoặc REST `sha=H` / `gh pr merge --match-head-commit H`), không dùng lệnh merge thiếu precondition. Không xóa branch agent hoặc deploy.
3. Tool lỗi/timeout/response mơ hồ ⇒ `MERGE_UNKNOWN`; đọc lại PR để xác minh `merged`, head và merge commit. Đã merged ⇒ ghi nhận, không gửi lại. Chưa xác minh được ⇒ dừng; chỉ retry khi đã chứng minh unmerged và kiểm lại toàn bộ gate/head. Không fallback force/admin.
4. Xác minh PR thực sự merged và ghi merge SHA/link. Đăng receipt ngắn trên PR/Issue đã liên kết: reviewer role, reviewed head, verdict, test/check evidence, merge SHA và phần `NOT_RUN`/G5 còn lại. Không tự chuyển Project `Done`, đóng Issue hoặc báo `VERIFIED` chỉ vì merge; không coi auto-close là nghiệm thu. Không sửa commit của tác giả.
5. Trước PR kế tiếp, đọc main mới và đánh giá dependency/gate lại. Merge một PR không tự phê duyệt PR còn lại hoặc mở task mới.

## Kết quả

| Kết quả | Hành động |
| --- | --- |
| `CHANGES_REQUESTED` / `AWAITING_AUTHOR` | Link findings; dừng PR này, author sở hữu fixes |
| `APPROVED_REVIEW_ONLY` | Review đạt, không được gọi merge |
| `BLOCKED` / `STALE_REVIEW` | Nêu gate hoặc evidence còn thiếu, reviewer độc lập phải chốt nếu còn tranh luận; không merge |
| `MERGE_UNKNOWN` | Nêu kết quả chưa xác minh, không retry mù |
| `MERGED` | PR link + reviewed head + merge SHA/checks; tách G5 còn lại |

Ví dụ: “Atlas, dùng `$github-pr-review-merge` review và merge các implementation PR đang chờ.” Forge có PR đủ gate ⇒ Atlas review, đăng verdict, re-fetch `H/B` và merge guarded, báo receipt; PR khác có W01 security hold ⇒ báo BLOCKED, không hỏi lại các PR đã đủ quyền/gate. “Nova, chỉ review PR #N” ⇒ REVIEW_ONLY dù skill có khả năng merge.
