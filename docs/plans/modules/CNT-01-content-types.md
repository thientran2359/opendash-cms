# CNT-01 — Content Types (CPT) · Living Module Brief

**Scope:** PRD V1 `AGREED` / ranh giới module đã thống nhất · **Milestone dự kiến:** M3 (`M3-S01`, `M3-S05`) · **PRD:** [Content & ownership](../../product/PRODUCT_REQUIREMENTS.md) · **Track:** `Full-Track` vì cần core contract, migration SQLite và API.

**Trạng thái hiện tại:** Research `REVIEWED` (source/CodeGraph tại `main@2bad893` và phản biện Codex ↔ Antigravity đến Turn 5, 07/11/2026) · Design `NOT_STARTED` · Implementation `NOT_STARTED` · Verification `NOT_RUN`. Các capability bên dưới là mục tiêu G0, **chưa có G2/Task READY**.

Đây là **Living Module Brief** chứa G0–G3 tại một nơi. Kết quả G1 là khảo sát/đề xuất cần phản biện, **không phải schema hay interface đã duyệt**. Chỉ tách `docs/research/` cho đánh giá lớn/benchmark, không tạo file task rời. Xem [planning consensus](../../discussions/threads/2026-11-07-roadmap-module-planning-workflow.md) và [Pre-M3 readiness](../../discussions/threads/2026-11-07-pre-m3-codegraph-readiness.md).

## G0 — Năng lực cần có

| ID | Capability | Primary acceptance slice dự kiến | Nghiệm thu dự kiến |
| --- | --- | --- | --- |
| `CNT-01.1` | Quản trị viên tạo và quản lý định nghĩa CPT qua Admin | `M3-S01` | Định nghĩa hợp lệ được lưu, khởi động lại vẫn xuất hiện; Editor không sửa schema khi thiếu quyền |
| `CNT-01.2` | Core, User, Theme, Plugin đăng ký định nghĩa theo ownership | `M3-S01` | Xử lý owner conflict, cập nhật cùng owner và phục hồi trạng thái qua restart |
| `CNT-01.3` | Tắt provider vẫn giữ schema và nội dung người dùng | `M3-S05` (plugin E2E) | Không xóa schema/entries khi plugin tắt/restart; theme switch E2E thuộc THM-03 ở M5 |
| `CNT-01.4` | Tích hợp với custom fields/entries | `M3-S02` | CNT-02/CNT-04 dùng chung API CPT, không xây đường lưu bản sao |

**Định hướng cần khảo sát, chưa chốt:** built-in `post`/`page` dưới `ResourceOwner::Core` và cách giữ tương thích bảng `home_page`/route `/` của M1. Seed ở migration hay bootstrap, khả năng sửa/xóa core types đều cần G2.

## G0 — Ranh giới và dependency

- **Cần trước:** FND-01 registry/owner và hook thực đã kiểm chứng trong M2.5 (`da79304`), FND-03 storage/migration, FND-02 quyền; research riêng persistence của CPT trong M3.
- **Cung cấp cho:** CNT-02 field schema, CNT-03 taxonomy, CNT-04 entries và public request context, BLD-05 binding, PLG-03 plugin registration bridge. Domain CNT-01 sở hữu validation/schema/SQLite, không chuyển persistence sang PLG-03.
- **Ngoài phạm vi CNT-01:** hiện thực toàn bộ custom field types, biểu mẫu soạn entry, UI builder nhiều tầng hoặc quản lý media. Không đổi migrations 0001–0003 hay plugin ABI v1 chỉ để mở CPT.

## G1 — Research đã review (ứng viên cho G2)

