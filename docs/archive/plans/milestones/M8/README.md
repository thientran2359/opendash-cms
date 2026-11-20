# M8 — Operations & Release V1

## Delivery stages — 16/11/2026

[ADR-0017](../../../../decisions/ADR-0017-usable-beta-staging.md) ưu tiên internal Windows
Beta trước common RC/V1 public. [Beta G3](../M7/slices/2026-11-16-usable-beta-g3.md) là
integration checkpoint, không thay M8 gates. #242/#245 Forge; #246 Harbor; #247/#248
Atlas, tất cả Backlog tại snapshot 16/11. #188 còn In progress dù đã có Mac exact-SHA
evidence; không suy từ memory fix thành full native acceptance. Các status/owner cũ
phía dưới là snapshot lịch sử, live Issue/Project có ưu tiên.

**Đích V1:** media lifecycle (referenced-delete/reconcile) từ D/#15, backup/restore thủ công DB+uploads+theme/plugin+site config, Linux VPS single binary, upgrade/migration/recovery và E2E toàn bộ V1. [Roadmap](../../ROADMAP_V1.md) · [PRD](../../../../product/PRODUCT_REQUIREMENTS.md).

**Modules chủ trì:** OPS-01, OPS-02; tích hợp FND-03/04, CNT-05, THM-01, PLG-01 và hồi quy các capability V1. Contract backup được nghiên cứu từ M3, theme/plugin package từ M5/M6; **M8 mới có backup engine và nghiệm thu restore**.

**Task breakdown:** [M8/TASK_PLAN.md](TASK_PLAN.md). Chỉ kết luận release `VERIFIED` khi có runner Linux thực và evidence đủ acceptance; Mac/Windows tests không thay thế Linux. Giấy phép/phương án kinh doanh vẫn là quyết định sản phẩm cần chốt, không tự chọn.

## G1→G2→G3 toàn milestone — 14/11/2026

- [G1 inventory/source/alternatives](../../../../../.agents/research/2026-11-14-m8-operations-release-g1.md).
- [ADR-0015 Final Decision](../../../../decisions/ADR-0015-m8-operations-release.md) và [normative contract](../../../../contracts/M8_OPERATIONS_RELEASE.md).
- [G3: đủ18 task / 8 candidate](slices/2026-11-14-m8-operations-release-g3.md), gồm nhiệm vụ phụ thuộc, native Windows/Linux/macOS, operator runbooks và final audit.
- [OPS-01 brief](../../modules/OPS-01-backup-restore.md), [OPS-02 brief](../../modules/OPS-02-linux-release.md), [CNT-05 lifecycle](../../modules/CNT-05-media.md).

G2/G3 có hiệu lực sau design PR merge; implementation NOT_STARTED, runtime/native/performance NOT_RUN. Không tự tạo Project Ready hoặc chiếm task Nova đang làm. Development có thể dựa trên M7 Windows acceptance #213; final V1 vẫn cần Linux thực, deferred macOS, M7 full closure và owner license/release authorization.

[Planning validation receipt](../../../../verification/2026-11-14-m8-planning.md) records actual checks, internal review and runtime limits.

## Implementation Issues — snapshot 14/11/2026

- Nova: **#231–#245**; first [M8-S01-T01 #231](https://github.com/thientran2359/opendash-cms/issues/231) is **Ready** after Atlas gate review.
- [#246 native macOS](https://github.com/thientran2359/opendash-cms/issues/246): Harbor, Backlog/deferred.
- [#247 runbooks](https://github.com/thientran2359/opendash-cms/issues/247), [#248 release audit](https://github.com/thientran2359/opendash-cms/issues/248): Atlas, Backlog.

All18 Issues carry immutable G3 source `4ec2429f72fe47d540575323bd1e835ff855a097`, scope/files/acceptance/linked dependencies. [Task index](../../TASK_INDEX.md) maps every candidate/task/Issue; [Project#3](https://github.com/users/thientran2359/projects/3) is live authority. Remaining17 Issues stay Backlog until actual predecessor/platform/license gates are satisfied. Nova can advance eligible successors after acceptance/seam handoff under ADR-0010, one active task per branch; no automatic dispatch or public release.
