# Living Module Brief — chức năng xuyên milestone

`modules/` là nơi mô tả **CMS cần có những capability nào**, khác với [`milestones/`](../milestones/README.md) mô tả **công việc thực hiện theo từng giai đoạn**. Một module giữ ID ổn định trong [Module Catalog V1](../MODULE_CATALOG_V1.md) và có thể được triển khai qua nhiều milestone, slice, task. Không di chuyển brief theo task Status và không sao chép một brief cho từng milestone.

Mỗi Living Module Brief trình bày scope/G0, research G1, **thiết kế G2 đã chốt hoặc còn PROPOSED**, dependency, owner/contract và Primary Acceptance Slice dự kiến. Phải tách `IMPLEMENTED` ở một slice khỏi `VERIFIED` theo bằng chứng và khỏi trạng thái toàn module; **GitHub Project #3** mới là nguồn Backlog/Ready/In progress/In review/Done của task, không phải thư mục này.

- [CNT-01 — Content Types/CPT](CNT-01-content-types.md): domain và CPT Admin đã có một phần qua M3.1–M3.4; provider registration/lifecycle, schema changes còn thiếu; **PARTIAL**. Phạm vi built-in Post/Page P07/P08/P16 có [G2 owner-direct AGREED](../../decisions/ADR-0004-m3-post-page-publication-boundary.md), G3 DRAFT.
- [CNT-05 — Media Library](CNT-05-media.md): upload, thumbnail, picker và media public theo tham chiếu có bằng chứng M3.5/M3.6 macOS; video/delete/recovery/backup và parity còn mở; **PARTIAL**. G2 P16 chốt bổ sung ACL draft media trong [ADR-0004](../../decisions/ADR-0004-m3-post-page-publication-boundary.md).

Các module khác tra tại [Module Catalog](../MODULE_CATALOG_V1.md). Không tạo brief trống chỉ để đủ số module. [M3 milestone và các slice đang thực hiện](../milestones/M3/README.md), [snapshot M3](../M3_STATUS_2026-11-09.md), [quy trình planning](../PLANNING_WORKFLOW.md) và [verification](../../verification/README.md) là các nguồn bổ trợ, **không thay thế trạng thái task GitHub**.

## M7 living briefs — 2026-11-14

- [SITE-01 settings/navigation](SITE-01-settings-navigation.md)
- [SITE-02 canonical URL/SEO/sitemap](SITE-02-seo.md)
- [SITE-03 contact/submissions/SMTP](SITE-03-contact.md)

G2/G3 documentation is effective on merged design PR; implementation/G5 M7 NOT_STARTED/NOT_RUN.