1. **Hiện trạng code:** tra `cms/crates/opendash-core/src/{content,registry}.rs`, `cms/crates/opendash-server/src/{storage,hooks,plugin}` và các caller bằng CodeGraph/source. API công khai và các nhánh dynamic registration hiện có ra sao?
2. **Ownership:** registry có phân biệt definition thuộc provider với dữ liệu nội dung thuộc website chưa? Có thể phát hiện conflict, deactivate/reactivate và re-register an toàn không?
3. **Persistence:** biểu diễn CPT schema trong SQLite hay registry runtime, kết hợp thế nào để load sau restart và nâng cấp từ DB cũ? Migration mới cần điều kiện gì?
4. **Versioning:** theme/plugin cập nhật định nghĩa CPT/field thì áp dụng thay đổi schema ra sao, nhất là đổi tên, xóa, đổi kiểu? Ai được thực hiện tác vụ xóa dữ liệu?
5. **API và quyền:** Admin tạo/sửa CPT theo API nào, Editor được xem/sử dụng phần nào, validation key có trùng ID của provider không?
6. **Tích hợp hooks:** đăng ký và lưu schema trước/sau commit thế nào để không có event báo thành công khi transaction thất bại?
7. **An toàn và tương thích:** các URL/slug, reserved key, rollback và thứ tự load provider được kiểm thử thế nào?
8. **M3 vs M5/M6:** cách nào chứng minh lifecycle `ResourceOwner::Theme` trên catalog mà không giả vờ đã có theme switch E2E? Plugin manifest khai báo `content_types` tùy chọn và bridge PLG-03 có tương thích package cũ, install/disable/restart không?
9. **Route namespace:** phân biệt kiểm tra cú pháp `SnakeKey` với URL prefix bảo lưu (`/api`, `/admin`, `/assets`); quyết định ở tầng router, tránh cấm nhầm CPT hợp lệ.

**Đầu ra G1:** bảng API/callers theo source + CodeGraph, so sánh ít nhất hai phương án owner/schema/init khi có tradeoff, test tối thiểu cho điểm chưa chắc chắn, lựa chọn đề xuất và rủi ro. Ghi tóm tắt tại đây, chỉ tách `docs/research/` nếu cần.

### G1.1 — Kiểm kê source và caller/callee (đã khảo sát và phản biện)

**Cách làm:** từ root tại `main@2bad893`, dùng `node cms/scripts/codegraph-query.mjs symbol RegistryCatalog`, `symbol save_page` và `callgraph` với `src/page.rs:17`, `storage/page.rs:18`, `plugin/manager.rs:59/87`, `storage/plugin.rs:7/119`, `hooks.rs:122`; sau đó đọc source để xác thực. Graph tìm đúng luồng `routes/builder.rs::save → page.rs::save_page → storage/page.rs::save_home_page → hooks.rs::dispatch` nhưng có **false positive** từ các method tên chung (`new`, `register_handler`, `len`, `update`). Chỉ các quan hệ đối chiếu trực tiếp bên dưới mới là `OBSERVED IN SOURCE`. Đây là **static audit**, chưa thực hiện thử nghiệm M3.

