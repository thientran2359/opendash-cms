# Active Project Skills (EGM + Multi-Agent Execution)

OpenDash kết hợp **EGM (Elephant-Goldfish Model)** cho khâu nghiên cứu, soạn đặc tả & review thiết kế (**Atlas / Nova / Aster**) với **bộ skill điều phối GitHub/CoS** cho khâu giao việc & thực thi đa nền tảng (**Forge** trên Windows, **Harbor** trên macOS).

| Skill | Vai trò chính | Mục đích |
| --- | --- | --- |
| [`opendash-elephant`](opendash-elephant/SKILL.md) | Atlas, Nova, Aster | Nạp ngữ cảnh kiến trúc, hợp đồng lõi (`CORE_FOUNDATION`, `PLUGIN_ABI`, `M8_WRITER_INVENTORY`), `GoldfishContext` mặc định và lệnh kiểm tra chuẩn cho EGM |
| [`atlas-task-dispatch`](atlas-task-dispatch/SKILL.md) | Atlas, Aster | Giao Issue triển khai từ Design Spec EGM đã `Approved` sang Forge (Windows) / Harbor (macOS) qua CoS adapter (`RUN`, `RESUME`, `STATUS`, `RUN PR`) |
| [`github-task-pickup`](github-task-pickup/SKILL.md) | Forge, Harbor (và Atlas/Aster/Nova) | Nhận/tiếp tục Issue triển khai trên GitHub Project #3 theo `## Detailed Implementation Plan` của Design Spec EGM đã duyệt, kiểm thử và mở PR |
| [`github-pr-review-merge`](github-pr-review-merge/SKILL.md) | Atlas, Aster, Nova | Review PR (cả PR Design Spec EGM lẫn PR code của Forge/Harbor), kiểm tra CI/bằng chứng thực chạy và merge an toàn với `expected_head_sha` |

## Các Skill EGM Đi kèm Plugin (`egm`)

1. **`/egm-brainstorm`**: Khám phá bài toán cùng chủ dự án và gọi Goldfish phản biện sớm (`concepts_brief.md`).
2. **`/egm-draft`**: Soạn đặc tả thiết kế kỹ thuật theo từng mục có cổng phê duyệt, lưu vào [`docs/designs/<feature>.md`](../../docs/designs/README.md) và đăng ký vào [`docs/designs/SITEMAP.md`](../../docs/designs/SITEMAP.md).
3. **`/egm-review`** / **`/egm-review-loop`** + **`/egm-resolve`**: Khởi chạy swarm Goldfish độc lập kiểm tra đặc tả và kế hoạch triển khai đối chiếu với code thật trong `cms/`.
4. **`/egm-bootstrap`**: Tạo/cập nhật cây `README.md` cấp thư mục trong `cms/`.
5. **`/egm-drift-report`**: Đối chiếu định kỳ `docs/` với `cms/` để phát hiện độ lệch.

> Skill tranh luận liên-agent cũ (`discussion-turn-responder`) và các hook/watcher nền đã được lưu trữ tại [`../archive/skills/`](../archive/skills/) vì khâu phản biện thiết kế đã do Goldfish đảm nhiệm. Không tải hoặc chạy các skill trong `archive/`.
