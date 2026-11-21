# GitHub workflow cho 5 agent — đề xuất chuẩn vận hành

> **Ủy quyền của chủ dự án:** [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md) giao các Coordinator (`Atlas`, `Aster`, `Nova`, `Orion`) điều phối dự án, chốt quyết định kỹ thuật và merge mọi PR đủ gate, kể cả PR của chính mình. Tất cả agent (`Atlas`, `Aster`, `Nova`, `Orion`, `Harbor`, `Forge`) đều tự nhận task đủ Design Spec EGM (`Status: Approved`) / ownership và tự review/merge PR của mình. Required CI, acceptance, security/data và GitHub protection vẫn áp dụng.

**Repository nguồn duy nhất:** `thientran2359/opendash-cms`.
Code nằm trong 5 worktree/checkout **cục bộ** tách biệt trên Windows và macOS;
mỗi agent dùng branch mặc định `agent/<name>` được gán trong [agents.yaml](agents.yaml).
Theo [ADR-0010 (owner 2026-11-19)](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md),
`task/*` không còn là nhánh bắt buộc cho từng Issue; chỉ là ngoại lệ cô lập
được giải thích trên Issue/PR, không được lách ownership/Ready hoặc một task active.
Tạo branch mới không tự tạo được một phiên Codex/Antigravity/CoS hoặc điều khiển
remote. Adapter điều phối Muse chưa được xác nhận/khởi chạy.

**Quy trình thao tác thủ công:** tất cả agent đọc [Agent Operating Standard](AGENT_OPERATING_STANDARD.md)
trong chat đang làm việc (không tạo agent/skill/script "Agent Preflight Check").
Ưu tiên GitHub Connector/Plugin cho Issue/PR/Project. Khi cần GitHub UI đã đăng
nhập trên CoS, dùng **Chat On Steroids Desktop Browser** và attach tab của
**Chrome đang đăng nhập đúng tài khoản GitHub**; Plugins Browser Playwright dùng
profile riêng, không chứng minh tài khoản đang đăng nhập.

## Danh tính 5 agent trên một tài khoản GitHub

Atlas, Nova, Forge, Harbor và Orion dùng chung tài khoản GitHub để thao tác
trên repo; **không tạo GitHub Apps hoặc thêm tài khoản bot**. Danh tính tác nghiệp
lấy từ [agents.yaml](agents.yaml), workspace/branch, Issue ownership và
metadata agent ghi trên PR, commit, comment. Nhãn `agent:<name>` thể hiện
**người chịu trách nhiệm thực hiện** trên Issue, không phải GitHub username.

- **Commit:** giữ Task ID trong tiêu đề và thêm Git trailer `Agent: <Agent>`
  (Atlas/Nova/Forge/Harbor/Orion, đúng agent thực sự tạo commit). **Mặc định
  cấu hình `user.name` theo từng Git worktree**, không đổi `git config
  --global` và không đặt `git config --local user.name` ở repo dùng chung
  nhiều worktree. Thực hiện từng bước an toàn, kiểm tra và rollback theo
  [Git worktree identity setup](GIT_WORKTREE_IDENTITY.md). Giữ `user.email`
  là địa chỉ GitHub no-reply đã xác minh và được cấp từ cấu hình hiện có;
  không tự đặt email giả hoặc secret vào Git.

  ```powershell
  # Chạy trong chính worktree của agent, sau khi kiểm tra common config.
  git config --local extensions.worktreeConfig true
  git config --worktree user.name "Forge"  # thay bằng tên agent hiện tại
  git config --show-origin --show-scope --get user.name
  git commit -m "M3-Sxx-Txx: mô tả thay đổi" -m "Agent: Forge"
  ```

  `extensions.worktreeConfig` bật một lần ở **shared repository config**
  cho những linked worktree cùng Git database; chỉ `user.name` viết bằng
  `--worktree` mới nằm trong config riêng của worktree. Không bật mù nếu
  `core.worktree`/`core.bare` hoặc shared config hiện tại có thiết lập
  bất thường; đọc hướng dẫn trước. Nếu không thể cấu hình an toàn, dùng
  `git -c user.name="Forge" commit ...` cho riêng lệnh commit và ghi lý do.
- **Pull Request:** tác giả ghi `Agent`, `Role`, `Issue/Task ID`, branch,
  baseline/head SHA, phạm vi và kiểm chứng trong body. Dùng
  [PR template](../.github/PULL_REQUEST_TEMPLATE.md) khi tạo PR bằng UI; khi
  dùng API/CLI điền thông tin tương đương. Không ghi nhận một reviewer đã
  thực hiện review chỉ vì tên họ nằm trong danh sách reviewer dự kiến.