| Phạm vi / module owner | Có thể tái dùng (`OBSERVED IN SOURCE`) | Khoảng trống hoặc rủi ro G1 |
| --- | --- | --- |
| **FND-01 / CNT-01** | [`opendash-core/src/registry.rs:14-50,72-158`](../../../cms/crates/opendash-core/src/registry.rs) có `ResourceOwner::{Core,User,Theme,Plugin}`, `OwnedDescriptor`, `ProviderStatus`, `RegistryCatalog::{register,update,deactivate_owner,activate_owner}` và owner-conflict; [`content.rs:8-10`](../../../cms/crates/opendash-core/src/content.rs) gọi chung `valid_snake_key`. | `register` luôn tạo `Active` và catalog là in-memory; chưa có factory/restore từ persisted `ProviderInactive` cho CPT. Kiểm tra caller ngoài unit test trước khi mở API mới. |
| **FND-03 / CNT-01** | [`storage/page.rs:18-41`](../../../cms/crates/opendash-server/src/storage/page.rs) dựng block catalog từ SQLite `theme_blocks`, suy Theme owner từ prefix; [`storage/mod.rs:13-34`](../../../cms/crates/opendash-server/src/storage/mod.rs) chạy SQLx migration cùng WAL/FK. | Chỉ có [migrations 0001–0003](../../../cms/migrations/README.md): `theme_blocks`, `home_page`, auth, plugins; **không có bảng CPT/field/entry/owner status**. Snapshot block không persist trạng thái inactive; không copy thẳng mô hình này vào CPT. |
| **PLG-01 / PLG-03** | [`PluginManager::install_plugin`](../../../cms/crates/opendash-server/src/plugin/manager.rs) → [`PluginPackage::from_zip`](../../../cms/crates/opendash-server/src/plugin/package.rs) → [`storage::plugin::upsert_plugin`](../../../cms/crates/opendash-server/src/storage/plugin.rs); disable/enable → `set_plugin_enabled`. | `upsert_plugin` tại `storage/plugin.rs:18-29` đặt `enabled=1` cả khi **reinstall**; enable/disable hiện chỉ ghi `plugins.enabled`. `PluginManifest` [`deny_unknown_fields`](../../../cms/crates/opendash-core/src/plugin.rs) chưa có `content_types`; ZIP parser chỉ chấp nhận `manifest.json`, `plugin.wasm`, `README.md`. Plugin bridge/CPT lifecycle **chưa tồn tại**. |
| **FND-02 / CNT-04** | [`auth.rs:213-263`](../../../cms/crates/opendash-server/src/auth.rs) có `require_admin_with_csrf` (quản lý schema) và `require_session_with_csrf` (Editor thao tác content); [`routes/builder.rs:101-118`](../../../cms/crates/opendash-server/src/routes/builder.rs) là mẫu guard → use case. | Cần G2 cụ thể quyền create/update/delete CPT vs nhập entry; Admin API CPT chưa có, không mặc định Editor sửa schema. |
| **Hooks / CNT-04** | [`page.rs:17-40`](../../../cms/crates/opendash-server/src/page.rs) render → lưu `home_page` → `PageAfterSave`. [`core/hooks.rs:11-20`](../../../cms/crates/opendash-core/src/hooks.rs) khai báo `ContentBeforeSave/ContentAfterSave`. | [`server/hooks.rs:78-90,122-136`](../../../cms/crates/opendash-server/src/hooks.rs) **chưa cho dispatch content hooks**; pre-save veto/transaction semantics chưa được thiết kế. Không dùng `PageAfterSave` như giả định thay thế `ContentBeforeSave`. |
| **CNT-04 / SITE-02 / THM-02** | [`app.rs:58-90`](../../../cms/crates/opendash-server/src/app.rs) chỉ route `/` public; [`routes/public.rs:10-30`](../../../cms/crates/opendash-server/src/routes/public.rs) đọc `home_page` và render block. | Chưa có CPT permalink resolver/RequestContext, namespace policy hay public template route. Trách nhiệm phân theo [Module Catalog](../MODULE_CATALOG_V1.md), không đưa route/template/render toàn bộ vào CNT-01. |

### G1.2 — Các lựa chọn kiến trúc cần so sánh (proposal, **không chốt G2**)

| Vấn đề | Phương án A | Phương án B | Đánh giá sơ bộ / việc cần xác minh |
| --- | --- | --- | --- |
| **Nguồn sự thật CPT definition** | Bảng SQLite riêng cho definition + `owner_kind/owner_id`, `status`; dựng `RegistryCatalog<T>` snapshot khi đọc. | Bảng SQLite giữ definition + owner, trạng thái **hiệu lực** suy từ nguồn provider (ví dụ `plugins.enabled`, theme hiện hành) tại load. | Cả hai giữ schema/data dù provider tắt. A thuận tiện audit nhưng dễ lệch `status` nếu thiếu transaction; B ít trạng thái lặp nhưng phải định nghĩa *missing/uninstalled package*, version và Theme owner, tránh lẫn `not installed` với `disabled`. **Không chọn A/B trước thử nghiệm**. |
| **Cập nhật provider/manifest** | Plugin khai báo `content_types` tùy chọn trực tiếp trong `manifest.json`, PLG-03 bridge đăng ký vào CNT-01. | Plugin lưu hoặc phát declaration qua một đường registration khác (ví dụ file schema/package metadata) được host xác thực; không cho guest tự ghi bảng domain. | A ít thay đổi ZIP layout và có thể giữ ABI Wasm v1, nhưng phải thêm field vào validator nghiêm ngặt, giới hạn payload, test version cũ/mới và cài lại disabled plugin. B tốn hợp đồng/package/validation mới; không tự chọn. |
| **`post`/`page` do Core sở hữu** | Seed định nghĩa built-in trong migration SQLx mới (idempotent, không sửa migration cũ). | Seed/ensure lúc startup sau migration, theo contract rõ ràng và guard không ghi đè. | A cho reproducibility theo version nhưng cần cân nhắc upgrade; B linh hoạt hơn nhưng có rủi ro concurrent startup/overwrite. `home_page` M1 không được làm mất dữ liệu hay đổi route `/` ngoài G2. |
| **CPT definition versioning** | Chỉ cho cập nhật metadata an toàn, thay đổi shape/rename cần migration/version rõ. | Giữ bản định nghĩa versioned và quá trình chuyển đổi riêng, reject xung đột. | Cả hai cần chính sách data retention, lỗi incompatible field và cách chuyển owner; **chưa có yêu cầu xây full migration designer V1**. |

