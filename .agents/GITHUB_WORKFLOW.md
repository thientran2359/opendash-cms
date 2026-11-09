# GitHub workflow cho 5 agent — đề xuất chuẩn vận hành

**Repository nguồn duy nhất:** `thientran2359/opendash-cms`.
Code nằm trong 5 worktree/checkout **cục bộ** tách biệt trên Windows và macOS;
mỗi agent chỉ sửa branch `agent/<name>` được gán trong [agents.yaml](agents.yaml).
Tạo branch mới không tự tạo được một phiên Codex/Antigravity/CoS hoặc điều khiển
remote. Adapter điều phối Muse chưa được xác nhận/khởi chạy.

## Nguồn sự thật cho task và tài liệu

| Dữ liệu | Nơi ghi | Ai cập nhật |
| --- | --- | --- |
| Scope/G0–G3 đã duyệt, contract, ADR | `docs/` | Người có thẩm quyền chốt; review PR |
| Điều tra, thảo luận, đề xuất/task draft | `.agents/research/`, `.agents/discussions/`, `.agents/tasks/` | Agent trên branch của mình |
| Giao task, mô tả dễ hiểu, mã ID, owner, dependency vận hành, state | **GitHub Issue** | Coordinator/Muse được ủy quyền, sau khi gate sẵn sàng |
| Source diff, test, phản biện, chấp thuận tích hợp | **GitHub PR** | Assignee rồi reviewer/CI |
| Evidence thực chạy/acceptance đã duyệt | `docs/verification/` | Theo quy trình review, không suy từ báo cáo AI |
| Chat/session/runtime secrets/locks | **Không đưa vào Git** | Từng công cụ giữ riêng |

**Task ID:** ID G3 theo `M{milestone}-S{slice}-T{task}` chỉ được gán khi
có breakdown chính thức `READY`. GitHub Issue number là mã vận chuyển duy nhất,
không thay đổi task ID trong kế hoạch; ghi cả hai trong Issue. Không tái cấp
những ID lịch sử đã retired. Nếu chưa có G3, Issue có thể là research/
design task, không cấp nhầm implementation `READY`.

## Chuẩn GitHub Issue: đọc được độc lập với source code

**Issue không chỉ có Task ID hoặc link tài liệu.** Một người quản lý phải mở
Issue là hiểu được: đang làm gì, vì sao cần, lợi ích với người dùng, phạm vi,
phần không làm, các acceptance checklist, dependency, agent, trạng thái và
đường dẫn review. Tiêu đề chuẩn: `[M3-S07-T01] Quản lý danh mục phân cấp`.
Mẫu tạo Issue thủ công ở [Implementation task](../.github/ISSUE_TEMPLATE/implementation.yml)
và [Research/Design](../.github/ISSUE_TEMPLATE/research-design.yml).

| Nội dung | Nguồn chuẩn / nơi cập nhật |
| --- | --- |
| Roadmap, module, capability, approved G0–G3, contract và acceptance | `docs/plans/`, `docs/contracts/` qua review |
| Bản diễn giải dễ hiểu, mục tiêu, phạm vi, ngoài phạm vi và acceptance của task cụ thể | **Issue có bản sao dễ đọc** từ task G3 tại commit nguồn; không được tự đổi yêu cầu riêng trong Issue |
| Người thực hiện, trạng thái, blocker, ngày/luồng giao việc, liên kết PR/chat | GitHub Issue/Project (không commit lại vào docs cho từng lần tiến độ đổi) |
| Kết quả test có phạm vi, môi trường, SHA | `docs/verification/`; dẫn link trong Issue/PR |

Với task code, Issue phải có ít nhất **8 mục**: tổng quan/giá trị, mục tiêu,
phạm vi, ngoài phạm vi, acceptance checklist, dependency, kỹ thuật/file
ownership/approved gate, kiểm thử/rủi ro/rollback; cuối cùng dẫn tài liệu
gốc tại **commit SHA cố định**, baseline code SHA và metadata task/module.
Ngôn ngữ phần đầu ưu tiên tiếng Việt dễ hiểu; phần kỹ thuật ghi đủ để coding
agent không suy đoán API/schema. Chỉ đánh dấu checkbox sau khi có bằng chứng.