- **Review hoặc thảo luận trên PR/Issue:** mỗi lượt ký `Agent` và `Role`,
  kèm đúng `Reviewed head SHA` nếu đang review code. Reviewer ghi
  `Verdict`, findings có vị trí/mức độ, kiểm chứng và giới hạn. Ví dụ:

  ```text
  Agent: Atlas
  Role: Reviewer
  Reviewed head SHA: <full-commit-sha>
  Verdict: CHANGES_REQUESTED
  Findings: P2 — path:line — nguyên nhân, ảnh hưởng, cách sửa
  Validation: NOT_RUN (review tài liệu)
  ```

  Follow-up review phải cập nhật SHA và kết luận sau khi đọc commit mới.
  Nếu tác giả thay mặt reviewer chuyển findings lên PR, ghi rõ
  `Relayed by` và nguồn review; không giả làm review trực tiếp.

**Giới hạn danh tính:** trên GitHub, `author.login`, người gửi review/comment
và người push vẫn là account thực đã xác thực. `Agent`, Git author name và
trailer là **metadata tự khai báo để truy vết**, không chứng minh danh tính độc
lập. Một comment ghi `Verdict: APPROVED` hoặc `CHANGES_REQUESTED` không
tương đương một GitHub review APPROVE/REQUEST_CHANGES và không thay thế required
review trong ruleset. Không tự gán kết quả AI thành approval GitHub, không bỏ
qua CI, ownership hoặc gate. **Atlas, Aster, Nova, Orion, Harbor và Forge áp dụng ADR-0010**:
được tự kiểm tra và merge PR đủ gates; ghi `Self-review: <Agent> — reviewed head <SHA>`
và validation thật trong PR. Không tạo review giả từ một tài khoản chung.

## Nguồn sự thật cho task và tài liệu

| Dữ liệu | Nơi ghi | Ai cập nhật |
| --- | --- | --- |
| Đặc tả thiết kế EGM (`Status: Approved`), contract nền tảng, ADR | `docs/designs/`, `docs/contracts/`, `docs/decisions/` | Elephant (`Atlas`, `Nova`, `Aster`, `Orion`) qua quy trình EGM + Goldfish review và phê duyệt của chủ dự án |
| Điều tra, thảo luận phụ, đề xuất/task draft | `.agents/research/`, `.agents/discussions/`, `.agents/tasks/` | Agent trên branch của mình |
| Giao task, mô tả dễ hiểu, mã ID, owner, dependency vận hành, state | **GitHub Issue** | Coordinator (`Atlas`, `Aster`, `Nova`, `Orion`), sau khi Design Spec đạt `Status: Approved` và merge vào `main` |
| Danh mục Design Spec EGM và baseline `M1–M8` | [`docs/designs/SITEMAP.md`](../docs/designs/SITEMAP.md) | Elephant đăng ký khi `/egm-draft`; cập nhật `Implemented`/`Verified` sau khi merge & kiểm chứng |
| Source diff, test, phản biện, chấp thuận tích hợp | **GitHub PR** | Assignee rồi reviewer/CI |
| Evidence thực chạy/acceptance đã duyệt | `docs/verification/` | Theo quy trình review, không suy từ báo cáo AI |
| Chat/session/runtime secrets/locks | **Không đưa vào Git** | Từng công cụ giữ riêng |

**Task ID:** Theo quy trình EGM mới, mỗi đặc tả triển khai dùng mã `EGM-<slug>` (hoặc `EGM-<slug>-S01` khi tách slice bằng `--slice`). Mã `M{milestone}-S{slice}-T{task}` chỉ còn dùng cho các task G3 lịch sử đã phát hành trước khi chuyển sang EGM. GitHub Issue number là mã vận chuyển duy nhất; ghi cả Task ID và đường dẫn Design Spec tại `source_sha` trong Issue.

## Chuẩn GitHub Issue: đọc được độc lập với source code

**Issue không chỉ có Task ID hoặc link tài liệu.** Một người quản lý phải mở
Issue là hiểu được: đang làm gì, vì sao cần, lợi ích với người dùng, phạm vi,
phần không làm, kế hoạch triển khai chi tiết, kiểm chứng, agent và trạng thái.
Tiêu đề chuẩn: `[EGM-builder-theme-S01] Theme & Template Builder — Slice S01: Theme Manifest & Resolver`.
Mẫu tạo Issue thủ công ở [Implementation task](../.github/ISSUE_TEMPLATE/implementation.yml)
và [Research/Design](../.github/ISSUE_TEMPLATE/research-design.yml).