**Phản biện Turn 2 — phương án C (Hybrid, đang kiểm nghiệm G1):** Antigravity đề xuất lưu `owner_kind/owner_id`, descriptor và `provider_status`, cập nhật trong transaction với plugin, rồi bảo vệ bằng kiểm tra `plugins.enabled` khi đọc. C là ứng viên mạnh vì `Theme(id)` chưa có nguồn trạng thái theme bền vững ở M3 và plugin có thể **rút một CPT khỏi manifest** trong khi vẫn được enable. Tuy nhiên phải tách rõ **(1) declaration còn được provider phiên bản hiện tại cung cấp hay đã withdrawn**, **(2) provider hiện được bật hay tắt**, **(3) trạng thái hiệu lực**. Nếu mỗi lần enable plugin đặt toàn bộ CPT của owner sang `active`, một CPT đã withdrawn có thể bị bật nhầm. Cần so sánh C với biến thể **D: persisted declaration-state + derived provider availability + effective-status khi đọc**, và test sự kiện `disable/reinstall/enable/withdraw/restart`. Chưa duyệt tên cột, bảng hay cách tính.

**G1 REVIEWED — ưu tiên biến thể D (hai trục độc lập) để thiết kế G2:** lưu định nghĩa, owner bất biến và trạng thái **còn được khai báo / đã withdrawn**; quản lý riêng khả dụng của owner và chỉ tổng hợp `effective_status` khi đọc. Đề xuất cụ thể tại [Turn 4](../../discussions/threads/2026-11-07-m3-g1-content-types-media-research.md) dùng `declaration_state` (`declared | withdrawn`) và `owner_enabled` (`0 | 1`) cùng guard `LEFT JOIN plugins` để phân biệt `active / disabled / withdrawn / package_missing`; đây là **SQL candidate**, chưa là schema G2. `Theme(id)` ở M3 chưa có bảng theme lifecycle (M5), nên G2 phải xác định nơi lưu trạng thái theme; việc lưu `owner_enabled` lặp trên từng CPT cùng owner cũng có rủi ro drift và cần quy tắc transaction/authority rõ. `Core/User` mặc định được khai báo và không bị tắt bởi lifecycle plugin. Khôi phục CPT đã rút chỉ cho **cùng owner** sau kiểm tra schema tương thích; provider khác vẫn nhận `OwnerConflict` kể cả khi CPT cũ đang withdrawn. `RegistryCatalog` thuần Rust chỉ nhận trạng thái hydrate; mọi SQL/availability join nằm tại `opendash_server::storage`.

**Seed `post/page` — khuynh hướng nghiên cứu A (migration):** [`server/page.rs:55-58`](../../../cms/crates/opendash-server/src/page.rs) và [`server/hooks.rs:229-232`](../../../cms/crates/opendash-server/src/hooks.rs) chạy `sqlx::migrate!` trực tiếp trên SQLite in-memory, không qua startup; [`0002_auth.sql`](../../../cms/migrations/0002_auth.sql) đã seed `bootstrap` trong migration. Vì vậy seed trong migration schema mới có lợi cho nhất quán bootstrap và test, **nhưng chưa chốt `0004` hoặc định nghĩa built-in** trước G2; vẫn kiểm trường hợp nâng cấp DB cũ và guard chống overwrite.

