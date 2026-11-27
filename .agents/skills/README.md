# Active Project Skills (Quy trình 7 Bước Feature-Worktree EGM)

Theo **Owner Directive 2026-11-27** ([`AGENTS.md`](../../AGENTS.md), [ADR-0010](../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md)), OpenDash đã ngừng giao Slice cho Harbor/Forge và chuyển sang luồng **1 Feature = 1 Worktree (`feat/<slug>`) = 1 PR** với test chạy 1 lần khi xong toàn bộ tính năng.

| Skill | Trạng thái | Vai trò chính | Mục đích |
| --- | --- | --- | --- |
| [`opendash-elephant`](opendash-elephant/SKILL.md) | **Active** | Elephant / Agent trực tiếp | Nạp ngữ cảnh kiến trúc, `GoldfishContext`, kiểm chứng 3 tầng và quy trình 7 bước EGM |
| [`github-pr-review-merge`](github-pr-review-merge/SKILL.md) | **Active** | Coordinator / Agent trực tiếp | Review Feature PR (`feat/<slug>`), kiểm chứng Tầng 1, merge sau khi Owner chốt (Bước 7) và dọn worktree |
| [`atlas-task-dispatch`](atlas-task-dispatch/SKILL.md) | **Standby / Retired (2026-11-27)** | Lịch sử | Trước đây giao Issue sang Forge/Harbor qua CoS adapter; chỉ dùng nếu Owner yêu cầu khôi phục |
| [`github-task-pickup`](github-task-pickup/SKILL.md) | **Standby / Retired (2026-11-27)** | Lịch sử | Trước đây Harbor/Forge tự nhận Issue Slice từ Project #3; hiện ngừng sử dụng |

## Các Skill EGM Đi kèm Plugin (`egm`)

1. **`/egm-brainstorm`**: Khám phá bài toán cùng chủ dự án và gọi Goldfish phản biện sớm (`concepts_brief.md`).
2. **`/egm-draft`**: Soạn đặc tả thiết kế kỹ thuật (kèm Slice nội bộ & đặt trước số migration nếu chạy song song), lưu vào [`docs/designs/<feature>.md`](../../docs/designs/README.md) và đăng ký vào [`docs/designs/SITEMAP.md`](../../docs/designs/SITEMAP.md).
3. **`/egm-review`** / **`/egm-review-loop`** + **`/egm-resolve`**: Khởi chạy swarm Goldfish đối chiếu đặc tả với code thật trong `cms/` theo cỡ tính năng.
4. **`/egm-bootstrap`**: Tạo/cập nhật cây `README.md` cấp thư mục trong `cms/`.
5. **`/egm-drift-report`**: Đối chiếu `docs/` với `cms/` ngay trên `feat/<slug>` (shift-left cuối Bước 5) và định kỳ trước release.

> Skill tranh luận liên-agent cũ (`discussion-turn-responder`) và các hook/watcher nền đã được lưu trữ tại [`../archive/`](../archive/README.md). Không tải hoặc chạy các skill trong `archive/`.
