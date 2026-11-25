# OpenDash — Mục lục Tài liệu (`docs/`)

Tài liệu kỹ thuật của OpenDash nằm tại `docs/` ở gốc workspace, độc lập với mã nguồn trong [`cms/`](../cms/README.md). Đọc [README gốc](../README.md) để biết cách chạy ứng dụng và kiểm chứng trên máy phát triển.

## Quy trình Thiết kế & Phát triển Chuẩn: EGM (Elephant-Goldfish Model)

Từ ngày **20/11/2026**, OpenDash áp dụng quy trình **EGM (Elephant soạn spec → Goldfish review độc lập → Chủ dự án duyệt → Code & Verify → Drift Report)** thay thế hoàn toàn quy trình G0–G5 cũ:

1. **Nạp ngữ cảnh dự án (`Elephant`):** Đọc skill [`.agents/skills/opendash-elephant/SKILL.md`](../.agents/skills/opendash-elephant/SKILL.md) cùng các tài liệu nền tảng trong `product/`, `architecture/` và `contracts/`.
2. **Khám phá & Soạn thảo Spec (`/egm-brainstorm` → `/egm-draft`):** Soạn đặc tả thiết kế kỹ thuật (gồm kiến trúc + `## Detailed Implementation Plan`) tại [`docs/designs/<feature>.md`](designs/README.md) và đăng ký vào [`docs/designs/SITEMAP.md`](designs/SITEMAP.md).
3. **Review Độc lập bằng Goldfish (`/egm-review` / `/egm-review-loop` + `/egm-resolve`):** Các subagent Goldfish trong ngữ cảnh mới kiểm tra spec đối chiếu với code thật trong `cms/` và các hợp đồng trong `contracts/` trước khi chủ dự án duyệt.
4. **Triển khai & Kiểm chứng (`Implement & Verify`):** Viết code theo `## Detailed Implementation Plan`, chạy `check-standards.py`, `check-plans.py`, `cargo test` và lưu bằng chứng vào [`docs/verification/`](verification/README.md).
5. **Kiểm tra Độ lệch Định kỳ (`/egm-drift-report`):** Đối chiếu tài liệu trong `docs/` với codebase thật trong `cms/`.

## Nguồn Quyết định và Thứ tự Ưu tiên

1. Chỉ đạo mới nhất của chủ dự án.
2. [Yêu cầu sản phẩm V1](product/PRODUCT_REQUIREMENTS.md) — phạm vi sản phẩm và ràng buộc nghiệp vụ đã xác nhận.
3. [Bộ khung lõi & Quy chuẩn code](contracts/CORE_FOUNDATION.md), [Plugin ABI](contracts/PLUGIN_ABI.md) và [Writer & Recovery Inventory](contracts/M8_WRITER_INVENTORY.md) — các bất biến phân tầng, Base APIs, sandbox Wasmi và khóa bảo trì `SiteCoordinator`.
4. [Design Sitemap (`docs/designs/SITEMAP.md`)](designs/SITEMAP.md) — danh mục các đặc tả thiết kế EGM và các hợp đồng nền tảng (`M5`–`M8`, Admin Navigation, Owner Admin Review, Content Foundation `0001`–`0041`).
5. [Tech stack](architecture/TECH_STACK.md) và [Kiến trúc CMS](architecture/CMS_ARCHITECTURE.md) — cấu trúc workspace 2 crate (`opendash-core`, `opendash-server`) + React Admin.

## Cấu trúc Thư mục `docs/`

| Mục đích | Vị trí | Quy ước |
| --- | --- | --- |
| **Đặc tả thiết kế tính năng (EGM)** | [`designs/`](designs/README.md) · [**`SITEMAP.md`**](designs/SITEMAP.md) | Mỗi tính năng 1 file spec hoàn chỉnh (`FeatureRange` + `Detailed Implementation Plan`); đăng ký trong `SITEMAP.md` |
| **Phạm vi và hành vi sản phẩm** | [`product/`](product/PRODUCT_REQUIREMENTS.md) — [PRODUCT_REQUIREMENTS](product/PRODUCT_REQUIREMENTS.md) · [Admin Navigation & Edit Screen](product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md) | Ràng buộc sản phẩm cốt lõi đã xác nhận bởi chủ dự án |
| **Kiến trúc và lựa chọn stack** | [`architecture/`](architecture/CMS_ARCHITECTURE.md) — [CMS_ARCHITECTURE](architecture/CMS_ARCHITECTURE.md) · [TECH_STACK](architecture/TECH_STACK.md) | Ranh giới crate, `SitePaths`, luồng dữ liệu và các thư viện đã pin |
| **Hợp đồng kỹ thuật (Contracts)** | [`contracts/`](contracts/CORE_FOUNDATION.md) — [Core Foundation](contracts/CORE_FOUNDATION.md) · [Plugin ABI](contracts/PLUGIN_ABI.md) · [Writer Inventory](contracts/M8_WRITER_INVENTORY.md) · [M5](contracts/M5_THEME_TEMPLATES.md) · [M6](contracts/M6_PLUGIN_EXTENSIONS.md) · [M7](contracts/M7_SITE_FEATURES.md) · [M8](contracts/M8_OPERATIONS_RELEASE.md) · [Admin Nav & Single Edit](contracts/ADMIN_NAVIGATION_AND_SINGLE_EDIT.md) · [Admin Review 2026-11](contracts/ADMIN_REVIEW_2026_11.md) · [Content Foundation 2026-11](contracts/CONTENT_FOUNDATION_2026_11.md) | Hợp đồng kỹ thuật và bất biến hệ thống; dùng làm `GoldfishContext` khi review spec và kiểm tra drift |
| **Quyết định kiến trúc (ADR)** | [`decisions/`](decisions/ADR-0001-workspace-layout.md) (`ADR-0001` đến `ADR-0019`) | Hồ sơ các quyết định kiến trúc đã chốt qua các giai đoạn phát triển |
| **Bằng chứng thực thi** | [`verification/`](verification/README.md) | Kết quả build, test, smoke/E2E và audit thực chạy kèm SHA và môi trường |
| **Hồ sơ lịch sử** | [`archive/`](archive/README.md) · [`research/`](research/README.md) | Trỏ tới lịch sử kế hoạch G0–G5 trong Git, ghi chú CoS và các báo cáo `/egm-drift-report` |

Sau khi cập nhật tài liệu trong `docs/`, luôn chạy:

```powershell
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
git diff --check
```
