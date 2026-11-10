# M6 — Plugin Extensions

**Kế hoạch V1.** M2 đã có plugin ZIP/worker baseline; M2.5 có HookDispatcher/Base APIs; M3 thực hiện **bridge CPT/fields tối thiểu** trước M6. M6 hoàn thiện action/event/server APIs, external calls trong quyền, declarative registration còn lại, declarative Admin UI và recovery.

**Modules:** PLG-01 (regression), PLG-02, PLG-03, PLG-04; phối hợp FND-01/02/05, CNT-01/02/03, BLD-01/02, THM-02, SITE-01. [Roadmap](../../ROADMAP_V1.md) · [PRD Plugin](../../../product/PRODUCT_REQUIREMENTS.md).

**Các task / acceptance:** [M6/TASK_PLAN.md](TASK_PLAN.md). Tắt plugin không xóa schema/entries/layout của website. Mọi API/ABI/permission mới là Full-Track và cần G2; không mặc định bật tác vụ định kỳ (ngoài V1).