**Phương án không đạt PRD:** chỉ nạp CPT từ manifest đang active ở runtime và xóa/mất schema khi tắt provider; PRD yêu cầu tiếp tục quản lý nội dung dù plugin bị tắt/theme đổi ([PRD](../../product/PRODUCT_REQUIREMENTS.md)).

### G1.3 — Ma trận trạng thái và rủi ro cần giải trước G2

| Case phải xác minh | Bất biến bắt buộc | Bằng chứng cần tạo |
| --- | --- | --- |
| Owner trùng key (Core/User/Theme/Plugin) | `OwnerConflict` không ghi đè definition/data của chủ khác; ID validation khác namespace URL | Unit + DB conflict/transaction test |
| Plugin cài mới / update / **reinstall khi disabled** | Registration + `plugins.enabled` không nửa vời; trạng thái effective có quy tắc nhất quán; data cũ tồn tại | State transition table; failpoint trước/sau SQL commit và restart |
| Plugin v1 có hai CPT → v2 rút một CPT (plugin vẫn enabled) | CPT withdrawn vẫn còn definition + entries/fields, **effective inactive**; enable lại plugin không hồi sinh CPT đã rút; version/diff theo owner không chiếm key khác | SQLite candidate test `declaration_present`/effective-status, conflict/restart và Admin diagnostics |
| Disable → restart → enable / missing package | Definition/schema, entry và field value vẫn quản lý được; không execute provider inactive; tái kích hoạt có điều kiện rõ | API/DB integration tests, Admin diagnostics |
| `post/page` mới + `home_page` hiện có | `/`, `/page`, `/preview`, `/save`, plugin ABI v1 vẫn hoạt động; migration 0001–0003 giữ nguyên byte | Upgrade trên **DB copy** của `cms/fixtures/baseline_0001.sqlite` và DB có `home_page` + plugins; checksum, FK, restart |
| Admin/Editor, CSRF, content event | Editor không sửa CPT schema; save entry commit trước after-hook, before-hook (nếu có) có contract rõ; timeout handler không ghi sau commit trái phép | HTTP 401/403/CSRF, transaction & hook negative tests |

**Nghiên cứu cần bổ sung trước G2:** chốt ma trận ownership/effective-status kể cả provider không còn cài; xác minh schema options bằng thử nghiệm DB copy riêng (không migration production); chọn ranh giới `CNT-01` persistence ↔ `PLG-03` declarative adapter ↔ `CNT-04` requests; test contract tối thiểu. **Chưa chạy các bài test M3, Windows smoke hoặc build release trong vòng khảo sát này**; bằng chứng M2.5 cũ chỉ dùng cho baseline.

**E1 — thí nghiệm G1 độc lập (07/11/2026):** Python `sqlite3.connect(':memory:')`, không sửa migration/source: mô hình plugin + CPT với `declaration_present` và effective status; xác minh `disabled → reinstall` giữ disabled bằng `ON CONFLICT DO UPDATE SET enabled = plugins.enabled`; `enable → withdraw v2` giữ CPT withdrawn inactive và dữ liệu. **PASS cho cơ chế SQLite ứng viên**, không chứng minh Rust/SQLx/runtime đã triển khai đúng, không phải migration/contract đã duyệt. Cần test tiếp `missing package`, theme lifecycle, owner conflict và crash/rollback thực.

**E1_two_axis — bổ sung của Antigravity tại Turn 4 (`EXPERIMENT`):** SQLite in-memory thử `Core(post/page)`, `User(faq)`, `Theme(aurora)` và `Plugin(shop)` với `declared/withdrawn` × `owner_enabled`; qua `v1 → v2 rút event → disable → enable`, `event` vẫn `withdrawn`, còn `portfolio` hoạt động; ngắt theme làm CPT của theme inactive và mất dòng plugin cho trạng thái `package_missing`. **PASS trên fixture minh họa**, chưa thử lại bằng SQLx migration, process restart, transaction concurrent, plugin host thực hoặc hard ownership conflict. G2 phải kiểm chứng thêm policy reinstall khi disabled: hiện `upsert_plugin` đặt `enabled = 1`, thay đổi hành vi này cần hợp đồng riêng.

