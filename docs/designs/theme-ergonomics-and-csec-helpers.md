# Đặc tả Thiết kế EGM — Spec 1: Theme Ergonomics, C-SEC Pure Helpers, Rich View Context & `--check-theme` CLI

- **Status:** Approved
- **Approved By:** Owner
- **Approval Evidence:** Owner chat decisions 1A-4A & directive 2026-11-24 (Issue từng Slice cho Nova)
- **Goldfish Review:** 9-round zero-context Goldfish review (egm-design, egm-codebase-details, egm-implementation-readiness) · 2026-11-24 · APPROVED
- **Date:** 2026-11-24
- **Author:** Orion (Elephant)
- **Thuộc Lộ trình:** Giai đoạn 1 trong Tầm nhìn 6 Giai đoạn (Decisions `1A`, `4A`: pin `jiff = "=1.0.2"` tại `opendash-server` và tách Spec 1 riêng)
- **Thành phần chạm tới:** `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-server/Cargo.toml`, `cms/migrations/0049_entry_published_at.sql`, `cms/migrations/README.md`, `cms/crates/opendash-server/src/{main,theme,template_sec,content_render,document,page}.rs`, `cms/crates/opendash-server/src/storage/{builder,entries,entry_table,menu,overrides,permalinks,public_content,seo,settings,theme}.rs`, `cms/crates/opendash-server/src/routes/{content,entries,public_content}.rs`, `cms/crates/opendash-server/tests/{content_foundation_m6_s11_t01.rs,studio_s02_preview_api.rs,theme_ergonomics_test.rs}`, `docs/designs/SITEMAP.md`

---

## Overview

Đặc tả này nâng cấp khả năng lập trình **Theme v3 & Template Overrides** trên OpenDash CMS mà **không nới lỏng bất kỳ biên bảo mật C-SEC nào** (giữ nguyên auto-escape HTML, `UndefinedBehavior::Strict`, fuel `10_000`, recursion `16`, output cap `512 KiB`, cấm `Expr::Call`, cấm `<script>`/`<form>`/`|safe` trong Theme).

Ba nhóm năng lực cốt lõi được cung cấp trong Spec 1:
1. **Bộ C-SEC Bounded Filters & `ALLOWED_TESTS`:** Thêm bộ lọc định dạng ngày giờ chuẩn múi giờ site (`date_format` qua crate `jiff = "=1.0.2"` với IANA tzdb nhúng), cắt chuỗi an toàn UTF-8 (`truncate`), định dạng số/tiền tệ (`format_number`), xử lý tập hợp có giới hạn chống OOM (`batch`, `selectattr`, `rejectattr`, `map` dùng `Rest<ValueOrKwargs>` + `from_args`), và thi hành whitelist 21 `ALLOWED_TESTS` tại AST validation.
2. **Tài nguyên `theme_assets`, Giải mã Entity ở `document.rs` & Máy trạng thái AST + HTML Toàn diện (`walk_theme_ast_with_url_scopes`):** Cung cấp bản đồ URL tài nguyên tĩnh `theme_assets` gắn `revision_digest`; decode HTML entities (`&#x2f;`, chặn control chars) trong `document.rs::is_authorized_stylesheet_link` cùng kiểm tra tên thẻ chữ cái ASCII trong `document.rs::tokenize` chống bogus-comment desync; thay thế `ALLOWED_DYNAMIC_URL_VARS`, scanner tuyến tính pre-render và `ensure_composition_body_context`/`ensure_composition_body_sequence` (giữ nguyên `enum HtmlScanState`) bằng `walk_theme_ast_with_url_scopes` (chặn `<?` & `</[^a-zA-Z]` & `</style >`/`</title >` & `EmitExpr` trong comment & khối điều khiển trong `<style>`, kiểm URL tĩnh & `<meta>` chặt, có pending states `BodyLt`/`BodyLtSlash`/`BodyLtBang`/`Doctype`, 6-state `CommentScanSubState`, hội tụ đồng thời `(states, UrlScopeStack)` trên `ForLoop`, phép giao nhánh `IfCond`, tách quyền `HrefOnly` vs `SrcAndHref` bảo toàn `GLOBAL_PUBLIC_URL_OPTION_KEYS` trong cả `theme_url_policy` & `global_theme_url_policy`), đồng thời thi hành `reject_reserved_context_rebinding` vô điều kiện + kiểm `entry` rebinding trực tiếp trên `UrlScopeStack`.
3. **Rich Public & Preview View Context + CLI `--check-theme`:** Bổ sung `content_entries.published_at` qua migration `0049_entry_published_at.sql` (+0 object `sqlite_schema`, sau `0047` và `0048`), làm giàu `EntryView` từ `EffectiveContentSchema` theo đúng thứ tự khai báo (`effective_fields`), nâng cấp `MenuItemView` (`is_active`, bottom-up `is_ancestor`, `is_external`, `depth`) với batch SQL `O(1)` hết N+1, thêm `PaginationView`, `breadcrumbs`, `SiteContext` (`origin`, `timezone`, `current_year`), semantic body classes ghép gộp, lazy opt-in `related_entries`/`recent_posts` quét đệ quy mọi bề mặt C-SEC (`ActiveTheme`, `DbOverride`, `header`/`footer` DB override-first, nested `od_part`) và truyền qua `ResolvedPublicHtml`, đồng bộ ngữ cảnh + placeholder an toàn cho Starter `taxonomy.html`/`single.html` dưới `UndefinedBehavior::Strict`, cập nhật admin preview routes (`routes/content.rs`, `routes/entries.rs`), và lệnh offline `opendash --check-theme <path>`.

---

## The Problem

Qua kiểm chứng trên theme chuẩn và mã nguồn `template_sec.rs` / `document.rs` / `content_render.rs` / `storage/{entries,menu,overrides,public_content,seo,settings}.rs`, lập trình viên Theme đang gặp 6 điểm nghẽn lớn:
1. **Không thể định dạng ngày/giờ & số tiền:** `entry.updated_at` là chuỗi UTC SQLite thô; không có filter `date_format` tôn trọng `GeneralSettings.timezone` (ví dụ `Asia/Ho_Chi_Minh`) và không có `format_number`/`truncate`.
2. **Thiếu dữ liệu bài viết, taxonomy & điều hướng cốt lõi:** `EntryView` thiếu `published_at`, `terms`, `primary_category`, `excerpt`, `reading_time_minutes` (và `entry_view_with_url` bỏ qua các key `null`, làm `UndefinedBehavior::Strict` lỗi HTTP 500 trên optional fields); không có `breadcrumbs`; phân trang chỉ có `previous_url`/`next_url`; `MenuItemView` thiếu `is_active`/`is_ancestor` và phát sinh N+1 query cùng với `query_entries_filtered`, `seo::resolve_public_entry_paths_by_ids` và việc gọi `resolve_active_slot_menus` nhiều lần mỗi request.
3. **Danh sách URL trong C-SEC bị khóa cứng 9 biến & scanner HTML tuyến tính thiếu nhận biết nhánh:** `ALLOWED_DYNAMIC_URL_VARS` (`template_sec.rs:522-532`) từ chối `term.url`, `crumb.url`, `page.url`, `entry.primary_category.url` hay `post.url`; `reject_options_rebinding` bỏ qua khi `permitted_href_expressions` rỗng; `parse_attributes` & `find_html5_comment_end_in_template` quét tuyến tính nên gãy trước thuộc tính có điều kiện (`{% if %}class=...{% endif %}`) và tiềm ẩn lệch nhánh qua `{% if %}<!--{% endif %}`; `HtmlScanState::Tag.prefix` cắt 7 byte nên không phân biệt đủ tên thẻ/thuộc tính.
4. **Lỗ hổng tiềm ẩn khi mở rộng filter/test & Preview placeholders:** `validate_expr(Expr::Test)` chưa kiểm tra whitelist tên test; `preview_studio_with_cache` dựng placeholder `"fields": {}` sai kiểu (`HashMap` thay vì `Vec<FieldView>`), thiếu `K::Taxonomy` non-null (`opendash-starter/taxonomy.html` dùng `{{ taxonomy.name }}`) và thiếu `archive_url` (`opendash-starter/single.html` dùng `{{ archive_url }}` không kiểm `{% if %}` trên `Single`/`SingleCustom`/`Singular`/`Index`); `preview_typed_override_with_cache` và `resolve_and_render_header_footer_with_cache` thiếu biến ngữ cảnh trang (`route`, `breadcrumbs`, `pagination`, `related_entries`, `recent_posts`) dẫn tới `UndefinedError` 500; `resolve_preview_entry` đang hardcode `FieldRole::Admin`.
5. **Khó trỏ tài nguyên tĩnh trong Theme v3 ZIP & `AutoEscape::Html` làm rớt `<link>`:** `theme_asset_url` không mang theo key tài nguyên cụ thể, `<link rel="stylesheet">` bị chặn nếu `href` dùng biến động `{{ theme_assets[...] }}`, dấu `"` trong `theme_assets["..."]` cắt chuỗi parser HTML, và `AutoEscape::Html` encode `/` thành `&#x2f;` khiến `document.rs::is_authorized_stylesheet_link` (`trimmed.starts_with('/')`) vô tình loại bỏ thẻ `<link>` khi đóng gói HTML.
6. **Chưa có công cụ kiểm tra gói Theme trước khi upload:** Lỗi thiếu file trong `assets[]`, sai `theme.json`, hay vi phạm C-SEC chỉ phát hiện khi admin upload ZIP lên server.

---

## Goals

1. Cung cấp bộ 7 filter thuần C-SEC có giới hạn cứng (`date_format` theo `jiff = "=1.0.2"`, `truncate`, `format_number`, `batch`, `selectattr`, `rejectattr`, `map`) và thực thi 21 `ALLOWED_TESTS` tại compile-time validation.
2. Cho phép Theme truy cập tài nguyên đã khai báo trong `theme.json -> assets[]` qua `{{ theme_assets["assets/..."] }}` hoặc `{{ theme_assets['assets/...'] }}` với URL `/theme/<revision_digest>/<path>` bất biến, hỗ trợ `<img src>`, `<a href>` và `<link rel="stylesheet" href>` (chỉ `.css`, qua được cả C-SEC pre/post-render lẫn `document.rs::normalize_public_document`).
3. Thay `ALLOWED_DYNAMIC_URL_VARS`, scanner tuyến tính (`post_render == false`) và `ensure_composition_body_context`/`ensure_composition_body_sequence` (giữ `enum HtmlScanState`) bằng `walk_theme_ast_with_url_scopes` chạy trên cả 3 điểm vào (`validate_template_csec_with_theme_urls`, `composition_sites:832`, `preview_composition_sites:283`), khóa chặt HTML5 bogus-comment (`<?`, `</[^a-zA-Z]`), RCDATA/RAWTEXT end-tag (`</title >`, `</style >`), `EmitExpr` trong comment, control flow trong `<style>`, URL tĩnh, `<meta>` allowlist, các trạng thái prefix `BodyLt`/`BodyLtSlash`/`BodyLtBang`/`Doctype`, đủ 6 trạng thái comment HTML5, hội tụ đồng thời `(states, UrlScopeStack)` trên `ForLoop`, phép giao nhánh `IfCond`, bảo toàn `GLOBAL_PUBLIC_URL_OPTION_KEYS` trong cả `theme_url_policy` & `global_theme_url_policy`, và kiểm rebinding `entry` + root keys ngay trên `UrlScopeStack`.
4. Làm giàu ngữ cảnh truyền vào mọi template Public & Preview (`EntryView` + `EffectiveContentSchema` theo declaration order + `strip_richtext_to_plain`, per-CPT effective taxonomy eligibility, `MenuItemView`, `PaginationView`, `BreadcrumbItem`, `SiteContext`, body classes gộp dedup, lazy `related_entries`/`recent_posts` quét đệ quy bề mặt C-SEC) theo chuẩn **`O(1)` truy vấn SQL (Zero N+1)** và **+0 object `sqlite_schema`**.
5. Đảm bảo `UndefinedBehavior::Strict` không bao giờ gây 500 trên `entry.values.*` rỗng, `404`, `header`/`footer` parts, `preview_studio_with_cache` (bao gồm `opendash-starter` `taxonomy.html` & `single.html` cho `Single`/`SingleCustom`/`Singular`/`Index`), `preview_typed_override_with_cache`, `wrap_entry_body_in_preview_shell_in_tx_with_budget`, `content_entry_preview` hay `entry_preview`.
6. Bổ sung `opendash --check-theme <dir|zip>` (`=` hoặc tách tham số qua `std::env::args_os()` cho mọi flag trong `main()`) chạy offline trước `SiteCoordinator::acquire` / DB init, kiểm tra pre-read chống FIFO/symlink/case-collision/độ sâu thư mục, in JSON chuẩn lên `stdout` cho cả exit `0` và `1`.

## Non-Goals

- Không nới lỏng C-SEC để cho phép `Expr::Call(_)` (gọi hàm tùy ý), `{% macro %}`, `{% include %}`, `|safe`, `<script>`, `<svg>`, hay `<form>` ngoài cơ chế nonce sẵn có.
- Không hỗ trợ dot-access `theme_assets.key` (asset path chứa `/` và `.` không phải định danh MiniJinja hợp lệ; chỉ hỗ trợ `theme_assets["assets/..."]` / `theme_assets['assets/...']`) và không hỗ trợ `options['key']` trong thuộc tính URL (`options.<key>` giữ dot-only).
- Không thêm filter `slice` (C-SEC đã hỗ trợ `Expr::Slice` `items[:5]`), không thêm filter `excerpt` hoặc `reading_time` trùng với trường tính sẵn trên `EntryView`.
- Không tính `site.logo_url` trùng với `options.site_logo`; không tự gán `is_active = true` cho `target_kind == "external"` trên Menu.
- Không chỉnh sửa 2 bản Starter nhúng `opendash-starter` (`1.0.0`) và `opendash-starter-1.0.1` (`1.0.1`) trong Spec 1 để giữ nguyên 100% canonical revision digest khởi động và tương thích nâng cấp với `1.1.0` của đặc tả Đa ngôn ngữ.
- Không đưa Plugin Hooks, Host Ops, Plugin Settings, hay `json_ld` plugin projection vào Spec 1 (thuộc Spec 2).
- Không thay đổi ranh giới Theme blocks (`blocks/<slug>/render.html`): block tiếp tục dùng schema riêng (`validate_block_template_csec`), không phụ thuộc global `theme_assets` để bảo toàn khi đổi Theme.

---

## Critical User Journeys (CUJs)

1. **CUJ-1 — Định dạng ngày tiếng Việt theo múi giờ site, `ContentFieldType::Date`, & giá/số:**
   - *Given* site đặt múi giờ `Asia/Ho_Chi_Minh` trong `site_general_settings`, bài viết có `published_at = "2026-11-23T18:30:00Z"`, field tùy chọn `event_date = "2026-11-24"` (`Date`), `price = 1250000`, và một field tùy chọn chưa nhập (`null`).
   - *When* template viết `{{ entry.published_at | date_format("%d/%m/%Y %H:%M") }}`, `{{ entry.values.event_date | date_format("%d/%m/%Y") }}`, `{{ entry.values.price | format_number(0, ".", ",") }} đ` và `{{ entry.values.optional_note | default("") }}`.
   - *Then* HTML render ra `"24/11/2026 01:30"`, `"24/11/2026"` (không bị lùi ngày dù đổi múi giờ âm), `"1.250.000 đ"` và không gặp `UndefinedError` 500 trên `optional_note` rỗng.

2. **CUJ-2 — Render bài viết đầy đủ chuyên mục, thẻ, thời gian đọc, tóm tắt, Breadcrumbs & bài liên quan:**
   - *Given* bài viết `post` đã xuất bản gắn chuyên mục chính *Kỹ thuật*, thẻ *Rust*, trường custom trong Field Group (`public_exposure = Public`, `min_view_role = Editor`), trường `internal_secret` (`min_view_role = Admin`, `public_exposure = Private`), một taxonomy bị vô hiệu hóa qua `content_type_taxonomy_overrides`, và nội dung dài `body`.
   - *When* khách truy cập URL bài viết qua Theme v3 dùng `breadcrumbs`, `entry.primary_category.url`, `entry.terms.category`, `entry.terms.tag`, `entry.reading_time_minutes`, `entry.excerpt` và `related_entries`.
   - *Then* trang hiển thị đúng link chuyên mục/thẻ/breadcrumbs hợp chuẩn C3; term thuộc taxonomy đã tắt trên CPT không bị rò rỉ; `entry.excerpt` được strip sạch thẻ HTML (không có `<p>`/`<a>`) và cùng `reading_time_minutes` bỏ qua mọi trường `Private` hoặc `Admin`-only; `related_entries` trả tối đa 6 bài cùng CPT có chung ít nhất 1 term công khai hợp lệ, ưu tiên bài mới xuất bản nhất.

3. **CUJ-3 — Navigation nổi bật mục đang đứng, conditional attributes & phân trang đánh số:**
   - *Given* Menu `primary` có mục cha *Sản phẩm* (`/san-pham`) chứa mục con *Phần mềm* (`/phan-mem`), một mục *Trang chủ* (`/`), một link ngoài `https://example.com`, template viết `<a {% if item.is_active %}class="active"{% else %}class="idle"{% endif %} href="{{ item.url }}">`, và trang Archive có 65 bài viết (`ARCHIVE_SIZE = 20`).
   - *When* khách xem `/phan-mem` hoặc `/posts?page=2`.
   - *Then* thuộc tính có điều kiện hai nhánh hợp lệ tại C-SEC; mục *Phần mềm* có `is_active = true, is_ancestor = false`, mục cha *Sản phẩm* có `is_active = false, is_ancestor = true` (mục Trang chủ `/` và link ngoài đều `is_active = false, is_ancestor = false`); thanh phân trang render qua `pagination.pages` với trang 2 có `is_current = true` và link trang 1 không kèm `?page=1`.

4. **CUJ-4 — Nhúng logo/CSS của gói Theme v3 bằng `theme_assets`:**
   - *Given* `theme.json` khai báo `"assets": ["assets/style.css", "assets/brand.png"]` ở revision `rev_abc123`.
   - *When* `header.html` viết `<link rel="stylesheet" href="{{ theme_assets["assets/style.css"] }}">` và `<img src="{{ theme_assets['assets/brand.png'] }}" alt="Logo">`.
   - *Then* C-SEC kiểm tra path thuộc `manifest.assets` ngay khi cài đặt Theme v3 (`400 Bad Request` tại `POST /api/admin/themes` nếu thiếu khai báo hoặc đuôi không phải `.css`) và khi compile/preview Studio (`overrides.rs` trả `400`, active theme hỏng runtime trả `500 ApiError::internal`); dù `AutoEscape::Html` xuất `href="&#x2f;theme&#x2f;rev_abc123&#x2f;assets&#x2f;style.css"`, `document.rs` giải mã entity trước khi duyệt nên thẻ `<link>` được đưa nguyên vẹn lên `<head>` HTML cuối. Asset `.txt` nằm trong `assets[]` là metadata-only (không được `routes/themes.rs:585` cấp MIME phục vụ HTTP).

5. **CUJ-5 — Kiểm tra gói Theme trước khi đóng gói bằng CLI:**
   - *Given* developer có thư mục `./my-theme` (hoặc file `./my-theme.zip`).
   - *When* chạy `./opendash --check-theme ./my-theme` (hoặc `--check-theme=./my-theme.zip`).
   - *Then* tiến trình chạy tức thì trước khi chạm DB/locks, phát hiện symlink/hidden file/va chạm hoa-thường Windows/thư mục lồng sâu `> 16` cấp, chạy đầy đủ `validate_theme_v3_files`, in 1 dòng JSON chuẩn lên `stdout` với các mảng chuỗi `String` đã sort, `theme_id` từ `manifest.theme_id` và `options_count: N` (exit `0`), hoặc `{ "ok": false, "error": "..." }` lên `stdout` (exit `1`).

---

## Technical Plan (Architecture)

### 1. C-SEC Bounded Pure Filters, `ALLOWED_TESTS` & `jiff = "=1.0.2"`

