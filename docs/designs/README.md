# OpenDash — Đặc tả Thiết kế theo Chuẩn EGM (`docs/designs/`)

Thư mục này là nơi lưu trữ các đặc tả thiết kế kỹ thuật (Design Specifications) của OpenDash theo quy trình **EGM (Elephant-Goldfish Model)**. Toàn bộ danh mục thiết kế được quản lý tại [`SITEMAP.md`](SITEMAP.md).

## Quy trình Làm việc EGM

1. **Khám phá & Phản biện ý tưởng (`/egm-brainstorm`):**
   - **Elephant** thảo luận cùng chủ dự án để làm rõ bài toán, ràng buộc và các phương án đánh đổi, sau đó gọi **Goldfish** phản biện sớm và xuất `concepts_brief.md` (lưu tại vùng artifact/scratch của phiên làm việc).
2. **Soạn thảo Đặc tả (`/egm-draft`):**
   - **Elephant** nạp ngữ cảnh dự án (qua skill [`.agents/skills/opendash-elephant/SKILL.md`](../../.agents/skills/opendash-elephant/SKILL.md)) và soạn từng mục của `design_draft.md` với cổng phê duyệt của chủ dự án.
   - Sau khi hoàn tất, đặc tả được ghi vào `docs/designs/<feature>.md` và đăng ký vào [`SITEMAP.md`](SITEMAP.md).
3. **Review Độc lập bằng Goldfish (`/egm-review` / `/egm-review-loop` + `/egm-resolve`):**
   - Các subagent **Goldfish** trong ngữ cảnh mới hoàn toàn kiểm tra tài liệu đối chiếu với mã nguồn thật trong `cms/` và 3 hợp đồng nền tảng ([`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md)):
     - **Full Swarm (6–8 Goldfish — tính năng kiến trúc lớn):** `egm-zoom-out`, `egm-inventive-catalyst`, `egm-comprehension`, `egm-design`, `egm-structural`, `egm-style`, `egm-implementation-readiness`, `egm-codebase-details`.
     - **Fast Review (3 Goldfish lõi — tính năng nhỏ/vừa hoặc bản sửa lỗi có phạm vi thiết kế):** `egm-design`, `egm-codebase-details`, `egm-implementation-readiness`.
   - **Elephant** phân loại các phát hiện (`PROPOSE FIX` vs `CONTEST`), trình chủ dự án quyết định và cập nhật trực tiếp vào file thiết kế (`Status: Approved`). CI (`check-plans.py`) tự động chặn spec `Approved` còn thiếu `Detailed Implementation Plan` / `Verification Plan` hoặc còn `Open Questions` / `TODO` / `TBD`.
4. **Triển khai & Kiểm chứng (Implement & Verify):**
   - Chỉ bắt đầu viết code sau khi Design Spec đã được duyệt (`Status: Approved`). Với spec lớn, chia nhỏ bằng heading `### Slice S01: <Tên slice>` trong `## Detailed Implementation Plan` và tạo GitHub Issue từng slice qua `python .agents/scripts/prepare-github-issue.py --source docs/designs/<feature>.md --slice S01`.
   - Triển khai bám sát mục `## Detailed Implementation Plan` trong file thiết kế, chạy đầy đủ `python cms/scripts/check-standards.py`, `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` cùng các bài test E2E liên quan, và ghi bằng chứng thực chạy vào [`docs/verification/`](../verification/README.md).
   - **Đóng vòng đời sau khi merge PR triển khai:** cập nhật `Status` trong [`SITEMAP.md`](SITEMAP.md) theo lộ trình `Approved` → `Implemented` → `**Verified**` (kèm link trực tiếp tới `docs/verification/*.md`); nếu đặc tả thay thế một phần hợp đồng nền tảng cũ trong `docs/contracts/`, gắn ghi chú `> Superseded by docs/designs/<feature>.md` tại mục bị thay thế và chạy `node cms/scripts/codegraph-index-docs.mjs`.
5. **Đối chiếu Chống trôi Tài liệu (`/egm-drift-report`):**
   - Định kỳ chạy `/egm-drift-report` trên `docs/` đối chiếu với `cms/` để phát hiện và xử lý kịp thời mọi khác biệt giữa thiết kế và mã nguồn.

## Cấu trúc Chuẩn của một File Thiết kế (`docs/designs/<feature>.md`)

Mỗi file thiết kế trong `docs/designs/` gồm hai phần được ngăn cách bởi tiêu đề `## Detailed Implementation Plan` để `/egm-review` tự động nhận diện ranh giới (`FeatureRange` và `DetailedRange`):

1. **Phần Đặc tả Tính năng & Kiến trúc (`FeatureRange`):**
   - `## Overview` & `## The Problem`
   - `## Goals` & `## Non-Goals`
   - `## Open Questions` (xóa dần khi đã chốt)
   - `## Critical User Journeys (CUJs)` (`Given` / `When` / `Then`)
   - `## Technical Plan (Architecture)` (sơ đồ luồng dữ liệu, schema SQLite, kiểu Rust/TypeScript, ranh giới quyền/bảo mật)
   - `## Alternatives Considered` (ghi rõ lý do bác bỏ từng phương án thay thế)
   - `## Non-Functional Requirements` (bảo mật, hiệu năng, bảo toàn dữ liệu & rollback)
2. **Phần Kế hoạch Triển khai Chi tiết (`DetailedRange`):**
   - `## Detailed Implementation Plan`
   - `### Target Files to Create/Modify` (liệt kê đầy đủ từng file trong `cms/` hoặc `docs/` kèm lý do và chữ ký hàm/API dự kiến; không viết code thực thi hoàn chỉnh trong spec)
   - `### Verification Plan` (các lệnh build, unit test, `check-standards.py`, `check-plans.py` và kịch bản E2E cụ thể)
