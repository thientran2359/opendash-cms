# Giao thức phối hợp các agent — OpenDash CMS

Năm agent Atlas, Nova, Forge, Harbor, Orion dùng checkout/branch riêng và GitHub làm nguồn sự thật. Áp dụng [ADR-0003](../decisions/ADR-0003-agent-consensus-governance.md) và [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md).

## Luồng quyết định

1. Draft nghiên cứu, phản biện và authored turns tại .agents/research, .agents/discussions hoặc GitHub Issue/PR có turn ký tên. Mỗi turn có In reply to, Next agent, evidence/source SHA, blocker; phân biệt shared GitHub login với role.
2. Sau khi các agent **đồng thuận có căn cứ**, coordinator ghi **Final Decision** gồm phương án được chọn, trade-offs, dependency, file ownership, acceptance/negative cases, phần chưa test và links evidence. **Status: CONCLUDED_BY_AGENTS; Next agent: none.** Không chuyển Next agent: owner theo mặc định. Đồng bộ metadata Issue (status/current turn/next agent) hoặc decision index sau khi xác nhận comment; **không** tự đổi Project Ready/Done.
3. Nếu còn phản biện substantive hoặc cần kiểm định độc lập do rủi ro, chuyển **reviewer độc lập** (ưu tiên Atlas/Nova). Reviewer ghi **DECIDED_BY_REVIEWER** nếu đủ evidence hoặc **BLOCKED** cùng việc cần bổ sung; không tự review việc mình viết, không waive security/data blocker.
4. Agent được giao ownership đưa phần đã chốt vào docs/plans, ADR hoặc docs/contracts qua PR; reviewer khác author kiểm source, diff, CI/docs guards và merge đủ gate theo workflow. Consensus không có nghĩa đã test PASS.
5. Coordinator dùng docs đã merge để phát hành G3/Issue và gán task theo Project Ready, dependency, ownership. Owner chỉ theo dõi; chỉ hỏi khi cần yêu cầu sản phẩm mới hoặc ủy quyền ngoài quyền agent.

## Lưu trữ và an toàn

- Issue là nguồn nhiệm vụ/tiến độ; PR là diff/review/merge; docs/verification là bằng chứng có lệnh, SHA, OS; .agents/tasks không phải queue.
- docs/discussions/threads và .agents/archive là lịch sử read-only, không tiếp tục các turn cũ.
- Không tự tạo agent loop, watcher, dispatch, deploy, force push, bypass required checks, gửi trùng sau timeout hay tiết lộ secrets. BLOCKED_SECURITY hoặc thiếu G5 phải giữ nguyên.