### Tạo Issue bằng Muse / CLI xuất JSON

1. G0–G3 của task đã được chốt và **merge vào `main`**, G3 ghi rõ `READY`;
   nếu Full-Track thì G2 phải `AGREED`. Task G1/G2 và ID historical
   `PROPOSED` không được chuyển thành implementation Issue READY. Đối chiếu
   source thực tế để không giao lại code đã làm.
2. Task có heading `### M3-Sxx-Txx — Tên nhiệm vụ` trong module brief (hoặc
   `#` nếu task brief riêng) và các heading theo
   [mẫu SLICE_TASK](../docs/plans/templates/SLICE_TASK.md). Không bắt buộc
   mỗi task có file `.md` độc lập.
3. Coordinator/Muse `git fetch origin` rồi dùng công cụ **read-only** để đọc
   blob tại `origin/main` hoặc commit đã duyệt:

   ```shell
   python .agents/scripts/prepare-github-issue.py --source docs/plans/modules/CNT-03-taxonomy.md --task-id M3-S07-T01 --ref origin/main --agent forge --format json
   ```

   Đường dẫn/ID trong ví dụ là minh họa, chỉ chạy khi file và task thực sự
   tồn tại và đã được duyệt. Công cụ từ chối metadata thiếu, G3 không READY,
   Full-Track G2 chưa AGREED, heading không duy nhất, phần mô tả/checklist
   chưa hoàn chỉnh. `--base-ref <code-sha>` cho phép tách baseline source
   code khỏi commit chứa tài liệu. Output có `title`, `body`, `labels`,
   `milestone` (GitHub number), `milestone_title`, `project_url`,
   `source_sha`, `base_sha`, `task_id`; **không tạo Issue hoặc gửi chat**.
4. Muse **kiểm tra cả Issue open và closed** theo Task ID (so sánh task ID
   chính xác, không chỉ chuỗi gần đúng). Nếu đã có, update cùng Issue khi
   source version đổi; nếu chưa, gọi GitHub API `create_issue`. Không có
   đảm bảo atomic chống trùng từ GitHub search: Muse phải serialize việc
   publish theo Task ID, xác nhận kết quả sau timeout/ambiguous failure
   trước khi retry. Không sinh Issue thứ hai cho cùng task/đợt triển khai.
5. Muse **xác nhận lại** GitHub milestone number + title từ API rồi truyền
   `milestone` vào `create_issue`; gắn `labels` của output. Nếu milestone
   bị đổi/xóa, **BLOCKED**, không âm thầm chọn mốc khác. GitHub Project đã có
   auto-add Issue mới từ repository; Muse xác nhận Issue xuất hiện ở Project
   và đặt `Status=Ready` sau khi xác minh G3, không đánh dấu hoàn thành.
   (GitHub không hiểu `agent:forge` là username assignee.) Chỉ gán GitHub
   assignee nếu là tài khoản thực có quyền; agent runtime identity nằm ở
   `agent:*` label.
6. Khi scope, contract hoặc acceptance thay đổi: sửa tài liệu trong repo,
   review/merge và **đồng bộ lại body Issue + source SHA**. Khi chỉ đổi
   tiến độ, owner hoặc blocker: chỉ cập nhật Issue/Project. Không duy trì
   JSON/YAML task mirror trong Git như một bảng trạng thái cạnh tranh.

