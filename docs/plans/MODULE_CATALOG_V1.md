# Danh mục Domain → Module → Capability của OpenDash V1

**Trạng thái:** bộ ID và **ranh giới sở hữu module** được thống nhất để lập kế hoạch theo [kết luận thảo luận](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md); các tính năng và thứ tự triển khai M3–M8 vẫn là **đề xuất** trên nền [PRD đã chốt](../product/PRODUCT_REQUIREMENTS.md), không phải thiết kế G2 đã duyệt hay code đã triển khai. **Mốc đối chiếu:** 07/11/2026; `da79304` (M2.5 verified trên Windows).

Đây là bản đồ chức năng ổn định của V1. Để tìm việc cần làm theo thứ tự thời gian, dùng [roadmap V1](ROADMAP_V1.md); để chia task, dùng [quy trình planning](PLANNING_WORKFLOW.md). Một module có thể xuất hiện ở nhiều milestone nhưng chỉ có một ID.

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
| **FND-05** | Admin shell tiếng Việt, navigation, API client, form primitives và phân quyền hiển thị | M1, M2.5; FND-02 | **Partial**: Admin/Builder một block đã chạy; các màn hình V1 chưa đủ | UI primitives dùng chung, form state, bảo toàn selectors/401 |

## Content & Media

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **CNT-01** | CPT: khai báo, quản lý qua Admin, owner core/user/theme/plugin, deactivate/reactivate vẫn quản trị được; nghiên cứu 2 built-in types `post`/`page` | M3; FND-01, FND-03 | **Planned**; [living brief G1/G2 chưa chốt](modules/CNT-01-content-types.md) | Registry ↔ SQLite, schema ownership, update, conflict, restart; tránh phá `home_page` M1 |
| **CNT-02** | Custom fields: short text, long text, rich text, number, toggle, date, select, image, gallery; schema và giá trị | M3; CNT-01, CNT-05 (media field) | **Planned** | Validation, rich text/URL safety, đổi schema/dữ liệu cũ; MediaRef G2 trước picker |
| **CNT-03** | Taxonomy: danh mục cha–con và tag; gắn vào nội dung, cấp quyền | M3; CNT-01, CNT-04 | **Planned** | Hierarchy, uniqueness, slug, xoá/đổi liên kết an toàn |
| **CNT-04** | Entry CRUD cho page/post/CPT; public route dispatch, permalink matching và RequestContext lookup độc lập theme (phối hợp CNT-03) | M3 → M5; CNT-01, CNT-02, FND-02; SITE-02/THM-02 cho delivery đầy đủ | **Planned** | Transaction, slug/route precedence, public visibility, M1 `/` compatibility; URL contract qua G1/G2 |
| **CNT-05** | Media library: tải ảnh/video, metadata, chọn ảnh/gallery, lưu trong uploads | M3; FND-03, FND-02 | **Planned**; [discovery brief](modules/CNT-05-media.md) | Storage/file references, xác thực loại file, tên file, dọn dẹp và backup |

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
| **THM-01** | Cài ZIP theme, đăng ký block/template và quản lý package | M1 → M5; FND-01 | **Baseline verified**: ZIP theme và **một block** trong [M1/M2](../verification/2026-11-07-m1-m2-import.md), chưa đủ V1 | Asset/package layout, owner và fallback khi thiếu provider |
| **THM-02** | Chọn template page/header/footer/archive/single từ RequestContext và ghép layout; Theme Options sử dụng settings engine chung | M5; CNT-04, BLD-02/05, SITE-01 tối thiểu | **Planned** | Không sở hữu HTTP URL matcher; template fallback, MiniJinja và site/theme settings contract |
| **THM-03** | Import dữ liệu mẫu có lựa chọn; đổi/cập nhật theme bảo toàn dữ liệu/bố cục người dùng | M5; THM-02, CNT-04 | **Planned** | Provenance nội dung, merge policy, lỗi package và rollback |

## Plugin Extension

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **PLG-01** | Cài ZIP plugin, Wasmi worker, quyền, fuel/timeout, log, disable/recover | M2; FND-02, FND-03 | **Baseline verified**: isolated worker và ZIP/smoke theo [M1/M2](../verification/2026-11-07-m1-m2-import.md) | Không làm hỏng recovery path khi mở extension APIs |
| **PLG-02** | Event/action/API server, quyền gọi dịch vụ ngoài theo khai báo | M2.5 → M6; FND-01, PLG-01 | **Partial**: PageAfterSave và PluginManualRun đã verified; content hooks và external calls chưa có bằng chứng V1 | Content event, transaction, timeout, permission và network scope |
| **PLG-03** | Declarative Registration Bridge: đọc khai báo CPT, fields, blocks, templates, settings từ plugin rồi gọi domain registry/API chủ sở hữu; không giữ bản sao dữ liệu website | M3/M4/M6; CNT-01/02, BLD-01/02, THM-02, SITE-01, PLG-02 | **Planned** | Manifest schema/compatibility, owner/lifecycle, rollback registration; các domain lưu schema/data |
| **PLG-04** | UI khai báo của plugin: form, bảng, list, tabs, buttons trong Admin | M6; FND-05, PLG-02 | **Planned** | Declarative UI schema, guard, data/action handlers và khả năng phục hồi |

