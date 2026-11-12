# M5 — G1 nghiên cứu và G2 thiết kế đề xuất: Theme & Templates

**Agent:** Forge (CoS Windows) · **Ngày:** 2026-11-12 · **Baseline:** `origin/main@2e6b5c53db10fd23a069ff795cc845cc1fd40f6b`
**Trạng thái:** G1 đã khảo sát source/contracts; **G2 PROPOSED** để Atlas quyết định theo [ADR-0010](../../docs/decisions/ADR-0010-agent-project-and-merge-authority.md). Tài liệu nghiên cứu không cấp G3 READY, không chứng minh M5 IMPLEMENTED/VERIFIED.

Nguồn chính: [PRD](../../docs/product/PRODUCT_REQUIREMENTS.md), [M5 candidate plan](../../docs/plans/milestones/M5/TASK_PLAN.md), [Roadmap](../../docs/plans/ROADMAP_V1.md), [Module Catalog](../../docs/plans/MODULE_CATALOG_V1.md), [Planning Workflow](../../docs/plans/PLANNING_WORKFLOW.md), [Core Foundation](../../docs/contracts/CORE_FOUNDATION.md), [CMS Architecture](../../docs/architecture/CMS_ARCHITECTURE.md), [ADR-0004](../../docs/decisions/ADR-0004-m3-post-page-publication-boundary.md), [ADR-0007](../../docs/decisions/ADR-0007-m4-builder-taxonomy-foundation.md), [ADR-0009](../../docs/decisions/ADR-0009-m4-core-element-field-binding.md), [ADR-0011](../../docs/decisions/ADR-0011-m4-builder-editor-responsive-list.md).

## 1. G0: phạm vi và bất biến cần giữ

M5 gồm **10 candidate P01–P10**: lifecycle/theme package, RequestContext và URL, page/header/footer/archive/single composition, Theme Options, selective sample import, theme switch/update bảo toàn layout/schema/content, fallback/security, E2E, menu persistence tối thiểu và theme CPT/fields/taxonomy declarations. M3 đã có Theme install/list/activate và General Settings cơ bản; M4 cung cấp AST, responsive, binding, taxonomy/list. Nghiên cứu có thể song song M4, còn implementation chờ đúng dependency/G3.

Bất biến PRD: CPT/fields/terms/entries, layout đã chỉnh, menu và asset site-owned phải còn quản lý/sửa được khi đổi hoặc cập nhật theme; thiếu provider có diagnostics. Preview chưa lưu độc lập public và có session hợp lệ. Post/page `draft`: **Lưu** chỉ lưu nháp; **Xuất bản** mới ra public. Post/page đã published: Lưu cập nhật public. Builder homepage: **Lưu** cập nhật public ngay. Không bổ sung draft workflow CPT hoặc Builder, không lộ media của draft. Giữ RBAC, CSRF/Origin, CAS 412, publication predicate và media ACL.

**Ngoài M5:** plugin schema/blocks/settings registration bridge M6; SEO/custom permalinks, menu CRUD UI và site settings nâng cao M7; backup engine và Linux release M8; không xây lại AdminShell M3. `M5-Pxx` chỉ là candidate, **không phải task ID/G3 Issue**.

## 2. G1: inventory code và khoảng trống

