# M7 — Site Features

**Kế hoạch V1:** mở rộng General Settings M3 thành site settings/navigation, SEO URL/title/description/OG image/canonical/robots/sitemap, contact form qua Builder với submissions+SMTP, và **media video V1** chuyển từ M3 cũ.

**Modules:** SITE-01/02/03, CNT-05 (video); dùng BLD-02/03, CNT-04, FND-02/05. General Settings core M3, Theme Options/URL policy M5 là **dependency**, không nghiệm thu lại như scope mới. [Roadmap](../../ROADMAP_V1.md) · [PRD](../../../product/PRODUCT_REQUIREMENTS.md).

**Task breakdown:** [M7/TASK_PLAN.md](TASK_PLAN.md). V1 một ngôn ngữ nội dung; không mở đa ngữ, history, advanced filtering hoặc publishing workflow ngoài phạm vi PRD.

## Bộ đặc tả 14/11/2026

- [G1: actual source và alternatives](../../../../.agents/research/2026-11-14-m7-site-features-g1.md)
- [G2: ADR-0014](../../../decisions/ADR-0014-m7-site-features.md) và [normative contract](../../../contracts/M7_SITE_FEATURES.md)
- [G3: 14 task / đủ8 candidate](slices/2026-11-14-m7-site-features-g3.md), kể cả dependency/native G5/closure
- Living briefs: [SITE-01](../../modules/SITE-01-settings-navigation.md), [SITE-02](../../modules/SITE-02-seo.md), [SITE-03](../../modules/SITE-03-contact.md), [CNT-05 video](../../modules/CNT-05-media.md)

**Status:** G2 AGREED/G3 READY hiệu lực trên main sau merge PR design; implementation NOT_STARTED, verification NOT_RUN. Không đồng nghĩa Project Ready: M5#157 và M6#188 cần G5 thật trước pickup M7. Chưa tạo implementation Issue/dispatch trong lượt chuẩn bị.

[Planning validation receipt](../../../verification/2026-11-14-m7-planning.md) records actual checks and limits; does not establish runtime G5.
