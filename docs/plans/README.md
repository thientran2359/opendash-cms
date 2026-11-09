# Kế hoạch triển khai

Kế hoạch tách biệt với kết quả thực chạy. Khi một bước có bằng chứng trên mã thực, ghi sang [verification](../verification/README.md).

## Quản lý roadmap và module V1

Roadmap và danh mục module được quản lý theo hai trục liên kết bằng ID cố định: **Release → Milestone → Slice → Task** để theo dõi tiến độ và **Product → Domain → Module → Capability** để kiểm soát phạm vi chức năng. Nghiên cứu và chốt thiết kế từng module trước khi giao nhiệm vụ code.

**Quy trình đã thống nhất:** [Codex ↔ Antigravity (Turn 1–5)](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) — Living Module Brief G0–G3, Full-Track/Fast-Track, primary acceptance slice, per-task file ownership và Stop-and-Propose. Đây là đồng thuận **về cách quản lý công việc**, không phải kết luận schema M3. Không nhân bản bảng trạng thái ở README.

1. [Roadmap V1](ROADMAP_V1.md) — mốc M1–M8, dependencies, gates và các slice M3 dự kiến.
2. [Module Catalog V1](MODULE_CATALOG_V1.md) — danh mục ID module, phạm vi capability và trạng thái theo bằng chứng (số lượng lấy từ catalog, không duy trì số đếm riêng tại đây).
3. [Planning Workflow](PLANNING_WORKFLOW.md) — G0–G5, Full-Track/Fast-Track, trạng thái/evidence, source of truth và cách chia task.
   [Muse → CoS Windows Phase 1 — G2 AGREED / G3 DRAFT](muse-cos-win-phase1-g2.md) — phạm vi Poller P0–P4 đã được duyệt, [8 work packages G3 ứng viên](muse-cos-win-phase1-g2.md#5-g3-breakdown--các-work-package-ứng-viên), Issue #5 là kênh thử; runtime NOT_RUN và Phase 2/P5 chưa duyệt.
4. Mẫu [Living Module Brief](templates/MODULE_BRIEF.md) và [handoff Slice Task](templates/SLICE_TASK.md); hai brief đầu tiên là [CNT-01 CPT](modules/CNT-01-content-types.md) và [CNT-05 Media](modules/CNT-05-media.md) — **G1 được review theo hồ sơ lịch sử; G2 ứng viên cần đối chiếu implementation hiện hành**.

**Hiện trạng:** M1/M2 và M2.5 đã có bằng chứng Windows; M2.5 được đóng ở commit `da79304`. M3.1–M3.6 [đã có source và evidence trên macOS](M3_STATUS_2026-11-09.md); **M3 còn PARTIAL**, các capability M3 chưa hoàn tất vẫn cần gate/verification. M4–M8 là kế hoạch tiếp theo, không mặc định đã triển khai.

## Kế hoạch milestone và hồ sơ lịch sử

- [Milestone Plans](milestones/README.md) lưu kế hoạch chi tiết từng mốc. M2.5 đã [hoàn thành và kiểm chứng](milestones/M2.5-core-foundation.md); xem [báo cáo M2.5](../verification/2026-11-07-m2-5-core-kernel.md) và [ADR-0002](../decisions/ADR-0002-core-extension-contract.md).
- [Bản đối chiếu nhập mã M1/M2](../archive/migrations/2026-11-07-m1-m2-module-map.md) là hồ sơ lịch sử của đợt nhập mã đã hoàn tất; kết quả kiểm chứng ở [workspace đích](../verification/2026-11-07-m1-m2-import.md).

Các nhiệm vụ mới được lập theo [Planning Workflow](PLANNING_WORKFLOW.md). Kế hoạch đã đóng vẫn giữ liên kết tới contract, ADR và báo cáo thực chạy để truy vết.

**Kiểm tra tự động:** từ root worktree, chạy `python cms/scripts/check-plans.py` và `python cms/scripts/check-plans.py --self-test`; checker chỉ xác thực tính nhất quán tài liệu, không thay thế code/tests hoặc phê duyệt G2.

## M3 — Quản lý nội dung (Content/Media)

Các slice M3.1–M3.6 đã triển khai với phạm vi ghi tại [M3 status](M3_STATUS_2026-11-09.md). Lộ trình ban đầu bắt đầu với [M3.1 — mô hình CPT & Custom Fields](2026-11-08-m3-content-types-and-fields.md) trong `opendash-core`, tiếp theo là [M3.2 — SQLite và API CPT](2026-11-08-m3-2-content-types-backend.md), [M3.3 — Content Entry CRUD](2026-11-08-m3-3-content-entry-crud.md), [M3.4 — Admin Content UI](2026-11-08-m3-4-admin-content-ui.md), [M3.5 — Media Library](2026-11-09-m3-5-media-library.md) và [M3.6 — Public CPT Rendering](2026-11-09-m3-6-public-content-rendering.md). Taxonomy và Builder nâng cao chia thành những slice riêng có nghiệm thu.
