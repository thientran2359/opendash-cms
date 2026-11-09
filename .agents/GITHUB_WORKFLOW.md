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
   `source_sha`, `base_sha`, `task_id`; **không tạo Issue hoặc gửi chat**.
4. Muse **kiểm tra cả Issue open và closed** theo Task ID (so sánh task ID
   chính xác, không chỉ chuỗi gần đúng). Nếu đã có, update cùng Issue khi
   source version đổi; nếu chưa, gọi GitHub API `create_issue`. Không có
   đảm bảo atomic chống trùng từ GitHub search: Muse phải serialize việc
   publish theo Task ID, xác nhận kết quả sau timeout/ambiguous failure
   trước khi retry. Không sinh Issue thứ hai cho cùng task/đợt triển khai.
5. Gắn label/Project status theo quy ước bên dưới. Tạo label trước khi gọi
   GitHub API (GitHub không tự hiểu `agent:forge` là username assignee). Chỉ
   gán GitHub assignee nếu có tài khoản thực được cấp quyền; agent runtime
   identity nằm ở `agent:*` label hoặc custom field.
6. Khi scope, contract hoặc acceptance thay đổi: sửa tài liệu trong repo,
   review/merge và **đồng bộ lại body Issue + source SHA**. Khi chỉ đổi
   tiến độ, owner hoặc blocker: chỉ cập nhật Issue/Project. Không duy trì
   JSON/YAML task mirror trong Git như một bảng trạng thái cạnh tranh.

### GitHub Project và nhãn đề xuất

Board GitHub Project `OpenDash CMS` có field `Status` cho READY, CLAIMED,
IN_PROGRESS, BLOCKED, SUBMITTED, REVIEWED, MERGED, VERIFIED; lọc theo
Milestone/Module/Agent. Issue chứa label `type:implementation` hoặc
`type:research`/`type:design`, `module:CNT-xx`, `milestone:M3`,
`agent:forge|harbor|atlas|nova|orion` khi được gán và **một nhãn**
`status:*` khớp Project Status. Chuyển trạng thái phải gỡ nhãn cũ; board
không thay thế bằng chứng verification. Issue Research/Design được theo dõi
riêng và không tự gán READY cho implementation. Đây là **quy ước cần cấu
hình** trên GitHub, không có Project/label hay tự đồng bộ nào được tạo ra
chỉ bằng việc thêm tài liệu vào repo.

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
