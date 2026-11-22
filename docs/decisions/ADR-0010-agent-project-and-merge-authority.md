# ADR-0010 — Quyền điều phối và merge của các agent

> **Ánh xạ quy trình EGM (cập nhật 2026-11-22):** OpenDash đã chuyển chuẩn phát triển sang **EGM** ([`AGENTS.md`](../../AGENTS.md)). Các thuật ngữ lịch sử trong phần bên dưới được ánh xạ tương đương cho mọi task mới: `G2/G3 READY` → Design Spec trong `docs/designs/<feature>.md` đạt `Status: Approved` (đủ `Approved By`, `Approval Evidence`, `Goldfish Review` và Slice Contract); `G5 VERIFIED` → hoàn tất `Verification Plan` sau merge kèm bằng chứng tại `docs/verification/` và cập nhật `docs/designs/SITEMAP.md`. Quyền tự chủ, nhánh mặc định `agent/<name>` và phân quyền 3 tầng (Owner cho PRD/kiến trúc lõi/breaking · Coordinator `Atlas`/`Aster`/`Nova`/`Orion` ủy quyền chốt Design Spec trong PRD & review/merge mọi PR đủ gate · Implementer self-review/self-merge PR của mình khi đủ CI) giữ nguyên hiệu lực.
>
> **Owner Directive — Scheduled Auto-Pickup (2026-11-21):** Chủ dự án cho phép **Harbor** và **Forge** chạy tự động nhận task định kỳ (ví dụ 1 giờ/lần) theo [`github-task-pickup/SKILL.md`](../../.agents/skills/github-task-pickup/SKILL.md#scheduled-auto-pickup-owner-directive-2026-11-21): chỉ pick Issue đã gắn đúng nhãn `agent:<name>`, đủ 4 điều kiện rảnh (1 task active / branch), được tự chuyển `Backlog → Ready` khi slice phụ thuộc đã merge vào `main` với đủ required CI PASS, và áp dụng Circuit Breaker dừng `BLOCKED` sau 2 lượt lỗi/CI fail liên tiếp.

## Owner directive — nhánh agent mặc định, 2026-11-19

Chủ dự án xác nhận **mỗi agent triển khai tuần tự các task trên nhánh cố định
`agent/<name>` trong `.agents/agents.yaml`**, thay vì mặc định tạo
`task/<agent>-...` cho mỗi Issue. Quy định này có hiệu lực cho task/PR mới của
mọi agent và **ưu tiên quy tắc bắt buộc tách task branch** trong ADR-0005,
ADR-0008. Các commit, PR và task branch lịch sử vẫn giữ nguyên; không đổi
nhánh của PR đang mở hoặc viết lại lịch sử để áp dụng hồi tố.

1. **Nhánh mặc định:** Atlas `agent/atlas`, Nova `agent/nova`,
   Forge `agent/forge`, Harbor `agent/harbor`, Orion `agent/orion`,
   Aster `agent/aster`. Mỗi agent chỉ có **một implementation task active**
   và một writer trên từng file/schema; không nhận task mới khi nhánh còn
   PR/diff chưa bàn giao hoặc task chưa đạt gate giải phóng.
2. **Trước task mới:** xác minh worktree sạch, không có PR/task khác đang chiếm
   nhánh, kiểm `git fetch origin`, `origin/main`, `origin/agent/<name>`
   và lịch sử chưa merge. Đồng bộ `agent/<name>` với `origin/main`:
   fast-forward nếu đủ điều kiện; nếu lịch sử phân kỳ thì chỉ merge được review
   và giải quyết conflict theo ownership. Không tự reset, rebase/force-push,
   bỏ commit chưa merge hoặc tự ghi đè worktree. Không đồng bộ được an toàn
   thì **BLOCKED**, không tạo nhánh khác để lách gate.
3. **PR và G5:** commit có Task ID và trailer `Agent: <Agent>`, non-force push
   lên chính `agent/<name>`, mở PR từ đó vào `main`. Sau merge lại đồng bộ
   từ `origin/main` trước task tiếp theo. Nghiệm thu G5 có thể dùng checkout
   detached/worktree cô lập tại **exact merge SHA**; không cần task branch
   lâu dài chỉ để chạy release/rollback.
4. **Ngoại lệ `task/*`:** chỉ dùng khi có lý do cụ thể cần cô lập trong phạm
   vi task được phép (ví dụ thử nghiệm/rollback, pin release hoặc xung đột worktree),
   được ghi rõ lý do, source/base SHA và ownership trên Issue/PR, đồng thời
   không lách điều kiện một task active, Ready, CI/review hay quyền file.
   Không tạo `task/*` theo thói quen. Sau ngoại lệ, trở về nhánh agent
   mặc định khi đã bàn giao an toàn.

Quy trình thực hành tương ứng nằm ở
[`GITHUB_WORKFLOW.md`](../../.agents/GITHUB_WORKFLOW.md) và
[`AGENT_OPERATING_STANDARD.md`](../../.agents/AGENT_OPERATING_STANDARD.md).
Các gate G2/G3, Project Ready, security, branch protection, CI, G5 và giới
hạn quyền merge của từng agent **không thay đổi**.

## Owner directive — macOS Beta and Harbor, 2026-11-17

Chủ dự án chuyển ưu tiên Usable Beta sang **macOS**. Aster điều phối, chốt
kỹ thuật và tự review; không cần peer review. Aster chỉ tham gia thảo luận
liên-agent khi chủ dự án tạo cuộc thảo luận. Harbor là implementer của toàn bộ
chín task remediation F01–F09 và thực hiện technical retest macOS; một task
active trên `agent/harbor`, không hai writer trên cùng file. Aster chuẩn bị
candidate và phối hợp human dogfood; automation không thay người thử thật.

Chỉ đạo này thay ownership, nền tảng Beta và yêu cầu peer review cũ bên dưới.
Các PR #310–#313 đã merge. G3 vẫn cần immutable source/base, dependencies,
file ownership và Project Ready được xác minh trước pickup. Bằng chứng Windows
cũ giữ nguyên; không suy thành macOS PASS. Beta yêu cầu native macOS B01–B10
và negative regressions trên cùng candidate. Windows/Linux/native common RC và
V1 gates vẫn riêng; không đóng chúng từ Beta PASS. F08 giữ security hold đến
khi có giải thích provenance và kiểm chứng macOS đầy đủ; đối chiếu Windows
vẫn là follow-up bắt buộc trước common RC. F09 cần full release suite macOS
hai lượt và isolated run; receipt Windows thất bại vẫn giữ và phải được đối
chiếu trước common RC, không gọi là Windows defect resolved.

Thứ tự Harbor: M8-S10-T01 → M7-S15-T01 → M5-S11-T01 → M7-S15-T02 →
M7-S15-T03 → M7-S15-T04 → M7-S15-T05 → M6-S09-T01 → M8-S10-T02.
Mọi task sau giữ Backlog đến khi dependencies và slot đủ gate. Aster ghi
self-review/acceptance tại head SHA, checks thực chạy và giới hạn; không waive
security/data, CI, human test hoặc release authorization.


**Quyết định của chủ dự án, 2026-11-11.** Áp dụng cho các milestone OpenDash. ADR này cập nhật phần phân quyền và yêu cầu review của [ADR-0003](ADR-0003-agent-consensus-governance.md), [ADR-0005](ADR-0005-m3-harbor-forge-autonomous-delivery.md) và [ADR-0008](ADR-0008-harbor-forge-autonomous-delivery.md).

## Quyền hạn

| Agent | Vai trò | Quyền |
| --- | --- | --- |
| Atlas, Aster, Nova, Orion | Điều phối & Triển khai tự chủ (`Coordinator / Elephant`) | Quản lý ưu tiên, phân công, handoff, Issue/Project, soạn & chốt Design Spec EGM (trong phạm vi PRD đã chốt), phân xử quyết định kỹ thuật, tự nhận và triển khai task. Có quyền review và merge mọi PR đủ điều kiện vào `main`, kể cả self-review & self-merge PR do chính mình viết. |
| Harbor, Forge | Triển khai tự chủ (`Implementer`) | Tự chọn task chưa có người nhận phù hợp hoặc tiếp tục task mình sở hữu khi Design Spec EGM (`Status: Approved`), tài liệu, dependencies và file ownership đủ. Tự quyết định chi tiết triển khai trong contract, chạy kiểm chứng, self-review và merge PR của mình vào `main`. |

Atlas, Aster, Nova, Orion, Harbor và Forge không cần chờ một agent khác duyệt PR của mình. Quyền tự chủ áp dụng cho task đủ tài liệu để triển khai; nếu cần đổi phạm vi, schema, quyền hoặc contract đã chốt, ghi Design Delta và xử lý theo luồng thiết kế EGM. Các Coordinator (`Atlas`, `Aster`, `Nova`, `Orion`) chốt các quyết định liên task khi cần và ghi Final Decision cùng bằng chứng. Peer review có thể được yêu cầu để thu thập bằng chứng hoặc theo ruleset GitHub.

## Thực hiện

- Trước pickup implementation, đối chiếu G3 nguồn trên `main`, base SHA, Issue/Project Ready, acceptance, dependencies và ownership. Agent tự chủ được đồng bộ Backlog → Ready khi source, scope và ownership đã đủ; có thể tự claim task Ready chưa có owner khi không trùng file hoặc nhánh. Ghi ownership trên Issue/Project trước khi viết code. Chuyển task đã có owner qua handoff được ghi lại.
- Trước merge, kiểm PR đúng repo/base `main`, diff, head/base SHA hiện hành, acceptance, test thực chạy, required CI, security/data, unresolved findings và GitHub mergeability. PR thiết kế cần Final Decision có evidence hoặc chỉ đạo trực tiếp của chủ dự án. Ghi `Self-review: <Agent> — reviewed head <SHA>` cho PR mình viết. Ghi reviewer và verdict thật khi review người khác.
- Merge qua PR với expected head SHA và tuân theo branch protection. Security/data blocker và required checks chưa đạt phải được xử lý trước. Merge commit là bằng chứng tích hợp; G5 và Project Done cần bằng chứng nghiệm thu riêng.

Hướng dẫn vận hành nằm tại [GitHub Workflow](../../.agents/GITHUB_WORKFLOW.md) và [Agent Operating Standard](../../.agents/AGENT_OPERATING_STANDARD.md).

## Bổ sung ủy quyền Aster — 2026-11-17

Chủ dự án trực tiếp đặt **Aster (Codex macOS)** vào vai trò điều phối tương
đương Atlas và yêu cầu thiết lập môi trường làm việc. Aster có cùng quyền
lập kế hoạch/G3, điều phối Issue/Project, chốt quyết định kỹ thuật, review và
merge mọi PR đủ điều kiện, kể cả PR của mình. Các quy tắc dành cho Atlas
trong workflow và manual skills áp dụng tương ứng cho Aster.

Aster dùng nhánh `agent/aster`, danh tính Git theo worktree `Aster` và trailer
`Agent: Aster`. Quyền này không thay đổi ownership của task hiện có; khi
Atlas và Aster cùng tham gia, ghi rõ coordinator chịu trách nhiệm trên
Issue/PR. Giữ nguyên gate G3/Ready, required CI, branch protection, bảo mật,
bảo toàn dữ liệu và phân biệt MERGED với VERIFIED.

## Bổ sung ủy quyền Nova & Orion (Coordinator / Elephant) — 2026-11-22

Chủ dự án chính thức nâng **Orion (Antigravity macOS, `agent/orion`)** lên vai trò Coordinator tương đương **Nova (Antigravity Windows, `agent/nova`)**, **Atlas (Codex Windows)** và **Aster (Codex macOS)**:
- Cả **Nova** và **Orion** đều giữ `role: coordinator` trong [`.agents/agents.yaml`](../../.agents/agents.yaml).
- Có đầy đủ thẩm quyền **Elephant / Coordinator**: soạn thảo, chạy Goldfish review và duyệt Design Spec EGM (trong phạm vi PRD đã chốt: `Approved By: Nova (delegated)` hoặc `Orion (delegated)`), mở/điều phối GitHub Issue/Project, trực tiếp triển khai các Slice code trên nhánh `agent/nova` / `agent/orion`, cũng như review, self-review và merge mọi PR đủ điều kiện vào `main`.
