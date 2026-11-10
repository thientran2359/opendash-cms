# Milestone M3 — Content & Media

**Trạng thái: PARTIAL / chưa CLOSED.** [Roadmap V1](../../ROADMAP_V1.md) xác định scope và điều kiện đóng mốc, [M3 status](../../M3_STATUS_2026-11-09.md) là snapshot theo source/evidence đến 09/11/2026. M3.1–M3.6 đã có source và một số bài test PASS trên **macOS theo SHA ghi trong từng báo cáo**, không đồng nghĩa G5 toàn M3 hoặc Windows/Linux được VERIFIED. Không tự chuyển các Issue lên `Done` khi plan hoặc code được chuyển thư mục.

`milestones/M3/slices/` giữ tài liệu triển khai theo **lát cắt**, không lưu theo trạng thái Ready/In progress/Done và không phải kho archive. [Module briefs](../../modules/README.md) quản lý các capability xuyên milestone; [GitHub Project #3](https://github.com/users/thientran2359/projects/3) quản lý trạng thái task và [GitHub PR](https://github.com/thientran2359/opendash-cms/pulls) quản lý review.

**Muốn biết toàn bộ phần việc M3:** đọc [M3 — TASK_PLAN](TASK_PLAN.md) với mapping CNT-01–05/PLG-03 → các slice đã có và task candidates còn thiếu. Nhóm A0/A1/B/C/D trong Final Decision #15 là **ưu tiên con**, không phải toàn bộ kế hoạch M3. Sau G3 review/merge mới có GitHub Issue task; đây **không phải queue `READY`**.

## Các slice và nguồn kiểm chứng

| Slice | Kế hoạch | Bằng chứng (phạm vi macOS tại SHA báo cáo) |
| --- | --- | --- |
| M3.1 | [CPT/Custom Fields](slices/2026-11-08-m3-content-types-and-fields.md) | [Domain schema/validation](../../../verification/2026-11-08-m3-1-content-domain.md) |
| M3.2 | [CPT SQLite/API](slices/2026-11-08-m3-2-content-types-backend.md) | [GET/POST, restart](../../../verification/2026-11-08-m3-2-content-types-backend.md) |
| M3.3 | [Entry CRUD](slices/2026-11-08-m3-3-content-entry-crud.md) | [CRUD/persistence](../../../verification/2026-11-08-m3-3-content-entry-crud.md) |
| M3.4 | [Admin Content UI](slices/2026-11-08-m3-4-admin-content-ui.md) | [Browser/draft/401/412](../../../verification/2026-11-08-m3-4-admin-content-ui.md) |
| M3.5 | [Media Library](slices/2026-11-09-m3-5-media-library.md) | [Raster/thumbnail/picker](../../../verification/2026-11-09-m3-5-media-library.md) |
| M3.6 | [Public CPT Rendering](slices/2026-11-09-m3-6-public-content-rendering.md) | [Public CPT/media ACL](../../../verification/2026-11-09-m3-6-public-content-rendering.md) |

**Còn thiếu để đóng M3:** taxonomy CNT-03, provider CPT/field bridge (CNT-01/02 × PLG-03), media delete/reconcile/video, các contract liên quan và Windows/Linux parity đúng acceptance. [Issue #15 — Final Decision](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410) quy định thứ tự nghiên cứu/verification, thể hiện thành các **candidate có gate/acceptance** trong [TASK_PLAN](TASK_PLAN.md), **không tự cấp task G3 READY**.

**Đường dẫn được đổi vì tổ chức tài liệu:** sáu file trên ban đầu nằm ở `docs/plans/<tên-file>`, sau đó từng xuất hiện tạm thời tại `docs/archive/plans/m3/<tên-file>` trên PR #17 chưa merge. Đường dẫn được chốt: `docs/plans/milestones/M3/slices/<tên-file>`. Các link tương đối trong repository đã được đối chiếu; link mutable `blob/main` ở bên ngoài phải chuyển sang đường dẫn mới sau PR merge, còn immutable historical-SHA URLs không đổi. Không giữ stub ở root `docs/plans/` để thư mục gọn.