**Kết luận G1 (`REVIEWED`):** thống nhất **ứng viên D — declaration lifecycle tách provider availability**; seed built-in `post/page` qua migration mới là ứng viên ưu tiên, giữ `home_page` và các migration cũ. Đã đủ khảo sát để **mở thiết kế G2**, nhưng cách lưu state cho mọi owner, migration, schema field, interface/event, plugin manifest bridge và chính sách upgrade/enable **chưa được chốt**. Không đánh dấu implementation/verification M3 từ các thí nghiệm cô lập. [Thread đồng thuận G1](../../discussions/threads/2026-11-07-m3-g1-content-types-media-research.md).

## G2 — Quyết định cần chốt (hiện `NOT_STARTED`)

- ID CPT, owner và override policy, hai trục `declared/withdrawn` × availability cùng quy tắc hiệu lực/diagnostic, bảo toàn qua restart; tránh `owner_enabled` lặp không nhất quán giữa các CPT cùng owner. `RegistryCatalog::register_with_status` chỉ là một ứng viên; không tự kích hoạt provider đang tắt.
- Schema SQLite, migration versioning và cơ chế khôi phục dữ liệu/definition khi provider vắng mặt.
- Khảo sát cách seed `post`/`page` idempotent và bảo toàn `home_page` M1; phân biệt reserved URL namespace với validator `SnakeKey`.
- HTTP/use case/core/storage interfaces, transaction và event order; quản lý quyền/CSRF và public visibility. Entry public route nếu có cần contract riêng ở CNT-04.
- Cầu nối plugin manifest tối thiểu nếu dùng ở M3: schema/compatibility, install/disable/restart, rollback an toàn; không sửa Wasm ABI v1 để né gate.
- Cách CNT-02/CNT-03/CNT-04 truy cập CPT mà không xây API khác.
- Test matrix (owner conflict, reactivation, restart, migration từ DB copy, rollback thực tế, giữ content, lỗi handler). **Git revert không rollback migration đã chạy**; migration phát hành cần forward-fix.

Nếu một quyết định thay đổi contract chung, tạo ADR hoặc cập nhật contract trong `docs/contracts/` trước khi sửa code.

## G3 — Breakdown task dự kiến (chưa sẵn sàng giao code)

| Task ID giữ từ bản nháp | Đầu ra khi qua G3 | Phụ thuộc và điều kiện |
| --- | --- | --- |
| `M3-S01-T03` | Core CPT/registry và storage theo contract G2 | Full-Track G2 đã duyệt; M2.5 verified |
| `M3-S01-T04` | HTTP Admin + form quản lý CPT | T03, FND-05; RBAC/CSRF |
| `M3-S01-T05` | Plugin adapter/lifecycle tối thiểu, chỉ theo manifest đã chốt | T03/T04, PLG-03; theme switch E2E vẫn thuộc M5 |
| `M3-S01-T06` | Tests HTTP/DB/restart/provider, báo cáo verification | T03–T05, negative tests trên DB copy |

**Ánh xạ lịch sử ID:** `M3-S01-T01` và `M3-S01-T02` trong discovery draft cũ từng chỉ G1/G2; nay chuyển về các mục G1/G2 ở trên và **giữ hai số ID đó ở trạng thái retired, không tái cấp phát**. `T03..T06` là *task candidates*, chưa `READY`; bảng này chỉ được chốt ở G3.

Khi qua G2, chuyển từng task sang [mẫu giao việc](../templates/SLICE_TASK.md) trong `.agents/sync/task-for-ag.md`, ghi file ownership theo lượt, test và rollback. Không tạo hàng loạt file task mới; chưa được phép triển khai M3 từ nội dung brief này.