#### 1.1 Dependency (`cms/Cargo.toml` & `cms/crates/opendash-server/Cargo.toml`)
- Thêm vào `[workspace.dependencies]` trong `cms/Cargo.toml` và `[dependencies]` trong `cms/crates/opendash-server/Cargo.toml`:
  ```toml
  jiff = { version = "=1.0.2", default-features = false, features = ["std", "tzdb-bundle-always", "perf-inline"] }
  ```
- **Ràng buộc kiến trúc:** `cms/crates/opendash-core/Cargo.toml` **tuyệt đối không** phụ thuộc `jiff` để giữ nguyên bất biến zero-I/O tại `docs/contracts/CORE_FOUNDATION.md`.
- `tzdb-bundle-always` nhúng sẵn `jiff-tzdb` vào binary nên định dạng ngày giờ & DST hoạt động đồng nhất trên macOS, Windows và Linux musl. Khi bắt đầu code S01, sau khi thêm dependency vào `Cargo.toml`, chạy `cargo build --manifest-path cms/Cargo.toml` (hoặc `cargo generate-lockfile --manifest-path cms/Cargo.toml`) để ghi `jiff = "=1.0.2"` vào `cms/Cargo.lock`, rồi chạy toàn bộ kiểm chứng với `--locked`.

#### 1.2 `RenderFormattingConfig` & Call Sites (`template_sec.rs`, `storage/builder.rs`, `storage/overrides.rs`, `page.rs`, `theme.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFormattingConfig {
    pub timezone: String,
}

impl Default for RenderFormattingConfig {
    fn default() -> Self {
        Self { timezone: "UTC".to_owned() }
    }
}
```
- **`template_sec.rs`:**
  - `restricted_environment()` giữ nguyên làm wrapper gọi `restricted_environment_with_config(&RenderFormattingConfig::default())`.
  - Trong `restricted_environment_with_config`:
    - Bọc `"default"` và `"d"` bằng `filter_default(state: &State, value: &Value, args: Rest<Value>) -> Result<Value, Error>` (theo `minijinja-3.0.5/src/filters.rs:681-697`, đổi điều kiện thành `value.is_undefined() || value.is_none() || (lax && !value.is_true())`) để `{{ entry.values.optional_note | default("") }}` và `| default("N/A")` thay thế sạch `Value::Null` (`none`).
    - Đăng ký `env.set_formatter(|out, state, value| if value.is_none() { Ok(()) } else { minijinja::escape_formatter(out, state, value) });` (`minijinja-3.0.5/src/environment.rs:546, 817-835` — `UndefinedBehavior::Strict` kiểm `Undefined` thật *trước* khi gọi formatter, nên render trực tiếp `{{ entry.values.optional_note }}` xuất `""` thay vì `"none"` trong khi `Undefined` vẫn báo lỗi và `is none`/`is defined` không đổi).
  - `pub fn render_csec_with_config(name: &str, source: &str, context: Value, fmt_cfg: &RenderFormattingConfig) -> Result<String, &'static str>` bọc luồng compile `validate_template_csec_with_theme_urls` + `restricted_environment_with_config(fmt_cfg)` + gọi hàm nội bộ `validate_html_and_css_context(&rendered, true)` (`template_sec.rs:1667`); giữ `render_csec` (`template_sec.rs:622-638`) gọi xuống với `&RenderFormattingConfig::default()`.
  - `CompiledCsecComposition::render_with_config_and_remaining_fuel(&self, context: Value, sanitized_content_html: Option<&str>, fuel_remaining: &mut u64, fmt_cfg: &RenderFormattingConfig) -> Result<String, &'static str>` (giữ `render_with_remaining_fuel` gọi `&RenderFormattingConfig::default()`).
  - `render_preview_csec_fragment_with_theme_urls_and_config(name: &str, source: &str, context: Value, theme_sources: &BTreeMap<String, String>, trusted_builder_parts: &BTreeMap<String, String>, budget: &mut PreviewCompositionBudget, policy: &ThemeUrlPolicy, fmt_cfg: &RenderFormattingConfig) -> Result<String, &'static str>` (giữ `render_preview_csec_fragment_with_theme_urls` tại `template_sec.rs:319` làm wrapper truyền `&RenderFormattingConfig::default()`).
  - `render_block_csec_with_options_and_budget_and_config(block_id: &str, template_source: &str, schema: &opendash_core::block_schema::BlockSchemaV1, values: &BTreeMap<String, serde_json::Value>, site_options: Option<&BTreeMap<String, serde_json::Value>>, is_preview: bool, budget: &mut RenderBudget, fmt_cfg: &RenderFormattingConfig) -> Result<String, &'static str>` (được định nghĩa tại `template_sec.rs:3012`; giữ `render_block_csec_with_options_and_budget` làm wrapper).
- **`storage/builder.rs` (`HomeState`, dòng `47-58`):**
  - Thêm `pub timezone: String` vào `HomeState`; thay `#[derive(Default)]` bằng `impl Default for HomeState` khởi tạo `timezone: "UTC".to_owned()` (tránh chuỗi rỗng `""` ở các test fixture `page.rs:2094, 3070, 3116`).
  - Trong `read_home_tx` (`builder.rs:334-397`), đọc `timezone` qua `settings::get_general_in_conn(&mut **tx).await?.timezone` để gán vào `HomeState.timezone`.
  - Bổ sung tham số `timezone_override: Option<&str>` vào `builder::preview_saved_page_body_in_tx_with_options_and_budget` (`storage/builder.rs:1713-1747`): sau khi áp dụng `site_options_override`, gán `if let Some(tz) = timezone_override { state.timezone = tz.to_owned(); }` (cập nhật 3 điểm gọi: `storage/builder.rs:1707` truyền `None`, `storage/overrides.rs:1563` và `storage/overrides.rs:2566` truyền `Some(general.timezone.as_str())`) để Builder block trong thân bài preview Studio tôn trọng `transient.general.timezone` chưa lưu.
- **`page.rs`:**
  - `render_plugin_block_csec_with_budget_and_config(block_id: &str, template_source: &str, values: &std::collections::BTreeMap<String, serde_json::Value>, budget: &mut RenderBudget, fmt_cfg: &RenderFormattingConfig) -> ApiResult<String>` (giữ `render_plugin_block_csec_with_budget` tại `page.rs:1979` làm wrapper).
  - Đường render Builder block trong `page.rs:1320, 1365` truyền `&RenderFormattingConfig { timezone: state.timezone.clone() }` vào cả 2 hàm `_and_config` trên.
- **`theme.rs` & `storage/overrides.rs`:**
  - `theme::render_content_with_config(template, data, fmt_cfg)` (`theme.rs:141-145`) gọi trực tiếp `template_sec::render_csec_with_config` (giữ `render_content` gọi `Default`).
  - `overrides::PublicCompositionBudget::render_theme_with_config(&mut self, compiled: &CompiledCsecComposition, context: &serde_json::Value, slot: Option<&str>, fmt_cfg: &RenderFormattingConfig) -> ApiResult<String>`.
  - Bổ sung `pub fmt_cfg: &'a RenderFormattingConfig` vào cả `PreviewRenderContext<'a>` (`overrides.rs:1241`) và `PublicBuilderContext<'a>` (`overrides.rs:1760`); cập nhật các điểm khởi tạo trong `overrides.rs` (`L1598, L1941, L1998, L2374, L2401, L2661, L2703, L2898`, trong đó `render_preview_named_part_in_tx` ở `overrides.rs:2355-2412` dựng trực tiếp `&RenderFormattingConfig { timezone: state.timezone.clone() }` từ tham số `state: &HomeState` hiện có tại `L2374`/`L2401`).
  - Truyền `fmt_cfg: &RenderFormattingConfig` vào `render_csec_template_for_preview` (`overrides.rs:2414-2448`, được gọi từ `L2739` và `L2924`) → `budget.render_theme_with_config(..., fmt_cfg)`, cũng như vào `template_sec::render_preview_csec_fragment_with_theme_urls_and_config` (`overrides.rs:1364`) và `render_public_part` → `budget.render_theme_with_config` (`overrides.rs:1842`). Khi Studio có `TransientStudioOverrides.general` (`overrides.rs:2508`), đồng bộ `state.timezone = general.timezone.clone()` sau `read_home_tx` và truyền `Some(general.timezone.as_str())` vào `preview_saved_page_body_in_tx_with_options_and_budget` (`L1563`, `L2566`). Ngoài ra, chuyển lời gọi fallback `content_render::render_single_with_site_and_archive` tại `overrides.rs:1585-1587` (`preview_typed_override_with_cache`) và `overrides.rs:2587-2589` (`preview_studio_with_cache`) sang `content_render::render_single_with_context(..., &fmt_cfg)`, và dựng `part_context` trong `resolve_and_render_header_footer_with_cache` (`overrides.rs:1992-1994`) qua `content_render::build_public_theme_context`.

#### 1.3 Đặc tả Chính xác 7 Filters & 21 `ALLOWED_TESTS` (`template_sec.rs`)

> **Quy tắc chung:** Mọi filter mới kiểm tra `value.is_undefined() || value.is_none()` trước tiên (string filters trả `""`; sequence filters `batch`, `selectattr`, `rejectattr`, `map` trả `[]`), có giới hạn cứng chống OOM/DoS (cấm `.collect()` không giới hạn), và giữ nguyên kiểm tra `check_block_expr_restrictions` trên biến URL/richtext nhạy cảm của Block.

1. **`date_format(value: Value, fmt: Option<&str>, tz: Option<&str>) -> Result<Value, Error>`**
   - Nếu `value.is_undefined() || value.is_none()` → trả `Ok(Value::from(""))`. Nếu `value.kind() != ValueKind::String` → trả `Err(Error::new(InvalidOperation, ...))` (`FilterError`). Nếu chuỗi input `> 128` bytes → trả `FilterError`; nếu `s.trim().is_empty()` → trả `Ok(Value::from(""))`.
   - `fmt`: mặc định `"%d/%m/%Y"`; nếu truyền `Some("")` hoặc `> 64` bytes → `FilterError`.
   - `tz`: nếu truyền `Some(t)` và `!t.trim().is_empty()`, nếu `t.len() > 64` hoặc `jiff::tz::TimeZone::get(t.trim())` lỗi → `FilterError`. Nếu `None` hoặc rỗng → `TimeZone::get(fmt_cfg.timezone.as_str()).unwrap_or(TimeZone::UTC)`.
   - *Phân tích thời gian trên `jiff = "=1.0.2"`:*
     1. **Pure `ContentFieldType::Date` (`"YYYY-MM-DD"`, đúng 10 bytes ASCII):** `if s.len() == 10 && s.is_ascii()` rồi `if let Some(dt) = jiff::civil::Date::strptime("%Y-%m-%d", s).ok().map(|d| d.at(0, 0, 0, 0))` → format trực tiếp `jiff::fmt::strtime::format(fmt, dt)` **không đổi múi giờ** (tránh lùi ngày ở múi giờ âm và chặn `"2026-1-5"`; `fmt` chứa `%Z`/`%z` trên pure civil `DateTime` trả lỗi từ `strtime::format` và map về `FilterError`).
     2. **RFC 3339 / ISO 8601 có offset/`Z`:** `if let Ok(ts) = s.parse::<jiff::Timestamp>()` → `let zdt: jiff::ZonedDateTime = ts.to_zoned_datetime(target_tz);` → `jiff::fmt::strtime::format(fmt, &zdt)`.
     3. **Naive SQLite / ISO datetime (`"YYYY-MM-DD HH:MM:SS"`, `"YYYY-MM-DDTHH:MM:SS"`, có thể kèm `.fff`):** `if let Ok(naive) = s.parse::<jiff::civil::DateTime>()` → `let utc_zdt = naive.to_zoned_datetime(jiff::tz::TimeZone::UTC).map_err(...)?;` → `let zdt = utc_zdt.with_time_zone(target_tz);` → `jiff::fmt::strtime::format(fmt, &zdt)`.
     - Chuỗi không khớp cả 3 dạng trả `Ok(Value::from(""))`; lỗi từ `jiff::fmt::strtime::format` hoặc `to_zoned_datetime` trả `FilterError`.

2. **`truncate(value: Value, max_chars: Option<i64>, suffix: Option<&str>) -> Result<Value, Error>`**
   - Nếu `value.is_undefined() || value.is_none()` → trả `Ok(Value::from(""))`. Nếu `value.kind() != ValueKind::String` → trả `FilterError`. Chuỗi input `> 128 * 1024` bytes (`MAX_FILTER_STRING_BYTES`) → `FilterError`.
   - `max_chars`: mặc định `160`. Nếu `<= 0` → `FilterError`; nếu `> 10_000` clamp về `10_000`. Đây là ngân sách ký tự phần prefix trước `suffix`.
   - `suffix`: mặc định `"..."`. Nếu `suffix.chars().count() > 32` → `FilterError`.
   - Nếu tổng scalar chars `<= max_chars` trả nguyên chuỗi. Ngược lại lấy đúng `max_chars` scalar chars đầu; tìm khoảng trắng Unicode cuối cùng tại `char_idx >= max_chars / 2` để cắt tròn từ, `.trim_end()`, rồi nối `suffix`.

3. **`format_number(value: Value, decimals: Option<i64>, thousands_sep: Option<&str>, decimal_sep: Option<&str>) -> Result<Value, Error>`**
   - Nếu `value.is_undefined() || value.is_none()` → trả `Ok(Value::from(""))`.
   - Nếu `matches!(value.kind(), ValueKind::Bool | ValueKind::Seq | ValueKind::Map | ValueKind::Bytes)` → trả `FilterError`.
   - Nếu `value.kind() == ValueKind::String`: `.trim()` rỗng → trả `Ok(Value::from(""))`; kiểm tra độ dài `<= 64` bytes (vượt → `FilterError`); thử parse `i128` thuần ASCII (không `.`, không `e`/`E`) trước, sau đó `f64`; nếu không phải số → trả `Ok(Value::from(""))`.
   - `decimals`: mặc định `0`; nếu `< 0 || > 6` → `FilterError`.
   - `thousands_sep` mặc định `"."`, `decimal_sep` mặc định `","`: nếu `.chars().count() > 4` → `FilterError`.
   - **Định dạng chính xác không tràn số:**
     - **Đường `i128` (nguyên):** Kiểm tra số chữ số `abs_val` `<= 32` (vượt → `FilterError`), định dạng phần nguyên chèn `thousands_sep` từ phải sang trái từng nhóm 3 chữ số và nối `decimal_sep` + `"0".repeat(decimals as usize)` khi `decimals > 0`.
     - **Đường `f64`:** Nếu `!num.is_finite()` → trả `FilterError`. Định dạng trị tuyệt đối sang chuỗi thập phân trước khi kiểm tra để tránh nhân `10^6` tràn `i128`: `let formatted = format!("{:.*}", decimals as usize, num.abs());`, tách `let (int_part, frac_part) = formatted.split_once('.').unwrap_or((&formatted, ""));` — nếu `int_part.len() > 32` → `FilterError`. Chèn `thousands_sep` trên `int_part`, ghép `decimal_sep + frac_part` khi `decimals > 0`.
     - **Dấu âm & `-0`:** Đặt `negative = is_neg && (int_part.bytes().any(|b| b != b'0') || frac_part.bytes().any(|b| b != b'0'))` để `-0.001` với `decimals = 2` luôn ra `"0,00"`, không bao giờ `"-0,00"`.

4. **`batch(value: Value, size: i64, extra: minijinja::value::Rest<Value>) -> Result<Value, Error>`**
   - Nếu `value.is_undefined() || value.is_none()` → trả `Ok(Value::from(Vec::<Value>::new()))`.
   - `if extra.len() > 1` → `FilterError`.
   - Kiểm tra `value.kind() == ValueKind::Seq` (loại trừ `String`/`Map`/`Number`/`Bool` bằng `FilterError`); đọc tối đa `501` phần tử (`seq.try_iter()?.take(501)`); nếu `len > 500` → `FilterError`.
   - `size`: nếu `<= 0 || > 100` → `FilterError`.
   - `fill`: nếu `extra.len() == 1 && !extra[0].is_undefined()`, lấy `Some(extra[0].clone())` (kể cả giá trị `none` tường minh); ngược lại `None`. Khi chunk cuối ngắn hơn `size` và có `fill = Some(pad_val)`, điền đủ `size` bằng `pad_val.clone()`.

5. **`selectattr(state: &minijinja::State, value: Value, attr: &str, extra: minijinja::value::Rest<Value>)` & `rejectattr(state: &minijinja::State, value: Value, attr: &str, extra: minijinja::value::Rest<Value>)`**
   - Nhận tham số đầu `state: &minijinja::State` (để gọi `minijinja::tests::is_in(state, &attr_val, rhs)` và chuẩn hóa `Cow<'_, str>` cho `is_startingwith`/`is_endingwith` trong `minijinja = "=3.0.5"`).
   - Nếu `value.is_undefined() || value.is_none()` → trả `Ok(Value::from(Vec::<Value>::new()))`.
   - Kiểm tra `value.kind() == ValueKind::Seq`; giới hạn lấy `take(501)` → vượt `500` trả `FilterError`.
   - Validate `attr`: chuỗi `1..=3` đoạn nối bằng `.`; mỗi đoạn khớp `[a-zA-Z0-9_]{1,64}` và **không** bắt đầu bằng `"__"`. Với phần tử không phải `ValueKind::Map` hoặc thiếu key / gặp `none` giữa đường dẫn `a.b`, độ phân giải thuộc tính trả về `Value::UNDEFINED` (không báo lỗi).
   - **Kiểm tra số đối số & `ALLOWED_TESTS` nghiêm ngặt:**
     - `extra.len() == 0` → đánh giá truthiness của `attr_val` (`is_true`).
     - `extra.len() == 1` → `test_name = extra[0].as_str()` **bắt buộc** thuộc nhóm unary trong `ALLOWED_TESTS`: `&["defined", "undefined", "even", "odd", "number", "string", "sequence", "mapping", "none", "true", "false"]`. Truyền binary test thiếu `rhs` hoặc test ngoài whitelist → `FilterError`.
     - `extra.len() == 2` → `test_name = extra[0].as_str()` **bắt buộc** thuộc nhóm binary trong `ALLOWED_TESTS`: `&["eq", "equalto", "ne", "lt", "le", "gt", "ge", "in", "startingwith", "endingwith"]`, đối chiếu `(attr_val, &extra[1])`. Truyền unary test kèm `rhs` thừa hoặc test ngoài whitelist → `FilterError`.
     - `extra.len() > 2` → `FilterError`.
   - `rejectattr` là phủ định logic của kết quả `selectattr` cho mọi phần tử.

6. **`map(value: Value, rest: minijinja::value::Rest<minijinja::value::ValueOrKwargs>) -> Result<Value, Error>`**
   - Nếu `value.is_undefined() || value.is_none()` → trả `Ok(Value::from(Vec::<Value>::new()))`.
   - Kiểm tra `value.kind() == ValueKind::Seq`; giới hạn lấy `take(501)` → vượt `500` trả `FilterError`.
   - Giải nén đối số đúng chuẩn MiniJinja `=3.0.5` (`argtypes.rs:936-941, 1027-1035`, nơi `Rest<T>` phải đứng cuối cùng trong chữ ký hàm):
     ```rust
     let raw = rest.into_values();
     let (args, kwargs) = minijinja::value::from_args::<(&[Value], minijinja::value::Kwargs)>(&raw)?;
     ```
     - **Dạng positional:** `args.len() == 1` → `attr = args[0].as_str()` (bắt buộc là string); gọi `kwargs.assert_all_used()?` (từ chối nếu có kwarg kèm theo); `default = Value::from(())`. Mọi chuỗi hợp lệ đều được diễn giải là attribute path (`map("title")`, `map("primary_category.name")`).
     - **Dạng kwargs:** `args.is_empty()` → đọc `attribute: &str = kwargs.get("attribute")?`, `default: Option<Value> = kwargs.get("default")?` (mặc định `Value::from(())` / `none`), rồi gọi `kwargs.assert_all_used()?` (từ chối `filter=...` hoặc bất kỳ kwarg lạ nào, hoặc thiếu `attribute`).
     - Bất kỳ tổ hợp `args.len() > 1` nào khác → `FilterError`.
   - Validate `attr` theo rule `1..=3` đoạn non-dunder; phần tử không phải Map hoặc thiếu attribute trên đường dẫn trả về `default.clone()`.