| Nội dung | Nguồn chuẩn / nơi cập nhật |
| --- | --- |
| Đặc tả thiết kế EGM (`Status: Approved`), kiến trúc, file đích và kế hoạch kiểm chứng | `docs/designs/<feature>.md`, `docs/designs/SITEMAP.md`, `docs/contracts/` |
| Bản diễn giải dễ hiểu, mục tiêu, phạm vi, Detailed Implementation Plan và Verification Plan | **Issue sinh tự động** từ `prepare-github-issue.py` tại `source_sha` trên `origin/main`; không tự đổi yêu cầu riêng trong Issue |
| Người thực hiện, trạng thái, blocker, ngày/luồng giao việc, liên kết PR/chat | GitHub Issue/Project (không commit lại vào docs cho từng lần tiến độ đổi) |
| Kết quả test có phạm vi, môi trường, SHA | `docs/verification/`; dẫn link trong Issue/PR và `docs/designs/SITEMAP.md` |

### Tạo Issue bằng CLI xuất JSON (`prepare-github-issue.py`)

1. Design Spec tại `docs/designs/<feature>.md` đã qua Goldfish review, được chủ dự án duyệt (`**Status:** Approved`), không còn `Open Questions` mở hay `TODO`/`TBD`, có đầy đủ `## Detailed Implementation Plan` (`### Target Files to Create/Modify` + `### Verification Plan`) và đã **merge vào `main`**.
2. Với đặc tả lớn, có thể chia nhiều slice bằng heading `### Slice S01: <Tên slice>` trong `## Detailed Implementation Plan` để tạo các Issue nhỏ độc lập.
3. Coordinator (`Atlas`, `Aster`, `Nova`, `Orion`) chạy `git fetch origin` rồi dùng công cụ **read-only** để đọc blob tại `origin/main`:

   ```shell
   python .agents/scripts/prepare-github-issue.py --source docs/designs/<feature>.md --ref origin/main --agent forge --format json
   # Hoặc tách riêng 1 slice:
   python .agents/scripts/prepare-github-issue.py --source docs/designs/<feature>.md --slice S01 --ref origin/main --agent forge --format json
   ```

   Công cụ tự động từ chối nếu spec chưa `Approved`, thiếu `Detailed Implementation Plan`/`Verification Plan`, còn `Open Questions` chưa chốt hoặc còn placeholder `TODO`/`TBD`. `--base-ref <code-sha>` cho phép tách baseline source code khỏi commit chứa tài liệu. Output có `title`, `body`, `labels`, `project_url`, `source_sha`, `base_sha`, `task_id`; **không tự tạo Issue hoặc gửi chat**.
4. Kiểm tra cả Issue open và closed theo `task_id` chính xác để tránh tạo trùng. Khi đã có, cập nhật cùng Issue nếu `source_sha` thay đổi; nếu chưa, tạo Issue trên GitHub và gắn đúng `labels` (`type:implementation`, `egm`, `agent:<name>`).
5. Project #3 có auto-add Issue mới; xác nhận Issue xuất hiện trên Project và đặt `Status=Ready` sau khi xác minh đủ gate. Agent runtime identity nằm ở label `agent:*`.
6. Khi scope hoặc thiết kế thay đổi: sửa `docs/designs/<feature>.md`, review/merge rồi **đồng bộ lại body Issue + `source_sha`**. Khi hoàn tất merge và kiểm chứng thực tế PASS, cập nhật `Status` trong [`docs/designs/SITEMAP.md`](../docs/designs/SITEMAP.md) (`Implemented` / `**Verified**` kèm link `docs/verification/*.md`). *(Lưu ý: quy trình G0–G5 và `docs/archive/plans/TASK_INDEX.md` đã được lưu trữ lịch sử, không dùng cho task mới.)*

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
  [Module Catalog](../docs/archive/plans/MODULE_CATALOG_V1.md), chia màu theo nhóm
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
| Backlog | Research/design đang mở (`Draft`/`Proposed`), chưa đạt `Status: Approved`; không được tự thực thi |
| Ready | Design Spec EGM `Status: Approved` đã merge vào `main` và đủ `Detailed Implementation Plan` / `Verification Plan` |
| In progress | CLAIMED/IN_PROGRESS (hoặc BLOCKED với blocker nêu trong Issue) |
| In review | SUBMITTED, REVIEWED hoặc MERGED nhưng chưa hoàn tất kiểm chứng thực tế (`Verification Plan`) |
| Done | Chỉ khi đã VERIFIED đúng phạm vi có evidence và chấp thuận đóng |

**Đồng bộ task đã Approved nhưng còn Backlog:** theo [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md),
Atlas, Aster, Nova, Orion, Harbor và Forge có thể trực tiếp chuyển Issue đã giao đúng agent từ `Backlog`
sang `Ready` **sau khi** đối chiếu Design Spec EGM đã merge (`Status: Approved`), dependency,
`Verification Plan`, file ownership và không có blocker thực. Kiểm lại `Ready` trên Project trước
khi claim. Không sử dụng thao tác này để bypass điều kiện `Ready` hoặc nhận
Issue của agent khác.

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
đồng bộ. Không tạo `task/*` để nhận task mới khi PR/task hiện tại vẫn
chiếm quyền; ngoại lệ cô lập chỉ phục vụ phạm vi đã ủy quyền theo ADR-0010.
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
   SHA hoặc lý do không sửa; G2 chỉ được chốt từ Final Decision có evidence
   hoặc chỉ đạo trực tiếp của owner. Atlas phân xử điểm còn tranh luận khi cần.