**Ngoại lệ hạ tầng POC Muse → CoS Phase 1 (G3 APPROVED 09/11/2026, Issues chưa READY):** các mã
`P1-W01..P1-W08` trong [breakdown Phase 1](../docs/plans/muse-cos-win-phase1-g2.md)
không phải product task `Mx-Sxx-Txx`. Công cụ
`prepare-github-issue.py` **không hỗ trợ** namespace/source/metadata này;
**không chạy CLI đó** hoặc gán module/milestone CMS giả. **Chỉ sau khi** G3
work package tương ứng được review/merge vào `main`, các điều kiện và
dependency đạt, coordinator có thể **tạo Issue riêng theo mẫu**
[Implementation task](../.github/ISSUE_TEMPLATE/implementation.yml):
tiêu đề `[P1-Wxx] ...`, nhãn `type:implementation`, `agent:forge`,
`area:agent-workflow`, đủ 8 mục kỹ thuật/nghiệp vụ, file ownership, gate
`READY`, approved `source_sha` (Git blob immutable), `base_sha`, owner,
acceptance và rollback. Không tự gán GitHub milestone M3 hoặc `Module:` giả.
Kiểm cả Issue open/closed theo **ID chính xác** trước khi publish và giữ
[Issue #5](https://github.com/thientran2359/opendash-cms/issues/5) **chỉ
coordination / `NO_DISPATCH`**, không dùng làm implementation Issue.
**Staged activation:** lúc đầu chỉ W01/W02 được xem xét `READY`; W03–W08
phải chờ P0 evidence + signal contract + cập nhật các brief tương ứng đã
review/merge, rồi pin lại SHA nguồn mới. Đây là ngoại lệ **riêng Phase 1**,
không nới guard của tool phát hành task module CMS.

### GitHub trực tiếp: cấu hình đã xác nhận 09/11/2026

- **Repository:** [Issues](https://github.com/thientran2359/opendash-cms/issues),
  [Labels](https://github.com/thientran2359/opendash-cms/labels),
  [Milestones](https://github.com/thientran2359/opendash-cms/milestones).
- **Project duy nhất:** [Opendash cms #3](https://github.com/users/thientran2359/projects/3).
  Đã có sẵn và liên kết với repo; **không tạo Project thứ hai**. Các view
  `Backlog` (board), `Priority board`, `Team items` (table),
  `Roadmap`, `My items` vẫn được giữ. View `Team items` hiển thị
  `Labels` và `Milestone` bên cạnh Status/PR để phân biệt module và mốc.
- **27 label `module:*`:** theo toàn bộ ID trong
  [Module Catalog](../docs/plans/MODULE_CATALOG_V1.md), chia màu theo nhóm
  FND/CNT/BLD/THM/PLG/SITE/OPS. Các nhóm khác:
  `type:implementation|research|design|verification|process`,
  `agent:atlas|nova|forge|harbor|orion`, `area:agent-workflow`.
  Giữ 10 nhãn mặc định của GitHub, không xóa/đổi nhãn sẵn có.
- **GitHub Milestone:** tạo 9 mốc từ roadmap; tên/mã và number sau được
  `prepare-github-issue.py` dùng để chuẩn bị dữ liệu. Trước mỗi lần tạo
  Issue, coordinator/Muse phải đối chiếu **số và tên live** qua GitHub API:

  | Roadmap ID | GitHub Milestone | Number |
  | --- | --- | --- |
  | M1 | M1 — CMS Baseline | 1 |
  | M2 | M2 — Plugin Sandbox Baseline | 2 |
  | M2.5 | M2.5 — Core Foundation | 3 |
  | M3 | M3 — Content & Media | 4 |
  | M4 | M4 — Visual Builder | 5 |
  | M5 | M5 — Theme & Templates | 6 |
  | M6 | M6 — Plugin Extensions | 7 |
  | M7 | M7 — Site Features | 8 |
  | M8 | M8 — Operations & Release | 9 |

  Chưa đặt ngày kết thúc. M1/M2 là baseline lịch sử theo evidence phạm vi,
  M2.5 đã verified, M3 còn PARTIAL, M4–M8 là kế hoạch; không suy trạng thái
  nghiệm thu toàn module từ việc có Milestone trên GitHub.

**Một trạng thái có một chủ sở hữu:** GitHub **Milestone** là mốc roadmap,
GitHub **Project Status** là trạng thái thực thi; **không tạo**
`milestone:M3` hoặc `status:ready` labels để lưu bản sao trạng thái.
Issue phải mang một `type:*`, một `module:*` cho task thuộc module (một
task liên module ghi primary owner và cross-module dependencies trong body);
và `agent:*` nếu đã phân công. Task quy trình liên module dùng
`type:process` + `area:agent-workflow` nhưng không gán module/milestone giả
(ví dụ [Issue #1](https://github.com/thientran2359/opendash-cms/issues/1)).

**Project Status hiện có (không tạo thêm trạng thái trùng):**

| Project Status | Task workflow |
| --- | --- |
| Backlog | Research/design đang mở, task chưa đủ G3; không được tự thực thi |
| Ready | Implementation G3 READY đã duyệt và đủ nguồn/acceptance |
| In progress | CLAIMED/IN_PROGRESS (hoặc BLOCKED với blocker nêu trong Issue) |
| In review | SUBMITTED, REVIEWED hoặc MERGED nhưng chưa có G5 verification |
| Done | Chỉ khi đã VERIFIED đúng phạm vi có evidence và chấp thuận đóng |

Project đã bật auto-add Issue mở từ repo `opendash-cms` (lọc
`is:issue is:open`) và workflow `Done → close Issue`. Workflow mặc định
`Pull request merged → Done` **đã được tắt**, vì merge không chứng minh
verified và sẽ khiến Issue bị đóng sớm. Trạng thái BLOCKED phải ghi nguyên
nhân/hướng xử lý trong Issue, không tự chuyển Done. Duy trì một nguồn trạng
thái Project duy nhất, không phát tán tiến độ sang JSON/Markdown bản sao.

**Branch protection + CI (G5):** [Ruleset #26833341](https://github.com/thientran2359/opendash-cms/settings/rules/26833341)
Active trên default branch: yêu cầu PR, cấm xóa và force-push, không bắt
buộc số review của tài khoản khác. Workflow
`.github/workflows/governance.yml` kiểm tra tài liệu/task handoff trên Ubuntu
và Windows, read-only, cho mọi PR vào `main` và push `main`. Xem
[báo cáo G5](../docs/verification/2026-11-09-github-governance-g5.md): chỉ
bật required checks sau khi GitHub phát sinh đúng tên check và có PASS.
Hai required checks hiện đã bật sau khi [run đầu tiên của PR #3](https://github.com/thientran2359/opendash-cms/actions/runs/42572931323)
PASS trên cả Ubuntu và Windows: `governance-ubuntu-latest` và
`governance-windows-latest` (source GitHub Actions). Không dùng lời khẳng định
PASS của agent thay cho trạng thái checks trên commit PR đang review.

Các hành động trên là **cấu hình GitHub hiện có**, không phải bằng chứng
Muse dispatch, adapter CoS hay tự động chốt G5 đã chạy. Muse khi được nối
GitHub cần dùng REST/GraphQL theo quyền được cấp, kiểm tra idempotency và
trạng thái post-create; không tự phê duyệt G2/G3/G5.

## Vòng đời trạng thái

`DRAFT` (chỉ .agents) → `READY` (Issue đủ gate/acceptance) → `CLAIMED`
(một assignee, một branch, tránh overlap file) → `IN_PROGRESS` →
`SUBMITTED` (đã commit, push, PR và khai kết quả kiểm thử) →
`REVIEWED` (CI/reviewer) → `MERGED` (được duyệt) → `VERIFIED`
(chỉ đúng scope có bằng chứng). `BLOCKED` có nguyên nhân và next action;
không tự coi lời nhắn đã gửi hoặc commit tồn tại là hoàn thành.

**Một implementation task đang active trên mỗi agent branch.** Nếu còn
PR chưa merge, không đưa task không liên quan vào cùng branch; chờ review/
đồng bộ hoặc dùng task branch tách riêng sau quyết định thay đổi quy ước.
Đảm bảo ownership file/schema/API tránh hai agent ghi cùng một hợp đồng.

## Review tài liệu/thiết kế qua PR giữa các agent

Áp dụng cả với **Draft PR** chứa nghiên cứu/G1/G2 còn `PROPOSED` (ví dụ
[PR #4](https://github.com/thientran2359/opendash-cms/pull/4)). Review thiết kế
không mặc nhiên cấp quyền triển khai hoặc chuyển G2 thành `AGREED`.

1. **Tác giả PR** chỉ định reviewer và ghi rõ trong PR body hoặc comment:
   mục tiêu, tài liệu/commit cần xem, tiêu chí phản biện, người sở hữu sửa
   đổi, hình thức phản hồi và điều kiện review lại. Mặc định tác giả PR sở hữu
   việc chỉnh sửa các file thuộc PR; không giao hai agent viết cùng file.
2. **Reviewer** đọc chính xác `head SHA` của PR và ghi findings **trực tiếp
   trên GitHub PR** bằng inline review comment (nếu gắn được dòng) hoặc
   Conversation comment/review summary. Mỗi finding nêu mức độ ưu tiên,
   file/đoạn liên quan, vấn đề, hậu quả và đề xuất sửa cụ thể. Nếu không có
   finding, ghi rõ phạm vi đã xem và giới hạn kiểm tra. Reviewer có thể giữ
   repo ở chế độ read-only; **được phép ghi GitHub review comment** không
   đồng nghĩa được phép sửa file, commit hoặc push.
3. Nếu reviewer **không có quyền hoặc công cụ ghi GitHub PR**, báo blocker
   cùng lỗi cụ thể và chuyển toàn bộ findings qua chat cho coordinator/tác
   giả; **tác giả đưa bản tổng hợp lên PR có ghi nguồn và liên kết phiên
   review**, rồi mới xử lý. Không coi phản hồi chat đơn lẻ là review đã được
   lưu trên GitHub; không tuyên bố đã comment khi API chưa xác nhận.
4. **Tác giả PR** phân loại từng finding (`accept`, `reject` có lý do,
   hoặc `needs-decision`), sửa tài liệu trên **branch head của mình**, chạy
   checker phù hợp, commit và non-force push. Trả lời từng thread bằng commit
   SHA hoặc lý do không sửa; không tự đánh dấu quyết định G2 đã được chốt.
5. **Reviewer** đọc lại `head SHA` mới, kiểm diff và các thread để xác nhận
   finding đã giải quyết; gửi review follow-up trên cùng PR. Chỉ người có
   thẩm quyền mới chốt G2/merge; CI PASS, một comment `LGTM` hoặc chat phản
   hồi chưa phải G5 verification.

**Ngoại lệ reviewer trực tiếp sửa tài liệu:** phải giao lại rõ Issue/PR,
file ownership, branch đích và phạm vi trước khi viết. Reviewer sửa trên
branch `agent/<reviewer>`, commit/push non-force và mở PR riêng hoặc quy
trình tích hợp được duyệt; **không push vào `agent/<author>`** chỉ vì đang
review. Sau khi giao sửa, cần một lượt review độc lập với thay đổi đó.

## Review và merge implementation PR được ủy quyền

Theo chỉ đạo chủ dự án ngày 10/11/2026, **Atlas và Nova được phép review và tự
merge implementation PR do agent khác thực hiện** khi được gọi luồng
[github-pr-review-merge](skills/github-pr-review-merge/SKILL.md) trong chat.
Đây là ngoại lệ cụ thể cho quy tắc không auto-merge: một lượt do người dùng
khởi tạo, không background polling; không cần xin lại quyền cho mỗi PR đủ gate.
Lệnh chỉ “review/re-review PR” không cấp quyền merge.

- Xác minh reviewer/implementer bằng role, Issue, branch và signed evidence;
  login GitHub dùng chung không chứng minh tự review hoặc độc lập. Không tự
  review/merge PR mình viết; Atlas và Nova phải review chéo nếu là tác giả.
- Chỉ implementation có scope/gate đã duyệt, đúng repo/main, acceptance trước
  merge đạt, dependency có evidence, findings bắt buộc đã giải quyết, required
  checks tại head/integration hiện tại PASS và protection cho phép. Không dùng
  quyền này để chốt PRD/G2/contract/risk acceptance mới hay bỏ security blocker.
- Review trên head/base SHA cụ thể; re-fetch trước merge và sau mỗi PR. Head
  đổi phải re-review; base đổi phải đánh giá integration/validation lại. Merge
  dùng expected head SHA, không bật auto-merge đợi checks, không force/admin
  bypass, không sửa branch tác giả hoặc xóa branch agent.
- Nếu account chung không được APPROVE, COMMENT ghi verdict/role chỉ đủ khi
  ruleset không đòi approval bằng account khác; không giả satisfied approval.
- Timeout merge phải đối chiếu PR/merge SHA trước retry. Ghi receipt lên PR/Issue;
  **MERGED khác VERIFIED**. Không tự Done/close; closing keywords trong PR hoặc
  commit còn có thể đóng Issue thiếu G5 phải được author/coordinator xử lý
  trước merge. Khi cần G5 sau merge, giữ Issue mở và ghi phần còn NOT_RUN.

Skill mới cũng là thay đổi do Atlas/Nova viết: tác giả không tự merge PR tạo
hoặc cập nhật skill của mình. Chi tiết điều kiện/chế độ và output ở SKILL.md.

## Khởi tạo code trên từng máy

Bước 1: clone độc lập ở máy đó từ GitHub nếu chưa có; kiểm tra `git remote -v`.
Bước 2: `git fetch --prune origin`, kiểm tra `origin/main` và `git status`.
Bước 3: tạo đúng worktree/branch của agent được giao (ví dụ Atlas Windows):

```powershell
python .agents/scripts/bootstrap-worktree.py --agent atlas --target "C:/Work/opendash-atlas"
python .agents/scripts/bootstrap-worktree.py --agent atlas --target "C:/Work/opendash-atlas" --apply
```

Lệnh mặc định chỉ in kế hoạch, `--apply` mới tạo worktree và yêu cầu repo clean;
không tự push, không force, không xóa thư mục. Trên Mac dùng
`python3 ... --agent harbor --target /path/to/opendash-harbor`.
Có thể dùng một clone chung làm bare/common Git database trên **từng máy**
và một worktree local cho mỗi agent. Không dùng remote filesystem cho IDE.
Không chạy lệnh bootstrap đồng thời trên cùng clone.

Branch `agent/<name>` lần đầu tạo từ `origin/main` nếu local/remote chưa có;
nếu remote đã có, checkout tracking branch đó. Sau khi có PR được merge,
coordinator yêu cầu agent đồng bộ: `git fetch origin` rồi
`git merge origin/main` trong branch của chính mình, xử lý conflict bằng
review, không reset --hard hoặc force push. Nếu có commit từ máy khác, trước
khi push phải fetch/merge/refactor theo phê duyệt; `push` từ chối non-fast-forward
là BLOCKED, không tự force.

## Checklist trước khi push / tạo PR

1. Nhận Issue có `task ID`, approved gate, exact file ownership, base SHA,
   acceptance và exclusions; xác nhận branch đúng, worktree clean ngoài task.
2. Kiểm tra `git diff`/`git status`, stage từng file thuộc task; không
   stage database/uploads, secret hoặc mọi thay đổi tình cờ của agent khác.
3. Chạy kiểm thử phù hợp; với tài liệu chạy
   `python cms/scripts/check-plans.py`,
   `python cms/scripts/check-plans.py --self-test`,
   `python .agents/scripts/check-agent-artifacts.py` và `git diff --check`.
4. Commit ghi task ID (ví dụ `M3-S03-T01: ...`), non-force
   `git push -u origin agent/<name>` sau khi HEAD/remote đúng.
5. Đính commit SHA, môi trường và lệnh PASS/FAIL/NOT_RUN vào Issue và PR;
   reviewer so sánh source, dependency, contract và DB migration impact.
6. Chỉ merge sau review/CI/approval. `MERGED` ≠ `VERIFIED` cho scope chưa
   chạy nghiệm thu; cập nhật trạng thái trong docs khi có evidence.

## Giới hạn Muse

Muse chỉ được đọc Issue/branch/PR và gửi task cho 2 CoS + 2 Antigravity
khi có adapter thực sự hoạt động và user đã cấp quyền. Adapter cần mapping
đúng host/session/worktree, task ID idempotent, receipt `accepted/running/
finished`, timeout, khả năng resume và chống gửi trùng sau lỗi mơ hồ.
Không có Muse adapter được cài trong lần chuẩn hóa tài liệu này.

**Chat CoS:** khi adapter thực sự được kiểm chứng, dispatch tới CoS Win/Mac
phải tạo **chat chính mới trong Project `opendash-cms`**, không dùng worker.
Payload chỉ mang Issue number, task ID, mô tả, source SHA và agent/worktree;
không dùng chat ID/runtime secret làm dữ liệu Git. `accepted` hoặc chat đã tạo
không có nghĩa task đã xong; Muse chỉ cập nhật `SUBMITTED`/`VERIFIED` khi có
bằng chứng tương ứng. Domain/tunnel và API theo host là cấu hình triển khai
riêng, không mặc định đã cài hoặc đang chạy từ tài liệu này.
