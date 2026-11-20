# ADR-0010 — Quyền điều phối và merge của các agent

## OpenDash EGM authority adaptation v1 — 2026-11-20

**Nguồn:** authority adaptation đã được owner chấp thuận và lưu trong
[migration design](../../.agents/research/egm-migration-2026-11-20/design_draft.md),
được nhập qua PR #413. Owner tiếp tục đồng ý hướng tài liệu-first và yêu cầu
Aster review/tích hợp trong [Issue #414](https://github.com/thientran2359/opendash-cms/issues/414).
**Final Decision: Aster — CONCLUDED_BY_COORDINATOR** cho integration tài liệu;
hiệu lực khi PR tương ứng merge. Không tuyên bố toàn migration đã hoàn tất.

[EGM workflow](../plans/EGM_WORKFLOW.md) là điểm vào duy nhất cho thiết kế mới.
Owner quyết định phạm vi/hành vi sản phẩm, ưu tiên lớn và public release.
Atlas/Aster, theo coordinator được ghi cho topic, quyết định kiến trúc,
API/schema, section acceptance, xử lý findings, chia task và ownership trong
phạm vi đó. Implementer tự quyết cách thực hiện trong contract đã chốt.
Không hỏi lại owner chỉ vì checkpoint upstream gọi người quyết định là tác giả.
Quyết định kỹ thuật phải có provenance; thay đổi scope sản phẩm vẫn về owner.

Đây là adaptation của OpenDash, không phải upstream EGM nguyên bản. Giữ plugin,
persona và standards nguyên vẹn. Goldfish phản biện trong context mới, không cấp
quyền triển khai hay thay required GitHub review. Một topic có một coordinator;
Atlas/Aster chuyển vai trò qua Issue/PR, không ra quyết định cạnh tranh.

Quyền merge, branch/file ownership, security/data, required CI, native/human và
release gates giữ nguyên. G0–G5 còn là nghĩa vụ/metadata tương thích theo bảng
mapping trong guide; không là bộ phê duyệt song song. Task EGM mới vẫn có
handoff đúng mẫu trong nguồn plans được công cụ hỗ trợ, nguồn immutable đã merge
và Project Ready được xác minh. Không đổi ID/receipt/lock hoặc kéo task resume
về Ready chỉ để đổi tài liệu. PR docs/design có Final Decision đi theo quyền
docs hiện có, không cần giả lập implementation G3/Ready.

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
| Atlas | Điều phối toàn dự án | Quản lý ưu tiên, phân công, handoff, Issue/Project, chốt thiết kế và phân xử quyết định kỹ thuật. Có quyền review và merge mọi PR đủ điều kiện vào `main`, kể cả PR do Atlas viết, trong phạm vi sản phẩm chủ dự án đã giao. |
| Nova, Harbor, Forge | Triển khai tự chủ | Tự chọn task chưa có người nhận phù hợp hoặc tiếp tục task mình sở hữu khi G3, tài liệu, dependencies và file ownership đủ. Tự quyết định chi tiết triển khai trong contract, chạy kiểm chứng, self-review và merge PR của mình vào `main`. |
| Orion | Triển khai | Nhận task theo workflow và chuyển PR cho Atlas review/merge. |

Atlas, Nova, Harbor và Forge không cần chờ một agent khác duyệt PR của mình. Quyền tự chủ của Nova, Harbor và Forge áp dụng cho task đủ tài liệu để triển khai; nếu cần đổi phạm vi, schema, quyền hoặc contract đã chốt, ghi Design Delta và xử lý theo luồng thiết kế. Atlas chốt các quyết định liên task khi cần và ghi Final Decision cùng bằng chứng. Peer review có thể được yêu cầu để thu thập bằng chứng hoặc theo ruleset GitHub.

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
