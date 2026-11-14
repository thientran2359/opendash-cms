# M8 — Operations & Release V1

**Đích V1:** media lifecycle (referenced-delete/reconcile) từ D/#15, backup/restore thủ công DB+uploads+theme/plugin+site config, Linux VPS single binary, upgrade/migration/recovery và E2E toàn bộ V1. [Roadmap](../../ROADMAP_V1.md) · [PRD](../../../product/PRODUCT_REQUIREMENTS.md).

**Modules chủ trì:** OPS-01, OPS-02; tích hợp FND-03/04, CNT-05, THM-01, PLG-01 và hồi quy các capability V1. Contract backup được nghiên cứu từ M3, theme/plugin package từ M5/M6; **M8 mới có backup engine và nghiệm thu restore**.

**Task breakdown:** [M8/TASK_PLAN.md](TASK_PLAN.md). Chỉ kết luận release `VERIFIED` khi có runner Linux thực và evidence đủ acceptance; Mac/Windows tests không thay thế Linux. Giấy phép/phương án kinh doanh vẫn là quyết định sản phẩm cần chốt, không tự chọn.

## G1→G2→G3 toàn milestone — 14/11/2026

- [G1 inventory/source/alternatives](../../../../.agents/research/2026-11-14-m8-operations-release-g1.md).
- [ADR-0015 Final Decision](../../../decisions/ADR-0015-m8-operations-release.md) và [normative contract](../../../contracts/M8_OPERATIONS_RELEASE.md).
- [G3: đủ18 task / 8 candidate](slices/2026-11-14-m8-operations-release-g3.md), gồm nhiệm vụ phụ thuộc, native Windows/Linux/macOS, operator runbooks và final audit.
- [OPS-01 brief](../../modules/OPS-01-backup-restore.md), [OPS-02 brief](../../modules/OPS-02-linux-release.md), [CNT-05 lifecycle](../../modules/CNT-05-media.md).

G2/G3 có hiệu lực sau design PR merge; implementation NOT_STARTED, runtime/native/performance NOT_RUN. Không tự tạo Project Ready hoặc chiếm task Nova đang làm. Development có thể dựa trên M7 Windows acceptance #213; final V1 vẫn cần Linux thực, deferred macOS, M7 full closure và owner license/release authorization.

[Planning validation receipt](../../../verification/2026-11-14-m8-planning.md) records actual checks, internal review and runtime limits.
