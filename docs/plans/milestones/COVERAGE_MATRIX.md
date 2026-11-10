# OpenDash V1 — Ma trận 27 Module → 9 Milestone → Task

**Nguồn thẩm quyền:** [PRD V1](../../product/PRODUCT_REQUIREMENTS.md) (scope), [Module Catalog](../MODULE_CATALOG_V1.md) (27 ID), [Roadmap](../ROADMAP_V1.md) (mốc M1, M2, M2.5, M3–M8), [Planning Workflow](../PLANNING_WORKFLOW.md) (G0–G5). Đây là **bản đồ kế hoạch tĩnh**, không phải G2 design được chốt hay task `READY`. Tham khảo [GitHub Milestones #1–#9](https://github.com/thientran2359/opendash-cms/milestones), [GitHub Project #3](https://github.com/users/thientran2359/projects/3) để xem milestones thực tế và trạng thái mỗi Issue.

**Phương pháp:** Mỗi module/capability có nơi chịu trách nhiệm thực hiện **chính** và các milestone **tiêu thụ/tích hợp**. Module đi qua nhiều mốc **không sinh một bản copy module brief**; milestone dùng task candidate trỏ về đúng scope. Task candidate `Mx-Pxx` chỉ là hàng phân rã, **không phải** `Mx-Sxx-Txx` G3 Task ID, Issue, hay bằng chứng hoàn thành. Khi chuẩn bị giao task: nghiên cứu source/evidence → G2 nếu cần → G3 được review/merge → tạo Issue trong milestone GitHub đúng number → Project Ready theo gate.

## Danh mục milestone đầy đủ

| Milestone | Trọng tâm | Kế hoạch và task | Phạm vi bằng chứng hiện hành |
| --- | --- | --- | --- |
| **M1** — CMS Baseline | Auth, SQLite, Admin, Builder 1 block, Theme ZIP, Windows executable | [M1](M1/README.md) · [Tasks](M1/TASK_PLAN.md) | M1 Windows baseline theo report |
| **M2** — Plugin Sandbox | ZIP, Wasmi worker, permission, disable/recovery | [M2](M2/README.md) · [Tasks](M2/TASK_PLAN.md) | M2 Windows baseline theo report |
| **M2.5** — Core Foundation | Owned registry, HookDispatcher, APIs/checker | [M2.5](M2.5/README.md) · [Tasks](M2.5/TASK_PLAN.md) · [Original plan](M2.5-core-foundation.md) | Roadmap CLOSED trong scope đã kiểm chứng |
| **M3** — Content & Media | CPT, fields, entry, taxonomy, media/video, minimal plugin bridge | [M3](M3/README.md) · [Tasks](M3/TASK_PLAN.md) · [M3.1–M3.6 slices](M3/README.md#các-slice-và-nguồn-kiểm-chứng) | PARTIAL; macOS slice evidence, Windows/Linux gaps |
| **M4** — Visual Builder | AST tree, element editor, responsive, data binding | [M4](M4/README.md) · [Tasks](M4/TASK_PLAN.md) | PLANNED, chưa có G5 toàn M4 |
| **M5** — Theme & Templates | RequestContext, template composition, Theme Options, sample import/preservation | [M5](M5/README.md) · [Tasks](M5/TASK_PLAN.md) | PLANNED, M1/M3 chỉ một số baseline |
| **M6** — Plugin Extensions | Events/actions, external calls, bridge còn lại, declarative UI | [M6](M6/README.md) · [Tasks](M6/TASK_PLAN.md) | PLANNED, M2/M2.5/M3 chỉ là baseline/bridge prerequisites |
| **M7** — Site Features | Settings/menu, SEO/sitemap, contact form/SMTP | [M7](M7/README.md) · [Tasks](M7/TASK_PLAN.md) | PLANNED |
| **M8** — Operations & Release | Backup/restore, Linux distribution, E2E V1 | [M8](M8/README.md) · [Tasks](M8/TASK_PLAN.md) | PLANNED, Linux NOT_RUN đến khi có runner |

## Coverage: mỗi Module Catalog ID phải xuất hiện

| ID | Capability/trách nhiệm V1 | Milestone triển khai/acceptance chính + task | Milestone phụ thuộc/tích hợp (không đóng 2 lần) |
| --- | --- | --- | --- |
| **FND-01** | Core ID/registry/hook/API convention | [M2.5-P01/P03](M2.5/TASK_PLAN.md), P04/P06 | M3 CPT provider, M4 AST, M6 plugin |
| **FND-02** | Auth/session/RBAC/CSRF/Origin | [M1-P02](M1/TASK_PLAN.md) | M2 plugin, M3 entry, M5 theme, M7 settings, M8 security G5 |
| **FND-03** | SQLite/migration/SitePaths | [M1-P01](M1/TASK_PLAN.md), [M2.5-P02/P06](M2.5/TASK_PLAN.md) | M3 data ownership, M8-P01/P03 backup/restore |
| **FND-04** | Windows single executable, distribution | [M1-P05](M1/TASK_PLAN.md); [M8-P04](M8/TASK_PLAN.md) Linux V1 release | M3-P13/P14 Windows/Linux parity |
| **FND-05** | Admin shell, primitives, role UI | [M1-P03](M1/TASK_PLAN.md), [M2.5-P05/P07](M2.5/TASK_PLAN.md) | M3 form, M4 inspector, M6 plugin UI, M7 settings |
| **CNT-01** | CPT định nghĩa/ownership/lifecycle | [M3-P01/P02/P07](M3/TASK_PLAN.md) | M5 theme declarations, M6 provider regression |
| **CNT-02** | Custom fields và validation | [M3-P01/P03/P10](M3/TASK_PLAN.md) | M4 binding, M5 templates |
| **CNT-03** | Category/tag, hierarchy, entry links | [M3-P08/P09](M3/TASK_PLAN.md) | M4 list query, M5 URLs, M6 plugin taxonomy bridge |
| **CNT-04** | Entry CRUD và RequestContext/URL dispatch | [M3-P03/P06/P09](M3/TASK_PLAN.md); [M5-P02](M5/TASK_PLAN.md) | M4 binding, M5 public delivery, M7 SEO |
| **CNT-05** | Media ảnh/video, picker, ACL, lifecycle | [M3-P05/P06/P11/P12](M3/TASK_PLAN.md) | M5 sample import, M7 OG image, M8 backup |
| **BLD-01** | Block registry/render/preview, missing-provider diagnostics | [M1-P04](M1/TASK_PLAN.md) baseline; [M4-P02/P07](M4/TASK_PLAN.md) | M5 template composition, M6 plugin |
| **BLD-02** | Section→Row→Column→Element AST + core elements | [M4-P01/P02](M4/TASK_PLAN.md) | M5 theme layouts, M6 plugin blocks |
| **BLD-03** | Tree/canvas/inspector | [M4-P03/P07](M4/TASK_PLAN.md) | M5 editable theme layouts, M7 contact form builder |
| **BLD-04** | Responsive desktop/tablet/mobile | [M4-P04](M4/TASK_PLAN.md) | M5 public rendering regression |
| **BLD-05** | Binding/query/list/pagination | [M4-P05/P06](M4/TASK_PLAN.md) | M5-P03/P08 public composition |
| **THM-01** | Theme ZIP/package registration | [M1-P04](M1/TASK_PLAN.md) baseline; [M5-P01/P07](M5/TASK_PLAN.md) | M6 declarative plugin bridge, M8 backup |
| **THM-02** | page/header/footer/archive/single templates, Theme Options | [M5-P03/P04/P08](M5/TASK_PLAN.md) | M4 AST/binding, M7 site options |
| **THM-03** | Import mẫu và bảo toàn layout khi đổi theme | [M5-P05/P06/P08](M5/TASK_PLAN.md) | M7 menu/site data, M8 restore regression |
| **PLG-01** | ZIP/Wasmi sandbox, permission, disable/recovery | [M2-P01..P05](M2/TASK_PLAN.md) | M6-P06 sandbox regression, M8 backup |
| **PLG-02** | Hooks/actions/server APIs/external calls | [M2.5-P04](M2.5/TASK_PLAN.md) baseline; [M6-P01/P02](M6/TASK_PLAN.md) | M3 content hooks/transactions |
| **PLG-03** | Declarative registration bridge | [M3-P10](M3/TASK_PLAN.md) CPT/fields; [M6-P03/P04](M6/TASK_PLAN.md) extensions | M5 theme/Builder/settings providers |
| **PLG-04** | Declarative plugin Admin UI | [M6-P05](M6/TASK_PLAN.md) | FND-05, M7 site settings experience |
| **SITE-01** | Settings/options/menu/navigation | [M5-P04](M5/TASK_PLAN.md) tối thiểu; [M7-P01/P02](M7/TASK_PLAN.md) đầy đủ | M5 Theme Options/import, M6 plugin settings |
| **SITE-02** | Custom URL/SEO/canonical/robots/sitemap | [M5-P02](M5/TASK_PLAN.md) URL contract tối thiểu; [M7-P03/P04](M7/TASK_PLAN.md) SEO đủ V1 | CNT-04 route, M4 list URL |
| **SITE-03** | Contact form qua Builder, submissions, SMTP | [M7-P05/P06](M7/TASK_PLAN.md) | M4 Builder, M5 public, M7 SITE-01 SMTP |
| **OPS-01** | Backup/restore DB+uploads+site config+theme/plugin | [M8-P01/P02/P03](M8/TASK_PLAN.md) | M3-P12 media/data safety, M5/M6 packages |
| **OPS-02** | Linux VPS distribution, V1 E2E | [M8-P04..P07](M8/TASK_PLAN.md) | M3-P13/P14 early Windows/Linux smoke, M1 FND-04 |

**Coverage goal:** 27/27 IDs được map vào ít nhất một task candidate và một milestone cụ thể; một ID có thể có **milestone đầu** và **milestone nghiệm thu đủ V1** khác nhau. Không dùng số hàng hoàn thành để suy `Done`; [báo cáo verification](../../verification/README.md) quyết định phần hành vi đã test theo OS/SHA, còn [Project Status](https://github.com/users/thientran2359/projects/3) quyết định task thực thi.

## Dependency chain và bước tiếp theo

M1 → M2 → M2.5 → M3 (CPT/field/entry/taxonomy/media/minimal provider) → M4 (Builder) → M5 (Theme/template) → M6 (plugin extensions) → M7 (Site features) → M8 (backup/release).

**Nghiên cứu có thể song song** nhưng **implementation phải qua từng G2/G3 và file ownership khi có contract chung**. Các phụ thuộc xuyên mốc quan trọng: SITE-01 settings tối thiểu trong M5 trước Theme Options, rồi hoàn thiện tại M7; SITE-02 URL tối thiểu trước M5 public delivery, SEO đủ V1 tại M7; PLG-03 CPT/fields tối thiểu trong M3 rồi mở rộng M6; OPS-01 backup contract trong M3 nhưng engine M8; OPS-02 sớm kiểm Windows/Linux từ M3 nhưng V1 Linux release chốt M8. M3-P13/P14 là nhóm A0/A1 của [Issue #15](https://github.com/thientran2359/opendash-cms/issues/15#issuecomment-6760145410), **không đại diện cho toàn bộ roadmap/task**.

**Điều kiện tạo Issues:** coordinator xác thực milestone GitHub number+title (M1=1, M2=2, M2.5=3, M3=4, M4=5, M5=6, M6=7, M7=8, M8=9), đối chiếu open/closed Issues và source SHA, tách candidate thành đúng slice/task `Mx-Sxx-Txx` không trùng lịch sử, review G1/G2 nếu Full-Track, chốt G3 `READY` trong docs merge, ghi acceptance/exclusions/exact file ownership và reviewer. Chỉ sau đó mới tạo/đồng bộ GitHub Issue và Project; **không tạo hàng loạt Issue từ ma trận này**.
