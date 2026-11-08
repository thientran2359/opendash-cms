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
| Giao task, mã ID, owner, dependency, state | **GitHub Issue** | Coordinator (Atlas/Nova), sau khi gate sẵn sàng |
| Source diff, test, phản biện, chấp thuận tích hợp | **GitHub PR** | Assignee rồi reviewer/CI |
| Evidence thực chạy/acceptance đã duyệt | `docs/verification/` | Theo quy trình review, không suy từ báo cáo AI |
| Chat/session/runtime secrets/locks | **Không đưa vào Git** | Từng công cụ giữ riêng |

**Task ID:** ID G3 theo `M{milestone}-S{slice}-T{task}` chỉ được gán khi
có breakdown chính thức `READY`. GitHub Issue number là mã vận chuyển duy nhất,
không thay đổi task ID trong kế hoạch; ghi cả hai trong Issue. Không tái cấp
những ID lịch sử đã retired. Nếu chưa có G3, Issue có thể là research/
design task, không cấp nhầm implementation `READY`.

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
