# OpenDash — Đặc tả Thiết kế theo Chuẩn EGM (`docs/designs/`)

Thư mục này là nơi lưu trữ các đặc tả thiết kế kỹ thuật (Design Specifications) của OpenDash theo quy trình **EGM (Elephant-Goldfish Model)**. Toàn bộ danh mục thiết kế được quản lý tại [`SITEMAP.md`](SITEMAP.md).

## Quy trình Làm việc EGM (7 Bước Tinh gọn — `Owner Directive 2026-11-27`)

1. **Bước 1–2 — Đề bài & Nghiên cứu EGM (`/egm-brainstorm` + `/egm-draft` + Goldfish theo cỡ):**
   - Chủ dự án nêu ý tưởng/tính năng. **Elephant** nạp ngữ cảnh qua [`.agents/skills/opendash-elephant/SKILL.md`](../../.agents/skills/opendash-elephant/SKILL.md), kiểm tra mã nguồn thật trong `cms/`, soạn `docs/designs/<feature>.md` (kèm `## Detailed Implementation Plan`, các `### Slice S01..Sn` nội bộ, `### Verification Plan`, và **đặt trước số migration** nếu làm song song nhiều tính năng) và đăng ký vào [`SITEMAP.md`](SITEMAP.md).
   - **Chọn cỡ Goldfish đối chiếu `cms/` và các hợp đồng nền tảng ([`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md)):**
     - **Kiến trúc lớn / đụng lõi, DB, bảo mật, C-SEC, Plugin ABI:** chạy 1–2 vòng `/egm-review-loop` (hoặc `/egm-review`) + `/egm-resolve`.
     - **Tính năng nhỏ / vừa / UI cô lập:** chạy 1 vòng Goldfish gọn (hoặc bỏ qua nếu chủ dự án yêu cầu làm nhanh).
2. **Bước 3–4 — Giải thích Tự nhiên & Duyệt hướng (`Status: Approved`):**
   - **Elephant** tóm tắt phương án thực hiện bằng **ngôn ngữ tự nhiên** trong chat: làm gì cho người dùng, thay đổi gì, phần không làm, và rủi ro/đánh đổi.
   - Khi chủ dự án đồng ý, chốt vào đầu file thiết kế:
     - `**Status:** Approved`
     - `**Approved By:** Owner | <Coordinator> (delegated)`
     - `**Approval Evidence:** <link PR/comment hoặc xác nhận ngày>`
     - `**Goldfish Review:** <preset/personas · ngày · reviewed baseline/SHA>`
   - CI (`check-plans.py`) tự động chặn spec `Approved` thiếu provenance, thiếu `Detailed Implementation Plan`/`Verification Plan`, thiếu Slice Contract (`**Dependencies:**`, `**File Ownership:**`, `**Acceptance Criteria:**`, `**Rollback:**`), hoặc còn `Open Questions`/`TODO`/`TBD`.
3. **Bước 5 — Triển khai trọn gói trên Feature Worktree (`feat/<slug>`) & Test cuối tính năng:**
   - **Không tạo Issue lẻ theo Slice và không mở PR cho từng Slice.** Agent triển khai tuần tự toàn bộ `S01..Sn` trên 1 worktree riêng (`feat/<slug>`).
   - Giữa các Slice trung gian (`S01..S(n-1)`): **chỉ chạy Tầng 0** (`python3 cms/scripts/run-tests.py --check`, thêm `npm --prefix cms/admin run typecheck` nếu sửa Admin) và `git commit` checkpoint — **không build binary, không chạy test**.
   - **Sau khi code xong toàn bộ tính năng:** chạy **1 lần duy nhất** Tầng 1 (`python3 cms/scripts/check-standards.py`, `python3 cms/scripts/check-plans.py`, `cargo fmt --check`, `npm run build` nếu sửa Admin, `python3 cms/scripts/run-tests.py --changed`, script E2E của tính năng) và 1 lượt kiểm tra drift gọn (`/egm-drift-report` ngay trên `feat/<slug>`).
4. **Bước 6–7 — Báo cáo Tự nhiên, Chốt Merge & Dọn Worktree:**
   - Agent báo cáo bằng **ngôn ngữ tự nhiên** trong chat: các tính năng đã làm xong, mức độ hoàn thiện, kết quả test thực chạy và link 1 Feature PR duy nhất.
   - Khi chủ dự án chốt đồng ý: sync `origin/main` 1 lần cuối (nếu cần), merge PR vào `main`, ghi bằng chứng vào [`docs/verification/`](../verification/README.md) (khi chạy nghiệm thu đầy đủ), cập nhật [`SITEMAP.md`](SITEMAP.md) (`Implemented` → `**Verified**`), gắn `> Superseded by docs/designs/<feature>.md` ở hợp đồng cũ nếu có, chạy `node cms/scripts/codegraph-index-docs.mjs`, rồi **xóa branch + xóa worktree + dọn `cms/target`**.
   - Bộ test nặng Tầng 2 (`run-tests.py --group all --release`) chỉ chạy theo đợt sau 3–5 tính năng hoặc trước release.
5. **Đối chiếu Chống trôi Tài liệu (`/egm-drift-report`):**
   - Thực hiện gọn ngay cuối Bước 5 trên `feat/<slug>` (shift-left) trước khi báo cáo ở Bước 6, và rà tổng hợp trước mỗi đợt release.

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
