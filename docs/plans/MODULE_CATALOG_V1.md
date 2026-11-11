# Danh mục Domain → Module → Capability của OpenDash V1

**Trạng thái cập nhật G0 10/11/2026:** chủ dự án chốt **M3 phương án A — Core CMS & WordPress-like Admin**, giữ nguyên 27 module IDs/capability V1, chỉ cập nhật milestone triển khai [Roadmap](ROADMAP_V1.md). M3.1–M3.6 có evidence **theo slice macOS** [trước đổi scope](M3_STATUS_2026-11-09.md); không mặc định đã G5 M3 mới hoặc G2 từng được duyệt.

Đây là bản đồ chức năng ổn định của V1. Để tìm việc cần làm theo thứ tự thời gian, dùng [roadmap V1](ROADMAP_V1.md); để chia task, dùng [quy trình planning](PLANNING_WORKFLOW.md). Một module có thể xuất hiện ở nhiều milestone nhưng chỉ có một ID.

**Rà soát coverage 08/11/2026:** giữ nguyên ID và phạm vi PRD; bổ sung owner/dependency và tiêu chí nghiệm thu xuyên module theo [roadmap](ROADMAP_V1.md#nghiệm-thu-xuyên-module-cần-đưa-vào-các-brief-khi-mở). Đây là cập nhật kế hoạch, không thay trạng thái thiết kế G2 hoặc bằng chứng implementation/verification.

## Hướng dẫn đọc trạng thái

- **Baseline verified**: chỉ một tập hành vi nêu rõ đã có bằng chứng kiểm thử, không mặc định toàn module V1 hoàn thành.
- **Partial**: đã có code hoặc bằng chứng một phần nhưng còn capability hoặc tiêu chí chưa đạt.
- **Planned**: chức năng có trong yêu cầu V1, việc phân chia module/thiết kế/task mới là đề xuất; implementation chưa kiểm chứng.
- Mỗi capability được đánh ID (`CNT-01.1`, `CNT-01.2`, …) trong Living Module Brief. Một capability có **Primary Acceptance Slice** duy nhất; trạng thái hiện hành có thể đổi khi phát hiện regression, bằng chứng cũ vẫn được lưu. Không coi toàn bộ hàng bảng là một task.
- Mỗi dòng `Baseline verified` phải dẫn báo cáo [verification](../verification/README.md) cho **phạm vi đã test**; các dòng `Partial` hoặc `Planned` không ngầm đồng nghĩa đã chốt thiết kế.

## Foundation & Admin

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **FND-01** | Core conventions: ID validation, registry với owner, hook dispatcher, common error/package/storage API | M2.5; FND-03, PLG-01 | **Baseline verified** [M2.5](../verification/2026-11-07-m2-5-core-kernel.md); registry CPT sẽ mở thêm trong M3 | Ownership CPT, migration, thứ tự dispatch/content transaction và tương thích API |
| **FND-02** | Auth, session, Admin/Editor, CSRF/Origin, giữ form khi 401 | M1; FND-03 | **Baseline verified**: Auth và browser smoke trên Windows theo [M1/M2](../verification/2026-11-07-m1-m2-import.md) và [M2.5](../verification/2026-11-07-m2-5-core-kernel.md) | Boundary giữa role, plugin và content routes mới |
| **FND-03** | SQLite, migration, SitePaths, restart và phân tách dữ liệu website | M1–M2; không có | **Baseline verified**: migrations 0001–0003 và path tests trong [M1/M2](../verification/2026-11-07-m1-m2-import.md) | Migration tương thích, transaction, snapshot và quyền sở hữu dữ liệu |
| **FND-04** | Build một executable trên Windows, đóng gói Admin & assets, health/runtime config | M1–M2; FND-03 | **Baseline verified**: Windows release/smoke trong [M1/M2](../verification/2026-11-07-m1-m2-import.md); Linux nằm ở OPS-02 | Lặp lại được build, phân phối và bản nâng cấp |
| **FND-05** | Admin shell tiếng Việt, navigation, API client, form primitives và phân quyền hiển thị | M1/M2.5 baseline → M3 Tailwind WordPress-like UI; FND-02 | **Partial**: Admin/Builder một block đã chạy; chưa có sidebar/screens M3 đầy đủ | UI primitives dùng chung, form state, bảo toàn selectors/401 |

## Content & Media

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **CNT-01** | CPT: khai báo/quản lý Admin, owner core/user/theme/plugin, deactivate vẫn quản trị được; **G0 M3 built-in `post`/`page`** | M3 built-in; **M5-P10 theme declarations**, M6-P08 plugin bridge; FND-01/03 | **Partial**: CPT schema và Admin tạo/list [M3.2](../verification/2026-11-08-m3-2-content-types-backend.md), [M3.4](../verification/2026-11-08-m3-4-admin-content-ui.md); chưa có seed post/page/theme provider | Registry ↔ SQLite, schema ownership, seed/idempotency, restart; giữ `home_page` M1 |
| **CNT-02** | Custom fields: short/long/rich text, number, toggle, date, select, image, gallery; schema/values và provider declarations | M3 core fields/post/page; **M5-P10 theme fields**, M6-P08 plugin fields; CNT-01/05 | **Partial**: 9 field types [M3.1](../verification/2026-11-08-m3-1-content-domain.md), entry validation [M3.3](../verification/2026-11-08-m3-3-content-entry-crud.md), picker [M3.5](../verification/2026-11-09-m3-5-media-library.md) | Validation độc lập provider, rich text/URL safety, schema evolution/data retention; MediaRef G2 |
| **CNT-03** | Taxonomy: category cha–con và tag; Admin CRUD hoặc định nghĩa từ theme/plugin, gắn entry, cấp quyền | **M4** category/tag/entry links, **M5-P10 theme taxonomy**, M6-P04 plugin taxonomy; CNT-01/04 | **Planned**: taxonomy V1 chưa triển khai | Ownership/hierarchy/uniqueness/slug, xóa/reparent; giữ terms/data khi provider tắt |
| **CNT-04** | Entry CRUD cho page/post/CPT; public route dispatch, permalink matching và RequestContext lookup độc lập theme (phối hợp CNT-03) | M3 → M5; CNT-01, CNT-02, FND-02; SITE-02/THM-02 cho delivery đầy đủ | **Partial**: CRUD [M3.3](../verification/2026-11-08-m3-3-content-entry-crud.md), SSR [M3.6](../verification/2026-11-09-m3-6-public-content-rendering.md); thiếu taxonomy/full URL policy | Transaction, slug/route precedence, public visibility, M1 `/` compatibility; URL contract qua G1/G2 |
| **CNT-05** | Media library: upload ảnh/video, metadata, chọn ảnh/gallery, lưu trong uploads và lifecycle an toàn | M3 trang media ảnh → **M7 video → M8 delete/reconcile**; FND-02/03 | **Partial**: upload/thumbnail/picker [M3.5](../verification/2026-11-09-m3-5-media-library.md), ACL [M3.6](../verification/2026-11-09-m3-6-public-content-rendering.md); thiếu video/delete/backup | Storage/file refs, validation, GC/reconcile, restore |

## Builder & Rendering

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **BLD-01** | Block contract, preview/public renderer và missing-provider diagnostics (phối hợp BLD-05 cho binding) | M1 → M4; THM-01 | **Baseline verified**: **một block** theo [M1/M2](../verification/2026-11-07-m1-m2-import.md) và [M2.5](../verification/2026-11-07-m2-5-core-kernel.md); các core elements V1 chưa có | Compatibility page cũ, escaping/CSP, diagnostics không lộ dữ liệu công khai |
| **BLD-02** | Cây Section → Row → Column → Element, built-in element descriptors Heading/Text/Image/Button, AST validation | M4; FND-01, BLD-01 | **Planned** | Phân biệt core elements và theme blocks; AST/normalization, persistence, drag/drop |
| **BLD-03** | Tree/canvas/inspector, chỉnh thuộc tính core elements; chọn phần tử từ cây hoặc preview | M4; BLD-02, FND-05 | **Planned** | State đồng bộ, selection, form input, keyboard accessibility |
| **BLD-04** | Thuộc tính responsive desktop/tablet/mobile và preview | M4; BLD-02, BLD-03 | **Planned** | Breakpoint model, responsive CSS, đảm bảo lưu không phá layout |
| **BLD-05** | Field binding, query/list block: CPT/category/sort/page size/pagination, template context | M4–M5; CNT-02, CNT-03, CNT-04, BLD-02 | **Planned** | Binding schema, query whitelist, escaping, cache và invalidation |

## Theme

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **THM-01** | Cài ZIP theme, đăng ký block/template và định nghĩa CPT/fields/taxonomy qua canonical domain APIs, quản lý package | M1 block → **M3 install/list/activate** → **M5-P10 theme CPT/fields/taxonomy**; FND-01, CNT-01/02/03 | **Partial baseline**: ZIP theme một block trong [M1/M2](../verification/2026-11-07-m1-m2-import.md), chưa có theme declarations V1 | Package lifecycle, owner/fallback, site-owned schema/entries còn edit được sau switch/update/restart |
| **THM-02** | Chọn template page/header/footer/archive/single từ RequestContext và ghép layout; Theme Options sử dụng settings engine chung | M5; CNT-04, BLD-02/05, SITE-01 tối thiểu | **Planned** | Không sở hữu HTTP URL matcher; template fallback, MiniJinja và site/theme settings contract |
| **THM-03** | Chọn nhập trang/menu/ảnh/cấu hình mẫu không ghi đè dữ liệu; đổi theme giữ schema/entries/layout user chỉnh | M5-P05/P06; **M5-P09 SITE-01 menu persistence** và P10 theme declarations | **Planned** | Site-owned menu/schema/entry/layout, provenance, merge, missing-provider fallback, rollback; M7 UI dùng menu model M5 |

## Plugin Extension

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **PLG-01** | Cài ZIP plugin, Wasmi worker, quyền, fuel/timeout, log, disable/recover | M2; FND-02, FND-03 | **Baseline verified**: isolated worker và ZIP/smoke theo [M1/M2](../verification/2026-11-07-m1-m2-import.md) | Không làm hỏng recovery path khi mở extension APIs |
| **PLG-02** | Event/action/API server, quyền gọi dịch vụ ngoài theo khai báo | M2.5 → M6; FND-01, PLG-01 | **Partial**: PageAfterSave và PluginManualRun đã verified; content hooks và external calls chưa có bằng chứng V1 | Content event, transaction, timeout, permission và network scope |
| **PLG-03** | Declarative Registration Bridge: khai báo CPT/fields/taxonomy/blocks/templates/settings từ plugin qua registry/API core, không giữ bản sao dữ liệu website | **M6** CPT/fields và extensions; CNT-01/02/03, BLD-01/02, THM-02, SITE-01, PLG-02 | **Planned**: plugin CPT/field manifest bridge chưa có E2E evidence | Manifest/ABI/owner/lifecycle/rollback, dữ liệu site-owned, G2 trước migration |
| **PLG-04** | UI khai báo của plugin: form, bảng, list, tabs, buttons trong Admin | M6; FND-05, PLG-02 | **Planned** | Declarative UI schema, guard, data/action handlers và khả năng phục hồi |

## Site features & Operations

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **SITE-01** | Declarative Settings/Options engine chung (owner core/user/theme/plugin), site settings, menu/navigation, ngôn ngữ nội dung | **M3 General Settings** → **M5-P09 menu storage + Theme Options** → M7 CRUD/binding; FND-05/03 | **Planned** | M5 menu stable IDs/refs/order/provenance persist trước import, M7 thêm Admin UI trên cùng model; phân biệt site/theme/plugin/SMTP |
| **SITE-02** | SEO: custom URL/redirect, canonical, robots, title, description, OG image, sitemap; chính sách URL phối hợp CNT-04 | Nghiên cứu URL từ M3, chốt trước route phụ thuộc và nghiệm thu public delivery M5; SEO đầy đủ M7; CNT-04, BLD-05 | **Planned** | URL collisions, route namespace, escaping, indexability, sitemap invalidation |
| **SITE-03** | Tạo contact form với trường cơ bản, đặt qua Builder, lưu submissions tại Admin, thông báo qua SMTP được cấu hình | M7; BLD-02/03, SITE-01, CNT-05, FND-02 | **Planned** | Validation, abuse limits, email failure semantics, credentials; nghiệm thu form do người dùng tạo và SMTP lỗi |
| **OPS-01** | Backup/restore thủ công: DB, uploads, site config, theme, plugin | Contract dữ liệu/file từ M3, package M5–M6; engine và nghiệm thu M8; FND-03, CNT-05, THM-01, PLG-01 | **Planned** | Snapshot consistency, write isolation, atomic restore, migration, recovery; restore site root riêng rồi chạy/chỉnh sửa website |
| **OPS-02** | Linux VPS distribution, release hardening, kiểm thử end-to-end V1 | Build/smoke Linux sớm ở M3 với FND-04/CNT-05; release M8 phụ thuộc tất cả V1 capabilities | **Planned**: Linux chưa có bằng chứng | Single binary, static assets, permissions, deploy/upgrade; ghi NOT_RUN khi chưa có môi trường, không suy từ Windows |

## Quan hệ phụ thuộc xuyên module cần giữ

1. **FND-01** phải xác lập ownership và dispatch thực tế trước khi **CNT-01** và **PLG-03** cam kết vòng đời provider.
2. **CNT-01/02/04** phải có cấu trúc dữ liệu và lifecycle đủ ổn định trước khi **BLD-05** bind dữ liệu hoặc **THM-02** resolve template.
3. **CNT-05** cung cấp media references cho **CNT-02**, **SITE-02** và backup **OPS-01**.
4. **BLD-02/03/04** phát triển theo lát cắt; **THM-02** dùng contract builder đã duyệt.
5. **PLG-03** đăng ký schema/block/template thông qua core registry và module chủ sở hữu tương ứng; không tạo nhánh persistence riêng cho dữ liệu người dùng.
6. **THM-03**, **PLG-03** và **OPS-01** đều phải kiểm chứng giữ dữ liệu/bố cục của website khi package bị đổi, cập nhật hoặc vô hiệu hóa.
7. **Public delivery** chia bốn nhiệm vụ: CNT-04 dispatch/RequestContext (CNT-03 hỗ trợ taxonomy); SITE-02 URL policy/canonical; THM-02 chọn template/ghép header/footer; BLD-01/05 render/bind và diagnostics. Interface/precedence cụ thể cần G1/G2, không mặc định hoàn thành ở M3.
8. **Settings/Options/menu** do SITE-01 sở hữu schema/storage/form. M3 General Settings là nền trước Theme Options M5; **M5-P09 xây menu model/persistence site-owned để M5-P05 nhập mẫu**, M7-P02 thêm menu Admin CRUD/binding trên chính model đó. THM-02 và PLG-03 là consumers, không tạo data store menu riêng.
9. **Nghiệm thu M5** phải nối CNT-04/SITE-02/THM-02/BLD-01/05 từ form nội dung tới URL, archive/phân trang; phân biệt Lưu draft/published/Builder theo PRD. **M5-P10 theme ZIP** khai báo CPT/fields/taxonomy qua domain registries, rồi M5-P06 đổi theme vẫn mở/sửa được schema/entry; M5-P09/P05 menu import tồn tại sau theme switch. Không chỉ kiểm row DB còn tồn tại. [Roadmap](ROADMAP_V1.md#nghiệm-thu-xuyên-module-cần-đưa-vào-các-brief-khi-mở).

## Ràng buộc chung của V1

- Chủ website quản trị bằng trình duyệt, developer triển khai VPS Linux; phát triển và test trước trên Windows; một executable và SQLite cho mỗi website.
- Preview dùng thay đổi chưa lưu. Với **Builder/homepage** và content đã `published`, **Lưu** cập nhật công khai như baseline; với `post`/`page` `draft`, **Lưu chỉ ghi DB, không công khai** cho đến thao tác **Xuất bản**; preview draft yêu cầu quyền hợp lệ. Không mở approval/scheduling/revision workflow V1. [Quy tắc gốc](../product/PRODUCT_REQUIREMENTS.md#nội-dung-lưu-và-phân-quyền).
- Dữ liệu CPT/custom fields/content/bố cục đã chỉnh thuộc về website, không bị xóa khi đổi theme hoặc tắt plugin. Việc xóa dữ liệu là thao tác riêng.
- Theme và plugin ZIP có thể thêm extension mà không build lại CMS; quyền plugin được hiện ra và tự áp dụng khi hợp lệ theo khai báo, không có bước duyệt quyền; chặn thao tác ngoài quyền và giữ đường phục hồi Admin.
- Các hạng mục [để sau V1](../product/PRODUCT_REQUIREMENTS.md#để-sau-bản-đầu) chỉ được đưa vào milestone khi có quyết định thay đổi phạm vi sản phẩm.

## Nguồn đánh giá hiện trạng

- [M1/M2 import verification](../verification/2026-11-07-m1-m2-import.md): xác nhận Auth, một block, theme ZIP, plugin Wasmi và build Windows.
- [M2.5 verification](../verification/2026-11-07-m2-5-core-kernel.md): xác nhận cả 8 mục M2.5 theo commit `da79304` trên Windows; Content hooks và CPT persistence thuộc M3.
- [CORE_FOUNDATION](../contracts/CORE_FOUNDATION.md), [PLUGIN_ABI](../contracts/PLUGIN_ABI.md) và [CMS_ARCHITECTURE](../architecture/CMS_ARCHITECTURE.md): ranh giới phải đối chiếu khi quyết định thiết kế chi tiết.
- [Thảo luận planning](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md): xác nhận 27 ID và ranh giới module. **G0 `post`/`page` built-in M3 đã được chủ dự án chốt 10/11/2026**, nhưng schema/migration vẫn cần G2; plugin CPT manifest và media lifecycle là vấn đề G2 về sau.