## Site features & Operations

| ID | Module và capability V1 | Mốc đầu / phụ thuộc chính | Bằng chứng hoặc trạng thái | Hướng nghiên cứu khi mở |
| --- | --- | --- | --- | --- |
| **SITE-01** | Declarative Settings/Options engine chung (owner core/user/theme/plugin), site settings, menu/navigation, ngôn ngữ nội dung | M5 (engine tối thiểu) → M7; FND-05, FND-03 | **Planned** | Schema/storage + auto-form, phân biệt site/theme/plugin/SMTP options, menu |
| **SITE-02** | SEO: custom URL/redirect, canonical, robots, title, description, OG image, sitemap; chính sách URL phối hợp CNT-04 | M7 (URL contract cần nghiên cứu từ M3); CNT-04, BLD-05 | **Planned** | URL collisions, route namespace, escaping, indexability, sitemap invalidation |
| **SITE-03** | Contact form dùng qua Builder, lưu submissions tại Admin, SMTP notifications | M7; BLD-02, CNT-05, FND-02 | **Planned** | Validation, abuse limits, email failure semantics, credentials |
| **OPS-01** | Backup/restore thủ công: DB, uploads, site config, theme, plugin | M8; FND-03, CNT-05, THM-01, PLG-01 | **Planned** | Snapshot consistency, atomic restore, migration, recovery |
| **OPS-02** | Linux VPS distribution, release hardening, kiểm thử end-to-end V1 | M8; tất cả V1 capabilities | **Planned**: Linux chưa có bằng chứng | Single binary, static assets, permissions, deploy/upgrade |

## Quan hệ phụ thuộc xuyên module cần giữ

1. **FND-01** phải xác lập ownership và dispatch thực tế trước khi **CNT-01** và **PLG-03** cam kết vòng đời provider.
2. **CNT-01/02/04** phải có cấu trúc dữ liệu và lifecycle đủ ổn định trước khi **BLD-05** bind dữ liệu hoặc **THM-02** resolve template.
3. **CNT-05** cung cấp media references cho **CNT-02**, **SITE-02** và backup **OPS-01**.
4. **BLD-02/03/04** phát triển theo lát cắt; **THM-02** dùng contract builder đã duyệt.
5. **PLG-03** đăng ký schema/block/template thông qua core registry và module chủ sở hữu tương ứng; không tạo nhánh persistence riêng cho dữ liệu người dùng.
6. **THM-03**, **PLG-03** và **OPS-01** đều phải kiểm chứng giữ dữ liệu/bố cục của website khi package bị đổi, cập nhật hoặc vô hiệu hóa.
7. **Public delivery** chia bốn nhiệm vụ: CNT-04 dispatch/RequestContext (CNT-03 hỗ trợ taxonomy); SITE-02 URL policy/canonical; THM-02 chọn template/ghép header/footer; BLD-01/05 render/bind và diagnostics. Interface/precedence cụ thể cần G1/G2, không mặc định hoàn thành ở M3.
8. **Settings/Options** do SITE-01 sở hữu schema/storage/form; THM-02 và PLG-03 là consumers. M5 cần engine tối thiểu trước Theme Options, không chờ toàn bộ M7.

## Ràng buộc chung của V1

- Chủ website quản trị bằng trình duyệt, developer triển khai VPS Linux; phát triển và test trước trên Windows; một executable và SQLite cho mỗi website.
- Preview sử dụng thay đổi chưa lưu, còn thao tác **Lưu** cập nhật website công khai ngay. Không lập kế hoạch workflow nháp/xuất bản riêng ở V1.
- Dữ liệu CPT/custom fields/content/bố cục đã chỉnh thuộc về website, không bị xóa khi đổi theme hoặc tắt plugin. Việc xóa dữ liệu là thao tác riêng.
- Theme và plugin ZIP có thể thêm extension mà không build lại CMS; quyền plugin được hiện ra và thực thi theo manifest, có đường phục hồi Admin.
- Các hạng mục [để sau V1](../product/PRODUCT_REQUIREMENTS.md#để-sau-bản-đầu) chỉ được đưa vào milestone khi có quyết định thay đổi phạm vi sản phẩm.

## Nguồn đánh giá hiện trạng

- [M1/M2 import verification](../verification/2026-11-07-m1-m2-import.md): xác nhận Auth, một block, theme ZIP, plugin Wasmi và build Windows.
- [M2.5 verification](../verification/2026-11-07-m2-5-core-kernel.md): xác nhận cả 8 mục M2.5 theo commit `da79304` trên Windows; Content hooks và CPT persistence thuộc M3.
- [CORE_FOUNDATION](../contracts/CORE_FOUNDATION.md), [PLUGIN_ABI](../contracts/PLUGIN_ABI.md) và [CMS_ARCHITECTURE](../architecture/CMS_ARCHITECTURE.md): ranh giới phải đối chiếu khi quyết định thiết kế chi tiết.
- [Thảo luận planning đã thống nhất](../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md): xác nhận 27 ID và ranh giới các module; các lựa chọn M3 như `post/page`, plugin CPT manifest và `media_id` chỉ là giả thuyết G1, không phải schema G2.
