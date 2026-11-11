# ADR-0008 — Harbor và Forge tự chủ triển khai, review và merge từ M4

> **Cập nhật phân quyền:** [ADR-0010](ADR-0010-agent-project-and-merge-authority.md) là nguồn hiện hành về vai trò, tự chủ triển khai, review và merge. Các quy định cũ dưới đây được giữ làm lịch sử quyết định.

- **Trạng thái:** **AGREED — owner direct, 2026-11-11**. Có hiệu lực như quy định chung sau khi tài liệu được merge vào `main`; chỉ đạo owner trong chat có ưu tiên ngay khi làm việc.
- **Phạm vi:** M4 và các milestone/tác vụ OpenDash tiếp theo; quyền M3 trong [ADR-0005](ADR-0005-m3-harbor-forge-autonomous-delivery.md) giữ nguyên. Không thay đổi các PR đã merge hoặc task thuộc agent khác.
- **Nguồn:** owner trực tiếp yêu cầu Harbor (CoS macOS) và Forge (CoS Windows) có **toàn quyền code, review và merge code giống M3** từ nay về sau, trong Chat On Steroids ngày 2026-11-11. Căn cứ triển khai: [ADR-0003](ADR-0003-agent-consensus-governance.md), [ADR-0005](ADR-0005-m3-harbor-forge-autonomous-delivery.md) và [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md).
- **Baseline:** `main@5b8b79f29dc467a54523a4bcf795b0399122a2ca`. Tác giả ghi nhận: **Harbor (CoS macOS)**.

## Quyền và quy trình

1. **Harbor và Forge tự thực hiện end-to-end:** agent được quyền chủ động nghiên cứu, chốt quyết định kỹ thuật trong G0/PRD hiện hành bằng chứng cứ, lập G2/G3, chia và nhận task đúng sở hữu, code, test, review và merge PR của chính mình **không bắt buộc chờ Atlas/Nova hoặc peer review**. Cả hai cũng có thể review PR của nhau khi có quyền/đề nghị. Reviewer khác vẫn có thể tham gia nhưng không là cổng mặc định đối với task Harbor/Forge.
2. **Nhận task có kiểm soát:** mỗi task implementation cần Issue G3 READY đã merge với `source_sha`, `base_sha`, phạm vi file, acceptance và dependency đã xác minh; cần GitHub Project #3 `Ready` thực. Khi một Issue đã giao đúng Harbor/Forge vẫn ở `Backlog` **chỉ do chưa đồng bộ Project**, agent được tự đối chiếu G3/dependency/ownership, cập nhật `Ready` trên Project bằng quyền đã cấp, xác nhận lại trạng thái rồi mới pickup. Không suy `Ready` từ label hoặc tự chuyển task thiếu gate sang `Ready`. Không đổi quyền sở hữu task của agent khác khi chưa có handoff rõ ràng.
3. **Tách nhánh và writer:** một task implementation active mỗi agent, một writer cho mỗi file/schema ở thời điểm tích hợp; branch/worktree theo task từ `origin/main` mới nhất. Không reset, force-push, ghi đè thay đổi chưa commit hoặc chiếm branch/PR của người khác.
4. **Tự review và tự merge có điều kiện:** tác giả kiểm tra diff/contract/security và ghi rõ `Self-review: Harbor` hoặc `Self-review: Forge` trong PR kèm head SHA và lệnh/kết quả test; **không ghi là peer review độc lập** và không tạo review giả từ tài khoản GitHub chung. Trước merge, fetch lại base/head, đảm bảo PR mergeable, required checks trên đúng head/integration PASS, acceptance phù hợp (bao gồm negative cases cần thiết), không còn security/data blocker. Merge có expected head SHA; không bypass branch protection, tắt checks, force-push hay push thẳng `main`.
5. **G5 riêng biệt:** commit/test/PR merge không đồng nghĩa `VERIFIED` hoặc Project `Done`. Ghi evidence thực chạy theo SHA, môi trường, fixture, kết quả và kiểm rollback/khả năng giữ dữ liệu phù hợp. Nếu thiếu bằng chứng hoặc quyền GitHub/Project, ghi `BLOCKED` với next action.

## Ranh giới vẫn giữ

- Tuân thủ auth/RBAC, CSRF/Origin, dữ liệu nháp/media ACL, SQL/HTML escaping, CSP, migration/backup/rollback an toàn, CI và ruleset. Không tự miễn security blocker hay chấp nhận rủi ro mất/lộ dữ liệu để merge.
- Mọi thay đổi sản phẩm vượt PRD/G0 hoặc thay đổi contract giữa nhiều owner cần được ghi nhận, đánh giá tác động và thống nhất phạm vi; quyền tự chủ không thay thế quyền sở hữu file hoặc bắt buộc đồng bộ giữa hai agent.
- Không tự khởi chạy watcher, cron, Muse dispatch, background merge/deploy; không dùng account GitHub chung để giả định reviewer độc lập.
- Atlas/Nova và các agent khác vẫn áp dụng governance của [ADR-0003](ADR-0003-agent-consensus-governance.md) đối với task của họ. Quyền reviewer độc lập đã ủy quyền cho Atlas/Nova không bị hủy.

## Cập nhật hướng dẫn

Quy định này ưu tiên so với câu chữ giới hạn **riêng M3** trong [Agent Operating Standard](../../.agents/AGENT_OPERATING_STANDARD.md), [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md), [github-task-pickup](../../.agents/skills/github-task-pickup/SKILL.md) và [AGENTS.md](../../AGENTS.md) khi agent đang làm việc là Harbor/Forge. Các gate Project, CI, security và evidence vẫn bắt buộc.
