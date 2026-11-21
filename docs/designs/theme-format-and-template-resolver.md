# Theme Format v3, Template Resolver 2 Trục, Page Layouts & Reading Settings

* **Author:** Nova (tổng hợp từ các quyết định sản phẩm & resolver đã chốt cùng chủ dự án ngày 19–20/11/2026)
* **Status:** Approved
* **Date:** 2026-11-20
* **Baseline:** `main@7f29bcf` (migrations `0001`–`0041`)
* **Approved By:** Owner
* **Approval Evidence:** Owner chat 2026-11-20 (Cách A Full Site Editor + OQ-02/03/04 chốt) · PR #424 (`e47d5d3`)
* **Goldfish Review:** 5-agent independent swarm (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`, `egm-comprehension`, `egm-zoom-out`) · 2026-11-20 · reviewed `main@7f29bcf` (all Blocker/Major resolved)
* **Related Spec:** [Design Spec 2: Generic CPT Content Builder, Pattern CPT (`core/pattern_ref`) & Pattern Library](cpt-content-builder-and-pattern-library.md)

---

## Overview

Đặc tả này chuẩn hóa kiến trúc **Theme Package Format v3**, **Template Resolver 2 trục** (độ cụ thể của candidate trước → nguồn DB override / active theme sau), **Visual Builder Template Overrides** (giống WordPress Full Site Editor — giữ `ast_json` kèm block `core/content_slot` và `core/template_part`), **Typed Composition tuân thủ C-SEC** cho file HTML trong gói theme, **Page Layouts** (`page-layouts/`), **Custom Single Templates** (`SingleCustom`), và **Reading Settings** (chọn 2 Page độc lập cho Trang chủ `/` và Trang bài viết Blog) trên một route authority duy nhất (`storage/permalinks.rs`).

Theo chỉ đạo ngày 20/11/2026 của chủ dự án, OpenDash đang trong giai đoạn phát triển trước phát hành nên **không duy trì tương thích ngược** với các định dạng theme/API/schema thử nghiệm cũ (v1 `block.json`, v2 `ThemeV2Manifest` cũ hay homepage thử nghiệm độc lập). Hệ thống chốt **một định dạng, một schema và một API đích duy nhất**; đồng thời vẫn giữ nguyên yêu cầu bảo toàn nội dung, bố cục người dùng đã chỉnh khi cập nhật/đổi theme, phân quyền, publication và backup/restore trên mô hình mới.

> **Phạm vi thay thế hợp đồng cũ:** Sau khi triển khai và kiểm chứng hoàn tất, đặc tả này thay thế mục 3 (thứ tự ưu tiên override trước theme) và định dạng gói v2 trong [`M5_THEME_TEMPLATES.md`](../contracts/M5_THEME_TEMPLATES.md).

---

## The Problem

1. **Thứ tự ưu tiên hiện tại che mất template cụ thể của theme:** Trong [`storage/overrides.rs`](../../cms/crates/opendash-server/src/storage/overrides.rs) (theo hợp đồng M5 cũ), mọi bản ghi DB override được xét trước mọi template của theme. Do đó, một DB override `single` chung sẽ đè mất template `single-project.html` chuyên biệt của theme đang kích hoạt.
2. **Định dạng theme v2 quá phẳng và còn gánh v1 legacy:** [`theme.rs`](../../cms/crates/opendash-server/src/theme.rs) hiện duy trì song song v1 (`block.json`) và v2 (`templates: BTreeMap<String, String>` với đúng 7 role cố định), không hỗ trợ khai báo có kiểu cho template theo CPT/taxonomy/custom template, `template-part/`, `page-layouts/` hay `patterns/`.
3. **Trang chủ tách rời khỏi hệ thống Page & Route Authority:** [`routes/public.rs`](../../cms/crates/opendash-server/src/routes/public.rs) đọc homepage từ `home_page` độc lập thay vì chọn một Page hợp lệ trong Settings; chưa có cấu hình Trang bài viết (Blog) để gán archive của built-in `post` vào URL canonical của một Page mà vẫn giữ nguyên body Page.
4. **Thiếu cơ chế ghép thành phần nhỏ an toàn (`template-part/`) và content slot:** [`template_sec.rs`](../../cms/crates/opendash-server/src/template_sec.rs) áp dụng bộ quy tắc bảo mật **C-SEC** (MiniJinja `Environment::empty()` với auto-escape HTML bắt buộc, cấm `{% include %}`/`{% extends %}`/`|safe`/thẻ `<script>`, giới hạn fuel `10_000`, recursion `16`, dung lượng render `512 KiB`), nhưng chưa có cơ chế composition có kiểu cho `template-part/` và `{{ od_content() }}` trên HTML theme cũng như block `core/content_slot` / `core/template_part` trên Visual Builder overrides.

---

## Goals

1. **Một định dạng Theme đích thống nhất (`manifest_version: 3`):** Template chính và `header.html`/`footer.html` nằm ngay root gói theme; 4 thư mục chuyên biệt gồm `template-part/`, `page-layouts/`, `patterns/` và `assets/`. Mọi tài nguyên phải khai báo rõ trong `theme.json`, không tự thực thi file theo tên thư mục.
2. **Template Resolver 2 trục:** Xét danh sách candidate có kiểu từ cụ thể đến tổng quát; **tại từng candidate**, tìm bản site-owned DB override trong SQLite trước, rồi tới tài nguyên tương ứng của active theme revision.
3. **Full Site Editor Experience (`ast_json`) + C-SEC Theme HTML:** Giữ `template_overrides` lưu **Visual Builder AST (`ast_json` — `BuilderDocument`)** với 2 block lõi mới `core/content_slot` (vị trí đổ nội dung bản ghi) và `core/template_part` (nhúng part theo key); đồng thời hỗ trợ `{{ od_part("<literal>") }}` và `{{ od_content() }}` trong file `.html` của Theme v3 mà vẫn giữ nguyên 100% ranh giới `C-SEC`.
4. **Page Layouts & Custom Single Templates độc lập nội dung:** Page chọn layout từ `page-layouts/` (`page_layout_key`); Post/CPT chọn custom template (`custom_template_key`). Đổi hoặc mất layout/template giữ nguyên reference trong DB, báo Admin và fallback an toàn.
5. **Reading Settings atomic qua 1 Route Authority:** Admin chọn 2 Page khác nhau (`homepage_page_id`, `posts_page_id`) đều `public` & `published` qua `GET/PUT /api/admin/settings/reading` có CAS `If-Match`, đồng bộ nguyên tử với `storage/permalinks.rs`, `storage/sitemap.rs` và `storage/seo.rs`.
6. **Bảo toàn dữ liệu & Backup/Restore v3:** Đổi/cập nhật theme (`A → B → A`) giữ nguyên site-owned overrides và provenance; cập nhật `backup_worker.rs` hỗ trợ `source_format == "v3"` để backup/restore không mất file theme; reset template an toàn qua `POST /api/admin/template-overrides/{id}/reset`.

---

## Non-Goals

- Không hỗ trợ tương thích ngược cho gói theme v1 (`block.json`), v2 cũ, adapter legacy, dual-read/dual-write hay tự động chuyển đổi dữ liệu `home_page` thử nghiệm thành Page.
- Không xây dựng trình soạn thảo mã HTML (code editor) cho Admin — Admin chỉnh sửa `template_overrides` bằng Visual Builder (`ast_json`).
- Không tự ý xóa dữ liệu hoặc thư mục runtime hiện có của site để thực hiện thiết kế mới.
- Không triển khai Generic CPT Content Builder (`ContentSupport::Builder`, `content_mode`), Pattern CPT `core/pattern_ref` hay giao diện Pattern Library trong đặc tả này (thuộc [Design Spec 2](cpt-content-builder-and-pattern-library.md)).
- Không triển khai Custom Builder Block authoring/SDK (đã chốt thảo luận riêng sau), child themes, PHP themes, file watchers nền hoặc các route chưa hỗ trợ (search, author/date archive, attachment, embed).

---

## Critical User Journeys (CUJs)

### CUJ 1: Cài đặt và kích hoạt Theme v3
* **Given:** Admin tải lên gói ZIP theme chuẩn `manifest_version: 3` gồm `theme.json`, các template root, `template-part/`, `page-layouts/`, `patterns/` và `assets/`.
* **When:** Admin gọi API cài đặt và kích hoạt revision mới.
* **Then:** Server kiểm tra allowlist đường dẫn/dung lượng qua `opendash_server::package`, validate `ThemeV3Manifest`, kiểm tra C-SEC + đồ thị `od_part` (không vòng lặp, depth ≤ 3, fan-out ≤ 32) + `od_content()`, lưu revision bất biến kèm `.complete` marker (`source_format = 'v3'`) và kích hoạt nguyên tử qua CAS. Backup mã hóa `age` sao chép đầy đủ `themes/revisions/<digest>/` của revision `v3`.

### CUJ 2: Phân giải Template theo độ cụ thể trước, nguồn sau
* **Given:** Site có DB override (Visual Builder `ast_json`) cho `single` chung và active theme có `single-project.html` khai báo cho CPT `project`.
* **When:** Người dùng truy cập một bản ghi `project` đã xuất bản.
* **Then:** Resolver xét candidate `single(type="project")` trước `single` chung; tại candidate đó không có DB override nên chọn `single-project.html` của theme. Nếu Admin tạo thêm DB override cho đúng `single(type="project")` bằng Visual Builder (có đặt 1 block `core/content_slot`), bản DB đó thắng theme ở cùng candidate.

### CUJ 3: Chọn Page Layout / Custom Single Template và xử lý mất Template khi đổi theme
* **Given:** Một Page đang dùng layout `canvas` (`page_layout_key = "canvas"`) hoặc một Post đang dùng custom template `landing` (`custom_template_key = "landing"`) do theme A đăng ký.
* **When:** Người dùng xem trang hoặc Admin chuyển sang theme B không có layout/template đó.
* **Then:** Khi theme A active, Page render `page-layouts/canvas.html` (không gọi `header`/`footer`) giữ nguyên nội dung Page. Khi chuyển sang theme B, `page_layout_key` / `custom_template_key` vẫn được giữ nguyên trong DB (kể cả khi Admin lưu chỉnh sửa khác qua `PatchField::Keep`), màn sửa báo cảnh báo thiếu layout/template và public fallback an toàn theo nhánh thường (`page → index → core` hoặc `single(type) → ... → core`).

### CUJ 4: Cấu hình Reading Settings (Trang chủ & Trang bài viết)
* **Given:** Có 2 Page khác nhau đã xuất bản (`Home`, `Blog`) và Admin mở màn Cài đặt → Reading.
* **When:** Admin gán `homepage_page_id = Home.id` và `posts_page_id = Blog.id` kèm header `If-Match: "vN"`.
* **Then:** Trong 1 transaction, `permalinks::sync_claims_in_tx` gán `/` cho `Home` (slug cũ của `Home` 301 về `/`), gán URL canonical của `Blog` sang `PublicRouteTarget::Archive("post")` mà không gây `409 permalink_collision` (render nhánh `archive-post → archive → index`, giữ nguyên `values_json` của Page `Blog` trong DB). Sitemap không bị lặp `/` và không đưa `Blog` vào danh sách entry thường. Nếu `Home` hoặc `Blog` sau đó bị chuyển về `draft` hoặc bị xóa, route tương ứng trả HTTP `404` + header `X-Robots-Tag: noindex` (không lộ draft và không tự chọn Page khác).

### CUJ 5: Lưu và Reset Template Override an toàn bằng Visual Builder
* **Given:** Admin đang chỉnh sửa override của `single(type="post")` trong Visual Builder và bấm Reset về mặc định.
* **When:** UI hiển thị `next_effective_source` thực tế (ví dụ: `active_theme: single-post.html` hoặc `db_override: single` hoặc `core_fallback`), Admin xác nhận Reset qua `POST /api/admin/template-overrides/{id}/reset` kèm `If-Match: "vN"`.
* **Then:** Server kiểm tra CAS `version` của override trong 1 transaction; nếu khớp thì xóa đúng 1 override mục tiêu (không sửa file theme bất biến, không đổi nội dung bản ghi) và trả về `next_effective_source` thực tế; nếu stale thì trả `412 Precondition Failed` và giữ nguyên bản nháp trên Admin.

---

## Technical Plan (Architecture)

### A. Cấu trúc Gói Theme v3 (`manifest_version: 3`)

Template chính và `header.html`/`footer.html` đặt ngay tại root gói theme; 4 thư mục con có vai trò cố định:

```text
my-theme/
├── theme.json
├── index.html
├── home.html
├── page.html
├── single.html
├── single-post.html
├── archive.html
├── archive-post.html
├── taxonomy.html
├── 404.html
├── header.html
├── footer.html
├── template-part/
│   ├── post-card.html
│   ├── breadcrumbs.html
│   └── pagination.html
├── page-layouts/
│   ├── full-width.html
│   ├── sidebar.html
│   └── canvas.html
├── patterns/
│   └── hero.json
└── assets/
    ├── styles.css
    └── images/
```

**Schema đích `ThemeV3Manifest` (đặt tại `opendash_core::theme`, tất cả struct dùng `#[serde(deny_unknown_fields)]`):**
- `manifest_version: u8` — bắt buộc `== 3`.
- `theme_id: String` — kiểm qua `opendash_core::id::validate_package_id` (gỡ bỏ `storage::theme::valid_v2_theme_id` dư thừa).
- `name: String`, `version: String` (`validate_semver`).
- `templates: Vec<ThemeTemplateDecl>` (bắt buộc không trùng `canonical_target_key` trong cùng manifest):
  - `kind: TemplateCandidateKind` (`Home`, `Page`, `Single`, `SingleCustom`, `Archive`, `Taxonomy`, `Singular`, `Index`, `NotFound`). *(Built-in `category` và `tag` dùng `Taxonomy` với `taxonomy: Some("category" | "tag")`.)*
  - `content_type: Option<String>` (`valid_snake_key`).
  - `taxonomy: Option<String>` (`valid_snake_key`).
  - `selector_slug: Option<String>` (`valid_kebab_segment_with_limit`). *(Không có `entry_id`/`term_id` trong theme ZIP vì ID DB mang tính cục bộ của từng site; chỉ DB override mới nhắm ID cục bộ.)*
  - `custom_key: Option<String>` (`valid_snake_key`, bắt buộc cho `SingleCustom`, kèm `label: String` và `allowed_types: Vec<String>`).
  - `file: String` — file `.html` nằm trực tiếp ở root (không chứa `/`).
- `parts: BTreeMap<String, String>` — khóa `valid_kebab_segment_with_limit`; `header`/`footer` trỏ tới `header.html`/`footer.html` ở root, các part khác trỏ tới `template-part/<file>.html`.
- `page_layouts: Vec<PageLayoutDecl>` — `{ key: SnakeKey, label: String, file: String }` với `file` bắt buộc trong `page-layouts/*.html`.
- `patterns: Vec<ThemePatternDecl>` — `{ key: SnakeKey, title: String, category: KebabSegment, description: Option<String>, preview_asset: Option<String>, file: String }` với `file` bắt buộc trong `patterns/*.json` (validate qua `BuilderDocument`), `preview_asset` (nếu có) bắt buộc là đường dẫn hợp lệ trong `assets/` (render thành `/theme/<theme_id>/<rel_path>`, không tạo URI scheme mới).
- `options`, `menus`, `sample`, `declarations`, `field_groups`, `admin_extensions`: giữ nguyên các kiểu đã chuẩn hóa hiện hành.
- DB lưu `source_format = 'v3'` trên `theme_packages` / `theme_revisions`; cập nhật `backup_worker.rs` sao chép `themes/revisions/<digest>` cho `source_format == "v3"` (và giữ đọc `"v2"` nếu cần trên DB thử nghiệm cũ cho tới khi xóa sạch).

### B. Typed Template Identity & Template Resolver 2 Trục

```text
Authorized PublicRequestContext (đã kiểm route, publication, visibility, quyền, has_archive)
  → Sinh danh sách có thứ tự Vec<TemplateCandidate>
  → Với TỪNG candidate (từ cụ thể → tổng quát):
      1. Tìm bản site-owned trong `template_overrides` khớp `canonical_target_key` (Visual Builder AST)
      2. Nếu không có → tìm bản trong active theme revision v3 khớp `canonical_target_key` (MiniJinja HTML)
      3. Nếu cả hai không có → sang candidate kế tiếp
  → Render nguồn trúng (Visual Builder AST hoặc MiniJinja C-SEC HTML) + ghép parts + đổ content slot
  → Fallback core bảo toàn ngữ cảnh nếu không candidate nào khả dụng
```

**Định dạng `canonical_target_key` (xác định duy nhất bởi `TemplateCandidate::canonical_key()` trong `opendash_core::theme`):**
- `home`, `page`, `page_layout:<layout_key>`, `single`, `single:type:<type_key>`, `single:type:<type_key>:slug:<slug>`, `single:type:<type_key>:id:<entry_id>`, `single_custom:<custom_key>`, `singular`, `archive`, `archive:type:<type_key>`, `taxonomy`, `taxonomy:key:<tax_key>`, `taxonomy:key:<tax_key>:slug:<slug>`, `taxonomy:key:<tax_key>:id:<term_id>`, `index`, `not_found`, `part:<part_key>`.
- Do mỗi segment đã bị ràng buộc charset (`SnakeKey` `[a-z0-9_]`, `KebabSegment` `[a-z0-9-]`, `i64`), dấu `:` không bao giờ va chạm.

**Bảng thứ tự Candidate (trái → phải):**

| Ngữ cảnh đã xác thực | Thứ tự Candidate |
|---|---|
| Một bản ghi Post/CPT (ngoài nhánh Page) | `SingleCustom(custom_key)` đã gán hợp lệ → `Single(type, entry_id)` *(DB only)* → `Single(type, slug)` → `Single(type)` → `Single` → `Singular` → `Index` |
| Page thông thường | `PageLayout(layout_key)` đã gán hợp lệ → `Page` → `Index` |
| Archive một CPT | `Archive(type)` → `Archive` → `Index` |
| Taxonomy term (gồm `category`, `tag`, custom taxonomy) | `Taxonomy(key, term_id)` *(DB only)* → `Taxonomy(key, term_slug)` → `Taxonomy(key)` → `Taxonomy` → `Archive` → `Index` |
| Trang chủ `/` (đã cấu hình `homepage_page_id`) | `Home` → nhánh Page của Homepage (`PageLayout` → `Page` → `Index`) |
| Trang bài viết (đã cấu hình `posts_page_id`) | `Archive("post")` → `Archive` → `Index` (ngữ cảnh archive `post`, không render body Page) |
| Chưa cấu hình Trang chủ / Trang bài viết | `/` trả HTTP `200` core unconfigured safe view (Admin hiện banner hướng dẫn chọn Homepage trong Reading Settings; không tự tạo Page) |
| Not Found (hoặc Homepage/Posts Page bị chuyển `draft`/xóa) | `NotFound` → `Index` với ngữ cảnh `NotFound` (HTTP `404` + `X-Robots-Tag: noindex`, không render bản ghi khác) |

**Phân biệt Missing vs Corrupt/Unsafe:** Thiếu tài nguyên thì sang candidate tiếp theo; tài nguyên hỏng/vi phạm C-SEC không bị bỏ qua im lặng — ghi diagnostic, dùng context-safe core fallback (nếu không render an toàn được thì trả HTTP `503` + `X-Robots-Tag: noindex`, bảo toàn dữ liệu gốc).

### C. Dual Rendering: Visual Builder Overrides (`ast_json`) & C-SEC Theme HTML (`od_part` / `od_content`)

1. **Khi candidate chọn DB Override (Visual Builder `ast_json` — giống WordPress FSE):**
   - `template_overrides.ast_json` chứa `BuilderDocument` (tái sử dụng `storage/media_refs.rs::collect_pins_in_tx` và `template_override_media_refs` hiện hành!).
   - Bổ sung 2 block lõi mới vào `opendash_core::builder::KnownElement` (`#[serde(tag = "type", content = "props", deny_unknown_fields)]`):
     - `KnownElement::ContentSlot(ContentSlotProps)` (`{ "type": "core/content_slot", "props": {} }`): đánh dấu vị trí đổ nội dung bản ghi (editor HTML đã sanitize hoặc Builder AST đã render). Entry template overrides bắt buộc có **đúng 1** `core/content_slot`; `Index` cho phép `0` hoặc `1`; non-entry/part overrides bắt buộc `0`.
     - `KnownElement::TemplatePart(TemplatePartProps)` (`{ "type": "core/template_part", "props": { "part_key": "header" } }`): nhúng một part theo `part_key` (giải quyết `DB part override → active theme part → core fallback` với `header`/`footer`, bỏ qua an toàn nếu part tùy biến của theme không tồn tại). Giới hạn depth ≤ 3, tổng mở rộng part ≤ 32/request.
2. **Khi candidate chọn Theme v3 HTML (`template_sec.rs`, giữ nguyên 100% C-SEC):**
   - **Không bật** `{% include %}`, `{% extends %}`, `|safe` hay `<script>`.
   - **`{{ od_part("<literal>") }}`:** AST validator chỉ cho phép dạng standalone `Stmt::EmitExpr` trong HTML element body (cấm đặt trong `Expr::Filter`, `Stmt::Set`, `Stmt::ForLoop`, HTML attribute, comment, `<style>`). Kiểm tra tĩnh DAG không vòng lặp, max depth = 3, và giới hạn runtime **tối đa 32 lần mở rộng `od_part` / request** dùng chung 1 ngân sách fuel (`10_000`) và dung lượng output (`512 KiB`). Part kế thừa MiniJinja context từ template cha.
   - **`{{ od_content() }}`:** AST validator chỉ cho phép dạng standalone `Stmt::EmitExpr` trong HTML element body (ngoài vòng lặp/attribute/`<style>`):
     - Entry templates (`Home`, `Page`, `PageLayout`, `Single*`, `Singular`): bắt buộc **đúng 1 lần**.
     - `Index`: cho phép **0 hoặc 1 lần**.
     - Non-entry (`Archive*`, `Taxonomy*`, `NotFound`) và `template-part/*`: bắt buộc **0 lần**.
   - **Quy trình render & bảo vệ `<form>` / CSP:** Core sinh một **per-render random sentinel nonce** (128-bit hex) thay cho `od_content()`, render khung MiniJinja thành `shell_html`, chạy `validate_html_and_css_context(&shell_html, true)` trên `shell_html` **trước**, xác nhận nonce xuất hiện đúng 1 lần, rồi mới chèn HTML nội dung bản ghi đã sanitize (có thể chứa `<form>` của Contact Form Builder) và kiểm tra tổng dung lượng ≤ `512 KiB`. Cờ `has_active_contact_form` / media từ nội dung bản ghi được truyền ra response header để bật CSP phù hợp (`rendered_layout_with_form_csp`).

### D. Page Layouts, Custom Single Templates & Template Override API

- Thêm 2 cột trên `content_entries`: `page_layout_key TEXT NULL` (chỉ cho phép khác `NULL` khi `content_type_key = 'page'`) và `custom_template_key TEXT NULL` (chỉ cho phép khác `NULL` khi `content_type_key != 'page'`). Khi lưu entry qua `CompositeEntrySaveInput`, dùng `PatchField<Option<String>>`: `PatchField::Keep` hoặc giá trị trùng DB hiện tại được phép giữ nguyên kể cả khi theme mới thiếu layout/template đó; gán giá trị mới khác `NULL` phải khớp layout/custom template của active theme (nếu sai trả `422 Unprocessable Entity`, nếu gán `page_layout_key` cho non-page trả `400`).
- **API Endpoints (đăng ký trong `cms/crates/opendash-server/src/app.rs`, tái sử dụng `routes::settings::{parse_if_match, no_store_headers}`):**
  - `GET /api/admin/themes/active/layouts` (`require_session`) → `200` `{ active_theme_id, page_layouts: [{ key, label }], custom_templates: [{ custom_key, label, allowed_types }] }` (trả mảng rỗng nếu chưa có theme active).
  - `GET /api/admin/template-overrides` (`require_admin`) → `200` `{ items: [{ id, target_kind, canonical_target_key, target, ast_json, base_theme_id, base_revision_digest, effective_source, version, updated_at }] }`.
  - `PUT /api/admin/template-overrides` (`require_admin_with_csrf` + `AdmittedMutation`) → Body `{ target: TemplateTargetInput, document: BuilderDocument }`. Nếu tạo mới không cần `If-Match` (trả `201` + `ETag: "v1"`), nếu cập nhật bắt buộc `If-Match: "vN"` (`428` nếu thiếu, `412` nếu stale, `200` + `ETag: "vN+1"` khi thành công). Validate `BuilderDocument` + số lượng `core/content_slot` + `core/template_part` DAG trước khi commit.
  - `POST /api/admin/template-overrides/{id}/reset` (`require_admin_with_csrf` + `AdmittedMutation` + `If-Match: "vN"`) → Xóa đúng 1 override mục tiêu trong transaction; trả `200` `{ reset_id, canonical_target_key, next_effective_source: { kind: "active_theme" | "db_override" | "core_fallback", label, target_key } }` (`428` nếu thiếu `If-Match`, `412` nếu stale, `404` nếu không tồn tại).
  - Gỡ bỏ các route override/homepage thử nghiệm cũ trong `app.rs` & `routes/builder.rs` (`/api/admin/builder/overrides*`, `/api/admin/builder/homepage*`), đồng bộ `docs/contracts/M8_WRITER_INVENTORY.md`.

### E. Atomic Reading Settings & Single Route Authority (`permalinks.rs` + `sitemap.rs` + `seo.rs`)

- **API Endpoints (`routes/settings.rs` + `app.rs`):**
  - `GET /api/admin/settings/reading` (`require_admin`) → `200` + `ETag: "vN"` + `{ homepage_page_id: Option<i64>, posts_page_id: Option<i64>, homepage_status: "unconfigured" | "published" | "draft" | "deleted", posts_page_status: "unconfigured" | "published" | "draft" | "deleted", version: i64 }`.
  - `PUT /api/admin/settings/reading` (`require_admin_with_csrf` + `AdmittedMutation` + `If-Match: "vN"`) → Body `{ homepage_page_id: Option<i64>, posts_page_id: Option<i64> }` → `200` + `ETag: "vN+1"` + body giống `GET`. Lỗi: `428` (thiếu `If-Match`), `412` (stale), `400` (trùng 2 ID, không phải `page`, không tồn tại, hoặc chưa `published` — không tự publish).
- **Quy tắc đồng bộ `permalinks::sync_claims_in_tx` (`permalink_route_claims` & `permalink_route_redirects`):**
  1. Nếu `homepage_page_id = Some(id)` và Page `id` đang `published`: claim `/` chuyển thành `("entry", id)` (thay thế `PublicRouteTarget::Home`); slug cũ của Page `id` ghi vào `permalink_route_redirects` (`301 → /`).
  2. Nếu `posts_page_id = Some(id)` và Page `id` đang `published`: bỏ qua việc phát claim `("entry", id)` cho Page đó; chuyển URL canonical của Page đó cho `("archive", "post")`, đồng thời xóa mọi redirect cũ xung đột giữa `("entry", id)` và `("archive", "post")` để **không phát sinh `409 permalink_collision`** khi gán, đổi hoặc bỏ chọn (`None`).
  3. **Xử lý khi Page được gán bị chuyển `draft` hoặc bị xóa (`storage/entries.rs::delete`):** Không dùng `ON DELETE SET NULL`! Giữ nguyên `homepage_page_id` / `posts_page_id` trong `site_reading_settings` (kèm tăng `version = version + 1` khi Page bị xóa) để hệ thống phân biệt rõ trạng thái `deleted`/`draft` (trả `404` + `X-Robots-Tag: noindex`, không lộ draft, không tự chọn Page khác) với trạng thái `unconfigured` (`NULL`, trả `200` safe view ở `/`).
  4. **Sitemap & SEO (`storage/sitemap.rs`, `storage/seo.rs`, `routes/seo.rs`):**
     - `storage/sitemap.rs`: chỉ phát `route_kind = 1` (`/`) khi `homepage_page_id` đang `published` (dùng `lastmod` của Homepage Page, loại `homepage_page_id` và `posts_page_id` khỏi `route_kind = 2` để không lặp `/`).
     - `storage/seo.rs`: `/` ưu tiên `entry_seo` của Homepage Page (fallback `site_home_seo`).
     - `routes/seo.rs`: `missing_public_page()` dùng Resolver 2 trục (`NotFound → Index → core`) giữ nguyên HTTP `404` + `X-Robots-Tag: noindex`.

### F. SQLite Migration (`0042_theme_v3_resolver_and_reading_settings.sql`)

```sql
-- 1. Atomic Reading Settings (no ON DELETE SET NULL to preserve deleted -> 404 noindex state)
CREATE TABLE site_reading_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    homepage_page_id INTEGER NULL,
    posts_page_id INTEGER NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO site_reading_settings (id, homepage_page_id, posts_page_id, version)
VALUES (1, NULL, NULL, 1);

-- 2. Page Layout & Custom Single Template assignments on entries
ALTER TABLE content_entries ADD COLUMN page_layout_key TEXT NULL;
ALTER TABLE content_entries ADD COLUMN custom_template_key TEXT NULL;

-- 3. Typed Template Overrides (Visual Builder AST) — recreated cleanly on target baseline
DROP TABLE IF EXISTS template_override_media_refs;
DROP TABLE IF EXISTS template_overrides;
CREATE TABLE template_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_kind TEXT NOT NULL,
    canonical_target_key TEXT NOT NULL UNIQUE,
    target_json TEXT NOT NULL,
    ast_json TEXT NOT NULL,
    base_theme_id TEXT NULL,
    base_revision_digest TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE template_override_media_refs (
    override_id INTEGER NOT NULL REFERENCES template_overrides(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL,
    ref_role TEXT NOT NULL,
    PRIMARY KEY (override_id, media_id, ref_role)
);
```

---

## Alternatives Considered

| Phương án thay thế | Cơ chế & Đánh đổi | Lý do bác bỏ |
|---|---|---|
| Lưu `template_overrides` bằng HTML MiniJinja thô (`template_source TEXT`) | Bỏ phụ thuộc Builder trên template nhưng buộc Admin phải viết code HTML/Jinja, mất trải nghiệm Full Site Editor. | Chủ dự án chốt mục tiêu giống WordPress Full Site Editor: giữ `ast_json` + block `core/content_slot` & `core/template_part`. |
| Giữ ưu tiên toàn bộ DB overrides trước mọi template theme (M5 cũ) | Đơn giản, chỉ 1 truy vấn DB. | Override `single` chung che khuất `single-project.html` cụ thể của theme. Chốt candidate specificity trước, nguồn sau. |
| Dùng `ON DELETE SET NULL` cho `site_reading_settings` | SQLite tự dọn FK khi xóa Page. | Làm mất phân biệt giữa `deleted` (`404 + noindex`) và `unconfigured` (`200` safe view) và gây silent CAS drift. |
| 9 cột target nullable trên `template_overrides` | Tách từng chiều khóa thành cột riêng. | 7–8 cột `NULL` mỗi dòng, trùng lặp với `canonical_target_key`. Rút gọn còn `(target_kind, canonical_target_key UNIQUE, target_json)`. |

---

## Non-Functional Requirements

- **Kiến trúc & Phân tầng ([CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md)):** `opendash-core` thuần Rust; 100% SQLite queries trong `storage/`; mọi mutation dùng `require_admin_with_csrf` + `AdmittedMutation` ([M8_WRITER_INVENTORY.md](../contracts/M8_WRITER_INVENTORY.md)); Admin gọi API qua `cms/admin/src/api/client.ts`.
- **Bảo mật (`C-SEC` & RBAC):** Không mở rộng allowlist HTML/URL/filter nguy hiểm; per-render random nonce cho `od_content()`; tối đa 32 mở rộng `od_part`/request dùng chung 10_000 fuel & 512 KiB output; lỗi `401`/`412` giữ bản nháp qua `EditorDraftStore`; draft và media private không bao giờ rò rỉ.
- **Độ bền & Backup/Restore:** Theme revision bất biến kèm `.complete` marker (`source_format = 'v3'`); `backup_worker.rs` sao chép đầy đủ revision `v3`; `site_reading_settings`, `page_layout_key`, `custom_template_key` và `template_overrides` được bảo toàn qua restart và `age` backup/restore.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | Reason |
|---|---|---|
| `cms/migrations/0042_theme_v3_resolver_and_reading_settings.sql` | Create | DDL cho `site_reading_settings`, `page_layout_key`, `custom_template_key`, `template_overrides` & `template_override_media_refs`. |
| `cms/crates/opendash-core/src/theme.rs` | Modify | Thêm `ThemeV3Manifest`, `TemplateCandidateKind`, `TemplateCandidate`, `canonical_key()`, `ThemeTemplateDecl`, `PageLayoutDecl`, `ThemePatternDecl`. |
| `cms/crates/opendash-core/src/builder.rs` | Modify | Thêm `KnownElement::ContentSlot(ContentSlotProps)` và `KnownElement::TemplatePart(TemplatePartProps)` (`#[serde(tag = "type", content = "props")]`). |
| `cms/crates/opendash-server/src/template_sec.rs` | Modify | AST validator & compiler cho `od_part("<literal>")` (DAG, depth ≤ 3, fan-out ≤ 32, shared fuel) và `od_content()` (standalone `Stmt::EmitExpr`, per-render nonce, C-SEC check trước khi inject). |
| `cms/crates/opendash-server/src/theme.rs` | Modify | Triển khai `parse_theme_zip_v3` xác thực root HTML + 4 thư mục + C-SEC composition; gỡ nhánh v1/v2 cũ. |
| `cms/crates/opendash-server/src/storage/theme.rs` & `storage/import.rs` | Modify | Lưu/nạp `ThemeV3Manifest` (`source_format = 'v3'`), gỡ `valid_v2_theme_id` (dùng `opendash_core::id::validate_package_id`), cập nhật `storage/import.rs` theo v3. |
| `cms/crates/opendash-server/src/backup_worker.rs` | Modify | Cập nhật `execute_snapshot_worker` để backup/restore `themes/revisions/<digest>` cho `source_format == "v3"`. |
| `cms/crates/opendash-server/src/storage/overrides.rs`, `storage/media_refs.rs`, `storage/public_content.rs` | Modify | Resolver 2 trục (`build_candidates`, `resolve_template_2axis`), `ast_json` overrides theo `canonical_target_key`, reset tính `next_effective_source`, cập nhật `collect_pins_in_tx` & public media ACL cho schema mới. |
| `cms/crates/opendash-server/src/page.rs` | Modify | Render `KnownElement::ContentSlot` & `KnownElement::TemplatePart` (depth ≤ 3, fan-out ≤ 32) và truyền cờ `has_active_contact_form` cho CSP. |
| `cms/crates/opendash-server/src/storage/entries.rs` & `routes/entries.rs` | Modify | Hỗ trợ `page_layout_key`, `custom_template_key` (`PatchField::Keep` bảo toàn khi theme mất layout), và xử lý xóa Page đang gắn trong `site_reading_settings`. |
| `cms/crates/opendash-server/src/storage/settings.rs`, `storage/permalinks.rs`, `storage/sitemap.rs`, `storage/seo.rs` | Modify | `get_reading`/`save_reading` (CAS `If-Match`), chuyển quyền claim `/` & `("archive", "post")` không collision trong `sync_claims_in_tx`, chống lặp `/` trong sitemap & SEO homepage. |
| `cms/crates/opendash-server/src/routes/public.rs`, `routes/public_content.rs`, `routes/seo.rs` | Modify | Dùng Resolver 2 trục + `od_part`/`od_content` + truyền CSP cho `/`, Single, Page, Archive (Posts Page), Taxonomy và 404 (`missing_public_page`). |
| `cms/crates/opendash-server/src/routes/themes.rs`, `routes/settings.rs`, `routes/builder.rs`, `app.rs` | Modify | Đăng ký routes mới trong `app.rs` (`/api/admin/settings/reading`, `/api/admin/themes/active/layouts`, `/api/admin/template-overrides*`, `/reset`), gỡ route override/homepage thử nghiệm cũ ở `routes/builder.rs`. |
| `docs/contracts/M8_WRITER_INVENTORY.md` | Modify | Đồng bộ danh sách HTTP mutation endpoints mới & gỡ endpoints cũ. |
| `cms/admin/src/api/{settings,themes,builder}.ts`, `App.tsx`, `shell/router.ts` | Modify | Typed clients + route/nav cho Reading Settings, Active Layouts và Template Overrides. |
| `cms/admin/src/features/settings/ReadingSettingsPanel.tsx` & `features/system/GeneralSettingsPanel.tsx` | Create / Modify | UI chọn Homepage Page & Posts Page (`If-Match` CAS, hiển thị trạng thái `published`/`draft`/`deleted`/`unconfigured`). |
| `cms/admin/src/features/editor/SingleEditWorkspace.tsx`, `features/library/TemplateLibraryPage.tsx`, `features/themes/ThemesPanel.tsx` | Modify | Select `Page Layout` / `Custom Template` ở sidebar (cảnh báo khi mất layout); quản lý Template Overrides bằng Visual Builder kèm nút Reset hiển thị `next_effective_source`. |
| `cms/crates/opendash-server/src/storage/theme_tests.rs` & test suites | Modify | Cập nhật fixtures sang Theme v3 và bổ sung đầy đủ test cho ma trận nghiệm thu. |

---

### Slice S01: Theme Package v3 Manifest, Core Types & C-SEC Typed Composition (Additive — Zero Compile Break)

* **Dependencies:** None (additive trên `main@e47d5d3`, không phá vỡ callers v2 hiện có).
* **File Ownership:** `cms/crates/opendash-core/src/theme.rs`, `cms/crates/opendash-core/src/builder.rs`, `cms/crates/opendash-server/src/template_sec.rs`, `cms/crates/opendash-server/src/theme.rs`, `cms/crates/opendash-server/src/backup_worker.rs`.
* **Công việc chính:**
  1. Định nghĩa `ThemeV3Manifest`, `TemplateCandidateKind`, `TemplateCandidate::canonical_key()`, `ThemeTemplateDecl`, `PageLayoutDecl`, `ThemePatternDecl` trong `opendash_core::theme`.
  2. Thêm `KnownElement::ContentSlot(ContentSlotProps)` và `KnownElement::TemplatePart(TemplatePartProps)` vào `opendash_core::builder`.
  3. Cài đặt `validate_and_compile_csec_composition()` trong `template_sec.rs` (`od_part("<literal>")` DAG depth ≤ 3, fan-out ≤ 32, shared fuel; `od_content()` standalone `Stmt::EmitExpr`, per-render nonce, C-SEC check trước khi inject).
  4. Cài đặt `parse_theme_zip_v3` trong `theme.rs` (tạm giữ struct v2 đến S02 để workspace compile & test PASS 100%) và cập nhật `backup_worker.rs` hỗ trợ `source_format == "v3"`.
* **Acceptance Criteria:**
  - [ ] Unit tests `opendash_core::theme` xác thực `ThemeV3Manifest` (`manifest_version == 3`, từ chối duplicate `canonical_target_key`, đường dẫn ngoài root/4 thư mục).
  - [ ] Unit tests `template_sec.rs` chặn `od_part` vòng lặp, depth > 3, fan-out > 32, `od_content()` trong loop/attribute hoặc sai số lượng (entry = 1, non-entry = 0, index = 0..=1), và cho phép `<form>` trong nội dung chèn sau nonce.
  - [ ] `backup_worker.rs` bao gồm revision `source_format == "v3"` trong snapshot; `cargo test --workspace --release --locked` PASS.
* **Rollback:** `git revert` commit của Slice S01 (chưa có migration SQLite nên không ảnh hưởng DB hiện hành).

### Slice S02: Migration 0042, 2-Axis Template Resolver, Visual Builder Overrides & Page Layouts

* **Dependencies:** Spec 1 Slice `S01`.
* **File Ownership:** `cms/migrations/0042_theme_v3_resolver_and_reading_settings.sql`, `cms/crates/opendash-server/src/theme.rs`, `cms/crates/opendash-server/src/storage/{theme,import,overrides,entries,media_refs,public_content}.rs`, `cms/crates/opendash-server/src/page.rs`, `cms/crates/opendash-server/src/routes/{themes,entries,public_content,builder,seo}.rs`, `cms/crates/opendash-server/src/app.rs`, `docs/contracts/M8_WRITER_INVENTORY.md`, `cms/admin/src/api/{themes,builder}.ts`, `cms/admin/src/features/editor/SingleEditWorkspace.tsx`, `cms/admin/src/features/library/TemplateLibraryPage.tsx`, `cms/admin/src/features/themes/ThemesPanel.tsx`, `cms/crates/opendash-server/src/storage/theme_tests.rs`.
* **Công việc chính:**
  1. Thêm Migration `0042` và chuyển toàn bộ `theme.rs`, `storage/theme.rs`, `storage/import.rs`, fixtures sang v3 duy nhất (gỡ v1/v2).
  2. Triển khai `build_candidates` + `resolve_template_2axis` trong `storage/overrides.rs` (DB `ast_json` → active theme v3 HTML theo từng candidate), cập nhật `page.rs` render `core/content_slot` & `core/template_part` + truyền cờ CSP, cập nhật `media_refs.rs` & `public_content.rs`.
  3. Hỗ trợ `page_layout_key` & `custom_template_key` (`PatchField::Keep`) trong `storage/entries.rs`, `routes/entries.rs` và `GET /api/admin/themes/active/layouts`.
  4. Triển khai `GET/PUT /api/admin/template-overrides` và `POST /api/admin/template-overrides/{id}/reset` trong `routes/themes.rs` & `app.rs` (gỡ route override cũ ở `routes/builder.rs`), nối UI Admin.
* **Acceptance Criteria:**
  - [ ] Integration tests chứng minh theme-specific (`single-project.html`) thắng DB-generic (`single`), DB `ast_json` cùng candidate thắng theme, `core/content_slot` & `core/template_part` render đúng kèm CSP.
  - [ ] Page Layout (`canvas` không header/footer) & Custom Single Template bảo toàn reference qua `PatchField::Keep` khi đổi theme và fallback an toàn.
  - [ ] `POST /api/admin/template-overrides/{id}/reset` kiểm `If-Match` (`428`/`412`) và trả `next_effective_source` chính xác; `npm run build` và `cargo test --workspace --release --locked` PASS.
* **Rollback:** `git revert` commit của Slice S02 và khôi phục DB fixture trước Migration `0042` (không có dữ liệu production cũ cần migrate ngược).

### Slice S03: Atomic Reading Settings (Homepage & Posts Page), Route Authority, Sitemap/SEO & Admin UI

* **Dependencies:** Spec 1 Slice `S02`.
* **File Ownership:** `cms/crates/opendash-server/src/storage/{settings,permalinks,sitemap,seo,entries}.rs`, `cms/crates/opendash-server/src/routes/{settings,public,public_content,seo,builder}.rs`, `cms/crates/opendash-server/src/app.rs`, `docs/contracts/M8_WRITER_INVENTORY.md`, `cms/admin/src/api/settings.ts`, `cms/admin/src/App.tsx`, `cms/admin/src/shell/router.ts`, `cms/admin/src/features/settings/ReadingSettingsPanel.tsx`, `cms/admin/src/features/system/GeneralSettingsPanel.tsx`.
* **Công việc chính:**
  1. Cài đặt `get_reading` / `save_reading` trong `storage/settings.rs` và `GET/PUT /api/admin/settings/reading` trong `routes/settings.rs` & `app.rs` (`require_admin_with_csrf`, `AdmittedMutation`, `If-Match`).
  2. Cập nhật `permalinks::sync_claims_in_tx` chuyển quyền sở hữu `/` & `("archive", "post")` không collision, xóa Page đang gắn trong `storage/entries.rs::delete` tăng `site_reading_settings.version` (giữ trạng thái `deleted` → `404 + noindex`), đồng bộ `storage/sitemap.rs` & `storage/seo.rs`.
  3. Chuyển `routes/public.rs` sang route authority + Resolver 2 trục (gỡ `/api/admin/builder/homepage*` thử nghiệm cũ), cập nhật `routes/seo.rs::missing_public_page` và hoàn thiện UI `ReadingSettingsPanel.tsx`.
* **Acceptance Criteria:**
  - [ ] `PUT /api/admin/settings/reading` từ chối trùng 2 Page ID, draft, non-page (`400`), kiểm `If-Match` (`428`/`412`); gán/đổi/bỏ chọn `posts_page_id` không phát sinh `409 permalink_collision`.
  - [ ] `/` render Homepage Page qua `home → PageLayout → page`; Posts Page render `archive-post → archive` giữ nguyên `values_json`; xóa/chuyển draft trả `404` + `X-Robots-Tag: noindex`; sitemap không lặp `/`.
  - [ ] `npm run build`, `check-standards.py`, `cargo test --workspace --release --locked` PASS.
* **Rollback:** `git revert` commit của Slice S03 (bảng `site_reading_settings` vẫn giữ sạch ở Migration `0042`).

---

### Verification Plan

1. **Lệnh kiểm chứng chuẩn (bắt buộc PASS độc lập ở từng Slice S01, S02, S03):**
   ```powershell
   python cms/scripts/check-standards.py
   python cms/scripts/check-standards.py --self-test
   python cms/scripts/check-plans.py
   python cms/scripts/check-plans.py --self-test
   cargo fmt --manifest-path cms/Cargo.toml --all --check
   cargo build --manifest-path cms/Cargo.toml --locked
   cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
   git diff --check
   ```
2. **Ma trận kiểm thử hành vi bắt buộc:**
   - **Theme v3 & C-SEC:** Từ chối file không khai báo trong `theme.json`, từ chối `{% include %}`/`|safe`/`<script>`, từ chối vòng lặp `od_part`, depth > 3, fan-out > 32, từ chối `od_content()` trong vòng lặp/attribute hoặc sai số lượng (entry = 1, non-entry = 0, index = 0..=1); Contact Form `<form>` bên trong `od_content()` vẫn render hợp lệ kèm đúng CSP `form-action 'self'`.
   - **Resolver 2 trục & Visual Builder Overrides:** Theme-specific (`single-project.html`) thắng DB-generic (`single`); DB `ast_json` cùng candidate thắng theme; `core/content_slot` & `core/template_part` hoạt động đúng; mất `page_layout_key`/`custom_template_key` giữ reference khi save (`PatchField::Keep`), báo Admin và fallback nhánh thường.
   - **Reading Settings & Route Authority:** Chọn 2 Page khác nhau (từ chối trùng ID, draft, non-page; `428`/`412` CAS); gán/đổi/bỏ chọn `posts_page_id` không gây `409 permalink_collision`; `/` render Homepage qua `home → PageLayout → page`; Posts Page render `archive-post → archive` và giữ nguyên `values_json` Page; xóa hoặc chuyển sang draft trả `404` + `X-Robots-Tag: noindex`; sitemap không lặp `/`.
   - **Reset & Backup/Restore v3:** `POST .../reset` kiểm `If-Match` CAS và trả đúng `next_effective_source`; theme `A → B → A` giữ overrides; `backup_worker.rs` sao chép đầy đủ revision `v3` và restore giữ nguyên `site_reading_settings`, `page_layout_key`, `custom_template_key`, `template_overrides`.


## Design Delta D01 — Explicit static assets (#426)

**Status:** Approved — implementation authority still requires merge and pinned handoff.
**Approved By:** Aster (delegated).
**Approval Evidence:** [PR #433 coordinator decision](https://github.com/thientran2359/opendash-cms/pull/433#issuecomment-7049933198).
**Goldfish Review:** Independent fresh-context `delta_review`; initial two P2 findings resolved; re-review PASS, recorded below.
**Coordinator:** Aster. **Baseline:** `284f000333b32d38629297c0368584761d54d67f`.

### Problem and selected direction

Goal 1 requires explicit declarations of all resources, but the manifest omits general CSS, font and image declarations. Preserve that boundary; do not implicitly allow every file under `assets/`. This delta concerns the unreleased v3 format, not SQLite, public routing, publication or content migration. Existing S01 approval does not approve this delta.

### Exact proposed schema and semantics

Add `#[serde(default)] pub assets: Vec<String>` to `ThemeV3Manifest`. Each string is a unique canonical package-relative file path below `assets/`; an omitted field means an empty set. No glob, directory, URL or executable declaration is accepted. `deny_unknown_fields` remains effective. Example: `"assets": ["assets/styles.css", "assets/images/hero.png"]`.

Canonical asset paths have at most 240 ASCII bytes after `assets/`. Each directory component matches `[a-z0-9_-]+`; the final filename uses only `[a-z0-9_.-]`, starts with a non-dot character, has an allowed lowercase extension, and contains no `..`. Core and server must use the same acceptance grammar, including rejection of `assets/icons.v2/logo.png` and `assets/a..b.png`, and tests must verify boundary agreement. Core validation rejects duplicates, empty components, dot/dot-dot and hidden components, backslashes, absolute paths, control characters and URI syntax; paths use case-sensitive forward slashes without normalization. Retain the package path validator and package extraction protections; server performs byte/existence checks, never I/O in core. Allowed extensions match the current safe package policy: css/txt, png/jpg/jpeg/webp/gif and woff2. No HTML, SVG, JavaScript, Wasm or additional executable type is introduced.

Every declared asset must exist as a regular validated ZIP member. Every ZIP regular file under assets/ must be declared. A pattern preview must name a declared raster image that exists, not merely a path with an allowed prefix. Existing template/part/layout/pattern declarations remain the authority for their own files. Reject duplicate regular ZIP members, traversal and hidden regular-file paths with the package safeguards before constructing a valid package. Empty ZIP directory entries remain subject to the shared reader path-safety validation and are ignored; they are not manifest resources and are not materialized or served. A hidden empty directory entry may be ignored, but any regular file beneath it is rejected by v3 validation. No shared package-reader change is authorized. Existing compressed/uncompressed aggregate, file-count and per-file size limits remain mandatory; manifest declarations never increase limits.

CSS continues through `validate_safe_css` before package acceptance; this delta does not relax CSS URLs, C-SEC, CSP or enable templating of asset bytes. Assets remain inert bytes served by the existing asset route. The manifest is an archive allowlist, not a new HTML/CSS dependency crawler or auto-enqueue feature.

### Detailed Implementation Plan

S01 ownership remains Harbor's existing five implementation files plus the prior bounded `page.rs` ACK. Within `cms/crates/opendash-core/src/theme.rs`, add the field and pure validation with in-file tests. Within `cms/crates/opendash-server/src/theme.rs`, enforce declared/existing asset set equality, preview membership and type, existing safe CSS checks and invoke `parse_theme_declarations` so malformed declarations/field_groups fail before ZIP acceptance; add focused in-file tests. Update S01 fixture literals in these owned files to carry assets as needed. No extra runtime file ownership is granted by this proposal. Any other fixture or caller needs an explicit exact-path handoff before editing.

Aster owns this spec amendment, review/provenance record and design index update. Only after review/approval and merge will Aster pin the new source SHA on #426 and issue the exact schema/file handoff. Harbor preserves existing task commits and dirty worktrees while blocked; no reset or force push.

### Verification Plan

Core tests: missing/empty assets accepted, valid declarations accepted, duplicate/invalid paths and unsupported extensions rejected. ZIP tests: declared CSS/image/font accepted, missing declaration rejected, missing declared file rejected, undeclared/hidden/traversal/duplicate regular ZIP members rejected, safe empty directory entries ignored with hidden child regular files rejected, preview missing or non-raster rejected, unsafe CSS and over-limit members rejected, malformed declarations/field_groups rejected. Preserve v2 behavior during additive S01. Run standards/plans checks and self-tests, artifact validation, fmt, locked build and full workspace release tests, plus all required CI at implementation head. This proposal records no tests as PASS before execution.

### Rollback and completion

Before implementation, revert only the documentation delta. After implementation, revert the bounded code changes through a protected PR; do not delete site data. No migration is introduced. D01 is complete only after independent EGM review findings are resolved, delegated approval is recorded, required CI passes, the delta merges and the exact source/schema/ownership handoff is posted on #426. Implementation verification remains separate.


### D01 independent review and disposition — 2026-11-21

Independent fresh-context reviewer `delta_review` read this amendment against the actual core/server theme validators and package-reader boundary. Initial verdict CHANGES_REQUESTED: two P2 findings, core/server path grammar mismatch and directory entries bypassing the v3 callback. Both were resolved by specifying the identical ASCII/length grammar and explicitly preserving safe empty directory entries while rejecting hidden regular files. Re-review verdict PASS: no remaining actionable P0/P1/P2. This review does not assert implementation verification.

Actual documentation validation: check-plans PASS; check-plans self-test 13 fixtures PASS; check-agent-artifacts PASS; standards and self-test PASS using Python 3.9 with a temporary pip-vendored tomli compatibility module, including child processes; diff whitespace PASS. CodeGraph indexing attempted but unavailable because the engine is not installed in this worktree. No CMS runtime files changed, and Rust/runtime/native tests were not run for this documentation-only delta. Required remote CI and delegated approval remain merge preconditions.


## Proposed D02 coordination amendment

See [S02 target-only Page Builder and provider selections](s02-target-only-page-builder.md). While Draft, this is a review proposal, not implementation authority. It brings the Page-only entry document foundation into Theme S02/0042; later CPT S01/0043 extends those tables rather than recreating them. Owner clarified that no previous release or historical-data compatibility is required. Generic CPT, Patterns and Reading Settings remain in their existing later slices.