7. **Thi hành 21 `ALLOWED_TESTS` & `Expr::GetItem` Anti-Dunder (`template_sec.rs`)**
   - `const ALLOWED_TESTS: &[&str] = &["defined", "undefined", "in", "startingwith", "endingwith", "even", "odd", "number", "string", "sequence", "mapping", "eq", "equalto", "ne", "lt", "le", "gt", "ge", "none", "true", "false"];`
   - Đăng ký 18 hàm từ `minijinja::tests::*` vào 19 tên test trong `restricted_environment_with_config` (`"eq"` & `"equalto"` → `is_eq`, `"ne"` → `is_ne`, `"lt"` → `is_lt`, `"le"` → `is_le`, `"gt"` → `is_gt`, `"ge"` → `is_ge`, `"none"` → `is_none`, `"true"` → `is_true`, `"false"` → `is_false`, `"in"` → `is_in`, `"startingwith"` → `is_startingwith`, `"endingwith"` → `is_endingwith`, `"even"` → `is_even`, `"odd"` → `is_odd`, `"number"` → `is_number`, `"string"` → `is_string`, `"sequence"` → `is_sequence`, `"mapping"` → `is_mapping`; giữ nguyên built-in `"defined"`/`"undefined"`).
   - Trong `validate_expr(Expr::Test(t))` (`template_sec.rs:1626`): kiểm tra `!ALLOWED_TESTS.contains(&t.name)` → trả lỗi ngay.
   - Trong `validate_expr(Expr::GetItem)` (`template_sec.rs:1591-1594`): từ chối hằng chuỗi subscript bắt đầu bằng `"__"` để đồng bộ với `Expr::GetAttr`.

---

### 2. `theme_assets`, Giải mã Entity `document.rs` & Máy Trạng thái AST + HTML Toàn diện

#### 2.1 `theme_assets`, `ThemeUrlPolicy` (Bảo toàn `GLOBAL_PUBLIC_URL_OPTION_KEYS`) & Giải mã `document.rs`
- Thêm `ActiveThemeV3::asset_url_map(&self) -> BTreeMap<String, String>` trả về `{ asset_path => format!("/theme/{}/{}", self.revision_digest, asset_path) }`.
- Mở rộng `ThemeUrlPolicy` (`template_sec.rs:25`), cung cấp constructor `pub(crate) fn new(permitted_href_expressions: BTreeSet<String>, permitted_src_expressions: BTreeSet<String>, declared_assets: BTreeSet<String>) -> Self` và giữ `ThemeUrlPolicy::from_option_keys(...)` làm wrapper tương thích cho bộ unit test cũ tại `template_sec.rs:3173`:
  ```rust
  #[derive(Debug, Clone, Default)]
  pub struct ThemeUrlPolicy {
      permitted_href_expressions: BTreeSet<String>,
      permitted_src_expressions: BTreeSet<String>,
      declared_assets: BTreeSet<String>,
  }
  ```
- Cập nhật đồng bộ **cả hai** hàm policy trong `storage/settings.rs` (`L1549-1574`, kèm import/re-export thêm `GLOBAL_PUBLIC_MEDIA_OPTION_KEYS` cùng `GLOBAL_PUBLIC_URL_OPTION_KEYS` tại `settings.rs:933-936`):
  - `theme_url_policy(manifest_json)` (`settings.rs:1549-1565`):
    - **`permitted_href_expressions` (`HrefOnly`):** Giữ nguyên toàn bộ `GLOBAL_PUBLIC_URL_OPTION_KEYS` (`social_facebook`, `social_youtube`, `social_tiktok`, `social_instagram`, `social_x`, `social_linkedin`, `social_email` tại `block_schema.rs:31-39`) **cộng với** các option `public: true` kiểu `ThemeOptionType::Url` trong `manifest_json` (bảo toàn `<a href="{{ options.social_* }}">` lẫn `template_sec.rs:3222-3224` vẫn từ chối `<img src="{{ options.cta_url }}">`).
    - **`permitted_src_expressions` (`SrcAndHref` & `HrefOnly`):** Nạp toàn bộ `GLOBAL_PUBLIC_MEDIA_OPTION_KEYS` (`site_logo` tại `block_schema.rs:40`) **cộng với** mọi option `public: true` kiểu `ThemeOptionType::ImageRef` hoặc `ThemeOptionType::Media` trong `manifest_json` (sửa gãy trên `tests/studio_s02_preview_api.rs:133`).
    - **`declared_assets`:** Decode mảng `assets` từ `manifest_json`.
  - `global_theme_url_policy()` (`settings.rs:1567-1574`, được gọi bởi `overrides::active_theme_url_policy(None)` tại `overrides.rs:1139`):
    - Giữ nguyên toàn bộ `GLOBAL_PUBLIC_URL_OPTION_KEYS` trong `permitted_href_expressions` **và** nạp `GLOBAL_PUBLIC_MEDIA_OPTION_KEYS` vào `permitted_src_expressions` để cả `<a href="{{ options.social_* }}">` lẫn `<img src="{{ options.site_logo }}">` hoạt động đồng nhất trên DB overrides khi chưa kích hoạt Theme v3.
- **Giải mã HTML5 Entity trong `document.rs::is_authorized_stylesheet_link` (`document.rs:156-168`):**
  - Vì `restricted_environment()` bật `AutoEscape::Html`, MiniJinja tự động chuyển `/` thành `&#x2f;` (`href="&#x2f;theme&#x2f;<digest>&#x2f;assets&#x2f;style.css"`), trong khi `document.rs::parse_attributes` giữ chuỗi thô `attrs_raw[val_start..i]`.
  - Cập nhật `is_authorized_stylesheet_link(attrs: &[(String, String)]) -> bool` từ chối ngay (`return false`) nếu danh sách `attrs` chứa nhiều hơn một thuộc tính `"rel"` hoặc nhiều hơn một `"href"` (fail-closed dù `document::parse_attributes` trả `Vec`), và gọi `opendash_core::richtext::decode_html5_entities` (giống `template_sec.rs:1330`) cho cả `rel` và `href`, yêu cầu:
    1. `decoded_rel.trim().eq_ignore_ascii_case("stylesheet")`
    2. `!raw_href.chars().any(|c| c.is_whitespace() || c.is_control())`
    3. `!decoded_href.chars().any(|c| c.is_whitespace() || c.is_control())` (chặn cả byte điều khiển decode ra từ `&#x7f;`, `&#x07;`)
    4. `!decoded_href.contains(['?', '#', '\\'])`
    5. `decoded_href.starts_with('/') && !decoded_href.starts_with("//") && !decoded_href.contains("..")`
  - Thêm kiểm phòng thủ đa tầng trong `document.rs::tokenize` (`document.rs:244-307`) và `find_case_insensitive_end_style` (`document.rs:193-210`) chống HTML5 bogus-comment / RCDATA-RAWTEXT desync mà **vẫn bảo toàn `<p>1 < 2</p>`, `<p>1 < 2 it's ok</p>`, `<p>a < b</p>`**:
    - Trong `tokenize`, khi gặp `bytes[i] == b'<'`, rẽ nhánh ngay theo **`bytes.get(i + 1)` TRƯỚC KHI gọi `find_tag_end` hay `.trim()`**:
      - `Some(b'?')` → `Err(DocumentNormalizationDiagnostic::new("malformed_tag", ...))`;
      - `Some(b'!')` → nếu `raw[i..].starts_with("<!--")` xử lý HTML5 comment (`L252-261`); ngược lại chỉ chấp nhận nếu `raw[i..].len() >= 9 && raw[i..i + 9].eq_ignore_ascii_case("<!doctype") && bytes.get(i + 9).is_some_and(|b| *b == b'>' || b.is_ascii_whitespace())` rồi quét tới `>` đầu tiên (từ chối `b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f`) để xuất `TokenKind::Doctype`; mọi `<!` khác (như `<!doctypex>`, `<!CDATA[`) trả ngay `Err(malformed_tag)`;
      - `Some(b'/')` → **bắt buộc** `bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic)` (ngược lại trả `Err(malformed_tag)`), gọi `find_tag_end(raw, i + 1)?`, yêu cầu `name_lower` chỉ gồm `[a-z0-9-]`, từ chối mọi token khác ngoài khoảng trắng sau `name_end` (`!rest[name_end..].trim().is_empty()`), kiểm `is_forbidden_tag`, rồi xuất `CloseTag`;
      - `Some(b)` với `b.is_ascii_alphabetic()` → đi tiếp vào `find_tag_end` + `OpenTag` (không `.trim()` phần đầu); và siết `find_case_insensitive_end_style` (`document.rs:193-210`) trả `Result<Option<(usize, usize)>, DocumentNormalizationDiagnostic>`: sau `b"</style"` **bắt buộc gặp ngay `b'>'`** (gặp ASCII whitespace hoặc `b'/'` trả ngay `Err(malformed_tag)` thay vì nhảy qua);
      - `None` hoặc mọi byte khác sau `<` (như khoảng trắng hoặc chữ số trong `1 < 2`, `<p>1 < 2 it's ok</p>`, hay `< !doctype>` có byte sau `<` là `' '`) → xuất `"<"` (`&raw[i..i + 1]`) dạng `TokenKind::Text` (không bao giờ thành `TokenKind::Doctype`) và tăng `i += 1` mà **không** gọi `find_tag_end` (khớp HTML5 §13.2.5.6 và `template_sec.rs`).
  - Thêm `#[cfg(test)]` unit test trong `document.rs` kiểm chứng cả `href="&#x2f;theme&#x2f;rev&#x2f;assets&#x2f;style.css"` (được giữ lại), giữ nguyên `<p>1 < 2</p>`, `<p>1 < 2 it's ok</p>`, `<p>a < b</p>`, `< !doctype>` được tokenize thành `TokenKind::Text` (không thành `TokenKind::Doctype`), lẫn từ chối `href="&#x2f;&#x2f;evil.example&#x2f;a.css"`, `href="&#x2f;theme&#x2f;rev&#x2f;a.css&#x07;"`, thẻ `<link>` trùng lặp `rel`/`href`, `<?x <div title="><meta ...>">`, `</1 <div title=">...">`, `<!doctypex>`, `<!CDATA[`, và `<style>a{}</style >`.
- **Ánh xạ mã lỗi HTTP & MIME:**
  - `TemplateOverrideRecord` trong DB (`overrides.rs:410-429`) lưu `BuilderDocument` (`ast_json`), không lưu raw MiniJinja template (chỉ có thể nhúng `core/template_part` thuộc ActiveTheme).
  - Tham chiếu `theme_assets["assets/missing.css"]` bị từ chối bằng **`400 Bad Request`** tại cài đặt Theme v3 (`theme.rs:455-485` → `V2ZipError::BadRequest`) và Studio Preview (`overrides.rs:1373, 2446` → `ApiError::bad_request`). ActiveTheme trong DB hỏng lúc compile runtime (`routes/public_content.rs:183-190`, `overrides.rs:1833-1840`) vẫn ánh xạ sang **`500 Internal Server Error`** (`ApiError::internal`).
  - Asset `.txt` nằm trong `manifest.assets` là metadata-only (không được `routes/themes.rs:585` cấp MIME HTTP) nên `<link rel="stylesheet">` chỉ nhận đuôi `.css`.

#### 2.2 Máy Trạng thái AST + HTML Toàn diện (`walk_theme_ast_with_url_scopes`)
- **Phạm vi thay thế rõ ràng:** `walk_theme_ast_with_url_scopes(ast: &Stmt, is_part: bool, is_full_doctype_doc: bool, policy: &ThemeUrlPolicy)` **thay thế hoàn toàn** `ALLOWED_DYNAMIC_URL_VARS` (`template_sec.rs:522-532`), scanner tuyến tính pre-render `validate_theme_html_context(..., false)` **VÀ** bộ `ensure_composition_body_context` (`L1147-1212`) / `ensure_composition_body_sequence` (`L1214-1224`); riêng `enum HtmlScanState` (`template_sec.rs:1022-1028`) **được giữ lại** cho `insert_preview_fragments` (`L465-471`) và `walk_block_ast_stmt` (`L2414-2589`). Đường `post_render == true` tiếp tục dùng `validate_html_and_css_context(..., true)` (được gia cố phòng thủ thêm như mô tả cuối §2.2), và `validate_block_template_csec` giữ nguyên đường kiểm block độc lập (không có tham số `is_block` trên `walk_theme_ast_with_url_scopes`).
- **Chạy `walk_theme_ast_with_url_scopes` trên TẤT CẢ 3 điểm vào biên dịch/preview Theme:**
  1. `validate_template_csec_with_theme_urls` (`template_sec.rs:597` — truyền `is_part = false`, `is_full_doctype_doc = trimmed_lower.starts_with("<!doctype")`; bao phủ `compiled_source` đã flatten tại `L724`, preview `program` tại `L344`, `DEFAULT_ARCHIVE` / `DEFAULT_SINGLE` / `content_templates` qua `render_csec_with_config` tại `L622-638`, và legacy plugin block ở `page.rs:1991`),
  2. `composition_sites` (`template_sec.rs:832` — nhận `is_part: bool`),
  3. `preview_composition_sites` (`template_sec.rs:283` ủy quyền tới `composition_sites(..., true, policy)` tại `L294`, dùng từ `PreviewFragmentCompiler::flatten` tại `L394`).
- **Hàm kiểm tra hợp lệ `<link>` stylesheet dùng chung (`is_valid_theme_stylesheet_href(raw_val: &str) -> bool`):**
  Dùng chung cho **cả** `<link rel="stylesheet">` tĩnh pre-render lẫn post-render (`post_render == true`, `template_sec.rs:1864-1870`):
  1. `!raw_val.chars().any(|c| c.is_whitespace() || c.is_control())` (trên chuỗi chưa trim);
  2. `let decoded = decode_html_entities_for_url(raw_val);`
  3. `!decoded.chars().any(|c| c.is_whitespace() || c.is_control())` và `!decoded.contains(['?', '#', '\\'])`;
  4. `let rest = decoded.strip_prefix("/theme/")?; let (digest, rel_path) = rest.split_once('/')?;`
  5. `digest.len() == 64 && digest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))`;
  6. `crate::package::validate_v2_relative_path(rel_path).is_ok() && rel_path.ends_with(".css")` (`crate::package` là module public, khớp `routes/themes.rs:28`, `storage/import.rs:15`).