| Nguồn code/DB thực kiểm | Kết quả và hậu quả cho M5 |
| --- | --- |
| [`Theme::from_zip`](../../cms/crates/opendash-server/src/theme.rs), [`package.rs`](../../cms/crates/opendash-server/src/package.rs) | Theme M3 nhận ZIP **phẳng** 2–5 file, `block.json` + `block.html` bắt buộc, tùy chọn `content-list.html`, `content-single.html`, `homepage-compat.json`, giới hạn 64 KiB. M5 cần manifest/version/multiple templates/assets; giữ parser v1. |
| [migration 0011](../../cms/migrations/0011_theme_activation.sql), [`storage/theme.rs`](../../cms/crates/opendash-server/src/storage/theme.rs), [theme route](../../cms/crates/opendash-server/src/routes/themes.rs) | Active theme hiện là **một block ID** (`site_active_theme.block_id`) + CAS version; activation không ghi lại `home_page`; alias compatibility cho block. Chưa có theme package/revision/update semantics. |
| [`theme_blocks`](../../cms/migrations/0001_builder_slice.sql), [migration 0007](../../cms/migrations/0007_content_templates.sql), [`storage/page.rs`](../../cms/crates/opendash-server/src/storage/page.rs) | CPT archive/single là hai template tùy chọn theo active block; chưa có page/header/footer và template overrides cho từng role/route. |
| [migration 0013](../../cms/migrations/0013_builder_homepage.sql), [`storage/builder.rs`](../../cms/crates/opendash-server/src/storage/builder.rs), [`page.rs`](../../cms/crates/opendash-server/src/page.rs) | Homepage AST site-owned, version và legacy projection; M5 phải giữ `home_page` và `builder_homepage` xuyên switch/update. M4 AST/responsive/list renderer là dependency. |
| [`app.rs`](../../cms/crates/opendash-server/src/app.rs), [`routes/public_content.rs`](../../cms/crates/opendash-server/src/routes/public_content.rs), [`content_render.rs`](../../cms/crates/opendash-server/src/content_render.rs) | Đã có `/`, `/content/{type_key}`, `/content/{type_key}/{slug}`; query taxonomy filters và published-only storage. URL trong entry/list còn được dựng trực tiếp; chưa có RequestContext, taxonomy URL hoặc canonical resolver thống nhất. |
| [migration 0004](../../cms/migrations/0004_content_types.sql), [`registry.rs`](../../cms/crates/opendash-core/src/registry.rs), [`storage/content.rs`](../../cms/crates/opendash-server/src/storage/content.rs) | `content_types` có owner `core/user/theme/plugin` + `provider_status`; registry từ chối takeover, nhưng storage mới tạo user CPT; chưa có theme declaration bridge và policy field evolution/withdrawn. |
| [migration 0012](../../cms/migrations/0012_taxonomy_terms.sql), [migration 0014](../../cms/migrations/0014_entry_terms.sql), [`storage/taxonomy.rs`](../../cms/crates/opendash-server/src/storage/taxonomy.rs) | Site-owned term ID/entry links; core kinds chỉ `category/tag`. Theme cần khai eligibility/binding, không chiếm ownership/xóa term khi theme inactive. |
| [migration 0010](../../cms/migrations/0010_site_general_settings.sql), [`storage/settings.rs`](../../cms/crates/opendash-server/src/storage/settings.rs), [`routes/settings.rs`](../../cms/crates/opendash-server/src/routes/settings.rs) | General Settings typed/CAS/Admin-only có sẵn; chưa có options engine typed mở rộng hoặc menu persistence. |
| [`storage/public_content.rs`](../../cms/crates/opendash-server/src/storage/public_content.rs), [`storage/builder_binding.rs`](../../cms/crates/opendash-server/src/storage/builder_binding.rs) | Public chỉ lấy published, media có reference ACL, binding có ranh giới dữ liệu; cần dùng lại cho template/list/menu/import. |

**Phương pháp và giới hạn:** đọc code thực tế, migrations, contracts và tìm caller qua `rg`; CodeGraph MCP memory trả 0 node nên không dùng kết quả graph để kết luận. Chưa chạy M5 prototype, migration hay browser. GitHub milestone M5 là số **6**; không có task M5 G3/Issue phát hành trong tra cứu GitHub lúc khảo sát. Trạng thái bàn giao M4 phải xác minh lại trên GitHub tại thời điểm cấp Ready.

## 3. G1: so sánh các phương án

