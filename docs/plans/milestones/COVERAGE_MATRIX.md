# OpenDash V1 — Ma trận 27 Module → 9 Milestone → Task

**Nguồn thẩm quyền:** [PRD V1](../../product/PRODUCT_REQUIREMENTS.md) (scope), [Module Catalog](../MODULE_CATALOG_V1.md) (27 ID), [Roadmap](../ROADMAP_V1.md) (mốc M1, M2, M2.5, M3–M8), [Planning Workflow](../PLANNING_WORKFLOW.md) (G0–G5). Đây là **bản đồ kế hoạch tĩnh**, không phải G2 design được chốt hay task `READY`. Tham khảo [GitHub Milestones #1–#9](https://github.com/thientran2359/opendash-cms/milestones), [GitHub Project #3](https://github.com/users/thientran2359/projects/3) để xem milestones thực tế và trạng thái mỗi Issue.

**Phương pháp:** Mỗi module/capability có nơi chịu trách nhiệm thực hiện **chính** và các milestone **tiêu thụ/tích hợp**. Module đi qua nhiều mốc **không sinh một bản copy module brief**; milestone dùng task candidate trỏ về đúng scope. Task candidate `Mx-Pxx` chỉ là hàng phân rã, **không phải** `Mx-Sxx-Txx` G3 Task ID, Issue, hay bằng chứng hoàn thành. Khi chuẩn bị giao task: nghiên cứu source/evidence → G2 nếu cần → G3 được review/merge → tạo Issue trong milestone GitHub đúng number → Project Ready theo gate.

## Danh mục milestone đầy đủ

| Milestone | Trọng tâm | Kế hoạch và task | Phạm vi bằng chứng hiện hành |
| --- | --- | --- | --- |
| **M1** — CMS Baseline | Auth, SQLite, Admin, Builder 1 block, Theme ZIP, Windows executable | [M1](M1/README.md) · [Tasks](M1/TASK_PLAN.md) | M1 Windows baseline theo report |
| **M2** — Plugin Sandbox | ZIP, Wasmi worker, permission, disable/recovery | [M2](M2/README.md) · [Tasks](M2/TASK_PLAN.md) | M2 Windows baseline theo report |
| **M2.5** — Core Foundation | Owned registry, HookDispatcher, APIs/checker | [M2.5](M2.5/README.md) · [Tasks](M2.5/TASK_PLAN.md) · [Original plan](M2.5-core-foundation.md) | Roadmap CLOSED trong scope đã kiểm chứng |
| **M3** — Core CMS & WordPress-like Admin | Built-in post/page, Admin Tailwind, Media/Users/Settings/Themes/Plugins screens, E2E usable core | [M3](M3/README.md) · [Tasks](M3/TASK_PLAN.md) · [M3.1–M3.6 slices](M3/README.md#các-slice-và-nguồn-kiểm-chứng) | PARTIAL; macOS slice evidence cũ, chưa có G5 scope mới |
| **M4** — Visual Builder + Taxonomy | AST tree, editor, responsive, data binding, category/tag + entry links | [M4](M4/README.md) · [Tasks](M4/TASK_PLAN.md) | PLANNED, chưa có G5 toàn M4 |
| **M5** — Theme & Templates | RequestContext, template composition, Theme Options, sample import/preservation | [M5](M5/README.md) · [Tasks](M5/TASK_PLAN.md) | PLANNED, M1/M3 chỉ một số baseline |
| **M6** — Plugin Extensions | Events/actions, external calls, CPT/fields + other declarative bridges/UI | [M6](M6/README.md) · [Tasks](M6/TASK_PLAN.md) | PLANNED; M3 chỉ có plugin ZIP/lifecycle UI |
| **M7** — Site Features + Media Video | Settings/menu nâng cao, SEO/sitemap, contact form/SMTP, video | [M7](M7/README.md) · [Tasks](M7/TASK_PLAN.md) | PLANNED |
| **M8** — Operations & Release | Media delete/reconcile, backup/restore, Linux distribution, E2E V1 | [M8](M8/README.md) · [Tasks](M8/TASK_PLAN.md) | PLANNED, Linux NOT_RUN đến khi có runner |

## Coverage: mỗi Module Catalog ID phải xuất hiện

| ID | Capability/trách nhiệm V1 | Milestone triển khai/acceptance chính + task | Milestone phụ thuộc/tích hợp (không đóng 2 lần) |
| --- | --- | --- | --- |
| **FND-01** | Core ID/registry/hook/API convention | [M2.5-P01/P03](M2.5/TASK_PLAN.md), P04/P06 | M3 CPT provider, M4 AST, M6 plugin |
| **FND-02** | Auth/session/RBAC/CSRF/Origin | [M1-P02](M1/TASK_PLAN.md) baseline; [M3-P12](M3/TASK_PLAN.md) Users | M3 auth/Users E2E, M5 theme, M7 settings, M8 security G5 |
| **FND-03** | SQLite/migration/SitePaths | [M1-P01](M1/TASK_PLAN.md), [M2.5-P02/P06](M2.5/TASK_PLAN.md) | M3 data ownership, M8-P01/P03 backup/restore |
| **FND-04** | Windows single executable, distribution | [M1-P05](M1/TASK_PLAN.md); [M8-P04](M8/TASK_PLAN.md) Linux V1 release | M3-P18/P19 Windows parity |
| **FND-05** | Admin shell, primitives, role UI | [M1-P03](M1/TASK_PLAN.md), [M2.5-P05/P07](M2.5/TASK_PLAN.md); [M3-P09/P10](M3/TASK_PLAN.md) Tailwind/Dashboard | M3 screens, M4 inspector, M6 declarative UI, M7 settings |
| **CNT-01** | CPT định nghĩa/ownership/lifecycle | [M3-P01/P02/P07](M3/TASK_PLAN.md) | M5 theme declarations, M6-P08 provider bridge |
| **CNT-02** | Custom fields và validation | [M3-P01/P03/P07/P08](M3/TASK_PLAN.md) | M4 binding, M5 templates, M6-P08 plugin fields |
| **CNT-03** | Category/tag, hierarchy, entry links | [M4-P09/P10](M4/TASK_PLAN.md) | M4 list query, M5 URLs, M6-P04 plugin taxonomy bridge |
| **CNT-04** | Entry CRUD và RequestContext/URL dispatch | [M3-P03/P06/P08/P16](M3/TASK_PLAN.md); [M4-P10](M4/TASK_PLAN.md); [M5-P02](M5/TASK_PLAN.md) | M4 binding, M5 public delivery, M7 SEO |
| **CNT-05** | Media ảnh/video, picker, ACL, lifecycle | [M3-P05/P06/P11](M3/TASK_PLAN.md); [M7-P08](M7/TASK_PLAN.md) video; [M8-P08](M8/TASK_PLAN.md) lifecycle | M5 sample import, M7 OG image, M8 backup |
| **BLD-01** | Block registry/render/preview, missing-provider diagnostics | [M1-P04](M1/TASK_PLAN.md) baseline; [M4-P02/P07](M4/TASK_PLAN.md) | M5 template composition, M6 plugin |
| **BLD-02** | Section→Row→Column→Element AST + core elements | [M4-P01/P02](M4/TASK_PLAN.md) | M5 theme layouts, M6 plugin blocks |
| **BLD-03** | Tree/canvas/inspector | [M4-P03/P07](M4/TASK_PLAN.md) | M5 editable theme layouts, M7 contact form builder |
| **BLD-04** | Responsive desktop/tablet/mobile | [M4-P04](M4/TASK_PLAN.md) | M5 public rendering regression |
| **BLD-05** | Binding/query/list/pagination | [M4-P05/P06](M4/TASK_PLAN.md) | M5-P03/P08 public composition |
| **THM-01** | Theme ZIP/package registration | [M1-P04](M1/TASK_PLAN.md) baseline; [M3-P14](M3/TASK_PLAN.md) install/list/activate; [M5-P01/P07](M5/TASK_PLAN.md) advanced | M6 declarative plugin bridge, M8 backup |
| **THM-02** | page/header/footer/archive/single templates, Theme Options | [M5-P03/P04/P08](M5/TASK_PLAN.md) | M4 AST/binding, M7 site options |
| **THM-03** | Import mẫu và bảo toàn layout khi đổi theme | [M5-P05/P06/P08](M5/TASK_PLAN.md) | M7 menu/site data, M8 restore regression |
| **PLG-01** | ZIP/Wasmi sandbox, permission, disable/recovery | [M2-P01..P05](M2/TASK_PLAN.md); [M3-P15](M3/TASK_PLAN.md) Admin screen | M6-P06 sandbox regression, M8 backup |
| **PLG-02** | Hooks/actions/server APIs/external calls | [M2.5-P04](M2.5/TASK_PLAN.md) baseline; [M6-P01/P02](M6/TASK_PLAN.md) | M3 content hooks/transactions |
| **PLG-03** | Declarative registration bridge | [M6-P08](M6/TASK_PLAN.md) CPT/fields; [M6-P03/P04](M6/TASK_PLAN.md) extensions | M4 taxonomy, M5 theme/Builder/settings providers |
| **PLG-04** | Declarative plugin Admin UI | [M6-P05](M6/TASK_PLAN.md) | FND-05, M7 site settings experience |
| **SITE-01** | Settings/options/menu/navigation | [M3-P13](M3/TASK_PLAN.md) General Settings; [M5-P04](M5/TASK_PLAN.md) Theme Options; [M7-P01/P02](M7/TASK_PLAN.md) nâng cao | M6 plugin settings |
| **SITE-02** | Custom URL/SEO/canonical/robots/sitemap | [M5-P02](M5/TASK_PLAN.md) URL contract tối thiểu; [M7-P03/P04](M7/TASK_PLAN.md) SEO đủ V1 | CNT-04 route, M4 list URL |
| **SITE-03** | Contact form qua Builder, submissions, SMTP | [M7-P05/P06](M7/TASK_PLAN.md) | M4 Builder, M5 public, M7 SITE-01 SMTP |
| **OPS-01** | Backup/restore DB+uploads+site config+theme/plugin | [M8-P01/P02/P03/P08](M8/TASK_PLAN.md) | M8-P08 media/data safety, M5/M6 packages |
| **OPS-02** | Linux VPS distribution, V1 E2E | [M8-P04..P07](M8/TASK_PLAN.md) | M3-P18/P19 early Windows/Linux smoke, M1 FND-04 |

**Coverage goal:** 27/27 IDs được map vào ít nhất một task candidate và một milestone cụ thể; một ID có thể có **milestone đầu** và **milestone nghiệm thu đủ V1** khác nhau. Không dùng số hàng hoàn thành để suy `Done`; [báo cáo verification](../../verification/README.md) quyết định phần hành vi đã test theo OS/SHA, còn [Project Status](https://github.com/users/thientran2359/projects/3) quyết định task thực thi.

## Dependency chain và bước tiếp theo

M1 → M2 → M2.5 → **M3 (Usable Core CMS: post/page, Admin Tailwind, Media/Users/Settings/Themes/Plugins)** → M4 (Builder + taxonomy) → M5 (advanced Theme/templates) → M6 (plugin CPT/field bridge + extensions) → M7 (Site features/video) → M8 (media lifecycle, backup/release).

**Nghiên cứu có thể song song**, implementation phải qua G2/G3/ownership. SITE-01 General Settings tối thiểu **M3**, Theme Options M5, đầy đủ M7; SITE-02 routes post/page cơ bản M3, URL RequestContext M5, SEO đủ V1 M7; PLG-03 bridge từ M3 cũ chuyển **M6**; CNT-03 taxonomy **M4**; media video **M7**, delete/reconcile **M8** cùng backup; OPS-02 Windows parity M3, Linux release M8. M3-P18/P19 là A0/A1 #15, **không đại diện toàn bộ roadmap**.

**Điều kiện tạo Issues:** coordinator xác thực milestone GitHub number+title (M1=1, M2=2, M2.5=3, M3=4, M4=5, M5=6, M6=7, M7=8, M8=9), đối chiếu open/closed Issues và source SHA, tách candidate thành đúng slice/task `Mx-Sxx-Txx` không trùng lịch sử, review G1/G2 nếu Full-Track, chốt G3 `READY` trong docs merge, ghi acceptance/exclusions/exact file ownership và reviewer. Chỉ sau đó mới tạo/đồng bộ GitHub Issue và Project; **không tạo hàng loạt Issue từ ma trận này**.
