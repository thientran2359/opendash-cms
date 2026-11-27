# OpenDash CMS — Khu vực hỗ trợ Agent (`.agents/`)

Thư mục `.agents/` lưu trữ các skill vận hành cho quy trình **7 bước EGM Tinh gọn (Feature Worktree `feat/<slug>`)** theo chỉ đạo ngày `2026-11-27` (`AGENTS.md`, [ADR-0010](../docs/decisions/ADR-0010-agent-project-and-merge-authority.md)), cùng hồ sơ nghiên cứu và tài liệu tham chiếu lịch sử. Tài liệu chính thức của dự án nằm tại [`docs/`](../docs/README.md) và đặc tả thiết kế nằm tại [`docs/designs/`](../docs/designs/SITEMAP.md).

- [`skills/README.md`](skills/README.md): 4 skill hợp lệ trong kiểm tra tự động:
  - [`opendash-elephant`](skills/opendash-elephant/SKILL.md) *(Active)*: nạp ngữ cảnh dự án, `GoldfishContext`, kiểm chứng 3 tầng và quy trình 7 bước EGM.
  - [`github-pr-review-merge`](skills/github-pr-review-merge/SKILL.md) *(Active)*: review & merge Feature PR (`feat/<slug>`) sau khi hoàn tất kiểm chứng Tầng 1 và Owner chốt bước cuối, kèm dọn worktree sau merge.
  - [`atlas-task-dispatch`](skills/atlas-task-dispatch/SKILL.md) *(Standby / Retired 2026-11-27)*: trước đây dùng để giao Issue cho Forge/Harbor qua CoS adapter (hiện ngừng sử dụng theo Owner Directive 2026-11-27).
  - [`github-task-pickup`](skills/github-task-pickup/SKILL.md) *(Standby / Retired 2026-11-27)*: trước đây dùng cho Harbor/Forge tự nhận Issue từng Slice trên GitHub Project #3 (hiện ngừng sử dụng theo Owner Directive 2026-11-27).
- [`agents.yaml`](agents.yaml), [`ASTER.md`](ASTER.md), [`AGENT_OPERATING_STANDARD.md`](AGENT_OPERATING_STANDARD.md), [`GITHUB_WORKFLOW.md`](GITHUB_WORKFLOW.md), [`GIT_WORKTREE_IDENTITY.md`](GIT_WORKTREE_IDENTITY.md): định danh agent, chuẩn vận hành Feature Worktree và worktree Git identity.
- `archive/skills/`: lưu trữ các skill đã ngừng sử dụng (`discussion-turn-responder`, các hook/watcher nền cũ).
- `research/`, `discussions/`, `tasks/`, `scripts/`: hồ sơ nghiên cứu và công cụ kiểm tra (`check-agent-artifacts.py`, cùng các script dispatch/Issue lịch sử giữ cho unit test CI).

Không lưu thông tin xác thực, khóa bí mật, log runtime hoặc bản sao cơ sở dữ liệu SQLite vào Git. Kiểm tra an toàn bằng `python3 .agents/scripts/check-agent-artifacts.py`.