| Quyết định | Phương án đơn giản | Phương án ưu tiên cho G2 | Trade-off |
| --- | --- | --- | --- |
| Theme | Mở rộng `theme_blocks` như một theme có thêm template | Package identity + revision bất biến + active selection; adapter block v1 | Đủ update/rollback/multi-template; cần migration compatibility. |
| Public URL | Mỗi template tự dựng URL/matcher | Server `RequestContext`, canonical URL builder thuộc CNT-04/SITE-02 | Tránh collision/ACL drift; cần sửa callers hardcode URL. |
| Design preservation | Copy theme defaults vào site, overwrite khi update | Default thuộc theme revision, chỉ lưu site override khi user Save | Không ghi đè user edit; resolver/fallback phức tạp hơn. |
| CPT/fields | Chỉ đọc schema từ active ZIP | Persist qua canonical CNT owner registry, tách availability và declaration state | Quản lý nội dung được khi inactive; cần policy upgrade. |
| Taxonomy | Theme tạo lại hoặc sở hữu terms | Category/tag core; theme khai type eligibility, sample terms site-owned | Giữ links; custom taxonomy kind phải mở G0 khác. |
| Menu | Menu HTML gắn theme | SITE-01 menu/item IDs, entry/term refs ổn định | Dùng được cho import M5 và Admin M7. |
| Import | Ghi từng phần và mỗi retry thêm bản sao | Preview plan + provenance + idempotent transaction + media recovery | Cần thiết kế staging filesystem chính xác. |

## 4. G2-A — Package, version và activation (P01)

**Đề xuất:** `theme_id` canonical bất biến; `revision_id` bất biến, `version` semver và digest gói; manifest `theme.json` schema v2 với templates, blocks, options, content types, taxonomy bindings, sample manifest và assets. Parser v1 tiếp tục chấp nhận ZIP cũ. **Không giả định** nhiều block v1 chung namespace là cùng một theme nếu dữ liệu cũ không bảo đảm uniqueness: phải thử trên fixtures.

Migration forward mới dự kiến `theme_packages(theme_id,...)`, `theme_revisions(revision_id,theme_id,version,digest,manifest_json,package_ref,...)`, `theme_templates(revision_id,role,variant,...)`. Lưu old revisions đủ rollback; pin active revision trong **một canonical selection** có CAS (migrate/adapter `site_active_theme.block_id`). Không duy trì hai nguồn truth có thể lệch. `theme_blocks`/legacy homepage/Builder data phải còn đọc được.

Validate toàn ZIP trước commit: file count, compressed/decompressed limits, per-file size, extension/MIME, duplicated entries, `..`/absolute/path separator, symlink, zip bomb/ratio, UTF-8, JSON schema và compile template. 64 KiB v1 không dùng làm quota v2. Mọi package assets được lưu bằng path an toàn trong site root, không thực thi mã tùy ý. Cần chốt package bytes vs files (nguồn canonical cho backup M8); không nâng dependency pin chỉ để né bài toán.

Install/update/activate/rollback Admin + CSRF/Origin, mutate theo `If-Match`, stale 412, owner/key conflict 409, invalid 400/413. Update cùng theme/version khác digest bị chặn, không sửa revision cũ; activate chỉ commit nếu tất cả declarations/compatibility hợp lệ, rollback transaction khi lỗi. Race activate với Builder Save và concurrent update phải được thử.

## 5. G2-B — URL policy và RequestContext (P02)

**Resolver server** trả typed `Home | Single{type_key,entry_id} | Archive{type_key,page,filters} | TaxonomyArchive{kind,term_id,page} | NotFound`; renderer nhận view model đã lọc public. Stable IDs cho menu/binding, URL canonical theo slug hiện hành từ central `url_for_entry/term/archive`, không để templates tự match route hoặc query DB.

Đề nghị **giữ nguyên** `/`, `/content/{type_key}`, `/content/{type_key}/{slug}` của M3 và thêm `/taxonomy/category/{slug}`, `/taxonomy/tag/{slug}`. Namespace `api/admin/auth/media/assets/builder/theme/page/registry/preview/save/content/taxonomy` là reserved; kiểm collisions, percent-encoded separator/path, double-encoding, query lặp/sai loại, pagination giới hạn. Published predicate phải ở storage trước paginate; unpublished/draft trả 404 và media vẫn private.

