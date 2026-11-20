# OpenDash — Đặc tả Thiết kế theo Chuẩn EGM (`docs/designs/`)

Thư mục này là nơi lưu trữ các đặc tả thiết kế kỹ thuật (Design Specifications) của OpenDash theo quy trình **EGM (Elephant-Goldfish Model)**. Toàn bộ danh mục thiết kế được quản lý tại [`SITEMAP.md`](SITEMAP.md).

## Quy trình Làm việc EGM

1. **Khám phá & Phản biện ý tưởng (`/egm-brainstorm`):**
   - **Elephant** thảo luận cùng chủ dự án để làm rõ bài toán, ràng buộc và các phương án đánh đổi, sau đó gọi **Goldfish** phản biện sớm và xuất `concepts_brief.md` (lưu tại vùng artifact/scratch của phiên làm việc).
2. **Soạn thảo Đặc tả (`/egm-draft`):**
   - **Elephant** nạp ngữ cảnh dự án (qua skill [`.agents/skills/opendash-elephant/SKILL.md`](../../.agents/skills/opendash-elephant/SKILL.md)) và soạn từng mục của `design_draft.md` với cổng phê duyệt của chủ dự án.
   - Sau khi hoàn tất, đặc tả được ghi vào `docs/designs/<feature>.md` và đăng ký vào [`SITEMAP.md`](SITEMAP.md).
3. **Review Độc lập bằng Goldfish (`/egm-review` / `/egm-review-loop` + `/egm-resolve`):**
   - Các subagent **Goldfish** trong ngữ cảnh mới hoàn toàn (`egm-zoom-out`, `egm-inventive-catalyst`, `egm-comprehension`, `egm-design`, `egm-structural`, `egm-style`, `egm-implementation-readiness`, `egm-codebase-details`) kiểm tra tài liệu đối chiếu với mã nguồn thật trong `cms/` và 3 hợp đồng nền tảng ([`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md)).
   - **Elephant** phân loại các phát hiện (`PROPOSE FIX` vs `CONTEST`), trình chủ dự án quyết định và cập nhật trực tiếp vào file thiết kế (`Status: Approved`).
4. **Triển khai & Kiểm chứng (Implement & Verify):**
   - Chỉ bắt đầu viết code sau khi Design Spec đã được duyệt.
   - Triển khai bám sát mục `## Detailed Implementation Plan` trong file thiết kế, chạy đầy đủ `python cms/scripts/check-standards.py`, `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` cùng các bài test E2E liên quan, và ghi bằng chứng thực chạy vào [`docs/verification/`](../verification/README.md).
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
