# Active agent discussions (Legacy / Optional)

Quy trình chính của OpenDash hiện là **EGM** (`docs/designs/`). Thư mục `.agents/discussions/` chỉ giữ cho các ghi chú thảo luận phụ khi cần; các luồng thảo luận cũ tại `docs/archive/discussions/` chỉ đọc để tham khảo lịch sử.

See [template](../templates/DISCUSSION.md), `PROTOCOL.md` (Git history) and [ADR-0003](../../docs/decisions/ADR-0003-agent-consensus-governance.md).

An OPEN/NEEDS_RESPONSE topic has explicit Next agent (atlas/nova/forge/harbor/orion). After evidence-backed consensus, coordinator adds a Final Decision CONCLUDED_BY_AGENTS, Next agent: none — no automatic owner signoff. If material disagreement or risk needs final review, refer the decision to Atlas under [ADR-0010](../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md); record the verdict or BLOCKED with missing evidence. Only an agent with explicit file ownership publishes resulting docs PR.

The `discussion-turn-responder` (Git history) skill is archived; no background monitor/agent loop.