**G2 decision cần chốt:** đổi slug trả 404 hay có redirect tối thiểu? Ưu tiên 404 trong M5 và để redirects/SEO nâng cao ở M7; nếu quyết định redirect ở M5 phải thiết kế no open-redirect/loops/canonical collision/ACL. Không đưa custom permalink tổng quát vào M5.

## 6. G2-C — Template composition, override, security (P03/P06/P07)

Role `home|page|header|footer|archive|single|taxonomy_archive`; selector `{role,type_key?,variant?}`. Resolver ưu tiên **site override đã Save** → active theme revision default → core safe fallback. Header/footer compose với route body; single dùng entry+fields đúng RequestContext, archive/query/list chỉ lấy nội dung public; preview authenticated có thể nhận draft riêng nhưng không ghi DB. Giới hạn cây/node/bytes và renderer dùng contract M4.

Theme default readonly theo revision; site overrides riêng (proposed `site_template_overrides(template_key,base_revision_id,document_json,version,...)`) chỉ tạo khi user chủ động Save. Update/switch không đè node/layout; “Dùng mặc định mới” là thao tác có xác nhận/CAS. Node thiếu provider giữ nguyên ID/props trong Admin kèm diagnostics, public fallback/ẩn an toàn; khi provider quay lại hiển thị đúng.

**Template trust boundary:** MiniJinja hiện bật HTML escaping + Strict + fuel trong [`theme.rs`](../../cms/crates/opendash-server/src/theme.rs). HTML escaping **không đủ** cho URL, CSS, rich text và template/filters có khả năng đánh dấu dữ liệu safe; G2 phải chốt template subset, whitelist filters, validation HTML attributes/asset URLs và CSP. Không compile user content thành template source; template ZIP không được đọc file/secret hoặc gọi SQL. Kiểm malicious `href`, `javascript:`, CSS injection, escaping bypass, missing include, recursion/fuel, Preview và public đồng hành.

## 7. G2-D — SITE-01 Theme Options (P04)

Đề xuất typed descriptor `{owner:Theme(theme_id), key, type, default, validation, expose_to_template}`, V1 ưu tiên `string|number|boolean|select|image_ref`. SITE-01 lưu definitions và values có version/owner/scope (site/theme); tiếp tục dùng General Settings M3, **không** làm một hệ settings song song bên THM-02. Default được resolve khi chưa có user value; update theme giữ user-set value. Schema incompatible giữ value gốc + diagnostics, không silent drop.

Admin có auto-form từ manifest, GET/PUT có RBAC/CSRF/Origin/ETag; Editor không sửa. Public chỉ nhận options đã whitelist, không cấp bí mật cho template; image_ref và media ACL được kiểm khi render. G2 cần chốt schema evolution và policy option bị rút/mất provider.

## 8. G2-E — Theme CPT/fields/taxonomy declarations (P10)

Adapter ZIP gọi **CNT-01/02/03 canonical domain APIs** để validate/register `ResourceOwner::Theme(theme_id)` trong transaction; SQL đặt ở `storage/`, không ghi trực tiếp trong route/theme HTML và không dùng plugin provider bridge M6. Owner conflict kể cả inactive/withdrawn trả 409; post/page core không chiếm được. Reinstall/update cùng owner idempotent khi definition compatible.

Current `content_types.provider_status` chỉ active/inactive. Đề xuất tách `declaration_state=declared|withdrawn` và `provider_availability=active|inactive|missing|invalid` trong read model, snapshot consistency với active revision. **Không** gọi `activate_owner` để hồi sinh withdrawn. Forward migration mới nếu cần; không sửa migration 0004. Fields nằm trong `ContentTypeDefinition` JSON đã có, giữ key/type/value/entry khi schema đổi; incompatible upgrade phải reject hoặc mở explicit upgrade plan/rollback, không drop data.

