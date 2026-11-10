# M5 — Theme & Templates

**Kế hoạch V1 — chưa có bằng chứng G5 toàn milestone.** [Roadmap](../../ROADMAP_V1.md) · [PRD](../../../product/PRODUCT_REQUIREMENTS.md). **M3 A** có theme ZIP/install/list/activate và General Settings tối thiểu; **M5 mở rộng** theme lifecycle/updates, template page/header/footer/archive/single, URL RequestContext, Theme Options; có **SITE-01 menu storage tối thiểu cho sample import (P09)** và **theme ZIP khai báo CPT/fields/taxonomy qua canonical domain APIs (P10)**. Import mẫu **không ghi đè** và phải giữ schema/dữ liệu/layout/menu site-owned khi đổi/cập nhật theme.

**Module chính:** THM-01/02/03; phối hợp CNT-01/02/03/04, BLD-01/02/03/05, SITE-01 (settings core **M3**, menu model M5-P09), SITE-02 (URL policy nâng cao), CNT-05. Theme block M1 và M3 basic theme activation là **baseline**, không chứng nhận M5. M7-P02 dùng lại menu model M5; M6-P08 nhận **plugin** declarations, không thay theme path M5.

**Phân rã:** [M5/TASK_PLAN.md](TASK_PLAN.md). [M4](../M4/README.md) cung cấp tree/persistence/binding, [M6](../M6/README.md) cung cấp extension manifest sâu hơn, [M7](../M7/README.md) hoàn thiện SEO/menu/settings. Các thay đổi về URL/theme activation phải có G2 và migration/rollback test.
