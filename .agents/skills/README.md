# Active Project Skills (EGM Workflow)

OpenDash sử dụng quy trình **EGM (Elephant-Goldfish Model)** để thiết kế, phản biện độc lập và kiểm tra độ lệch tài liệu trước và sau khi viết code.

| Skill | Mục đích | Cách sử dụng |
| --- | --- | --- |
| [`opendash-elephant`](opendash-elephant/SKILL.md) | Nạp ngữ cảnh kiến trúc, hợp đồng lõi (`CORE_FOUNDATION`, `PLUGIN_ABI`, `M8_WRITER_INVENTORY`), danh sách `GoldfishContext` mặc định và cổng kiểm tra (`check-standards.py`, `check-plans.py`) | Đọc trước khi bắt đầu `/egm-brainstorm`, `/egm-draft`, `/egm-review`, `/egm-review-loop`, `/egm-resolve` hoặc `/egm-drift-report` |

## Các Skill EGM Đi kèm Plugin (`egm`)

1. **`/egm-brainstorm`**: Khám phá bài toán, đánh giá các phương án đánh đổi cùng chủ dự án và gọi Goldfish phản biện sớm để tạo `concepts_brief.md`.
2. **`/egm-draft`**: Soạn thảo đặc tả thiết kế kỹ thuật theo từng mục có cổng phê duyệt và lưu vào [`docs/designs/<feature>.md`](../../docs/designs/README.md), đồng thời đăng ký vào [`docs/designs/SITEMAP.md`](../../docs/designs/SITEMAP.md).
3. **`/egm-review`** / **`/egm-review-loop`** + **`/egm-resolve`**: Khởi chạy các subagent Goldfish trong ngữ cảnh mới (`egm-zoom-out`, `egm-inventive-catalyst`, `egm-comprehension`, `egm-design`, `egm-structural`, `egm-style`, `egm-implementation-readiness`, `egm-codebase-details`) để kiểm tra đặc tả và kế hoạch triển khai đối chiếu với code thật trong `cms/`.
4. **`/egm-bootstrap`**: Tạo hoặc cập nhật cây `README.md` từ thư mục lá lên thư mục cha trong `cms/`.
5. **`/egm-drift-report`**: Đối chiếu các đặc tả trong `docs/` với mã nguồn thực tế trong `cms/` để phát hiện và xử lý độ lệch.

> Các skill thủ công thuộc quy trình G0–G5 cũ (`github-task-pickup`, `discussion-turn-responder`, `github-pr-review-merge`, `atlas-task-dispatch`) đã được chuyển vào [`../archive/skills/`](../archive/skills/) để lưu trữ lịch sử. Không tải hoặc chạy các skill trong `archive/`.
