# OpenDash CMS — Khu vực hỗ trợ Agent (`.agents/`)

Thư mục `.agents/` lưu trữ skill nạp ngữ cảnh dự án cho quy trình **EGM (Elephant-Goldfish Model)** cùng các bản nháp nghiên cứu và hồ sơ điều phối lịch sử. Tài liệu chính thức của dự án nằm tại [`docs/`](../docs/README.md) và đặc tả thiết kế nằm tại [`docs/designs/`](../docs/designs/SITEMAP.md).

- [`skills/opendash-elephant/SKILL.md`](skills/opendash-elephant/SKILL.md): Skill nạp ngữ cảnh kiến trúc, hợp đồng lõi và `GoldfishContext` mặc định của OpenDash cho Elephant và Goldfish.
- [`skills/README.md`](skills/README.md): Hướng dẫn sử dụng `opendash-elephant` kết hợp với bộ skill của plugin EGM (`/egm-brainstorm`, `/egm-draft`, `/egm-review`, `/egm-resolve`, `/egm-bootstrap`, `/egm-drift-report`).
- [`GIT_WORKTREE_IDENTITY.md`](GIT_WORKTREE_IDENTITY.md): Cấu hình Git identity theo worktree để bảo toàn global Git config của người dùng.
- `archive/skills/`: Lưu trữ các skill của quy trình G0–G5 cũ (chỉ dùng để tham khảo lịch sử, không kích hoạt).
- `research/`, `discussions/`, `tasks/`, `scripts/`: Lưu trữ hồ sơ nghiên cứu và công cụ kiểm tra lịch sử từ giai đoạn G0–G5.

Không lưu thông tin xác thực, khóa bí mật, log runtime hoặc bản sao cơ sở dữ liệu SQLite vào Git. Kiểm tra an toàn bằng `python .agents/scripts/check-agent-artifacts.py`.
