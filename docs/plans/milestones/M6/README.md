# M6 — Plugin Extensions

**Kế hoạch V1.** M2 có plugin ZIP/worker baseline; M2.5 có HookDispatcher/Base APIs; **M3 A có màn Plugins ZIP/list/enable/disable nhưng chưa CPT/field bridge**. M6 sở hữu CPT/field provider bridge (di chuyển từ M3 cũ), hoàn thiện action/event/server APIs, external calls trong quyền, registration blocks/templates/settings/taxonomy, declarative Admin UI và recovery.

**Modules:** PLG-01 (regression), PLG-02, PLG-03, PLG-04; phối hợp FND-01/02/05, CNT-01/02/03, BLD-01/02, THM-02, SITE-01. [Roadmap](../../ROADMAP_V1.md) · [PRD Plugin](../../../product/PRODUCT_REQUIREMENTS.md).

**Các task / acceptance:** [M6/TASK_PLAN.md](TASK_PLAN.md). Tắt plugin không xóa schema/entries/layout của website. Mọi API/ABI/permission mới là Full-Track và cần G2; không mặc định bật tác vụ định kỳ (ngoài V1).