Taxonomy V1 chỉ `category/tag` core; theme khai `taxonomy_bindings: [{type_key,kind}]` (khả năng gắn terms), không giành owner của `taxonomy_terms`. Seed terms đi qua P05 nhập mẫu và trở thành site-owned. Nếu cần custom taxonomy kinds, phải thay scope G0 trước G2/G3. Sau switch theme, Admin/Editor vẫn sửa CPT, fields, terms và entries đã có.

## 9. G2-F — SITE-01 menu model (P09)

Đề xuất `site_menus(id,stable_key,name,origin,version)` và `site_menu_items(id,menu_id,parent_id,position,label,target_kind,target_id,external_url,origin_key,...)`, optional mapping theme slot→menu ID. IDs và order site-owned; link nội bộ bằng **entry ID/term ID** chứ không dùng slug/URL làm identity; URL canonical dựng khi render. External URL phải kiểm protocol/host/controls và escape attribute, không nhận script/data URLs.

Unique origin/stable keys, parent cùng menu, giới hạn depth/items, chặn cycle và duplicate position ambiguity; sort xác định `parent,position,id`. Khi entry/term mất hoặc chưa published, Admin báo unresolved và public bỏ link/đưa fallback an toàn; không biến thành link trỏ dữ liệu khác. Theme switch/restart vẫn giữ menu. P09 xây persistence + internal API để P05 import; UI CRUD và binding đầy đủ thuộc M7-P02, dùng lại model.

## 10. G2-G — Chọn nhập mẫu, provenance, atomicity (P05)

User chọn page/menu/media/settings theo từng nhóm; endpoint *dry run* lập plan với `theme_id,revision_id,sample_key,resource_kind`, target refs, conflict/skip/created và expected versions. Import phải chống ghi đè content/menu/settings hiện hữu, giữ publication policy (không tự publish draft), không cấp media public ACL khi chỉ import. Tất cả data mutations phải nằm trong server storage/API canonical.

Proposed `import_runs` và `import_objects` có unique provenance key → site target ID; retry cùng sample idempotent, user-changed target được skip/diagnostic. Dữ liệu DB/menu/refs/options commit nhất quán trong transaction. Vì SQLite không transaction filesystem, media file dùng staging + journal + fsync/rename/recovery idempotent, dọn orphan có kiểm chứng; **không** xóa file dùng chung/user-owned khi rollback. G2 cần chốt quota và transaction boundary; ưu tiên all-or-nothing trong một run có giới hạn V1, với recovery sau crash.

## 11. G2-H — Preservation, missing provider và acceptance (P06–P08)

Một snapshot nhất quán của active revision + entry publication + templates/overrides/options/menu; giao diện Admin thể hiện missing provider/schema/option/term bằng diagnostic cụ thể. Public chỉ render template active/hợp lệ hoặc core fallback, không dùng HTML theme inactive trái owner. Negative tests: malicious ZIP/asset/path, XSS/safe/URL, 401/403/CSRF, 412 race, owner collision, missing package, invalid schema, outdated revision, draft/media leakage và SQLite/file partial import.

**M5 E2E primary acceptance** trên site copy: ZIP v1 legacy vẫn mở/preview/Save; cài Theme A v2 declare CPT/fields/category/tag eligibility; tạo term/entry/menu/selected sample; preview draft và public được phân biệt; Publish→URL/archive/list/pagination/header/footer/single; sửa Builder layout → update A → switch B thiếu provider → restart → A; nội dung/schema/term/menu/layout vẫn hiển thị trong Admin và chỉnh được, re-activate không duplicate; failure/race không đổi active hoặc mất dữ liệu. G5 báo binary/commit, OS, fixture DB/files, lệnh PASS/FAIL/NOT_RUN và rollback qua **snapshot site + binary tương ứng**; verify riêng Windows/macOS theo roadmap, Linux release còn ở M8.

## 12. Coverage từng candidate và trình tự phụ thuộc

