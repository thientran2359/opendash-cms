# Kiểm chứng đồng bộ tài liệu và workflow agent — 09/11/2026

**Loại:** kiểm tra tài liệu, cấu hình Git và công cụ phụ trợ; không phải
kiểm thử chức năng CMS hoặc bằng chứng rằng Muse đã giao task cho agent.

**Baseline Git:** `main@b561805` đồng bộ với `origin/main` trước các chỉnh
sửa của lượt này; các thay đổi dưới đây còn trong working tree và chưa được
commit/push khi lập báo cáo.

## Phạm vi thay đổi

- Bổ sung [trạng thái M3 hiện hành](../plans/M3_STATUS_2026-11-09.md), cập
  nhật [Roadmap](../plans/ROADMAP_V1.md), [Module Catalog](../plans/MODULE_CATALOG_V1.md),
  CNT-01/CNT-05 briefs để tách bằng chứng M3.1–M3.6 khỏi đề xuất G1/G2 lịch sử.
- Chuẩn hóa [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md): một task ID
  và Issue/assignee, một PR/review, không gộp hoặc force-push tùy tiện.
  Có script bootstrap worktree ở chế độ dry-run trước khi dùng `--apply`.
- Giữ ba skill cũ dưới `.agents/archive/skills/`, hooks/scripts ngừng dùng
  dưới `.agents/archive/legacy-handoff/`; không còn skill active.
- Đánh dấu tài liệu research/threads cũ là hồ sơ lịch sử; bản nháp mới
  vào `.agents/`, tài liệu đã chốt và evidence vào `docs/`.
- Cập nhật `.gitignore`, kiểm tra artifact có khả năng chứa secret và mở rộng
  danh sách tài liệu chính thức được CodeGraph lập chỉ mục.

## Kiểm chứng đã thực hiện trên checkout Windows

| Kiểm tra | Kết quả |
| --- | --- |
| `python cms/scripts/check-plans.py` | **PASS** — links, module IDs/evidence; bao gồm `.agents/` đang hoạt động |
| `python cms/scripts/check-plans.py --self-test` | **PASS** — 8 fixtures |
| `git diff --check` | **PASS** — không có whitespace/conflict marker còn lại |
| `python -m py_compile .agents/scripts/bootstrap-worktree.py .agents/scripts/check-agent-artifacts.py` | **PASS** |
| `python .agents/scripts/check-agent-artifacts.py` | **PASS** với các file hiện có trong working tree |
| `node --check cms/scripts/codegraph-index-docs.mjs` | **PASS** |
| `node cms/scripts/codegraph-index-docs.mjs --list-sources` | **PASS** — đủ 16 file chính thức, không chọn `.agents/` draft |
| `node cms/scripts/codegraph-index-docs.mjs` | **PASS** — 16 file được engine cục bộ lập chỉ mục, có kết quả số đoạn cho từng file |
| `python .agents/scripts/bootstrap-worktree.py --agent atlas --target <đường dẫn chưa tồn tại>` | **PASS (dry-run)** — không tạo thư mục/branch, không push |

## Giới hạn và bước vận hành riêng

- **Không** chạy lại Rust/Cargo/UI hoặc M3 smoke tests; bằng chứng M3.1–M3.6
  chỉ được **đối chiếu** từ các báo cáo đã có.
- Năm remote branch `agent/*` không tự xuất hiện chỉ vì khai báo
  `.agents/agents.yaml` hoặc chạy bootstrap dry-run. Chỉ tạo bằng thao tác
  `--apply` đúng máy/checkout, push sau task và xác minh GitHub riêng.
- **Không** cài hoặc gọi Muse/CoS/Antigravity adapter; việc tự động giao
  task vẫn cần kiểm thử API và cấp quyền.
- Những tài liệu research/threads trong `docs/` giữ nguyên lịch sử vì còn
  liên kết nguồn từ các brief; không dùng làm yêu cầu thực thi mới.
- Git stash an toàn từ trước vẫn được giữ; không commit, push, merge,
  triển khai production hoặc làm thay đổi dữ liệu SQLite/website.
