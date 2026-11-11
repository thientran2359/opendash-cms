# ADR-0010 — Quyền điều phối và merge của các agent

**Quyết định của chủ dự án, 2026-11-11.** Áp dụng cho các milestone OpenDash. ADR này cập nhật phần phân quyền và yêu cầu review của [ADR-0003](ADR-0003-agent-consensus-governance.md), [ADR-0005](ADR-0005-m3-harbor-forge-autonomous-delivery.md) và [ADR-0008](ADR-0008-harbor-forge-autonomous-delivery.md).

## Quyền hạn

| Agent | Vai trò | Quyền |
| --- | --- | --- |
| Atlas | Điều phối toàn dự án | Quản lý ưu tiên, phân công, handoff, Issue/Project, chốt thiết kế và phân xử quyết định kỹ thuật. Có quyền review và merge mọi PR đủ điều kiện vào `main`, kể cả PR do Atlas viết, trong phạm vi sản phẩm chủ dự án đã giao. |
| Nova, Harbor, Forge | Triển khai tự chủ | Tự chọn task chưa có người nhận phù hợp hoặc tiếp tục task mình sở hữu khi G3, tài liệu, dependencies và file ownership đủ. Tự quyết định chi tiết triển khai trong contract, chạy kiểm chứng, self-review và merge PR của mình vào `main`. |
| Orion | Triển khai | Nhận task theo workflow và chuyển PR cho Atlas review/merge. |

Atlas, Nova, Harbor và Forge không cần chờ một agent khác duyệt PR của mình. Quyền tự chủ của Nova, Harbor và Forge áp dụng cho task đủ tài liệu để triển khai; nếu cần đổi phạm vi, schema, quyền hoặc contract đã chốt, ghi Design Delta và xử lý theo luồng thiết kế. Atlas chốt các quyết định liên task khi cần và ghi Final Decision cùng bằng chứng. Peer review có thể được yêu cầu để thu thập bằng chứng hoặc theo ruleset GitHub.

## Thực hiện

- Trước pickup implementation, đối chiếu G3 nguồn trên `main`, base SHA, Issue/Project Ready, acceptance, dependencies và ownership. Agent tự chủ được đồng bộ Backlog → Ready khi source, scope và ownership đã đủ; có thể tự claim task Ready chưa có owner khi không trùng file hoặc nhánh. Ghi ownership trên Issue/Project trước khi viết code. Chuyển task đã có owner qua handoff được ghi lại.
- Trước merge, kiểm PR đúng repo/base `main`, diff, head/base SHA hiện hành, acceptance, test thực chạy, required CI, security/data, unresolved findings và GitHub mergeability. PR thiết kế cần Final Decision có evidence hoặc chỉ đạo trực tiếp của chủ dự án. Ghi `Self-review: <Agent> — reviewed head <SHA>` cho PR mình viết. Ghi reviewer và verdict thật khi review người khác.
- Merge qua PR với expected head SHA và tuân theo branch protection. Security/data blocker và required checks chưa đạt phải được xử lý trước. Merge commit là bằng chứng tích hợp; G5 và Project Done cần bằng chứng nghiệm thu riêng.

Hướng dẫn vận hành nằm tại [GitHub Workflow](../../.agents/GITHUB_WORKFLOW.md) và [Agent Operating Standard](../../.agents/AGENT_OPERATING_STANDARD.md).