5. **Reviewer** đọc lại `head SHA` mới, kiểm diff và các thread để xác nhận
   finding đã giải quyết; gửi review follow-up trên cùng PR. **Final Decision có evidence** là căn cứ chốt phương án.
   Quyền merge và self-review áp dụng theo ADR-0010. CI PASS, một comment `LGTM`
   hoặc chat phản hồi chưa phải G5 verification.

**Ngoại lệ reviewer trực tiếp sửa tài liệu:** phải giao lại rõ Issue/PR,
file ownership, branch đích và phạm vi trước khi viết. Reviewer sửa trên
branch `agent/<reviewer>`, commit/push non-force và mở PR riêng hoặc quy
trình tích hợp được duyệt; **không push vào `agent/<author>`** chỉ vì đang
review. Sau khi giao sửa, kiểm lại head và áp dụng quyền review/merge theo ADR-0010.

## Review và merge implementation PR được ủy quyền

Theo [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md), các Coordinator (`Atlas`, `Aster`, `Nova`, `Orion`) có quyền review và merge mọi PR đủ gate, kể cả PR của chính mình. Tất cả agent (`Atlas`, `Aster`, `Nova`, `Orion`, `Harbor`, `Forge`) đều tự review và merge PR thuộc task mình sở hữu mà không cần agent khác phê duyệt. Luồng [github-pr-review-merge](skills/github-pr-review-merge/SKILL.md) áp dụng khi Coordinator hoặc Implementer được gọi review/merge PR; yêu cầu chỉ review giữ nguyên phạm vi.

- Xác minh tác giả bằng Issue, branch và signed evidence. Tác giả tự review ghi
  self-review cùng head SHA và validation. Atlas review PR của agent khác ghi
  verdict và bằng chứng thực tế. Shared GitHub login không chứng minh review độc lập.
- Chỉ implementation có scope/gate đã chốt **hoặc technical-doc/design PR có Final Decision**,
  đúng repo/main, acceptance trước merge đạt, dependency có evidence, findings
  bắt buộc đã giải quyết, required checks tại head/integration hiện tại PASS và
  protection cho phép. Reviewer được chốt phương án kỹ thuật khi cần nhưng
  **không** được tự thay mục tiêu sản phẩm, waive security blocker hoặc risk-accept
  ảnh hưởng live secrets/data chỉ để merge.
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

PR thay đổi skill tuân theo cùng quyền và gate merge. Chi tiết điều kiện/chế độ và output ở SKILL.md.

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
nếu remote đã có, checkout tracking branch đó. **Mỗi task mới dùng lại branch
agent mặc định**, không tự tạo `task/*`. Trước khi nhận task khác, kiểm
worktree sạch, PR/task cũ đã giải phóng, `git fetch origin` và xác minh
`origin/main`, `origin/agent/<name>`, các commit chưa merge.
Đồng bộ branch của chính mình bằng `git merge --ff-only origin/main` khi
lịch sử cho phép; nếu phân kỳ, review các commit và merge `origin/main`
sau khi xử lý conflict đúng ownership, hoặc dừng BLOCKED nếu chưa thể an toàn.
Không tự reset --hard, rebase/force push hoặc làm mất diff/commit.
Nếu có commit từ máy khác, phải đối chiếu trước khi push; non-fast-forward
là BLOCKED, không tự force. Sau PR merge, lặp lại bước sync trước task kế tiếp.
Checkout detached tại exact merged SHA được khuyến nghị cho G5/release test.
Chỉ dùng `task/*` khi cần cô lập thật, ghi rõ lý do/base/head/ownership trên
Issue/PR; ngoại lệ không cho phép hai task active hay né gate.

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
   `git push -u origin agent/<name>` sau khi HEAD/remote đúng; chỉ dùng và
   push `task/*` khi ngoại lệ đã được ghi rõ trên Issue/PR. Commit có
   trailer `Agent: <name>`; PR thường từ `agent/<name>` vào `main`.
5. Đính commit SHA, môi trường và lệnh PASS/FAIL/NOT_RUN vào Issue và PR;
   reviewer hoặc agent được tự review theo ADR-0010 so sánh source, dependency, contract
   và DB migration impact; ghi verdict cùng `Reviewed head SHA` theo quy chuẩn.
6. Chỉ merge khi đủ các gate áp dụng, bao gồm review nếu quy trình yêu cầu,
   CI và quyền merge theo ADR-0010.
   `MERGED` ≠ `VERIFIED` cho scope chưa
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
