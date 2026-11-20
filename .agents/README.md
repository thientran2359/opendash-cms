# OpenDash CMS — Khu vực hỗ trợ Agent (`.agents/`)

Thư mục `.agents/` lưu trữ các skill vận hành cho quy trình **EGM (Elephant-Goldfish Model) + Multi-Agent Execution** (Atlas, Nova, Aster nghiên cứu/soạn spec/điều phối → Forge, Harbor triển khai & kiểm chứng đa OS) cùng hồ sơ nghiên cứu lịch sử. Tài liệu chính thức của dự án nằm tại [`docs/`](../docs/README.md) và đặc tả thiết kế nằm tại [`docs/designs/`](../docs/designs/SITEMAP.md).

- [`skills/README.md`](skills/README.md): 4 active skills:
  - [`opendash-elephant`](skills/opendash-elephant/SKILL.md): nạp ngữ cảnh dự án và `GoldfishContext` cho EGM.
  - [`atlas-task-dispatch`](skills/atlas-task-dispatch/SKILL.md): Atlas/Aster giao Issue triển khai từ Design Spec EGM sang Forge/Harbor qua CoS adapter.
  - [`github-task-pickup`](skills/github-task-pickup/SKILL.md): Forge/Harbor nhận và thực thi Issue từ GitHub Project #3 theo `## Detailed Implementation Plan`.
  - [`github-pr-review-merge`](skills/github-pr-review-merge/SKILL.md): Atlas/Aster/Nova review và merge PR đủ gate.
- [`agents.yaml`](agents.yaml), [`ASTER.md`](ASTER.md), [`AGENT_OPERATING_STANDARD.md`](AGENT_OPERATING_STANDARD.md), [`GITHUB_WORKFLOW.md`](GITHUB_WORKFLOW.md), [`GIT_WORKTREE_IDENTITY.md`](GIT_WORKTREE_IDENTITY.md): định danh agent, nhánh `agent/<name>`, chuẩn vận hành GitHub/CoS và worktree Git identity.
- `archive/skills/`: lưu trữ các skill đã ngừng sử dụng (`discussion-turn-responder`, các hook/watcher nền cũ).
- `research/`, `discussions/`, `tasks/`, `scripts/`: hồ sơ nghiên cứu và công cụ kiểm tra (`check-agent-artifacts.py`, `cos_dispatch.py`).

Không lưu thông tin xác thực, khóa bí mật, log runtime hoặc bản sao cơ sở dữ liệu SQLite vào Git. Kiểm tra an toàn bằng `python .agents/scripts/check-agent-artifacts.py`.
