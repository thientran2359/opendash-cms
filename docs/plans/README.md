# Kế hoạch triển khai

Kế hoạch tách biệt với kết quả thực chạy. Khi một bước có bằng chứng trên mã thực, ghi sang [verification](../verification/README.md).

**Điểm bắt đầu hiện hành:** [Module Catalog](MODULE_CATALOG_V1.md) → [Coverage Matrix — 27 module, 9 milestone, task theo mốc](milestones/COVERAGE_MATRIX.md) → [mục lục milestone và task](milestones/README.md) → [M3 Core CMS phương án A đã chốt](milestones/M3/README.md). **M3 vẫn PARTIAL**; [snapshot M3.1–M3.6](M3_STATUS_2026-11-09.md) là evidence của scope cũ trên macOS, không mặc định Windows/Linux hay acceptance mới G5. [Issue #15 Final Decision](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410) chỉ là ưu tiên nghiên cứu kỹ thuật lịch sử, không đại diện toàn V1.

**Phân loại nguồn, không nhân bản trạng thái vận hành:**

- **Active — kế hoạch/quy trình:** [Roadmap V1](ROADMAP_V1.md) (milestone/dependency), [Planning Workflow](PLANNING_WORKFLOW.md) (gates), [Module Catalog](MODULE_CATALOG_V1.md) (scope), [GitHub workflow](../../.agents/GITHUB_WORKFLOW.md). **GitHub Issues/Project #3** là nguồn trạng thái task/assignee đang chạy; **PR** là nguồn diff/review; [verification](../verification/README.md) là bằng chứng kiểm chứng theo SHA/OS.
- **Implemented-by-slice — chưa đồng nghĩa Done:** [M3.1–M3.6 status](M3_STATUS_2026-11-09.md), [kế hoạch M3 theo milestone/slices](milestones/M3/README.md), [bằng chứng M3.5 macOS](../verification/2026-11-09-m3-5-media-library.md), [bằng chứng M3.6 macOS](../verification/2026-11-09-m3-6-public-content-rendering.md). M2.5 đã có [evidence Windows theo phạm vi](../verification/2026-11-07-m2-5-core-kernel.md).
- **Historical-Proposed — chỉ để truy vết:** các phương án G2/G3 chưa chốt trong [CNT-01](modules/CNT-01-content-types.md), [CNT-05](modules/CNT-05-media.md) và hồ sơ nghiên cứu cũ; không thay schema/API đang triển khai và không tạo task Ready từ đề xuất lịch sử.
- **Paused — tự động hóa agent:** [Muse/CoS Phase 1 (hồ sơ lịch sử)](muse-cos-win-phase1-g2.md) ghi G2/G3 đã duyệt tại thời điểm cũ và amendment Task Watcher **PAUSED**. Cách làm hiện tại là các skill gọi **thủ công trong chat**: [nhận task](../../.agents/skills/github-task-pickup/SKILL.md), [trả lời thảo luận](../../.agents/skills/discussion-turn-responder/SKILL.md), [review/merge độc lập](../../.agents/skills/github-pr-review-merge/SKILL.md). [PR #9](https://github.com/thientran2359/opendash-cms/pull/9) vẫn draft PAUSED; [PR #7](https://github.com/thientran2359/opendash-cms/pull/7) còn blocker W01-SEC-01; [Issue #5](https://github.com/thientran2359/opendash-cms/issues/5) `NO_DISPATCH`.

**Tổ chức được chủ dự án chốt ngày 10/11/2026:** `milestones/<mốc>/slices/` giữ các plan thực hiện theo milestone **bất kể đang Backlog/Ready/In progress/In review/Done**; `modules/` giữ capability và contract **xuyên milestone**; [GitHub Project #3](https://github.com/users/thientran2359/projects/3) là nguồn trạng thái task, không sao chép thành thư mục `pending/doing/done`. Sáu plan M3.1–M3.6 đã được chuyển khỏi gốc `plans/` vào [M3/slices](milestones/M3/README.md), **không archive, không coi đã hoàn thành task hoặc đóng M3**. [Báo cáo G5](../verification/README.md), [snapshot M3](M3_STATUS_2026-11-09.md), living briefs và các G2 lịch sử giữ nguyên ý nghĩa. Các link Markdown trong repo đã được chỉnh; URL `blob/main/docs/plans/<tên-cũ>` bên ngoài cần cập nhật, link GitHub tại commit SHA lịch sử vẫn tồn tại.

## Quản lý roadmap và module V1

Roadmap và danh mục module được quản lý theo hai trục liên kết bằng ID cố định: **Release → Milestone → Slice → Task** để theo dõi tiến độ và **Product → Domain → Module → Capability** để kiểm soát phạm vi chức năng. Nghiên cứu và chốt thiết kế từng module trước khi giao nhiệm vụ code.

**Quy trình đã thống nhất:** [Codex ↔ Antigravity (Turn 1–5)](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) — Living Module Brief G0–G3, Full-Track/Fast-Track, primary acceptance slice, per-task file ownership và Stop-and-Propose. Đây là đồng thuận **về cách quản lý công việc**, không phải kết luận schema M3. Không nhân bản bảng trạng thái ở README.

1. [Roadmap V1](ROADMAP_V1.md) — mốc M1–M8, dependencies, gates và các slice M3 dự kiến.
2. [Module Catalog V1](MODULE_CATALOG_V1.md) — danh mục ID module, phạm vi capability và trạng thái theo bằng chứng (số lượng lấy từ catalog, không duy trì số đếm riêng tại đây).
3. [Milestone Coverage Matrix](milestones/COVERAGE_MATRIX.md) và [9 Milestone Task Plans](milestones/README.md) — liên kết module→mốc→task candidate/dependency/acceptance; chỉ Issue/Project giữ trạng thái thực thi.
4. [Planning Workflow](PLANNING_WORKFLOW.md) — G0–G5, Full-Track/Fast-Track, trạng thái/evidence, source of truth và cách chia task.
   [Muse → CoS Windows Phase 1 — lịch sử G2/G3, hiện PAUSED](muse-cos-win-phase1-g2.md) — lưu Poller P0–P4 và [8 work packages G3 từng được duyệt](muse-cos-win-phase1-g2.md#5-g3-breakdown--8-work-packages-đã-được-phê-duyệt); **không vận hành Poller/Task Watcher, không có implementation Issue READY**, Issue #5 vẫn NO_DISPATCH, P0–P4 chưa có G5 runtime PASS.
5. [Modules — giải thích chức năng xuyên milestone](modules/README.md), mẫu [Living Module Brief](templates/MODULE_BRIEF.md) và [handoff Slice Task](templates/SLICE_TASK.md); hai brief đầu tiên là [CNT-01 CPT](modules/CNT-01-content-types.md) và [CNT-05 Media](modules/CNT-05-media.md) — **G1 được review theo hồ sơ lịch sử; G2 ứng viên cần đối chiếu implementation hiện hành**.

**Giới hạn hiện trạng:** M1/M2 và M2.5 đã có bằng chứng Windows theo mốc lịch sử; M2.5 được đóng ở commit `da79304`. M3.1–M3.6 [đã có source và evidence theo slice trên macOS](M3_STATUS_2026-11-09.md); **M3 còn PARTIAL**, các capability chưa hoàn tất vẫn cần gate/verification và parity Windows/Linux riêng. M4–M8 là kế hoạch tiếp theo, không mặc định đã triển khai.

## Kế hoạch milestone và hồ sơ lịch sử

- [Milestone Plans M1–M8](milestones/README.md) lưu đầy đủ kế hoạch từng mốc và task candidates theo module; [ma trận coverage 27 module](milestones/COVERAGE_MATRIX.md) giúp phát hiện module nào chưa có việc. M2.5 đã [hoàn thành và kiểm chứng trong scope](milestones/M2.5-core-foundation.md); xem [báo cáo M2.5](../verification/2026-11-07-m2-5-core-kernel.md) và [ADR-0002](../decisions/ADR-0002-core-extension-contract.md).
- [Bản đối chiếu nhập mã M1/M2](../archive/migrations/2026-11-07-m1-m2-module-map.md) là hồ sơ lịch sử của đợt nhập mã đã hoàn tất; kết quả kiểm chứng ở [workspace đích](../verification/2026-11-07-m1-m2-import.md).

Các nhiệm vụ mới được lập theo [Planning Workflow](PLANNING_WORKFLOW.md). Kế hoạch đã đóng vẫn giữ liên kết tới contract, ADR và báo cáo thực chạy để truy vết.

**Kiểm tra tự động:** từ root worktree, chạy `python cms/scripts/check-plans.py` và `python cms/scripts/check-plans.py --self-test`; checker chỉ xác thực tính nhất quán tài liệu, không thay thế code/tests hoặc phê duyệt G2.

## M3 — Core CMS & WordPress-like Admin (phương án A, G0 10/11/2026)

**Đọc trước:** [M3 scope & acceptance mới](milestones/M3/README.md) và [task plan M3 P01–P19](milestones/M3/TASK_PLAN.md). Có built-in post/page, Admin Tailwind WordPress-inspired, Media/Users/Settings/Themes/Plugins; taxonomy/plugin bridge/video/delete nâng cao chuyển M4/M6/M7/M8, không bỏ khỏi V1. [Snapshot M3 09/11](M3_STATUS_2026-11-09.md) chỉ dùng đối chiếu source/evidence giới hạn OS trước đổi G0.

**Sáu plan M3.1–M3.6 vẫn ở M3/slices/, không archive:** [M3.1 — CPT & Fields](milestones/M3/slices/2026-11-08-m3-content-types-and-fields.md), [M3.2 — SQLite/API CPT](milestones/M3/slices/2026-11-08-m3-2-content-types-backend.md), [M3.3 — Entry CRUD](milestones/M3/slices/2026-11-08-m3-3-content-entry-crud.md), [M3.4 — Admin Content UI](milestones/M3/slices/2026-11-08-m3-4-admin-content-ui.md), [M3.5 — Media](milestones/M3/slices/2026-11-09-m3-5-media-library.md), [M3.6 — Public Rendering](milestones/M3/slices/2026-11-09-m3-6-public-content-rendering.md). [**M3 Task Plan — 19 planning candidates**](milestones/M3/TASK_PLAN.md), không phải bảng task Ready; các slice cũ chỉ có bằng chứng macOS giới hạn, Done phụ thuộc GitHub Project/G5.
