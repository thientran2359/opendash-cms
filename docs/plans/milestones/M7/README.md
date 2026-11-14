# M7 — Site Features

**Kế hoạch V1:** mở rộng General Settings M3 thành site settings/navigation, SEO URL/title/description/OG image/canonical/robots/sitemap, contact form qua Builder với submissions+SMTP, và **media video V1** chuyển từ M3 cũ.

**Modules:** SITE-01/02/03, CNT-05 (video); dùng BLD-02/03, CNT-04, FND-02/05. General Settings core M3, Theme Options/URL policy M5 là **dependency**, không nghiệm thu lại như scope mới. [Roadmap](../../ROADMAP_V1.md) · [PRD](../../../product/PRODUCT_REQUIREMENTS.md).

**Task breakdown:** [M7/TASK_PLAN.md](TASK_PLAN.md). V1 một ngôn ngữ nội dung; không mở đa ngữ, history, advanced filtering hoặc publishing workflow ngoài phạm vi PRD.

## Bộ đặc tả 14/11/2026

- [G1: actual source và alternatives](../../../../.agents/research/2026-11-14-m7-site-features-g1.md)
- [G2: ADR-0014](../../../decisions/ADR-0014-m7-site-features.md) và [normative contract](../../../contracts/M7_SITE_FEATURES.md)
- [G3: 14 task / đủ8 candidate](slices/2026-11-14-m7-site-features-g3.md), kể cả dependency/native G5/closure
- Living briefs: [SITE-01](../../modules/SITE-01-settings-navigation.md), [SITE-02](../../modules/SITE-02-seo.md), [SITE-03](../../modules/SITE-03-contact.md), [CNT-05 video](../../modules/CNT-05-media.md)

**Status:** G2 AGREED/G3 READY hiệu lực trên main sau merge PR design; implementation NOT_STARTED, verification NOT_RUN. Không đồng nghĩa Project Ready: M5/M6 Windows development gate cần PASS trước pickup M7; macOS NOT_RUN được hoãn đến release theo ADR-0014. Issues đã phát hành #202–#215 (14/11/2026); chưa dispatch hoặc chuyển Ready.

[Planning validation receipt](../../../verification/2026-11-14-m7-planning.md) records actual checks and limits; does not establish runtime G5.

## Issues đã phát hành — snapshot 14/11/2026

[Nova Windows: #202–#213](https://github.com/thientran2359/opendash-cms/issues?q=is%3Aissue+milestone%3A%22M7+%E2%80%94+Site+Features%22+label%3Aagent%3Anova) · [macOS #214](https://github.com/thientran2359/opendash-cms/issues/214) · [Atlas closure #215](https://github.com/thientran2359/opendash-cms/issues/215). [Task index](../../TASK_INDEX.md) có mapping đầy đủ; immutable G3 source trong từng Issue là `ff7145ba245b70efe6ca206e147a7b2ad6886aa2`.

Mọi Issue đang Backlog trên Project #3 khi kiểm tra; owner dự kiến ở G3 được thay bằng phân công vận hành trong Issue theo owner request. Nova tiếp tục kiểm chứng Windows M5/M6 còn thiếu trước pickup M7-S01-T01 (#202), rồi lần lượt theo dependencies; một implementation task active mỗi nhánh. Không tự coi Windows gate đã PASS hoặc macOS VERIFIED; không gửi chat/dispatch tự động.
