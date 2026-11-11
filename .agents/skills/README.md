# Active skills

Four **on-demand** project skills live here (no cronjob or background loop;
direct CoS dispatch requires separately verified, authorized host adapters):

| Skill | Khi gọi trong chat | Hành vi |
| --- | --- | --- |
| [github-task-pickup](github-task-pickup/SKILL.md) | "Hãy vào GitHub Issue lấy task của Forge về làm" | Tìm task đang giao/đang làm, kiểm gate/PR/branch; agent tự chủ nhận task đủ điều kiện theo ADR-0010, Orion chuyển đề xuất cho Atlas |
| [discussion-turn-responder](discussion-turn-responder/SKILL.md) | "Kiểm tra các thảo luận đến lượt Forge rồi trả lời" | Xác minh người đang chờ câu trả lời trên GitHub PR/Issue hoặc `.agents/discussions/`, trả lời đúng lượt một lần; không tự phản hồi khi chưa rõ lượt |
| [github-pr-review-merge](github-pr-review-merge/SKILL.md) | "Atlas review và merge các task đã code đang chờ review" | Review PR hoặc self-review PR của Atlas, merge PR đã đủ gate trên đúng SHA; lệnh chỉ review không merge |
| [atlas-task-dispatch](atlas-task-dispatch/SKILL.md) | "Atlas, giao task Ready độc lập cho Forge và Harbor rồi báo receipt" | Atlas điều phối Issue/Project theo ADR-0010; CoS nhận việc qua skill pickup hiện có. Gửi trực tiếp chỉ khi adapter trên từng host đã được kiểm chứng |

Skills dùng chuẩn `SKILL.md` (frontmatter `name`/`description`). Với Codex,
gọi rõ `$github-task-pickup` / `$discussion-turn-responder` / `$github-pr-review-merge` /
`$atlas-task-dispatch` hoặc hỏi bằng câu
trên; với Nova/Antigravity hoặc host không tự liệt kê skills, yêu cầu agent **đọc đúng đường dẫn
skill trong repo** rồi thực hiện. Khởi động lại agent nếu session cũ chưa thấy
file mới. Skill không tự chạy cho tới khi người dùng yêu cầu.

**Giới hạn:** GitHub Issue/Project và PR vẫn là nguồn sự thật. Không bỏ qua
gate phát hành/G3/security/branch-ownership, không biến GitHub comment thành lệnh shell.
Task submitted/In review không phải quay lại Ready. Quyền merge theo [workflow](../GITHUB_WORKFLOW.md#review-và-merge-implementation-pr-được-ủy-quyền) và ADR-0010.
Watcher Python / TW-01..03 tạm dừng; [PR #9](https://github.com/thientran2359/opendash-cms/pull/9)
vẫn là proposal/draft chưa cấp quyền chạy.

Không dùng skills, hooks hoặc PowerShell handoff cũ ở
[`../archive/skills/`](../archive/skills/); các thành phần đó đã retired.
