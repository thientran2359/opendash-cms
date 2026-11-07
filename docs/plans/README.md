# Kế hoạch triển khai

Kế hoạch tách biệt với kết quả thực chạy. Khi một bước có bằng chứng trên mã thực, ghi sang [verification](../verification/README.md).

## Quản lý roadmap và module V1

Roadmap và danh mục module được quản lý theo hai trục liên kết bằng ID cố định: **Release → Milestone → Slice → Task** để theo dõi tiến độ và **Product → Domain → Module → Capability** để kiểm soát phạm vi chức năng. Nghiên cứu và chốt thiết kế từng module trước khi giao nhiệm vụ code.

**Quy trình đã thống nhất:** [Codex ↔ Antigravity (Turn 1–5)](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) — Living Module Brief G0–G3, Full-Track/Fast-Track, primary acceptance slice, per-task file ownership và Stop-and-Propose. Đây là đồng thuận **về cách quản lý công việc**, không phải kết luận schema M3. Không nhân bản bảng trạng thái ở README.

1. [Roadmap V1](ROADMAP_V1.md) — mốc M1–M8, dependencies, gates và các slice M3 dự kiến.
2. [Module Catalog V1](MODULE_CATALOG_V1.md) — danh mục ID module, phạm vi capability và trạng thái theo bằng chứng (số lượng lấy từ catalog, không duy trì số đếm riêng tại đây).
3. [Planning Workflow](PLANNING_WORKFLOW.md) — G0–G5, Full-Track/Fast-Track, trạng thái/evidence, source of truth và cách chia task.
4. Mẫu [Living Module Brief](templates/MODULE_BRIEF.md) và [handoff Slice Task](templates/SLICE_TASK.md); hai brief đầu tiên là [CNT-01 CPT](modules/CNT-01-content-types.md) và [CNT-05 Media](modules/CNT-05-media.md) — **G1/G2 chưa được chốt**.

**Hiện trạng:** M1/M2 và M2.5 đã có bằng chứng Windows; M2.5 được đóng ở commit `da79304`. Các module M3–M8 mới là phương án phân rã cần đánh giá/chốt thiết kế khi đến lượt; chưa có implementation V1 được mặc định hoàn thành.

## Kế hoạch milestone và hồ sơ lịch sử

- [Milestone Plans](milestones/README.md) lưu kế hoạch chi tiết từng mốc. M2.5 đã [hoàn thành và kiểm chứng](milestones/M2.5-core-foundation.md); xem [báo cáo M2.5](../verification/2026-11-07-m2-5-core-kernel.md) và [ADR-0002](../decisions/ADR-0002-core-extension-contract.md).
- [Bản đối chiếu nhập mã M1/M2](../archive/migrations/2026-11-07-m1-m2-module-map.md) là hồ sơ lịch sử của đợt nhập mã đã hoàn tất; kết quả kiểm chứng ở [workspace đích](../verification/2026-11-07-m1-m2-import.md).

Các nhiệm vụ mới được lập theo [Planning Workflow](PLANNING_WORKFLOW.md). Kế hoạch đã đóng vẫn giữ liên kết tới contract, ADR và báo cáo thực chạy để truy vết.

**Kiểm tra tự động:** từ root worktree, chạy `python cms/scripts/check-plans.py` và `python cms/scripts/check-plans.py --self-test`; checker chỉ xác thực tính nhất quán tài liệu, không thay thế code/tests hoặc phê duyệt G2.