- **Cơ chế máy trạng thái trên `BTreeSet<ThemeHtmlState>` (giới hạn cứng `MAX_STATES = 64`):**
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
  enum CommentScanSubState {
      Start,     // Ngay sau "<!--": '>' đóng, '-' -> StartDash
      StartDash, // Ngay sau "<!---": '>' đóng, '-' -> Dash2
      Body,
      Dash1,
      Dash2,     // Thấy "--": '>' đóng, '!' -> Dash2Bang, '-' -> Dash2
      Dash2Bang, // Thấy "--!": '>' đóng, '-' -> Dash1
  }

  #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
  enum TagPhase {
      TagName,
      BeforeAttr,
      AttrName,
      AfterAttrName,
      BeforeAttrValue,
      AttrValue,
      SelfClosingSlash, // Chỉ vào khi !is_close từ TagName (!tag_name.is_empty()) | BeforeAttr | AttrName (!attr_name.is_empty()) | AfterAttrName; chỉ nhận '>', mọi byte khác -> Err
  }

  #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
  enum ThemeHtmlState {
      Body,
      BodyLt,                                  // Vừa gặp '<' — giữ ngữ cảnh qua biên EmitRaw/IfCond
      BodyLtSlash,                             // Vừa gặp "</"
      BodyLtBang { buf: [u8; 7], len: u8 },    // Tích lũy sau "<!" ("--" hoặc "doctype"; len==7 chờ '>'/whitespace)
      Doctype,                                 // Đang bên trong "<!doctype ...>" tới '>'
      Comment(CommentScanSubState),
      Style { closing_prefix: usize, css_buf: String }, // Theo dõi 7-byte b"</style" (0..=7); kiểm byte thứ 8
      Title { closing_prefix: usize },                  // Theo dõi 7-byte b"</title" (0..=7); kiểm byte thứ 8
      Tag {
          tag_name: String,            // tối đa 32 bytes ASCII lowercase [a-zA-Z0-9-] (vượt -> Err)
          is_close: bool,
          phase: TagPhase,
          attr_name: String,           // tối đa 32 bytes ASCII lowercase [a-zA-Z0-9_:-], bắt đầu [a-zA-Z] (vượt -> Err)
          quote: Option<u8>,           // Some(b'"' | b'\'') | None
          attr_has_static_bytes: bool,
          attr_expr_count: u8,
          seen_attrs: BTreeSet<String>,
          link_rel_stylesheet: bool,
          link_has_valid_href: bool,
          meta_ok: bool,
          attr_static_buf: String,     // tối đa 4096 bytes cho attr kiểm bảo mật (vượt -> Err)
      },
  }
  ```
- **Chuyển trạng thái & Kiểm tra Bảo mật Chi tiết Trong Khi Duyệt AST:**
  - `Stmt::EmitRaw(raw)` bước từng byte trên mọi trạng thái đang mở `BTreeSet<ThemeHtmlState>`:
    - **`Body` gặp `<`** → luôn chuyển sang `BodyLt` (không ở lại `Body`, triệt tiêu hoàn toàn bypass khi `<`/`</`/`<!` nằm cuối node `EmitRaw` trước `{{ ... }}` hoặc `{% if %}`).
    - **`BodyLt`:**
      - `[a-zA-Z]` → `Tag { is_close: false, phase: TagPhase::TagName, tag_name: lowercase_char }`
      - `/` → `BodyLtSlash`
      - `!` → `BodyLtBang { buf: [0; 7], len: 0 }`
      - `<` → `BodyLt`
      - **`b'?'`** → **`Err("Template không được chứa processing instruction hoặc bogus comment `<?`.")`** (chặn desync HTML5 `<?x <div title="><meta ...>">` giữa scanner và trình duyệt).
      - Mọi byte khác (như khoảng trắng, chữ số trong `1 < 2`) → `Body` (giữ hợp lệ phép so sánh chữ thường trong plain text).
    - **`BodyLtSlash`:** Chỉ duy nhất `[a-zA-Z]` → `Tag { is_close: true, phase: TagPhase::TagName, tag_name: lowercase_char }`; **MỌI byte khác** (kể cả `<`, `>`, khoảng trắng, chữ số, `/`, chặn `</1`, `</ `, `</>`) → **`Err("Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII.")`**.
    - **`BodyLtBang`:** Tích lũy byte ASCII lowercase vào `buf[..len]` (`len <= 7`):
      - Khi `len == 2 && &buf[..2] == b"--"` → `Comment(CommentScanSubState::Start)` (đóng chuẩn HTML5 với `<!-->`, `<!--->`, `<!-- --!>` qua đủ 6 trạng thái giống `template_sec.rs:1038-1068`).
      - Khi `len == 7 && &buf[..7] == b"doctype"`: nếu `is_part` → `Err("Part không được chứa DOCTYPE.")`; ngược lại ở byte kế tiếp ngay sau `doctype`, chỉ chấp nhận `b'>'` → `Body` hoặc ASCII whitespace (`b' ' | b'\t' | b'\n' | b'\r' | 0x0c`) → `Doctype` (mọi byte khác như `<!doctypex>` → `Err`).
      - Nếu buffer không còn là prefix của `b"--"` lẫn `b"doctype"` → `Err`.
    - **`Doctype`:** `b'>'` → `Body`; từ chối ngay `b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f` (`Err`, giữ ranh giới khớp `document.rs::find_tag_end`); mọi byte khác tiếp tục ở `Doctype`.
    - **Kiểm toàn bộ `FORBIDDEN_TAGS` & thẻ đóng void/RCDATA/RAWTEXT:** Kiểm danh sách `FORBIDDEN_TAGS` (`template_sec.rs:495-520`: `script`, `iframe`, `object`, `embed`, `svg`, `math`, `template`, `base`, `form`, `frame`, `frameset`, `portal`, `noscript`, `applet`, `input`, `textarea`, `select`, `fieldset`, `foreignobject`, `xmp`, `noembed`, `noframes`, `plaintext`) ở cả thẻ mở lẫn thẻ đóng (`is_close`). **Từ chối ngay mọi thẻ đóng `is_close && matches!(tag_name.as_str(), "link" | "meta" | "title" | "style")`** (`</link>`, `</meta>`, và mọi `</title>`/`</style>` rời rạc trong `Body`). Riêng `<title>` mở: chỉ chấp nhận khi **`!is_close && !is_part && is_full_doctype_doc`** (từ chối ở part, fragment thiếu DOCTYPE, hoặc tự đóng `<title />`); khi đóng `>` hợp lệ chuyển sang `Title { closing_prefix: 0 }`.
    - **RAWTEXT `<style>` & RCDATA `<title>` (chặn end-tag desync `</title >`, `</style/>`):**
      - `<style ...>` giữ trạng thái `Tag { tag_name: "style", is_close: false, ... }` qua các phase thuộc tính để kiểm toàn bộ attribute (chặn `<style onload=...>` hay thuộc tính cấm), từ chối tự đóng `<style />`, chỉ chuyển sang `Style { closing_prefix: 0, css_buf: String::new() }` khi gặp `>`.
      - Cả `Style` (với 7-byte prefix `b"</style"`) và `Title` (với 7-byte prefix `b"</title"`) theo dõi `closing_prefix: 0..=7`:
        - Khi `closing_prefix == 7` *(kiểm trước khi append vào `css_buf`)*: duy nhất `b'>'` đóng hợp lệ (với `Style`: `css_buf.truncate(css_buf.len() - 7)` loại sạch `b"</style"`, kiểm `css_buf.len() <= MAX_RENDERED_HTML_BYTES`, gọi `validate_safe_css(&css_buf)?`, rồi về `Body`; với `Title`: về `Body`); nếu gặp **ASCII whitespace (`\t\n\x0c\r `) hoặc `b'/'`** → **trả `Err` ngay** (chặn triệt để bypass WHATWG §13.2.5.11/15 `</title >`, `</title/>`, `</style >`, `</style/>`); nếu byte khác → ghi byte vào `css_buf` (cho `Style`) và đặt `closing_prefix = if b == b'<' { 1 } else { 0 }`.
        - Khi `closing_prefix < 7`: trong `Style`, luôn ghi byte `b` vào `css_buf` (`<= MAX_RENDERED_HTML_BYTES + 8`, vượt → `Err`, bảo toàn prefix dở dang như `</st` trong CSS); nếu `b.to_ascii_lowercase() == expected[closing_prefix]` tăng `closing_prefix += 1`, ngược lại đặt `closing_prefix = if b == b'<' { 1 } else { 0 }` (nhờ đó `<</style>` vẫn khớp đúng).
    - **Bảng quy tắc ký tự ASCII nghiêm ngặt cho từng `TagPhase` (cấm `b'\0'` ở mọi phase trong `Tag`):**
      - `TagName`: `[a-zA-Z0-9-]` (vượt `32` bytes → `Err`); ASCII whitespace → kiểm `FORBIDDEN_TAGS`/stray close tags rồi sang `BeforeAttr`; `b'/'` → nếu `!is_close && !tag_name.is_empty()` kiểm `FORBIDDEN_TAGS` + từ chối `"style" | "title"` rồi sang `SelfClosingSlash` (ngược lại `Err`); `b'>'` → kiểm `FORBIDDEN_TAGS` + chốt thẻ (`finalize_tag`) sang `Body`/`Title`/`Style`; mọi byte khác → `Err`.
      - `BeforeAttr`: **nếu `is_close == true` chỉ chấp nhận ASCII whitespace (giữ nguyên) hoặc `b'>'` (chốt thẻ đóng về `Body`), từ chối mọi thuộc tính và `/` (`</div onclick="...">`, `</div/>` → `Err`)**; nếu `!is_close`: ASCII whitespace giữ nguyên; `[a-zA-Z]` khởi tạo `attr_name` mới (xóa `quote`, `attr_has_static_bytes`, `attr_expr_count`, `attr_static_buf`) sang `AttrName`; `b'/'` → từ chối `"style" | "title"` rồi sang `SelfClosingSlash`; `b'>'` → `finalize_tag`; mọi byte khác → `Err`.
      - `AttrName`: `[a-zA-Z0-9_:-]` (vượt `32` bytes → `Err`); ASCII whitespace → `AfterAttrName`; `b'='` → `BeforeAttrValue`; `b'/'` → gọi `finalize_attr` (boolean attr) + kiểm không phải `"style" | "title"` + `SelfClosingSlash`; `b'>'` → gọi `finalize_attr` (boolean attr) + `finalize_tag`; mọi byte khác → `Err`.
      - `AfterAttrName`: ASCII whitespace giữ nguyên; `b'='` → `BeforeAttrValue`; `b'/'` → `finalize_attr` (boolean attr) + kiểm không phải `"style" | "title"` + `SelfClosingSlash`; `b'>'` → `finalize_attr` (boolean attr) + `finalize_tag`; `[a-zA-Z]` → `finalize_attr` (boolean attr) rồi khởi tạo ngay `AttrName` kế tiếp; mọi byte khác → `Err`.
      - `BeforeAttrValue`: ASCII whitespace giữ nguyên; `b'"' | b'\''` → đặt `quote = Some(b)` và sang `AttrValue`; từ chối `b'>' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f` (`Err`); mọi byte non-whitespace còn lại (bao gồm `'/'` trong `<a href=/about>`) khởi đầu `AttrValue { quote: None }` và được tiêu thụ làm byte đầu của giá trị unquoted.
      - `AttrValue` với `quote == Some(q)`: `b == q` → `finalize_attr` + `BeforeAttr`; `b == b'\0'` → `Err`; byte khác → tiêu thụ (`attr_has_static_bytes = true`; kiểm `attr_expr_count == 0` nếu là URL attr; ghi `attr_static_buf <= 4096` nếu thuộc attr kiểm bảo mật).
      - `AttrValue` với `quote == None` (unquoted): từ chối ngay `b'"' | b'\'' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f` (WHATWG HTML §13.2.5.38); ASCII whitespace → `finalize_attr` + `BeforeAttr`; `b'>'` → `finalize_attr` + `finalize_tag`; mọi byte khác (giữ `'/'` làm dữ liệu đường dẫn `/about`) tiêu thụ tiếp vào giá trị unquoted.
      - `SelfClosingSlash`: **duy nhất `b'>'`** → `finalize_tag` (về `Body`); mọi byte khác → `Err`.
    - **Kiểm tra thuộc tính trong `finalize_attr` & chốt thẻ `finalize_tag`:**
      - Kiểm **trùng lặp thuộc tính** theo nhánh trong `Tag.seen_attrs` (hỗ trợ `<a {% if item.is_active %}class="active"{% else %}class="idle"{% endif %} href="{{ item.url }}">`).
      - Chặn toàn bộ thuộc tính nguy hiểm (`L1873-1878`): `on*`, `srcdoc`, `formaction`, `action`, `xmlns`, `xlink:href`.
      - Thuộc tính tĩnh **`style`**, **`srcset`**, **`ping`**: nếu thuần tĩnh (`attr_expr_count == 0`), kiểm `style` bằng `validate_safe_css` và `srcset`/`ping` bằng `validate_safe_url` (`L1882-1955`), **nhưng cấm tuyệt đối mọi `Stmt::EmitExpr` bên trong chúng**.
      - Thuộc tính URL **`href`**, **`src`**, **`poster`**, **`cite`** ngoài `<link>` khi thuần tĩnh (`attr_expr_count == 0`): giải mã entity `let decoded = decode_html_entities_for_url(&attr_static_buf);` và kiểm bắt buộc bằng `validate_safe_href_url(&decoded)?` với `<a href>` hoặc `validate_safe_url(&decoded)?` với các trường hợp `href`/`src`/`poster`/`cite` còn lại (bắt đứng tại compile time mọi payload như `<a href="javascript:alert(1)">` kể cả trong nhánh chết `{% if false %}`).
      - Vượt `tag_name > 32`, `attr_name > 32`, hoặc `attr_static_buf > 4096` bytes trên thuộc tính kiểm bảo mật (`href`, `src`, `poster`, `cite`, `style`, `srcset`, `ping`, `rel`, `http-equiv`, `content`, `name`) → **trả `Err` ngay** (không cắt cụt; `attr_static_buf` chỉ giới hạn giá trị tĩnh pre-render, nên giá trị `{{ options.<key> }}` dài ở runtime như test `4096` scalar chars / `16_381` UTF-8 bytes tại `template_sec.rs:3142` đi qua `EmitExpr` + post-render `parse_attributes` không bị chạm ngưỡng).
      - **Quy tắc single-expression CHỈ áp dụng riêng cho thuộc tính URL (`href`, `src`, `poster`, `cite`):** Chỉ trên 4 thuộc tính URL này mới kiểm `if attr_expr_count > 0 && attr_has_static_bytes` (hoặc `attr_expr_count > 1`) → **trả `Err` ngay** (chặn cả tĩnh đứng trước lẫn tĩnh đứng sau như `<a href="{{ entry.url }}/tail">`). Mọi thuộc tính HTML thông thường không thuộc URL/nhóm cấm (`alt`, `class`, `title`, `aria-*`, `data-*`) **được phép trộn tự do chữ tĩnh với `{{ ... }}`**, bảo toàn 100% `alt="Ảnh minh họa {{ entry.title }}"` trong `DEFAULT_ARCHIVE` (`content_render.rs:25`), `opendash-starter/archive.html:1`, và `archive-post.html:1`.
      - **Trên `<link>` & `<meta>`:**
        - `<link>` (`!is_close`): khi `attr_name == "rel"` kết thúc (thuần tĩnh), nếu giá trị trim bằng `"stylesheet"` (case-insensitive) đặt `link_rel_stylesheet = true`. Khi `attr_name == "href"` kết thúc thuần tĩnh (`attr_expr_count == 0`), đặt `link_has_valid_href = is_valid_theme_stylesheet_href(&attr_static_buf)` (hoặc khi nhận `EmitExpr(LinkCss)` đặt `link_has_valid_href = true`). Khi chốt `<link>` tại `finalize_tag`, **bắt buộc `link_rel_stylesheet && link_has_valid_href`** (thiếu một trong hai → `Err`).
        - `<meta>` (`!is_close`): **chỉ cho phép tập attr `{ "charset", "name", "content" }` — từ chối ngay `"http-equiv"` (chặn `<meta charset="utf-8" http-equiv="refresh">`) và mọi attr lạ**; nếu `attr_name == "charset"` đặt `meta_ok = true`; nếu `attr_name == "name"` bắt buộc `attr_expr_count == 0 && attr_static_buf.trim().eq_ignore_ascii_case("viewport")` (ngược lại → `Err`) rồi đặt `meta_ok = true`. Khi chốt `<meta>` tại `finalize_tag`, bắt buộc `meta_ok == true`.
    - Jinja comments `{# ... #}` bị `minijinja::machinery::parse` lược bỏ từ đầu nên không làm nhiễu lexer.
  - **Cấm điều khiển/biểu thức Jinja trong `<style>` & phòng thủ khối điều khiển (`IfCond`/`ForLoop`/`WithBlock`/`Set`/`SetBlock`):**
    - Tại đầu xử lý **MỌI** statement khác `EmitRaw` (`EmitExpr`, `IfCond`, `ForLoop`, `WithBlock`, `Set`, `SetBlock`): nếu `states.iter().any(|s| matches!(s, ThemeHtmlState::Style { .. }))` → **trả `Err("C-SEC cấm nội suy và khối điều khiển Jinja bên trong <style>.")` ngay lập tức** (khớp `template_sec.rs:1754` và triệt tiêu triệt để nguy cơ DoS nhân bản `64 × 512 KiB = 32 MiB` `css_buf` qua `BTreeSet`).
  - **`Stmt::IfCond`:** Clone tập `states` và `UrlScopeStack` cho `true_body` & `false_body`, hợp nhất hai tập trạng thái đầu ra (`BTreeSet` `<= 64`) và thực hiện phép giao `UrlScopeStack` giữa hai nhánh.
  - **`Stmt::WithBlock(w)`:** Đẩy frame mới lên `UrlScopeStack` chèn `(ident, None)` (tombstone) cho mọi định danh được bind trong `w.assignments` (`extract_bound_idents(&target)?`: nhận `Expr::Var`/`Tuple`/`List`, **fail-closed `Err` với `Expr::GetAttr`/`Expr::GetItem`/dạng khác** như `template_sec.rs:1502-1503`, đồng thời cấm `"entry"`), duyệt tuần tự `w.body` với `(states, scoped_stack)`, và rút frame `WithBlock` khi thoát.
  - **`Stmt::Set` & `Stmt::SetBlock(sb)`:**
    - Từ chối ngay nếu `extract_bound_idents(&target)?` chứa `"entry"`.
    - `Stmt::SetBlock(sb)`: **bắt buộc duyệt `sb.body`** từ trạng thái khởi đầu sạch `BTreeSet::from([ThemeHtmlState::Body])` với `(states_out, sb_scopes)` khởi tạo từ bản sao `UrlScopeStack`, xác nhận mọi trạng thái thoát `sb.body` bằng `ThemeHtmlState::Body` (không để `<script>`, `onclick=`, hoặc URL xấu ẩn trong `{% set unused %}...{% endset %}`), giữ nguyên `states` của block bao quanh, rồi hợp nhất **`scopes = intersect_scopes(&scopes, &sb_scopes)`** để lan truyền mọi tombstone `Set` bên trong `sb.body` ra scope bao quanh (MiniJinja `SetBlock` không tạo frame riêng), cuối cùng vô hiệu hóa mọi định danh trong `sb.target` thành `None` trên frame hiện hành.
    - `Stmt::Set(s)`: vô hiệu hóa mọi định danh trong `s.target` thành `None` trên frame hiện hành của `UrlScopeStack`.
  - **`Stmt::ForLoop(f)` (Kiểm đầy đủ `f.else_body`, Live `entry` Rebinding & Hội tụ Kép `(states, UrlScopeStack)`):**
    - Validate `f.filter_expr` (nếu có) theo quy tắc `validate_expr`.
    - Lưu `(pre_states, pre_scopes)` trước vòng lặp; duyệt `f.else_body` từ `(pre_states, pre_scopes)` và **bắt lấy `(else_states, else_scopes)`** (nếu `f.else_body` rỗng, `(else_states, else_scopes) == (pre_states, pre_scopes)`, đại diện tự nhiên cho nhánh chạy 0 lần).
    - Giải `target_idents = extract_bound_idents(&f.target)?`: nếu `target_idents` chứa `"entry"`, **từ chối nếu không phải single-ident `Expr::Var("entry")` hoặc nếu `f.iter` KHÔNG phân giải ra `Some(UrlSourceKind::TrustedEntry)` trên `pre_scopes` sống** (chặn `{% set row = [...] %}{% for entry in row %}` lẫn tuple unpacking `{% for i, entry in entries %}`). Tính `loop_targets`: chỉ gắn `Some(kind)` khi `f.target` là single `Expr::Var` lặp trên collection tin cậy giải từ `pre_scopes`; ngược lại gán **`None` (tombstone)** cho mọi định danh trong `target_idents` (tuple/list hoặc nguồn untrusted đều shadow hoàn toàn outer frame!).
    - Đẩy 1 loop frame lên `pre_scopes` với `loop_targets`, chạy vòng lặp hội tụ điểm bất động tối đa **`64` lượt** (`MAX_STATES = 64`, khớp `template_sec.rs:1188`):
      - Bắt đầu lượt với `loop_states`, `loop_scopes` đã chứa `loop_targets` trên `top_frame`; duyệt `f.body` thu `(after_states, after_scopes)`.
      - Cập nhật `next_states = loop_states ∪ after_states` (`<= 64`) và `next_scopes = intersect_scopes(loop_scopes, after_scopes)` (bảo toàn mọi tombstone `None` trên các biến khác kể cả frame cha từ lượt `k -> k+1`).
      - Tạo **`next_header_scopes`** bằng cách làm mới **duy nhất** các khóa `loop_targets` trên `next_scopes.top_frame` (mô phỏng VM `Iterate` nạp phần tử kế tiếp ở đầu lượt `k + 1`, ngay cả khi `f.body` từng gán lại chính biến lặp `{% set item = "fallback" %}`!).
      - Kiểm tra hội tụ bằng **`next_states == loop_states && next_header_scopes == loop_scopes`** (`settled = true` nếu khớp; ngược lại đặt `loop_states = next_states`, `loop_scopes = next_header_scopes` và tiếp tục). Nếu sau 64 lượt chưa hội tụ hoặc `len() > 64` → **trả `Err` ngay**.
    - Rút frame vòng lặp khỏi `next_scopes` thu `outer_after_loop`, rồi hợp nhất đầu ra cuối cùng: **`final_states = next_states ∪ else_states`** (kiểm `<= 64`) và **`final_scopes = intersect_scopes(outer_after_loop, else_scopes)`**.
  - **Kết thúc template/part:** Bắt buộc mọi trạng thái trong `states` là `ThemeHtmlState::Body` (từ chối treo dở `BodyLt`/`BodyLtSlash`/`BodyLtBang`/`Doctype`/`Tag`/`Comment`/`Style`/`Title`).
  - **`Stmt::EmitExpr(expr)` (hỗ trợ nguyên AST cả `{{ ... }}` lẫn `{{- ... -}}`):**
    1. **Kiểm `od_content()` / `od_part()` TRƯỚC TIÊN:** Nếu `parse_composition_call(&expr.expr)?.is_some()`, bắt buộc **toàn bộ** trạng thái trong `states` bằng `ThemeHtmlState::Body` (bảo toàn test `template_sec.rs:3457-3458, 3780`) và giữ nguyên `states`.
    2. **Chặn vùng cấm & trạng thái chưa hoàn tất:** Nếu có bất kỳ trạng thái nào là **`Style(_) | Comment(_) | BodyLt | BodyLtSlash | BodyLtBang { .. } | Doctype | Title { closing_prefix: 1.. }`** → **trả `Err` ngay** (chặn `<{{ "script" }}>`, `<!{{ ... }}>`, runtime comment breakout `<!-- a{{ options.banner_text }}><div ...> -->`, và RCDATA end-tag breakout `</titl{{ site.title }}e>` / `</title{{ empty }}>`; bình thường `<title>{{ entry.title }} | {{ site.title }}</title>` luôn có `closing_prefix == 0` nên vẫn hợp lệ, còn comment template dùng `{# ... #}`).
    3. Nếu tập trạng thái chứa hỗn hợp `Body`/`Title` cùng `Tag` → `Err`.
    4. Nếu đang ở trong `Tag`: bắt buộc mọi trạng thái đang hoạt có `phase == AttrValue`, `quote.is_some()` (cấm nội suy trong giá trị không ngoặc kép), đồng thuận cùng `(tag_name, attr_name)` cùng nhóm bảo mật, và cấm nội suy trong `style`, `srcset`, `ping`, `rel` (trên `<link>`), và các attr của `<meta>`.
    5. Nếu là URL attribute (`href`, `src`, `poster`, `cite`):
       - Bắt buộc `!attr_has_static_bytes && attr_expr_count == 0`, sau đó tăng `attr_expr_count = 1`.
       - Validate `&expr.expr` trực tiếp trên cây AST MiniJinja (`Expr::Var`, `Expr::GetAttr`, hoặc `Expr::GetItem` với key string literal — từ chối mọi `Expr::IfExpr`, `Expr::BinOp`, `Expr::Filter`, `Expr::Call`):
         - **`options.<key>` bắt buộc dùng dot-access (`Expr::GetAttr`)** — từ chối `options['cta_url']` (bảo toàn `template_sec.rs:3220`).
         - **`HrefOnly` (`<a href>`, `<blockquote cite>`, `<q cite>` — `<a href>` cũng chấp nhận các biểu thức thuộc `SrcAndHref`):** Cho phép `<trusted_entry_var>.url`, `<trusted_entry_var>.primary_category.url`, `archive_url`, `previous_url`, `next_url`, `pagination.previous_url`, `pagination.next_url`, `options.<public_url_key>` (`permitted_href_expressions`), `<trusted_link_var>.url`. *(Đặt `cite` vào `HrefOnly` riêng cho `<blockquote>` / `<q>`; `<cite cite="...">` vẫn bị **từ chối** để bảo toàn `template_sec.rs:3224`, và root `"entry"` nằm sẵn trên frame gốc của `UrlScopeStack` với `TrustedEntry`.)*
         - **`SrcAndHref` (`<a href>`, `<img src>`, `<video poster>`):** Cho phép `<trusted_entry_var>.cover_url`, `<trusted_field_var>.image_url`, `theme_asset_url`, `theme_assets["assets/..."]`, `theme_assets['assets/...']` (chỉ path trong `theme_policy.declared_assets`), `options.<public_media_or_image_ref_key>` (`permitted_src_expressions`), `<trusted_raw_url_var>` *(đã loại bỏ `field.image_url` trần để ngăn `{% set field = ... %}`)*.
         - **`LinkCss` (`<link href>`):** Chỉ cho phép literal `theme_assets["assets/..."]` / `theme_assets['assets/...']` với `<path>` nằm trong `theme_policy.declared_assets` và kết thúc bằng `.css`, đồng thời đặt `link_has_valid_href = true`.
  - **Gia cố phòng thủ đa tầng ở post-render `validate_html_and_css_context_with_url_vars` (`template_sec.rs:1735-1860`):**
    - **Thống nhất 1 pass HTML + RAWTEXT `<style>` + RCDATA `<title>`:** Thay vòng tiền xử lý `lower.find("<style")` tách rời (`template_sec.rs:1740-1759`) bằng việc xử lý `<style>` và `<title>` trực tiếp ngay trong vòng quét thẻ chính (`L1764-1837`) sau nhánh `<!--` và `<!doctype`:
      1. Khi gặp `!is_close && tag_name == "style"`: kiểm `block_url_vars.is_none()`, kiểm thuộc tính `<style>` (chặn tự đóng `<style />` và thuộc tính cấm), rồi quét xuôi không phân biệt hoa-thường tìm `"</style"` kèm byte kế tiếp là `b'>'` (trả `Err` ngay nếu thiếu hoặc gặp ASCII whitespace / `b'/'` như `</style >`, `</style/>`), gọi `validate_safe_css(css_body)?`, và nhảy `i` qua `</style>`.
      2. Khi gặp `!is_close && tag_name == "title"`: bắt buộc `is_full_doctype_doc == true` (ngược lại `Err`), kiểm thuộc tính `<title>` (chặn tự đóng `<title />` và thuộc tính cấm), rồi quét RCDATA xuôi tìm `"</title"` kèm byte kế tiếp là `b'>'` (trả `Err` ngay nếu thiếu hoặc gặp ASCII whitespace / `b'/'` như `</title >`, `</title/>`), và nhảy `i` qua `</title>` — triệt tiêu hoàn toàn RCDATA desync trên mọi đường post-render không đi qua `document.rs`.
    - Phần quét thẻ chung (`L1785-1825`): từ chối thẻ đóng rời rạc `is_close && matches!(tag_name.as_str(), "style" | "title" | "link" | "meta")`, từ chối `<?` (`bytes.get(i + 1) == Some(&b'?')`), từ chối `</` khi byte sau không phải `[a-zA-Z]` hoặc thẻ đóng có attr, từ chối mọi `<!` lạ (ngoài `<!--` hoặc `<!doctype` kèm whitespace/`>`), và giữ nguyên `1 < 2` (`i += 1; continue;` khi sau `<` là whitespace/digit thay vì nhảy quote qua `>`).
    - Trong `validate_tag_attributes` cho `<meta>` (`L1851-1859`): từ chối `"http-equiv"` và mọi thuộc tính ngoài `{ "charset", "name", "content" }`, và nếu có `"name"` thì bắt buộc `name.trim().eq_ignore_ascii_case("viewport")`.

#### 2.3 Ngữ pháp Provenance, Mô hình `UrlScopeStack` Tombstone & Chỉnh test Cũ
- **Pre-seed `UrlScopeStack` & `reject_reserved_context_rebinding(&ast)` (chạy vô điều kiện):**
  - Khung gốc `UrlScopeStack` được khởi tạo sẵn **`("entry", Some(UrlSourceKind::TrustedEntry))`** tại điểm vào `walk_theme_ast_with_url_scopes`. Nhờ vậy root `entry.*` lẫn `{% for entry in row %}` đều được giải nhất quán qua ngữ cảnh `UrlScopeStack` sống!
  - `reject_reserved_context_rebinding(&ast)` cấm tuyệt đối (`Stmt::Set`, `Stmt::SetBlock`, `Stmt::WithBlock`, `Stmt::ForLoop`) gán lại các root key hệ thống: `options`, `theme_options`, `theme_assets`, `site`, `menus`, `entries`, `pagination`, `breadcrumbs`, `taxonomy`, `related_entries`, `recent_posts`, `route`, `locale`, `t`, `content_type`, `theme_asset_url`, `previous_url`, `next_url`, `archive_url`, `od_content`, `od_part`.
  - Quy tắc rebinding riêng cho `entry` chạy **ngay trong `walk_theme_ast_with_url_scopes`** trên `UrlScopeStack` đang hoạt động: cấm `Set`, `SetBlock`, `WithBlock`, và tuple/list `ForLoop` chạm `"entry"`; **chỉ cho phép** single-ident `{% for entry in <iter> %}` khi `<iter>` phân giải ra `Some(UrlSourceKind::TrustedEntry)` trên `pre_scopes` hiện hành (chặn hoàn toàn `{% set row = [{"url": "..."}] %}{% for entry in row %}`, đồng thời vẫn cho phép `{% for row in entries | batch(3) %}{% for entry in row %}`).
- **Ngữ pháp `UrlSourceKind` & Tháo Wrapper:**
  - `TrustedEntry` từ: `entries`, `related_entries`, `recent_posts`, hoặc `<trusted_entry_batch_var>`.
  - `TrustedEntryBatch` từ: `(entries | related_entries | recent_posts) | batch(...)`.
  - `TrustedLink` từ: `breadcrumbs`, `pagination.pages`, `menus.<slot>.items`, `menus['<slot>'].items`, `<trusted_link_var>.children`, `<trusted_entry_var>.terms.<tax>`, `<trusted_entry_var>.terms['<tax>']`, `<trusted_entry_var>.all_terms` *(với `entry` nằm sẵn trong `UrlScopeStack`)*.
  - `TrustedField` từ: `<trusted_entry_var>.fields`.
  - `TrustedRawUrl` từ: `<trusted_field_var>.gallery_urls` *(loại bỏ `field.gallery_urls` trần)*.
  - Wrapper hợp lệ quanh collection nguồn: tháo đệ quy `Expr::Slice` (`entries[:5]`) và sequence filter an toàn `selectattr`, `rejectattr`, `sort`, `reverse` (với `batch` nâng `TrustedEntry` → `TrustedEntryBatch`).
- **Mô hình Frame & Tombstone Chính xác của `UrlScopeStack`:**
  - Mỗi frame là `BTreeMap<String, Option<UrlSourceKind>>`: `Some(kind)` là biến tin cậy, `None` là **tombstone untrusted tường minh**.
  - `lookup(var)` duyệt từ frame trong cùng (top) ra ngoài và **dừng ngay tại frame đầu tiên chứa khóa `var`** (gặp `None` trả ngay không tin cậy, tuyệt đối không rơi xuyên xuống frame cha).
  - `intersect_scopes(a, b)` giao từng frame tương ứng: mọi khóa `k` ở frame của `a` hoặc `b` chỉ giữ `Some(kind)` khi CẢ HAI frame đều có `Some(kind)` giống hệt nhau; ngược lại lưu tombstone `None`. Nhờ vậy `WithBlock`, `ForLoop` tuple/untrusted, `Set`/`SetBlock` (kể cả trong `IfCond` hoặc `f.else_body`) và vòng lặp `ForLoop` lượt `k -> k+1` đều chặn triệt để provenance aliasing.
- **Ranh giới `od_part` & test cập nhật:** Mỗi template và từng part tự kiểm provenance vòng `for` trong chính file của nó (`collect_composition_sites` cấm `od_part` trong `{% for %}`). S01 cập nhật luôn các unit test cũ ở `template_sec.rs` nếu đặt `item.url` ngoài `{% for %}` và cập nhật `cms/crates/opendash-server/tests/studio_s02_preview_api.rs:133` từ `{% set url = options.logo_ref %}<img src="{{ url }}" alt="Logo">` sang `<img src="{{ options.logo_ref }}" alt="Logo">`.

---

### 3. Rich Public & Preview View Context, `published_at` & Zero N+1 (`O(1)`)

#### 3.1 Migration `0049_entry_published_at.sql` (+0 Schema Objects) & Toàn bộ Write/Read Paths
- Trên nền codebase hiện tại đã có `0047_multilingual_content_and_theme_l10n.sql` và `0048_global_site_options.sql` (`47` file migration `0001`–`0048`, `cms/migrations/README.md:19-20`), tạo migration kế tiếp **`cms/migrations/0049_entry_published_at.sql`** và cập nhật `cms/migrations/README.md`:
  ```sql
  ALTER TABLE content_entries ADD COLUMN published_at TEXT;

  UPDATE content_entries
     SET published_at = updated_at
   WHERE publication_status = 'published'
     AND published_at IS NULL;
  ```
- **Cập nhật struct & query (`storage/entries.rs`, `storage/entry_table.rs`, `storage/public_content.rs`, `content_render.rs`):**
  - Thêm `pub published_at: Option<String>` vào `ContentEntryRecord` (`entries.rs:244`); cập nhật `EntryRow` (9 → 10 cột), `decode_row`, mọi câu `SELECT` ở `storage/entries.rs` (`get_entry_in_tx:554-572`, `query_entries_filtered:848-919` dùng bởi `list_entries_filtered`, `create_entry_with_fence:931-997`, `update_entry_with_fence:1022-1106`, `create_entry_with_schema_fence:1143-1203`, `update_entry_with_schema_fence:1205-1282`, và `commit_composite_entry_save:2234-2237`), `storage/entry_table.rs:220-250`, `storage/public_content.rs` (`get_by_slug`, `get_published_by_id_in_tx`, và tuple 9 cột tại `list_taxonomy_published_in_tx:503-605` kèm hydrate `primary_category_term_id`), và các struct literal test trong `content_render.rs:458-477, 522-537`. Giữ nguyên đầy đủ các cột `locale`, `translation_group_id` từ `0047` trong mọi `INSERT`/`SELECT` hiện hữu.
  - Cập nhật **4 hàm ghi publication status:**
    1. `create_entry_with_fence` & `create_entry_with_schema_fence`: gán `published_at = datetime('now')` khi CPT là `immediate`; ngược lại `NULL`.
    2. `set_publication_status` (`entries.rs:2317-2358`, câu `UPDATE` ở `L2339-2341`): khi `status == "published"`, cập nhật `published_at = COALESCE(published_at, datetime('now'))`.
    3. `commit_composite_entry_save` (`entries.rs:2129-2241`): đối với luồng tạo `DraftPublish` và xuất bản ngay (`entries.rs:2154-2165`, do trigger `content_entries_core_insert_draft` ở `0047_multilingual_content_and_theme_l10n.sql:396-402` bắt buộc `INSERT` ban đầu giữ `'draft'`), giữ nguyên `published_at = NULL` lúc `INSERT` ban đầu rồi gán `published_at = COALESCE(published_at, datetime('now'))` ngay tại `UPDATE` kế tiếp (`L2158`); với `!is_draft_capable` gán ngay lúc `INSERT`; còn nhánh update thường (`L2180-2189`) dùng `published_at = CASE WHEN COALESCE(?, publication_status) = 'published' THEN COALESCE(published_at, datetime('now')) ELSE published_at END` (`CompositeEntrySaveInput.created_at`/`updated_at` không ghi đè `published_at`).
- Cập nhật `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs:1956` từ `assert_eq!(migration_version, 48);` lên **`assert_eq!(migration_version, 49);`**.

#### 3.2 `EntryView`, `EffectiveContentSchema`, `strip_richtext_to_plain` & Taxonomy ACL (`content_render.rs`, `storage/{public_content,permalinks,overrides}.rs`)
```rust
#[derive(Debug, Clone, Serialize)]
pub struct PublicTermView {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub url: Option<String>,
}
```
- **Mở rộng `EntryView`:** Giữ toàn bộ trường hiện có (`id`, `title`, `slug`, `url`, `cover_url`, `fields`, `values`, `updated_at`) và bổ sung:
  - `pub type_key: String`
  - `pub created_at: String`
  - `pub published_at: Option<String>`
  - `pub excerpt: String`
  - `pub reading_time_minutes: u32`
  - `pub terms: BTreeMap<String, Vec<PublicTermView>>` (luôn pre-seed khóa `"category": []`, `"tag": []` + tất cả taxonomy public hợp lệ trên CPT đó)
  - `pub all_terms: Vec<PublicTermView>`
  - `pub primary_category: Option<PublicTermView>`
- **Chữ ký chuẩn theo `EffectiveContentSchema` (`opendash-core/src/content.rs:883-905`) & ACL:**
  ```rust
  pub fn entry_view_with_context(
      record: &ContentEntryRecord,
      effective_schema: &EffectiveContentSchema,
      url: Option<String>,
      terms: &[PublicTermView],
      eligible_public_taxonomies: &[String],
      max_view_role: FieldRole,
  ) -> EntryView

  pub fn authenticated_preview_view_with_context(
      record: &ContentEntryRecord,
      effective_schema: &EffectiveContentSchema,
      url: Option<String>,
      terms: &[PublicTermView],
      eligible_public_taxonomies: &[String],
      actor_role: FieldRole,
  ) -> EntryView
  ```
  - **Đường dẫn trường & Thứ tự khai báo chính xác trên `EffectiveContentSchema`:**
    - Dựng danh sách thứ tự khai báo tất định (declaration order): duyệt `f.key` trong `effective_schema.base.descriptor.fields`, tiếp theo là các `f.key` trong từng `group.fields` của `effective_schema.attached_groups` (chống trùng bằng `seen: BTreeSet<&str>`, và cuối cùng bổ sung key còn lại trong `effective_schema.effective_fields` như `featured_image`/`body` được inject), rồi tra cứu `fe: &EffectiveFieldEntry` từ `effective_schema.effective_fields.get(key)`.
    - Chỉ giữ field thỏa `fe.effective_status == EffectiveFieldStatus::Active && fe.definition.public_exposure == PublicExposure::Public && fe.definition.min_view_role <= max_view_role` (route Public truyền `FieldRole::Editor`; Preview truyền `actor_role`, đồng thời giữ yêu cầu `PublicExposure::Public`). Giữ wrapper `entry_view_with_url` / `authenticated_preview_view` cho các unit test cũ trong `content_render.rs`.
  - Trong `entry_view_with_context`: với mọi active field hợp lệ chưa có giá trị trong `record.values`, **bắt buộc chèn `(fe.definition.key.clone(), serde_json::Value::Null)` vào `EntryView.values`** (giữ `fields: Vec<FieldView>` theo declaration order và `field.text` nguyên trạng để không phá `content_render.rs:478`). `authenticated_preview_view_with_context` giữ nguyên fallback `url = Some(format!("/api/admin/content-types/{}/entries/{}/preview", schema.key, view.id))` khi `url.is_none()` (`content_render.rs:204-207`) và rewrite `cover_url` / `field.image_url` / `field.gallery_urls` sang `/api/admin/media/...`.
  - **Batch Helper `eligible_public_taxonomies` (`storage/public_content.rs` — `O(1)` SQL, không rò rỉ taxonomy private):**
    - Bổ sung 3 hàm sau tại `storage/public_content.rs` (cấm truyền trực tiếp kết quả của `storage::content::load_effective_taxonomies_in_tx` vốn không lọc `d.public = 1` và chạy vòng lặp N+1 ở `storage/content.rs:1950-1961`):
      ```rust
      pub async fn batch_load_eligible_public_taxonomies_in_conn(
          conn: &mut SqliteConnection,
          type_keys: &[String],
      ) -> ApiResult<BTreeMap<String, Vec<String>>>

      pub async fn load_eligible_public_taxonomies_in_conn(
          conn: &mut SqliteConnection,
          type_key: &str,
      ) -> ApiResult<Vec<String>>

      pub async fn batch_load_eligible_public_taxonomies(
          db: &SqlitePool,
          type_keys: &[String],
      ) -> ApiResult<BTreeMap<String, Vec<String>>>
      ```
    - Dùng tối đa 2 truy vấn batch qua `json_each(?)`: Query 1 đọc `content_type_taxonomy_overrides` theo tập `type_keys` (ghi nhận CPT có dòng override kể cả khi mảng `[]`) + JOIN `json_each(o.taxonomies_json)` với `taxonomy_definitions d` (`d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'`); Query 2 đọc `content_type_taxonomy_eligibility e JOIN taxonomy_definitions d` cho các CPT không có override (`e.provider_status = 'active' AND e.declaration_state = 'declared' AND d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'`), sort tăng dần.
  - `PublicEntryRenderSnapshot` (`storage/public_content.rs:73`) thay `ContentTypeDefinition` bằng `pub effective_schema: EffectiveContentSchema` (+ `pub eligible_public_taxonomies: Vec<String>` từ `load_eligible_public_taxonomies_in_conn`); `overrides::resolve_preview_entry` (`L1384, L1467`) nhận thêm `actor_role: FieldRole` và trả `ApiResult<Option<(entries::ContentEntryRecord, EffectiveContentSchema)>>` (thay thế `FieldRole::Admin` ở `L1467` bằng `actor_role` thật); `render_canonical_archive` dùng `content::load_effective_content_schema` + `load_eligible_public_taxonomies_in_conn` (hay pool wrapper); `render_canonical_taxonomy` (`routes/public_content.rs:696-710`) cache `content::load_effective_content_schema(db, &type_key)` theo từng `type_key` phân biệt trên trang *(bị chặn bởi tối đa 20 mục nên không sửa `storage/content.rs`)* kết hợp batch 1 lần `batch_load_eligible_public_taxonomies(db, &distinct_type_keys)`; `query_related_entries`, `query_recent_posts`, 3 luồng preview `overrides.rs` (`L1505, 2504, 2816`) và 2 admin preview routes (`routes/content.rs`, `routes/entries.rs`) đều nạp `effective_schema` + `eligible_public_taxonomies` tương ứng.
- **Quy tắc chính xác cho `excerpt`, `reading_time_minutes` & `primary_category`:**
  - **Field hợp lệ:** Duyệt theo declaration order các `EffectiveFieldEntry` đã qua ACL có `fe.definition.field_type` thuộc `ContentFieldType::ShortText | ContentFieldType::LongText | ContentFieldType::RichText` (loại trừ `Number | Boolean | Date | Select | Image | Gallery`).
  - **`excerpt`:** Lấy text từ field `summary` nếu có & sau khi strip/trim không rỗng; ngược lại fallback sang `body`, rồi các text field hợp lệ còn lại cách nhau 1 dấu cách. Với `RichText`, gọi `extract_visible_text` nếu `contains_blocks(s)`, ngược lại gọi `strip_richtext_to_plain`. Nếu kết quả vượt `200` scalar chars, cắt ở ranh giới khoảng trắng cuối cùng trong phần prefix sao cho chuỗi kết quả cộng `"..."` không vượt quá `200` scalar chars (cắt cứng tại `197` + `"..."` nếu không có khoảng trắng trong nửa sau).
  - **`reading_time_minutes`:** Lấy `body` (sau strip HTML/block) nếu có và khác rỗng; ngược lại gom toàn bộ các field `ShortText | LongText | RichText` hợp lệ. Đếm từ qua `.split_whitespace().count()` (`words`): `0` → `0`, ngược lại `((words + 199) / 200).max(1) as u32`.
  - **`primary_category`:** Lọc term có `kind == "category"` (đã kiểm eligibility). Nếu `record.primary_category_term_id` khớp ID trong đó thì chọn; ngược lại chọn term có `t.id` nhỏ nhất (`ORDER BY t.id ASC`, khớp `permalinks.rs:810`); nếu không có → `None`.
- **Batch Public Terms & Pure C3 Term Paths (`storage/public_content.rs`, `storage/permalinks.rs`):** *(Toàn bộ hàm batch ID dưới đây truyền mảng ID qua `json_each(?)` — cùng pattern `storage/public_content.rs:531` — hoặc chia chunk `<= 500` ID để không vượt trần bind parameter SQLite.)*
  - Thêm `permalinks::batch_canonical_term_paths_in_tx(conn, term_ids: &[i64]) -> ApiResult<BTreeMap<i64, String>>`: bám chuẩn **100% `canonical_term_path_in_tx` (`permalinks.rs:548-563`)**, chỉ truy vấn batch 1 câu SQL trên `permalink_route_claims c` (`c.target_kind = 'term' AND c.target_id = CAST(t.id AS TEXT) AND c.locale = t.locale`, tận dụng index 3 cột `0047:392`) JOIN `json_each(?)` JOIN `taxonomy_terms t` JOIN `taxonomy_definitions d` (`d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'`). **Không dùng fallback `term_path(...)`**; term chưa có claim trả `None` (`url: None`).
  - Thêm **`public_content::batch_load_public_entry_terms_in_conn(conn: &mut SqliteConnection, entries: &[(i64, String)]) -> ApiResult<BTreeMap<i64, Vec<PublicTermView>>>`** làm hàm lõi trong transaction (cùng wrapper `batch_load_public_entry_terms(db: &SqlitePool, ...)`): chạy **1 câu `SELECT` batch** JOIN `entry_terms`, `taxonomy_terms`, `taxonomy_definitions` (`d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'`, lọc entry IDs qua `json_each(?)`), áp dụng per-CPT effective eligibility (`content_type_taxonomy_eligibility` + `content_type_taxonomy_overrides`, khớp `public_content.rs:530-547`), dùng `batch_canonical_term_paths_in_tx`, sắp xếp `ORDER BY t.kind ASC, t.id ASC` (hỗ trợ cả `draft` trong Admin Preview).

#### 3.3 Chấm dứt Toàn bộ N+1 Loops & Chuẩn hóa `MenuItemView` (`storage/{menu,entries,seo}.rs`, `routes/public_content.rs`)
- **Fix N+1 thực chất (`O(1)` truy vấn SQL bất kể `N`):**
  1. `storage/entries.rs::query_entries_filtered` (`L848-919`, phục vụ `list_entries_filtered`): bỏ vòng `decoded_with_terms` (2N queries); câu `SELECT` chính lấy đầy đủ `EntryRow` (10 cột) **cộng đủ 4 cột** `page_layout_key, custom_template_key, content_mode, primary_category_term_id` (bảo toàn 100% Admin `list_entries`), và hydrate `term_ids` bằng 1 batch `SELECT` trên `entry_terms` qua `json_each(?)` / chunk `<= 500`. Bổ sung các hàm đọc một-snapshot chính xác sau (dùng chung exact `WHERE` builder và tách `require_public_archive_in_conn` để tránh deadlock pool `max_connections(1)` khi chạy trong `tx`):
     ```rust
     // storage/public_content.rs
     pub async fn require_public_archive_in_conn(
         conn: &mut SqliteConnection,
         type_key: &str,
     ) -> ApiResult<()>

     pub async fn list_published_with_total(
         db: &SqlitePool,
         type_key: &str,
         limit: i64,
         offset: i64,
         filters: EntryFilters,
     ) -> ApiResult<(Vec<ContentEntryRecord>, i64)>

     // storage/entries.rs
     pub(crate) async fn count_entries_filtered_in_tx(
         tx: &mut Transaction<'_, Sqlite>,
         type_key: &str,
         filters: &EntryFilters,
         published_only: bool,
     ) -> ApiResult<i64>

     pub(crate) async fn list_published_filtered_with_total_in_tx(
         tx: &mut Transaction<'_, Sqlite>,
         type_key: &str,
         limit: i64,
         offset: i64,
         filters: EntryFilters,
         sort: PublishedListSort,
     ) -> ApiResult<(Vec<ContentEntryRecord>, i64)>
     ```
     Trong đó `require_public_archive(db, type_key)` (`public_content.rs:250`) ủy quyền cho `require_public_archive_in_conn`; còn `list_published_with_total` mở **một read transaction duy nhất** (`db.begin()`), gọi `require_public_archive_in_conn(&mut *tx, type_key).await?`, rồi gọi `entries::list_published_filtered_with_total_in_tx(&mut tx, type_key, limit, offset, filters, entries::PublishedListSort::IdDesc).await?` (hàm này gọi `validate_filter_in_tx` **một lần** rồi đọc `count_entries_filtered_in_tx` + `query_entries_filtered` trên cùng snapshot SQLite, triệt tiêu torn read giữa `has_next` và `total_pages`).
  2. `storage/seo.rs::resolve_public_entry_paths_by_entry_ids_in_conn(conn: &mut SqliteConnection, ids: &[i64]) -> ApiResult<BTreeMap<i64, String>>` (và wrapper `resolve_public_entry_paths_by_ids(db, entries)` thay `L114-132`): viết lại thành **1 batch SQL JOIN duy nhất** trên `content_entries e` + `content_types t` + `permalink_route_claims c` + `json_each(?)`, giữ nguyên 100% điều kiện hiển thị với `seo::resolve_public_entry_path_in_tx` (`seo.rs:98-103`) và `permalinks::canonical_entry_path_in_tx` (`permalinks.rs:535-540`) — tức `e.publication_status = 'published'`, `e.content_type_key NOT IN ('pattern', 'section')`, `t.provider_status = 'active'`, `COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1`, `c.target_kind = 'entry'`, và **`c.locale = e.locale`** (khớp index 3 cột `0047:392`; **không** thêm `t.declaration_state = 'declared'` đơn phương để tránh CPT withdrawn mất `entry.url` gây 500 ở archive). **Không** dùng `CanonicalUrlBuilder::entry` fallback; ID nào chưa có claim trả `None`.
  3. `storage/menu.rs` (`L16-20`, `L805-924`, `L947-968`, `L1149-1270`): chuẩn hóa URL resolution cho 3 biến thể thực tế của `MenuTargetKind` (`Entry` `"entry"`, `Term` `"term"`, `External` `"external"`) thành tối đa 2 batch SQL queries (`seo::resolve_public_entry_paths_by_entry_ids_in_conn` + `permalinks::batch_canonical_term_paths_in_tx`) qua `resolve_menu_view_for_path_in_tx(tx, menu_id, current_path: Option<&str>)` dùng chung với `preview_menu` (`resolve_menu_view_in_tx` ủy quyền truyền `None`); cập nhật `resolve_slot_menus_for_locale_and_path_in_tx` (các hàm `resolve_slot_menus_in_tx` & `resolve_slot_menus_for_locale_in_tx` ủy quyền với `None`), và thêm `resolve_active_slot_menus_for_path_in_conn(conn: &mut SqliteConnection, current_path: Option<&str>, locale: Option<&str>)` (mặc định dùng `settings::get_default_locale_code_in_conn(conn)` như `menu.rs:951-968`; kèm wrapper `resolve_active_slot_menus_for_path(db, ...)`).
  4. `routes/public_content.rs` & `storage/overrides.rs`: giải `menus` **tối đa 1 lần** trong request sau khi đã kiểm tra shell (chỉ bỏ qua — 0 query menu — khi `CoreFallback` **và** không có cả DB override `part:header`/`part:footer` lẫn ActiveTheme `header`/`footer` part), truyền thẳng qua `ResolvedPublicHtml` xuống `overrides::resolve_and_render_header_footer_with_context`. Trong `overrides::build_preview_context_in_tx` (`overrides.rs:2312-2350`), **giữ nguyên** `menu::list_slot_assignments_in_tx` + logic merge & kiểm lỗi `422` cho `transient.menu_slots` và gọi `menu::resolve_menu_view_for_path_in_tx(tx, menu_id, None)`; `preview_typed_override_with_cache` (`overrides.rs:1540-1544`) gọi `menu::resolve_slot_menus_in_tx(&mut tx, &active.manifest.theme_id)` (đã được batch hóa). Các caller đọc entry/terms/taxonomies khi đang giữ `tx` (`overrides.rs:1505, 2504, 2816`) bắt buộc dùng `_in_conn(&mut **tx, ...)` để tránh deadlock trên pool `max_connections(1)`.
- **`MenuItemView` fields (`storage/menu.rs`):**
  - `pub is_active: bool`, `pub is_ancestor: bool`, `pub is_external: bool`, `pub depth: usize` (**1-based** `1..=MAX_MENU_DEPTH`, khớp `build_tree(None, ..., 1)` ở `menu.rs:916`).
  - `is_external = (target_kind == "external")`.
  - Hàm `normalize_menu_path(p: &str) -> String`: cắt bỏ `#fragment` và `?query`, loại bỏ `/` thừa ở cuối trong khi giữ nguyên `"/"`.
  - `is_active = !is_external && current_path.is_some_and(|p| normalize_menu_path(p) == normalize_menu_path(&url))`; **`is_ancestor`** tính thuần bottom-up (`children.iter().any(|c| c.is_active || c.is_ancestor)`). Truyền `current_path = None` tại 404 và Preview để không match nhầm.

#### 3.4 `PaginationView`, `SiteContext`, `breadcrumbs`, Body Classes, Kiến trúc Luồng `ResolvedPublicHtml` & Full Preview Parity
- **`PaginationView` (`content_render.rs`, `storage/entries.rs`, `storage/public_content.rs`):**
  - Đọc `(entries, total_items)` trong **một transaction duy nhất** qua `public_content::list_published_with_total` (tránh torn read giữa `has_next` và `total_pages`), tính `computed_pages = (total_items + per_page - 1) / per_page`, clamp `total_pages = computed_pages.min(crate::public_context::MAX_ARCHIVE_PAGE)` (`10_000`), đặt `has_prev = previous.is_some()` và `has_next = next.is_some()`.
  - Công thức `pages: Vec<PaginationPageLink>` (`{ pub number: Option<i64>, pub url: Option<String>, pub is_current: bool, pub is_ellipsis: bool }`):
    - `total_pages == 0` → `vec![]`.
    - `total_pages <= 9` → `1..=total_pages`.
    - `total_pages > 9` (`p = page.clamp(1, total_pages)`): `p <= 5` → `[1..=7, Ellipsis, total_pages]`; `p >= total_pages - 4` → `[1, Ellipsis, (total_pages - 6)..=total_pages]`; ngược lại → `[1, Ellipsis, (p - 2)..=(p + 2), Ellipsis, total_pages]` (luôn `<= 9` phần tử). Dùng `CanonicalUrlBuilder::with_query` (bỏ `?page=1`), `is_current = (number == Some(page))`.
    - Route không phân trang / 404 / Preview: `{ page: 1, per_page: 20, total_items: 0, total_pages: 0, has_prev: false, has_next: false, previous_url: None, next_url: None, pages: [] }`.
- **`SiteContext<'a>` (`content_render.rs`, `routes/public_content.rs`, `storage/overrides.rs`):**
  - Thêm `timezone: &'a str`, `origin: &'a str` (tải `PublicSiteSettings::public_origin` trong 4 handlers của `routes/public_content.rs`, mở rộng tuple trả về của `overrides::build_preview_context_in_tx` (`overrides.rs:2266-2352`) thành `(GeneralSettings, PublicSiteSettings, BTreeMap<String, Value>, BTreeMap<String, MenuViewModel>)` qua `settings::get_public_in_conn(&mut **tx)` cho `L2509` và `L2835`, và trực tiếp tại `overrides::preview_typed_override_with_cache` (`overrides.rs:1522-1526`), trim `/` cuối, `""` nếu chưa cấu hình), `current_year: i32` (`i32::from(jiff::Timestamp::now().to_zoned_datetime(tz).year())`). Cập nhật `const DEFAULT_SITE: SiteContext<'static>` (`timezone: "UTC", origin: "", current_year: 1970`) và test literals (`content_render.rs:493-496`).
- **`breadcrumbs: Vec<BreadcrumbItem>` trên cả 5 Route:**
  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct BreadcrumbItem {
      pub name: String,
      pub url: Option<String>,
      pub is_current: bool,
  }
  ```
  - Tham số `(home_label: &str, home_url: Option<&str>, not_found_label: &str)` (mặc định `"Trang chủ"`, `Some("/")`, `"404"`) làm seam localize:
    1. `Home`: `[ { home_label, home_url, true } ]`.
    2. `Archive`: `[ { home_label, home_url, false }, { content_type.label, Some(archive_url), true } ]`.
    3. `Taxonomy`: `[ { home_label, home_url, false } ]` + chuỗi term cha trực hệ tải bằng **1 truy vấn `WITH RECURSIVE`** (cùng `taxonomy_kind`, giới hạn tối đa `8` cấp khớp `public_content.rs:495`, kèm `visited: HashSet<i64>` trên Rust chống vòng lặp) + 1 lần gọi `batch_canonical_term_paths_in_tx` (`O(1)` SQL, `is_current: false`) + `{ term.name, term_url, true }`.
    4. `Single`: `[ { home_label, home_url, false } ]` + (nếu `type_key != "page"` và CPT `has_archive`: `{ content_type.label, archive_url, false }`) + (nếu có `primary_category`: `{ cat.name, cat.url, false }`) + `{ entry.title, entry.url, true }`.
    5. `NotFound`: `[ { home_label, home_url, false }, { not_found_label, None, true } ]`.
- **Semantic Body Classes & Ngôn ngữ 404/Preview (`content_render.rs`, `routes/public_content.rs`, `overrides.rs`):**
  - Thêm `compose_document_with_seo_with_body_classes(header_html, rendered_layout, footer_html, seo, body_classes: &[&str]) -> ApiResult<String>` (giữ `compose_document_with_seo` gọi `&[]`): nếu `parts.body_attributes` đã có cặp `("class", existing)`, **tách whitespace, ghép & deduplicate** `body_classes` thành 1 thuộc tính `class="..."` duy nhất; nếu chưa có mới thêm `"class"`. Các class chuẩn: `["od-route-home"]`, `["od-route-single", "od-type-<key>"]`, `["od-route-archive", "od-type-<key>"]`, `["od-route-taxonomy", "od-tax-<kind>"]`, `["od-route-not-found"]`.
  - Cập nhật `SeoView::for_noindex_page(&pub_settings.content_language, ...)` tại `routes/public_content.rs:248`, `overrides.rs:2799`, và `overrides.rs:2966`. `ReadingPageState::Unconfigured` (`routes/public.rs:41-52`) không đổi.
- **Lazy `related_entries` & `recent_posts`: Kiểm Đệ quy DB-Override-First & Luồng Dữ liệu `ResolvedPublicHtml` (`overrides.rs`, `routes/public_content.rs`):**
  - Thêm `pub uses_related_entries: bool` và `pub uses_recent_posts: bool` ngay trên `CompiledCsecComposition` (ghi nhận khi validate AST đã flatten gồm cả mọi nested `od_part`).
  - Trong `storage/overrides.rs`, định nghĩa đầy đủ các kiểu dữ liệu & hàm đệ quy DB-override-first (`depth <= 3`, cycle-checked theo `stack`, khớp `overrides.rs:1806-1874`):
    ```rust
    #[derive(Debug, Default)]
    pub struct PrecompiledPublicShell {
        pub header: Option<crate::template_sec::CompiledCsecComposition>,
        pub footer: Option<crate::template_sec::CompiledCsecComposition>,
    }

    pub struct PublicSurfaceInspection {
        pub precompiled_body: Option<crate::template_sec::CompiledCsecComposition>,
        pub precompiled_shell: PrecompiledPublicShell,
        pub uses_related_entries: bool,
        pub uses_recent_posts: bool,
        pub has_shell_or_theme: bool,
    }

    pub async fn inspect_public_surfaces_and_precompile(
        db: &SqlitePool,
        resolution: &TemplateResolution,
        active: Option<&ActiveThemeV3>,
        url_policy: &crate::template_sec::ThemeUrlPolicy,
    ) -> ApiResult<PublicSurfaceInspection>

    pub async fn resolve_and_render_header_footer_with_context(
        db: &SqlitePool,
        active_v3: Option<&ActiveThemeV3>,
        theme_context: &serde_json::Value,
        precompiled_shell: &PrecompiledPublicShell,
        fmt_cfg: &crate::template_sec::RenderFormattingConfig,
        budget: &mut PublicCompositionBudget,
        cache: &page::BlockTemplateCache,
    ) -> ApiResult<(Option<String>, Option<String>, bool)>
    ```
    - Nếu body là `TemplateResolution::ActiveTheme { target_key, html }` (`overrides.rs:437`): compile `CompiledCsecComposition` một lần cho body và **lưu sẵn `precompiled_body: Option<CompiledCsecComposition>`** vào `PublicSurfaceInspection` để tránh compile lặp.
    - Nếu body là `TemplateResolution::DbOverride(doc)`: duyệt đệ quy `BuilderDocument` theo thứ tự ưu tiên DB override `part:<key>` trước → `active.parts.get(key)` (gồm `core/template_part` lồng trong `part:*` override tới độ sâu `<= 3`), hợp `OR` các cờ `uses_related_entries` / `uses_recent_posts` của các ActiveTheme part thực sự chạm tới (bỏ qua part bị DB override khác thay thế hoàn toàn).
    - Nếu shell `header`/`footer` sẽ render (`!body_canvas_layout`): kiểm tra đệ quy tương tự cho slot `"header"` và `"footer"`, đồng thời **giữ sẵn bản `CompiledCsecComposition`** của active `header` / `footer` trong `PrecompiledPublicShell` để tái sử dụng khi render shell (không compile 2 lần!). Trong `resolve_and_render_header_footer_with_context`: mỗi slot `"header"`/`"footer"` luôn kiểm tra DB override `part:<slot>` trước; nếu không có DB override mới dùng `precompiled_shell.<slot>` nếu `Some(...)`, và **fallback compile `active_v3.parts.get(slot)` nếu `None`** (bảo toàn nhánh `PageLayout` `normalize_public_document` thất bại tại `routes/public_content.rs:121-128` trả `canvas_layout: false`).
  - Thay tham số untyped `route_context: serde_json::Value` ở `routes/public_content.rs::resolve_public_template_html` (`L44-75`) và `render_resolved_public_template_html` (`L77-108`) bằng struct có kiểu tĩnh `PublicRouteContext<'a>`, bổ sung `cache: &page::BlockTemplateCache`, và trả về `ResolvedPublicHtml` mở rộng *(giữ nguyên tên trường `has_active_contact_form` trong Spec 1; việc đổi sang `has_active_public_form` giữ nguyên làm seam Spec 2 / Stage 5 theo `concepts_brief.md:86`)*:
    ```rust
    // routes/public_content.rs
    struct PublicRouteContext<'a> {
        route_kind: &'a str,
        route_path: Option<&'a str>,
        entry_id: Option<i64>,
        entry_type_key: Option<&'a str>,
        breadcrumbs: &'a [content_render::BreadcrumbItem],
        pagination: &'a content_render::PaginationView,
        archive_url: Option<&'a str>,
        previous_url: Option<&'a str>,
        next_url: Option<&'a str>,
        entries: &'a [content_render::EntryView],
        taxonomy: Option<&'a serde_json::Value>,
        content_type: Option<&'a serde_json::Value>,
        entry: Option<&'a content_render::EntryView>,
    }

    async fn resolve_public_template_html(
        db: &SqlitePool,
        candidates: &[opendash_core::theme::TemplateCandidate],
        active: Option<&theme_storage::ActiveThemeV3>,
        site_ctx: &SiteContext<'_>,
        theme_opts: &BTreeMap<String, serde_json::Value>,
        fallback_html: &str,
        route_entry: Option<(&str, &str)>,
        route_ctx: PublicRouteContext<'_>,
        budget: &mut overrides::PublicCompositionBudget,
        cache: &page::BlockTemplateCache,
    ) -> ApiResult<ResolvedPublicHtml>

    async fn render_resolved_public_template_html(
        db: &SqlitePool,
        resolution: TemplateResolution,
        active: Option<&theme_storage::ActiveThemeV3>,
        site_ctx: &SiteContext<'_>,
        theme_opts: &BTreeMap<String, serde_json::Value>,
        fallback_html: &str,
        route_entry: Option<(&str, &str)>,
        route_ctx: PublicRouteContext<'_>,
        budget: &mut overrides::PublicCompositionBudget,
        cache: &page::BlockTemplateCache,
    ) -> ApiResult<ResolvedPublicHtml>

    pub struct ResolvedPublicHtml {
        pub html: String,
        pub has_active_contact_form: bool,
        pub canvas_layout: bool,
        pub theme_context: serde_json::Value,
        pub precompiled_shell: PrecompiledPublicShell,
    }

    // storage/public_content.rs
    pub async fn query_related_entries(
        db: &SqlitePool,
        entry_id: i64,
        type_key: &str,
        limit: usize,
    ) -> ApiResult<Vec<crate::content_render::EntryView>>

    pub async fn query_recent_posts(
        db: &SqlitePool,
        exclude_post_id: Option<i64>,
        locale: Option<&str>,
        limit: usize,
    ) -> ApiResult<Vec<crate::content_render::EntryView>>
    ```
    1. **Hoist `url_policy`, kiểm bề mặt & truyền `PublicRouteContext<'_>` từ 4 handler (`L228`, `L368`, `L548`, `L767`):** Mỗi handler (`render_not_found_response:228`, `render_canonical_archive:368`, `render_entry_response_with_cache:548`, `render_canonical_taxonomy:767`) dựng `PublicRouteContext<'_>` trực tiếp từ tham chiếu Rust sẵn có (không qua trung gian `serde_json::json!`). Trong `render_resolved_public_template_html`, tạo `let url_policy = overrides::active_theme_url_policy(active)?;` trước khi gọi `inspect_public_surfaces_and_precompile(db, &resolution, active, &url_policy)`. Chỉ bỏ qua `resolve_active_slot_menus_for_path(db, route_ctx.route_path, None)` (`0` query menu) khi `!inspection.has_shell_or_theme && matches!(resolution, TemplateResolution::CoreFallback)`. Khi có shell/theme hoặc DB override, dựng đầy đủ `theme_context` qua `content_render::build_public_theme_context(PublicThemeContextInput { ... })` ánh xạ trực tiếp từ `route_ctx`, `menus` (`current_path = route_ctx.route_path`), `site`, `options`, `theme_assets` và `related_entries`/`recent_posts`, tạo `let fmt_cfg = RenderFormattingConfig { timezone: site_ctx.timezone.to_owned() };`, chuyển `TemplateResolution::DbOverride` (`routes/public_content.rs:141`) từ `overrides::render_resolved_public_builder` sang `overrides::render_resolved_public_builder_with_cache(..., cache)` (`overrides.rs:1928`), tái sử dụng `inspection.precompiled_body` trên `ActiveTheme`, **chuyển `budget.render_theme` tại `routes/public_content.rs:191` sang `budget.render_theme_with_config(&compiled_body, &context, Some(&slot_html), &fmt_cfg)`** (cùng chuyển 3 fallback `content_templates` ở `routes/public_content.rs:356, 539, 753` sang `render_archive_with_site_and_config`/`render_single_with_context` truyền `&fmt_cfg`), rồi trả toàn bộ ra `ResolvedPublicHtml`.
    2. **Chi tiết thực thi `query_related_entries` & `query_recent_posts`:**
       - `query_related_entries(db, entry_id, type_key, limit)` tự đọc `locale` của bài gốc bên trong read transaction của chính nó (`SELECT locale FROM content_entries WHERE id = ?`, trả `Ok(vec![])` nếu không tồn tại — giữ nguyên `ContentEntryRecord` không thêm cột `locale`), clamp `limit.min(20) as i64`, tìm tối đa `limit` bài cùng `e.content_type_key = ? AND e.locale = ? AND e.id != ?` có chung >= 1 public eligible active declared taxonomy term (`e.publication_status = 'published'`, `t.provider_status = 'active'`, `COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1`, `ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC LIMIT ?`, không backfill).
       - `query_recent_posts(db, exclude_post_id, locale, limit)` mặc định `locale` qua `settings::get_default_locale_code_in_conn(&mut *tx)` khi `locale.is_none()`, lọc `e.content_type_key = 'post' AND e.locale = ? AND e.publication_status = 'published'` (trừ `exclude_post_id` nếu `Some`), `t.provider_status = 'active'`, `COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1`, `ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC LIMIT ?`. Cả hai hàm đều **batch-hydrate theo chuẩn `O(1)` SQL** (dùng `seo::resolve_public_entry_paths_by_entry_ids_in_conn`, `batch_load_public_entry_terms_in_conn`, `load_eligible_public_taxonomies_in_conn`/`batch_load_eligible_public_taxonomies_in_conn`, `content::load_effective_content_schema_in_tx`).
       - Trong `render_resolved_public_template_html`: chỉ gọi `public_content::query_related_entries(db, eid, tkey, 6)` khi `inspection.uses_related_entries` và `(Some(eid), Some(tkey)) = (route_ctx.entry_id, route_ctx.entry_type_key)`; chỉ gọi `public_content::query_recent_posts(db, exclude_post_id, None, 6)` khi `inspection.uses_recent_posts` (với `exclude_post_id = (route_ctx.entry_type_key == Some("post")).then_some(route_ctx.entry_id).flatten()`; đồng bộ `locale: None` → default locale giống `resolve_active_slot_menus_for_path`); ngược lại dùng `&[]`.
    3. Cả 4 route (`L240, L389, L562, L786`; trong đó 3 hàm `L213, L268, L642` chưa có tham số `cache` dùng `crate::page::BlockTemplateCache::global()`) truyền `&body.theme_context`, `&body.precompiled_shell`, `&fmt_cfg`, và `cache` vào `overrides::resolve_and_render_header_footer_with_context`.
- **Thống nhất Toàn bộ Preview, Schema & Chữ ký `content_render::build_public_theme_context`, Starter Strict Parity & Admin Preview Routes:**
  - **Kiểu & Chữ ký chuẩn tại `content_render.rs`:** Mọi route Public lẫn mọi đường Preview dùng chung `build_public_theme_context` để đảm bảo luôn có đủ khóa nền (`"site"`, `"options"`, `"theme_options"`, `"theme_assets"`, `"menus"`, `"route": { "kind", "path" }`, `"breadcrumbs"`, `"pagination"`, `"archive_url"`, `"previous_url"`, `"next_url"`, `"entries"`, `"related_entries"`, `"recent_posts"`, `"taxonomy"`, `"content_type"`, `"entry"`), giúp 100% template Starter (`taxonomy.html`, `index.html`, `single.html`) lẫn Theme v3 vượt `UndefinedBehavior::Strict`:
    ```rust
    pub struct PublicThemeContextInput<'a> {
        pub site: &'a SiteContext<'a>,
        pub options: &'a BTreeMap<String, serde_json::Value>,
        pub theme_assets: &'a BTreeMap<String, String>,
        pub menus: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
        pub route_kind: &'a str,
        pub route_path: Option<&'a str>,
        pub breadcrumbs: &'a [BreadcrumbItem],
        pub pagination: &'a PaginationView,
        pub archive_url: Option<&'a str>,
        pub previous_url: Option<&'a str>,
        pub next_url: Option<&'a str>,
        pub entries: &'a [EntryView],
        pub related_entries: &'a [EntryView],
        pub recent_posts: &'a [EntryView],
        pub taxonomy: Option<&'a serde_json::Value>,
        pub content_type: Option<&'a serde_json::Value>,
        pub entry: Option<&'a EntryView>,
    }

    pub fn build_public_theme_context(
        input: PublicThemeContextInput<'_>,
    ) -> ApiResult<serde_json::Value>

    pub fn render_archive_with_site_and_config(
        schema: &ContentTypeDefinition,
        entries: &[EntryView],
        previous_url: Option<&str>,
        next_url: Option<&str>,
        pagination: &PaginationView,
        breadcrumbs: &[BreadcrumbItem],
        taxonomy: Option<&serde_json::Value>,
        selected_template: Option<&str>,
        site: &SiteContext<'_>,
        fmt_cfg: &crate::template_sec::RenderFormattingConfig,
    ) -> ApiResult<String>

    pub fn render_single_with_context(
        schema: &ContentTypeDefinition,
        entry: &EntryView,
        selected_template: Option<&str>,
        site: &SiteContext<'_>,
        archive_url: Option<&str>,
        breadcrumbs: &[BreadcrumbItem],
        fmt_cfg: &crate::template_sec::RenderFormattingConfig,
    ) -> ApiResult<String>

    pub fn render_role_fragment_with_context_and_config(
        template: &str,
        site: &SiteContext<'_>,
        options: &BTreeMap<String, serde_json::Value>,
        menus: &BTreeMap<String, crate::storage::menu::MenuViewModel>,
        fmt_cfg: &crate::template_sec::RenderFormattingConfig,
    ) -> Option<String>
    ```
    *(Giữ các wrapper `render_archive_with_site`, `render_single_with_site_and_archive`, `render_role_fragment_with_context` cho callers/tests cũ.)*
  - Hợp nhất ngữ cảnh dựng cho Public & **cả hai nhánh `Some(entry)` lẫn `None`** trong `overrides::resolve_and_render_header_footer_with_context`, `overrides::render_preview_named_part_in_tx` (`overrides.rs:2355-2412`, gọi từ `L2766-2787` và `L2939-2960`), `overrides::preview_typed_override_with_cache` (`L1545-1605`), `overrides::preview_studio_with_cache` (`L2541-2628`), và `overrides::wrap_entry_body_in_preview_shell_in_tx_with_budget` (`L2815-2978`) thông qua `build_public_theme_context(...)`:
    - Luôn cấp `"theme_assets"` (từ `ActiveThemeV3::asset_url_map` khi có theme active, hoặc `{}`), `"route": { "kind": ..., "path": Option<String> }` (`path: null` ở 404/part/preview chưa xuất bản), `"previous_url": null`, `"next_url": null` (trừ `Archive`/`Taxonomy` có URL phân trang), và `"taxonomy"` là object non-null `{ "key": tax_kind, "id": 0, "slug": "sample-term", "name": "Chuyên mục mẫu" }` cho `K::Taxonomy` (các kind khác là `null`).
    - **`"archive_url"`:** Luôn có mặt dưới dạng key trên `theme_context`:
      - Chế độ placeholder (`entry.is_none()`): điền `Some("/posts")` cho **`K::Single | K::SingleCustom | K::Singular | K::Index | K::Archive`**, và `None` cho `K::Home | K::Taxonomy | K::NotFound | K::Part` (bảo vệ toàn bộ chuỗi kế thừa reset của `opendash-starter/single.html:1`).
      - Chế độ bài thật (`Some(entry)` trong Preview & các route Public): giải bằng `permalinks::canonical_archive_path_in_tx` (`None` nếu CPT tắt archive/page theo chuẩn C3 — lưu ý Starter `single.html` bất biến vốn dùng cho CPT có archive, còn `page` dùng `page.html` và `opendash-starter-1.0.1/single-post.html` bọc `{% if archive_url %}`).
    - Khi `Some(entry)` trong 3 luồng preview `overrides.rs` (`L1505, L2504, L2816` đang giữ `tx`): luôn chuyển sang `authenticated_preview_view_with_context` + **`batch_load_public_entry_terms_in_conn(&mut **tx, ...)`** + **`load_eligible_public_taxonomies_in_conn(&mut **tx, ...)`**, truyền **`related_entries: &[]`** và **`recent_posts: &[]`** (đồng bộ biên giới preview và tuyệt đối không gọi truy vấn `&SqlitePool` trong khi giữ `tx`, tránh deadlock pool `max_connections(1)`), và dựng **`breadcrumbs`** thuần in-memory theo quy tắc `Single` từ `archive_url` (`permalinks::canonical_archive_path_in_tx(&mut **tx, ...)` đã gọi) + `view.primary_category` (0 SQL bổ sung), cập nhật fallback ở `overrides.rs:1585-1587` & `L2587-2589` sang `content_render::render_single_with_context(&effective_schema.base, &view, None, &site, archive_url.as_deref(), &breadcrumbs, &fmt_cfg)`, rồi truyền toàn bộ qua `build_public_theme_context` (`build_preview_context_in_tx` **vẫn giữ** `list_slot_assignments_in_tx` + merge/validate `transient.menu_slots` và gọi `menu::resolve_menu_view_for_path_in_tx(tx, menu_id, None)`). Khi `None`: xóa `"fields": {}` sai kiểu (`overrides.rs:2611`), dùng placeholder `EntryView` đầy đủ (`fields: []`, `values: {}`, `terms: {"category": [], "tag": []}`, `all_terms: []`, `primary_category: null`, `excerpt: ""`, `reading_time_minutes: 0`, `published_at: null`, `type_key`), `content_type: { "key", "label" }`, `entries: []`, `breadcrumbs`, `pagination` (rỗng an toàn), `previous_url: null`, `next_url: null`, `related_entries: []`, `recent_posts: []`.
  - Đồng thời `resolve_and_render_header_footer_with_cache` (`overrides.rs:1992-1994`) dựng `part_context` qua `content_render::build_public_theme_context` (với `K::Part` nền an toàn) để cả caller/test legacy cũng vượt `UndefinedBehavior::Strict`.
  - Hai route Admin Preview `content_entry_preview` (`routes/content.rs:107-146`) và `entry_preview` (`routes/entries.rs:555-632`) tải `eligible_public_taxonomies` via `batch_load_eligible_public_taxonomies`, dựng `EntryView` qua `authenticated_preview_view_with_context` (`actor_role` thật + `batch_load_public_entry_terms`), giải `archive_url` qua `permalinks::canonical_archive_path_in_tx` (hoặc `None`) và dựng `Single` `breadcrumbs`, tải `GeneralSettings` + `PublicSiteSettings` rồi gọi `render_single_with_context`.

---

### 4. Offline CLI Validator `opendash --check-theme` (`main.rs`, `theme.rs`)

- **Xử lý mọi tham số CLI sớm bằng `std::env::args_os()` (`main.rs:40-64`):**
  - Chuyển cả `--check-config`, `--operations-worker`, `--plugin-worker`, và `--check-theme` sang duyệt `std::env::args_os()` (không dùng `std::env::args()` dễ panic trên đối số non-UTF-8).
  - Chấp nhận đúng 2 dạng `--check-theme <path>` và `--check-theme=<path>`, dùng helper đa nền tảng tách prefix `--check-theme=` trực tiếp trên `OsStr` (`#[cfg(unix)] std::os::unix::ffi::OsStrExt`, `#[cfg(windows)] std::os::windows::ffi::OsStrExt::encode_wide`). Từ chối `--check-theme=` rỗng, thiếu `<path>`, hoặc thừa đối số lạ với `{ "ok": false, "error": "..." }` lên `stdout` và `std::process::exit(1)`.
- **Bảo vệ đọc an toàn (`theme::check_theme_path(path: &Path)` & `theme::load_v3_files_from_dir(root: &Path)`):**
  1. Kiểm `fs::metadata(path)` phân biệt regular file vs directory (cho phép root arg hoặc parent trên OS là symlink như `/var` trên macOS).
  2. Nếu là regular file (kể cả không có đuôi `.zip`): kiểm `meta.len() <= V2_MAX_ZIP_BYTES` **trước** khi đọc, sau đó dùng `std::fs::File::open` + `Read::take((V2_MAX_ZIP_BYTES + 1) as u64)` để chống TOCTOU/file phình to OOM (vượt → `Err`), rồi chạy `theme::parse_theme_zip_v3(&bytes)`.
  3. Nếu là thư mục: duyệt đệ quy với giới hạn độ sâu `MAX_THEME_DIR_DEPTH = 16` qua `fs::symlink_metadata`: từ chối entry bắt đầu bằng `.`, tên file non-UTF-8 (`Err`), symlink nội bộ, FIFO/device/socket; chuẩn hóa `/` đa OS, kiểm `<= V2_MAX_FILES`, đọc từng file qua `File::open` + `.take((cap + 1) as u64)` kết hợp pre-read `meta.len() <= cap` & tổng `<= V2_MAX_DECODED_BYTES`, chống va chạm ASCII case-insensitive (`to_ascii_lowercase()`), rồi gọi `theme::validate_theme_v3_files(files)`.
- **Định dạng JSON cố định trên `stdout` + `std::process::exit`:**
  - Thành công (`exit(0)`): 1 dòng JSON gồm `ok: true`, `theme_id: String` (`manifest.theme_id` tại `opendash-core/src/theme.rs:354`), `version: String` (`manifest.version`), `templates`, `parts`, `page_layouts`, `patterns`, `blocks`, `assets` (đều là `Vec<String>` sort ASCII tăng dần), và `options_count: usize` (`parse_theme_options_schema(&manifest_json)?.len()`).
  - Thất bại (`exit(1)`): 1 dòng JSON `{ "ok": false, "error": "<thông báo chi tiết>" }`.

---

## Alternatives Considered

| Điểm quyết định | Phương án đã bác bỏ | Lý do bác bỏ |
|---|---|---|
| Cú pháp tài nguyên Theme | Directive `{{ od_asset("assets/logo.png") }}` hoặc `theme_assets.key` | `od_asset(...)` đụng lệnh cấm `Expr::Call`; asset path chứa `/` và `.` không thể truy cập bằng dấu chấm; `theme_assets["assets/..."]` kết hợp giải mã HTML entity ở `document.rs` an toàn tuyệt đối. |
| Scanner HTML & Provenance URL | Giữ `ensure_composition_body_context` / lookahead `EmitRaw` cục bộ / hội tụ chỉ theo `ThemeHtmlState` | Gây lệch quy tắc `1 < 2`, hở biên `<{{ "script" }}>` và hở vòng lặp `ForLoop` lượt 2+ khi `{% set %}` đột biến frame cha; hợp nhất thành `walk_theme_ast_with_url_scopes` có `BodyLt*` + hội tụ kép `(states, UrlScopeStack)` + phép giao `IfCond` chặt chẽ hoàn toàn. |
| Hook lazy `related_entries` / `recent_posts` | Đặt trong `resolve_public_template_html` hoặc quét 1 tầng `DbOverride` | Bỏ sót Single/Homepage (`L548`) và bỏ sót `core/template_part` trong DB `part:*` override lồng nhau; `inspect_public_surfaces_and_precompile` (DB-override-first, depth 3) + `ResolvedPublicHtml` giải quyết triệt để. |
| Cập nhật Starter nhúng ở S03 | Sửa trực tiếp `opendash-starter` (`1.0.0`) hoặc `1.0.1` | Gãy test `v100_digest` và gây `409 Conflict` trên DB hiện hữu; tách fixture Theme v3 độc lập bảo toàn Starter nhúng bất biến. |
| Múi giờ & ngày thuần `ContentFieldType::Date` | Parse `"YYYY-MM-DD"` thành UTC nửa đêm rồi convert TZ | Lệch ngược sang ngày hôm trước trên múi giờ âm (`America/New_York`); nhánh civil `Date::at(0,0,0,0)` tách riêng khắc phục triệt để. |
| Batch C3 URL cho `related_entries` & terms | Fallback `CanonicalUrlBuilder::entry` hoặc `term_path(...)` khi thiếu claim | Tạo URL giả cho CPT private hoặc route đụng độ; giữ JOIN thuần trên `permalink_route_claims` + `taxonomy_definitions` tuân thủ tuyệt đối bất biến C3. |

---

## Non-Functional Requirements

1. **`O(1)` Truy vấn SQL (Zero N+1):** Số lượng truy vấn SQL cho một trang Archive hoặc Single giữ ở mức `O(1)` cố định bất kể số lượng bài viết `N` trên trang (thay các vòng `O(N)` ở Entries/Terms/SEO/Menu bằng `<= 5` truy vấn batch cố định); trang `CoreFallback` không có shell DB/Theme chạy `0` truy vấn menu.
2. **Bảo toàn `MAX_SCHEMA_OBJECTS = 256`:** Chỉ thêm 1 cột (`ALTER TABLE content_entries ADD COLUMN published_at TEXT` tại `0049`), tạo thêm **0 schema objects mới**, an toàn tuyệt đối cho `storage/restore.rs`.
3. **Chống OOM, Bypass & Crash `UndefinedBehavior::Strict`:** Trần cứng mọi filter (`500` items, `1000` output, `128 KiB` string, `1..=100` batch, `<= 32` chữ số integer), chặn dunder `__` ở cả `GetAttr`/`GetItem`/`attr` path, đầy đủ placeholder & nullable values.
4. **Không gãy tương thích ngược Theme v2/v3:** Giữ nguyên toàn bộ API/khóa cũ (`previous_url`, `next_url`, `options`, `theme_options`, `GLOBAL_PUBLIC_URL_OPTION_KEYS`, `entry.values`, `entry.fields`, `updated_at`, `DEFAULT_SITE`, `compose_document_with_seo`, tĩnh `style`/`srcset`/`ping`).
5. **Single-Binary Portable:** `jiff = "=1.0.2"` với `tzdb-bundle-always`, `--check-theme` chạy offline 100% qua `args_os()` đa nền tảng không cần DB/locks.

---

## Detailed Implementation Plan

### Slice S01: C-SEC Bounded Filters/Tests, `jiff` Pin, `theme_assets`, `document.rs` Entity Decode & `walk_theme_ast_with_url_scopes`
- **Dependencies:** None (thuần C-SEC + Cargo workspace + `document.rs` + `ThemeUrlPolicy` + `HomeState.timezone`).
- **File Ownership:**
  - `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-server/Cargo.toml`
  - `cms/crates/opendash-server/src/template_sec.rs` (`render_csec_with_config`, wrapper `default`/`d` cho `none` + `env.set_formatter`, 7 filters dùng `Rest<ValueOrKwargs>` / `State` & `s.len() == 10 && s.is_ascii()` trong `date_format` + 21 `ALLOWED_TESTS`, `ThemeUrlPolicy::new`, `walk_theme_ast_with_url_scopes` thay `ALLOWED_DYNAMIC_URL_VARS` + linear pre-render scanner + `ensure_composition_body_context` `L1147-1212` / `ensure_composition_body_sequence` `L1214-1224` trong khi giữ `enum HtmlScanState`, gia cố phòng thủ post-render `validate_html_and_css_context_with_url_vars` thành 1 pass HTML + RAWTEXT `<style>` + RCDATA `<title>`)
  - `cms/crates/opendash-server/src/document.rs` (`is_authorized_stylesheet_link` decode HTML5 entities `&#x2f;` + chặn control chars, `tokenize` rẽ nhánh `bytes.get(i + 1)` trước `find_tag_end` bảo toàn `1 < 2` & `< !doctype>` làm `Text` trong khi chặn `<?`/`</[^a-zA-Z]`/`<!doctypex>`/`<!CDATA[`, `find_case_insensitive_end_style` đòi `b'>'` ngay sau `</style>`, và `#[cfg(test)]`)
  - `cms/crates/opendash-server/src/theme.rs` (`render_content_with_config` gọi `template_sec::render_csec_with_config`; `validate_theme_v3_files` giữ nguyên wiring `ThemeUrlPolicy`)
  - `cms/crates/opendash-server/src/storage/theme.rs` (`ActiveThemeV3::asset_url_map`)
  - `cms/crates/opendash-server/src/storage/settings.rs` (`L933-936` re-export `GLOBAL_PUBLIC_MEDIA_OPTION_KEYS`, `theme_url_policy` & `global_theme_url_policy` `L1549-1574`: giữ `GLOBAL_PUBLIC_URL_OPTION_KEYS` + thêm `GLOBAL_PUBLIC_MEDIA_OPTION_KEYS` & `ImageRef`/`Media`/`declared_assets`)
  - `cms/crates/opendash-server/src/storage/builder.rs` (`HomeState.timezone`, `impl Default for HomeState`, `read_home_tx`, thêm `timezone_override: Option<&str>` vào `preview_saved_page_body_in_tx_with_options_and_budget` `L1713-1747` & caller `L1707`)
  - `cms/crates/opendash-server/src/storage/overrides.rs` (`pub(crate) fn active_theme_url_policy` ở `L1134`, `PublicCompositionBudget::render_theme_with_config`, `PreviewRenderContext.fmt_cfg` `L1241`, `PublicBuilderContext.fmt_cfg` `L1760`, `render_preview_named_part_in_tx:2355-2412` tạo `fmt_cfg` từ `state.timezone`, sync `state.timezone` & truyền `Some(general.timezone.as_str())` tới `preview_saved_page_body_in_tx_with_options_and_budget` ở `L1563`/`L2566`, `render_csec_template_for_preview` `L2414-2448`, `render_preview_part` `L1364`, `render_public_part` `L1842`)
  - `cms/crates/opendash-server/src/page.rs` (`render_plugin_block_csec_with_budget_and_config`, truyền `RenderFormattingConfig` vào `template_sec::render_block_csec_with_options_and_budget_and_config` và plugin blocks)
  - `cms/crates/opendash-server/tests/studio_s02_preview_api.rs` (cập nhật dòng `133` từ `{% set url = options.logo_ref %}<img src="{{ url }}" alt="Logo">` sang `<img src="{{ options.logo_ref }}" alt="Logo">`)
- **Acceptance Criteria:**
  1. `jiff = { version = "=1.0.2", default-features = false, features = ["std", "tzdb-bundle-always", "perf-inline"] }` chỉ nằm trong `opendash-server`; `cargo tree -p opendash-core` không chứa `jiff`.
  2. Toàn bộ 7 filters (`date_format` kèm `s.len() == 10 && s.is_ascii()`, `truncate`, `format_number`, `batch`, `selectattr`/`rejectattr` với `state: &State`, `map` với `Rest<ValueOrKwargs>` + `from_args`), wrapper `default`/`d` thay thế `Value::Null` (`none`), `env.set_formatter` in `""` cho `none`, và 21 `ALLOWED_TESTS` + chặn `"__"` trên `Expr::GetItem` pass mọi `#[cfg(test)]` ở `template_sec.rs`.
  3. `walk_theme_ast_with_url_scopes` chạy trên cả 3 điểm vào (`validate_template_csec_with_theme_urls`, `composition_sites:832`, `preview_composition_sites:283`), thay `ensure_composition_body_context`/`ensure_composition_body_sequence` (giữ `enum HtmlScanState`), pre-seed `("entry", Some(TrustedEntry))` trên `UrlScopeStack` gốc và thi hành live-scope `entry` rebinding, bỏ `field.image_url`/`field.gallery_urls` trần, chặn `b'?'` ở `BodyLt`, chỉ cho `[a-zA-Z]` ở `BodyLtSlash`, chặn `</title >`/`</style >`/`</title/>`/`</style/>` + giữ nguyên CSS partial prefix + cấm mọi non-`EmitRaw` trong `Style`, chặn `Comment(_)` và `Title { closing_prefix: 1.. }` trong `EmitExpr`, chặn `BodyLt*`/`Doctype` + `Body`-only cho `od_content`/`od_part`, kiểm chặt ký tự ASCII trên mọi `TagPhase` (cấm attr/`/` ở `is_close`, cấm quotes/`<` ở `Doctype`, chốt boolean attr từ `AttrName`/`AfterAttrName` sang `BeforeAttrValue`/`/`/`>`/attr kế), duyệt `SetBlock::body` từ `{Body}` + truyền `sb_scopes` + `WithBlock::body` (`extract_bound_idents` fail-closed) với `Option<UrlSourceKind>` tombstone (`None`), bắt `(else_states, else_scopes)` từ `f.else_body` + `next_header_scopes` hội tụ kép trên `ForLoop` (`<= 64`) + giao `IfCond`, 6-state `CommentScanSubState`, `<meta>` chỉ nhận `{charset, name("viewport"), content}` (chặn `http-equiv`), `</link>`/`</meta>`/`</title>`/`</style>`, `<title>`, `FORBIDDEN_TAGS`/`srcdoc`/`formaction`/`action`/`xmlns`/`xlink:href`, tĩnh `style`/`srcset`/`ping`, URL tĩnh `href`/`src`/`poster`/`cite` (cả trong `{% if false %}`), quy tắc single-expr (`attr_expr_count > 0 && attr_has_static_bytes`) **chỉ áp dụng riêng cho URL attrs (`href`/`src`/`poster`/`cite`)** (bảo toàn `alt="... {{ entry.title }}"` trên `DEFAULT_ARCHIVE` & Starter), `<blockquote cite>`/`<q cite>` trong `HrefOnly` (chặn `<cite cite>`), và `GLOBAL_PUBLIC_URL_OPTION_KEYS` giữ nguyên ở cả `theme_url_policy` & `global_theme_url_policy`; post-render `validate_html_and_css_context_with_url_vars` (1 pass HTML + RAWTEXT `<style>` + RCDATA `<title>`) và `document.rs` (`is_authorized_stylesheet_link` + `tokenize` + `find_case_insensitive_end_style`) khóa cùng ranh giới bogus-comment / `<meta>` / `<title>` / `</style >` mà vẫn giữ nguyên plain text `<p>1 < 2</p>`.
  4. `CompiledCsecComposition` xuất chính xác `uses_related_entries` & `uses_recent_posts` từ flattened AST; toàn bộ test `template_sec.rs`, `document.rs` và `studio_s02_preview_api.rs` xanh 100%.
- **Rollback:** Revert PR S01; DB schema & dữ liệu runtime không đổi.

### Slice S02: `0049` `published_at`, Enriched View Context, Zero N+1 Batching, `ResolvedPublicHtml` & Preview Parity
- **Dependencies:** S01 (`RenderFormattingConfig`, `theme_assets`, `document.rs`, `walk_theme_ast_with_url_scopes`, AST usage flags, `timezone_override`, `pub(crate) fn active_theme_url_policy`).
- **File Ownership:**
  - `cms/migrations/0049_entry_published_at.sql`, `cms/migrations/README.md`
  - `cms/crates/opendash-server/src/storage/{entries,entry_table,menu,overrides,permalinks,public_content,seo}.rs`
  - `cms/crates/opendash-server/src/content_render.rs`
  - `cms/crates/opendash-server/src/routes/{content,entries,public_content}.rs`
  - `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs` (`assert_eq!(migration_version, 49);`)
- **Acceptance Criteria:**
  1. Migration `0049_entry_published_at.sql` thêm `content_entries.published_at` (+0 objects trong `sqlite_schema`); 4 đường ghi và toàn bộ đường đọc (`entries.rs` giữ cột `0047` + 4 cột phụ trong `query_entries_filtered`, thêm `require_public_archive_in_conn`, `count_entries_filtered_in_tx`, `list_published_filtered_with_total_in_tx`, `public_content::list_published_with_total` đọc 1 snapshot, `entries.rs:2234-2237`, `entry_table.rs`, `public_content.rs::list_taxonomy_published_in_tx`) biên dịch sạch và đồng bộ `published_at`.
  2. `entry_view_with_context` / `authenticated_preview_view_with_context` (giữ `/api/admin/.../preview` fallback + `/api/admin/media/...` rewrite) dùng `effective_schema.effective_fields` theo declaration order (`base.descriptor.fields` + `attached_groups`) + `strip_richtext_to_plain` + lọc `ShortText | LongText | RichText` (loại `Number | Boolean | Date | Select | Image | Gallery`) + chèn `Value::Null` cho field public rỗng; `primary_category` fallback `ORDER BY t.id ASC`; `batch_load_eligible_public_taxonomies_in_conn`/`load_eligible_public_taxonomies_in_conn` lọc `d.public = 1` không N+1 (không gọi raw `load_effective_taxonomies_in_tx`); `batch_load_public_entry_terms_in_conn` + `batch_canonical_term_paths_in_tx` (`AND c.locale = t.locale`) thuần C3.
  3. Zero N+1 (`O(1)` SQL) trên Archive/Menu/SEO (`query_entries_filtered`, `seo::resolve_public_entry_paths_by_entry_ids_in_conn` khớp `seo.rs:98-103` + `c.locale = e.locale` không thêm `t.declaration_state = 'declared'` đơn phương và không fallback `CanonicalUrlBuilder::entry`, `menu` 3-variant `MenuTargetKind` dùng <= 2 batch queries qua `resolve_menu_view_for_path_in_tx` chung với `preview_menu`, `resolve_active_slot_menus_for_path_in_conn` tôn trọng `locale`, 0 query menu chỉ khi `CoreFallback && !inspection.has_shell_or_theme`); `MenuItemView` có `depth` 1-based, `is_external`, `is_active`, bottom-up `is_ancestor`.
  4. `PaginationView` (`<= 9` window, `MAX_ARCHIVE_PAGE = 10_000`, `has_prev`, `has_next`), `SiteContext` (`origin`, `timezone`, `current_year: i32::from(zdt.year())`), `breadcrumbs` đủ 5 route (`WITH RECURSIVE` `<= 8` + `HashSet`), `compose_document_with_seo_with_body_classes` gộp dedup `"class"`, `content_language` từ `pub_settings` tại `public_content.rs:248` & `overrides.rs:2799, 2966`, `PublicRouteContext<'a>` + `resolve_public_template_html`/`render_resolved_public_template_html` + `PublicSurfaceInspection`/`PrecompiledPublicShell`/`inspect_public_surfaces_and_precompile` (hoist `url_policy`, `TemplateResolution::ActiveTheme { target_key, html }`, fallback `PageLayout`, `routes/public_content.rs:141` dùng `render_resolved_public_builder_with_cache` + `L191` dùng `render_theme_with_config` + `L356, 539, 753` dùng `_with_config` + `L213, 268, 642` dùng `BlockTemplateCache::global()`), `query_related_entries`/`query_recent_posts` (tự đọc `locale` trong tx + batch `O(1)`), `build_preview_context_in_tx` (4-tuple trả về gồm `PublicSiteSettings` + giữ nguyên `transient.menu_slots` overlay/422 + `resolve_menu_view_for_path_in_tx`), nhánh `Some(entry)` preview `overrides.rs` truyền `related_entries: &[]`, `recent_posts: &[]`, `Single` `breadcrumbs` in-memory + gọi `render_single_with_context` ở `L1585, L2587` & `build_public_theme_context` ở `L1992`, `_in_conn` cho mọi caller giữ `tx` (`overrides.rs:1505, 2266, 2504, 2816`), và `PublicThemeContextInput` vượt `UndefinedBehavior::Strict` 100%.
- **Rollback:** Revert PR S02; trường `published_at` giữ tương thích ngược hoặc phục hồi bằng snapshot M8.

### Slice S03: Offline `opendash --check-theme` CLI & E2E Theme Ergonomics Test Suite
- **Dependencies:** S01, S02.
- **File Ownership:**
  - `cms/crates/opendash-server/src/main.rs`
  - `cms/crates/opendash-server/src/theme.rs` (`check_theme_path`, `load_v3_files_from_dir`)
  - `cms/crates/opendash-server/tests/theme_ergonomics_test.rs`
  - `docs/designs/SITEMAP.md` (cập nhật trạng thái khi đóng vòng đời spec)
  - *(Không chỉnh sửa `builtin_themes/opendash-starter/` hay `opendash-starter-1.0.1/`.)*
- **Acceptance Criteria:**
  1. Toàn bộ flag sớm trong `main()` (`--check-config`, `--operations-worker`, `--plugin-worker`, `--check-theme`) dùng `std::env::args_os()` + `OsStrExt` prefix helper đa nền tảng, kiểm pre-read metadata/size + `File::open` + `.take((cap + 1) as u64)` chống TOCTOU, từ chối symlink/hidden/non-UTF-8/FIFO/case-collision & độ sâu `> 16`, xuất JSON chuẩn (`theme_id` từ `manifest.theme_id`, mảng `Vec<String>` sorted, `options_count`) lên `stdout` với exit `0` / `1`.
  2. Unit test dựng trực tiếp struct nội bộ (`HomeState`, `ContentEntryRecord`, `SiteContext`, `MenuItemView`, `document::{is_authorized_stylesheet_link, tokenize, find_case_insensitive_end_style}`) đặt trong `#[cfg(test)]` ở `src/` (do `opendash-server` là `[[bin]]` crate), còn `tests/theme_ergonomics_test.rs` chạy E2E HTTP/CLI với fixture Theme v3 độc lập kiểm chứng CUJ-1..CUJ-5, `<link>` qua `AutoEscape::Html` + `document.rs`, bảo toàn `<p>1 < 2</p>`, `<p>1 < 2 it's ok</p>`, `<p>a < b</p>`, `< !doctype>` thành `TokenKind::Text` qua `document.rs` lẫn từ chối `<!doctypex>` / `<!CDATA[` / `<style>a{}</style >`, Studio preview trên `opendash-starter/taxonomy.html`, `opendash-starter/index.html`, và `opendash-starter/single.html` (`SingleCustom`/`Singular`/`Index`) cùng `transient.menu_slots` & `transient.general.timezone` trên Builder body, Zero N+1 Archive, archive của CPT `withdrawn` không gây 500, chống bypass (`<title></titl{{ site.title }}e><div title="</title><script>alert(1)</script>">`, `<title></title{{ empty }}>`, `<?x <div title="><meta ...>">`, `</1 <div title=">...">`, `<title>x</title ><a href="...">`, `<!-- a{{ options.banner_text }}><a href="..."> -->`, `{% set field = ... %}<img src="{{ field.image_url }}">`, `{% set row = ... %}{% for entry in row %}`, `<style>{% if x %}body{}{% endif %}</style>`, `<meta charset="utf-8" http-equiv="refresh">`, `<a href="javascript:alert(1)">` trong `{% if false %}`, `<{{ "script" }}>`, `<!-- --!>`, `{% set unused %}<script>{% endset %}`, `{% with item = ... %}`, `{% for x in [] %}{% else %}<{% endfor %}`, `ForLoop` lặp `{% set %}`, `{% if %}` `{% set %}`, conditional attributes, `</link>`/`</meta>`, `<br/>`, `<link>` `?`/`#`/whitespace, batch OOM, `Bool` `format_number`), và `--check-theme`.
- **Rollback:** Revert PR S03; không tác động DB hay Starter themes.

### Target Files to Create/Modify
- `cms/Cargo.toml` *(Modify)*
- `cms/Cargo.lock` *(Modify)*
- `cms/crates/opendash-server/Cargo.toml` *(Modify)*
- `cms/migrations/0049_entry_published_at.sql` *(Create)*
- `cms/migrations/README.md` *(Modify)*
- `cms/crates/opendash-server/src/template_sec.rs` *(Modify)*
- `cms/crates/opendash-server/src/document.rs` *(Modify)*
- `cms/crates/opendash-server/src/theme.rs` *(Modify)*
- `cms/crates/opendash-server/src/page.rs` *(Modify)*
- `cms/crates/opendash-server/src/content_render.rs` *(Modify)*
- `cms/crates/opendash-server/src/main.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/builder.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/entries.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/entry_table.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/menu.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/overrides.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/permalinks.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/public_content.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/seo.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/settings.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/theme.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/content.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/entries.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/public_content.rs` *(Modify)*
- `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs` *(Modify)*
- `cms/crates/opendash-server/tests/studio_s02_preview_api.rs` *(Modify)*
- `cms/crates/opendash-server/tests/theme_ergonomics_test.rs` *(Create)*
- `docs/designs/SITEMAP.md` *(Modify)*

### Verification Plan
Chạy tuần tự sau mỗi slice và trước khi đóng PR *(dùng `python` và `.\cms\target\release\opendash.exe` trên Windows cho Forge; các lệnh gắn `(S03+)` chỉ chạy từ Slice S03 sau khi `main.rs` & `theme_ergonomics_test.rs` được thêm)*:
```shell
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo tree -p opendash-core --manifest-path cms/Cargo.toml
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash --release --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
cargo build --manifest-path cms/Cargo.toml --release --locked # (S03+)
cargo test --manifest-path cms/Cargo.toml -p opendash-server --test theme_ergonomics_test --release --locked # (S03+)
git diff --check
```
Kiểm chứng thủ công `--check-theme` *(S03+)*: chạy `./cms/target/release/opendash --check-theme cms/crates/opendash-server/src/builtin_themes/opendash-starter` (hoặc `.\cms\target\release\opendash.exe --check-theme ...` trên Windows) và kiểm tra exit code `0` cùng JSON hợp lệ trên `stdout`.