| Candidate | Hợp đồng G2 chính | Điều kiện G3/triển khai | Kiểm chứng trọng tâm |
| --- | --- | --- | --- |
| P01 THM-01 | A package/revision/activation | M3 Theme, fixture nâng cấp v1 | Update/rollback, ZIP guard, no data loss |
| P02 CNT-04/SITE-02 | B RequestContext/URL | M3 public, M4 taxonomy/list G5 | Published-only URL/collision/query |
| P03 THM-02 | C composition/overrides | P01+P02+M4 AST | Page/header/footer/archive/single render |
| P04 SITE-01 | D Options | P01, M3 General Settings | Typed form/CAS/switch persistence |
| P05 THM-03 | G import | P01+P04+P09+P10+media | Selection/idempotency/rollback |
| P06 THM-03 | C+E preservation | P01+P03+P05+P10 | A→B→A, schema/entry/layout editable |
| P07 THM-01/02 | H fallback/security | P01…P06+P10 | Missing provider/security negatives |
| P08 THM-02/03 | H E2E | P02…P07+P09+P10+M4 G5 | Cross-module multi-OS G5 |
| P09 SITE-01 | F menu storage | P02 URL identity+M3 Content | Stable refs/restart/theme switch |
| P10 THM-01/CNT | E declarations | P01+M3 CPT/fields+M4 taxonomy | Owner collision/schema evolution |

**Critical path đề xuất:** song song P01 và P02 sau khi contract chung chốt; P01→P10/P04; P02→P09; P01+P02→P03; P04+P09+P10→P05; P03+P05+P10→P06; P06→P07→P08. G3 phải cấp exact `M5-Sxx-Txx`, owner/branch/base SHA, file ownership theo lượt, acceptance/exclusions/test và migration numbering. Shared writer seams: `app.rs`, `routes/mod.rs`, `storage/mod.rs`, `storage/theme.rs`, `storage/page.rs`, `page.rs`, `theme.rs`, `content_render.rs`, `storage/content.rs`, `registry.rs`, Admin shell và migrations; không hai agents sửa cùng file một lúc.

## 13. Quyết định cần Atlas chốt trước G3

1. Package identity/revision/ZIP v2: mapping legacy block ID, file/bytes asset canonical, quota, migration và active pointer một nguồn.
2. URL namespace/taxonomy route/canonical, slug-change 404 hay redirect trong M5 và public query/pagination semantics.
3. Template role/selector, site override provenance/CAS, fallback và MiniJinja/asset/CSP safe subset.
4. SITE-01 Options schema và menu stable-ID/ordering/slot model để M7 dùng lại.
5. Theme CNT declarations/field compatibility, withdrawn vs inactive và taxonomy-binding chỉ category/tag.
6. Selective import provenance, publication policy, transactional boundary và media crash recovery.
7. M4 G5 handoff SHA, ownership các seam/migrations, slice primary acceptance và multi-OS fixtures.

Sau khi Atlas ghi Final Decision có evidence, đưa các quyết định liên module vào ADR/contracts qua PR; tiếp đó soạn G3 **cho đủ M5-P01..P10**, dù task còn chờ dependency; chỉ phát hành Issue/Project Ready khi G3 đã merge và guard thỏa. Các ứng viên G2 ở đây cần review, không phải source authority thay cho docs đã chốt.

## 14. Validation scope của nghiên cứu

Đã kiểm tra implementation/DB tại baseline ghi trên và so sánh 10 candidate với PRD/roadmap. **PASS trên Windows, worktree nghiên cứu:** `python cms/scripts/check-plans.py` (markdown/module/task index), `python cms/scripts/check-plans.py --self-test` (12 fixtures) và `python .agents/scripts/check-agent-artifacts.py`. `git diff --check` phải được chạy lại sau khi stage tài liệu. **NOT_RUN:** M5 binary, schema migration prototype, integration/browser, CI, safety fuzzing; không có code M5 để kiểm nghiệm. Không nâng G2 lên AGREED khi chưa có Final Decision/merge.
