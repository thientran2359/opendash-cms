# Theme v3 PageLayout Chrome Contract, C-SEC Composition Cache & Safe Inline `od_icon()`

* **Author:** Elephant (Antigravity)
* **Status:** Verified
* **Date:** 2026-11-25
* **Baseline:** `main@22574b8`
* **Approved By:** Owner
* **Approval Evidence:** Owner chat 2026-11-25 (chốt phương án A `PageLayoutChrome` + B `ThemeCompositionCache::global()` + C-InlineSVG `IconNonceTable`)
* **Goldfish Review:** 3-agent independent review loop (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`) · 2026-11-25 · reviewed `main@22574b8` (all Blocker/Major/Minor/Nit resolved)
* **Related Specs:**
  * [Theme Format v3, Template Resolver 2 Trục, Page Layouts & Reading Settings](theme-format-and-template-resolver.md)
  * [Declarative `block.json` Block Engine & OpenDash Studio Schema-Driven Blocks](declarative-block-json-engine.md)
  * [Theme Ergonomics, C-SEC Pure Helpers, Rich View Context & `--check-theme` CLI](theme-ergonomics-and-csec-helpers.md)

---

## Overview

Đặc tả này giải quyết ba vấn đề kỹ thuật và trải nghiệm phát triển theme đã được xác minh trên nền tảng Theme v3 & C-SEC hiện tại:

1. **Chuẩn hóa `PageLayout` Chrome Contract (`PageLayoutChrome::Default | None`):** Sửa lỗi mọi `PageLayout` đều bị hardcode tắt auto-wrap `header`/`footer` (khiến `page-layouts/default.html` và `page-layouts/full-width.html` của `opendash-starter` bị mất Header & Footer), chuyển sang khai báo tường minh `chrome` trong `theme.json` và gom toàn bộ 9 điểm quyết định bọc chrome vào một helper dùng chung.
2. **Cache `CompiledCsecComposition` theo Theme Revision (`ThemeCompositionCache`):** Loại bỏ chi phí re-parse AST, chạy lại static C-SEC/URL/DAG validation và `check_theme_part_depth` trên từng HTTP `GET` public bằng cache in-memory bất biến theo `revision_digest` (tái sử dụng pattern `OnceLock` + `RwLock<BTreeMap<..., Arc<...>>>` của `BlockTemplateCache::global()`).
3. **Cơ chế Icon SVG an toàn (`{{ od_icon("name", "class"?) }}`):** Cung cấp bộ 48 icon SVG lõi cho Theme v3 HTML, `template-part/*` và Declarative Block `render.html` thông qua per-request `IconNonceTable` (128-bit random nonce, transactional per-segment merge) và bơm `<svg>` thật vào DOM một lần duy nhất **sau** `document::normalize_public_document()` với single-pass `HtmlScanState::Body` guard, giữ nguyên 100% lệnh cấm `<svg>`/`<script>` trên input người dùng/theme và giữ nguyên CSP `img-src 'self'; script-src 'none'`.

---

## The Problem

1. **Mọi `PageLayout` bị ép ẩn Header/Footer một cách ngầm định:**
   - Tại 9 vị trí trong [`routes/public_content.rs`](../../cms/crates/opendash-server/src/routes/public_content.rs) (`render_resolved_public_template_html`) và [`storage/overrides.rs`](../../cms/crates/opendash-server/src/storage/overrides.rs) (`inspect_public_surfaces_and_precompile`, `preview_studio_with_cache_and_locale`, `wrap_entry_body_in_preview_shell_in_tx_with_budget`), mọi `PageLayout` đều bị coi là canvas không bọc `header`/`footer`. Ngoài ra, `preview_studio_with_cache_and_locale` khởi tạo `canvas_layout` từ `target` mà không reset về `false` khi fallback sang `ActiveTheme("page")` hoặc `CoreFallback`.
   - Trong khi đó, [`builtin_themes/opendash-starter/page-layouts/default.html`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/page-layouts/default.html) (*"Mặc định (Khung chuẩn)"*) và [`page-layouts/full-width.html`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/page-layouts/full-width.html) (*"Tràn viền"*) đều viết dạng fragment `<main>...</main>` không tự gọi `od_part("header")`/`od_part("footer")`. Kết quả là mọi Page chọn 2 layout này bị mất hoàn toàn Header & Footer.
2. **Biên dịch C-SEC lặp lại trên mỗi request public:**
   - Dù `themes/revisions/<digest>/` là bất biến, [`storage/overrides.rs`](../../cms/crates/opendash-server/src/storage/overrides.rs) (`inspect_part_slot_in_tx`, `inspect_public_surfaces_and_precompile`, `render_public_part`, `render_csec_template_for_preview`) và [`routes/public_content.rs`](../../cms/crates/opendash-server/src/routes/public_content.rs) vẫn gọi `check_theme_part_depth()` và `validate_and_compile_csec_composition_with_theme_urls()` lại từ đầu trên mỗi HTTP `GET`, gây tốn CPU không cần thiết.
3. **Theme & Declarative Block không thể hiển thị vector UI icon:**
   - [`template_sec.rs`](../../cms/crates/opendash-server/src/template_sec.rs) và [`document.rs`](../../cms/crates/opendash-server/src/document.rs) cấm hoàn toàn thẻ `<svg>` để ngăn XSS (`onload`, `<script>`, `<foreignObject>`, `xlink:href`). Tác giả theme/block hiện buộc phải dùng ký tự Unicode hoặc ảnh raster cho các icon UI cơ bản (mũi tên, menu, lịch, đồng hồ, check, tìm kiếm...).

---

## Goals

1. **Khai báo tường minh `chrome` trên `PageLayoutDecl`:** Thêm `chrome: PageLayoutChrome` (`"default"` | `"none"`, mặc định `"default"`) trong `opendash_core::theme`. Cập nhật `opendash-starter` (`1.0.0`, `1.0.1`, `1.1.0`) đặt `"chrome": "none"` cho `canvas`; `default` và `full_width` tự động giữ Header & Footer.
2. **Một nguồn sự thật cho `is_canvas_page_layout`:** Hợp nhất toàn bộ 9 điểm kiểm tra bọc `header`/`footer` tại public routes và preview thông qua một helper dùng chung, reset đúng khi fallback trong `preview_studio_with_cache_and_locale`, kèm fallback bảo toàn `layout_key == "canvas"` cho DB override `page_layout:*` khi đổi sang theme không còn khai báo layout đó.
3. **Cache `CompiledCsecComposition` theo `revision_digest`:** Bổ sung `ThemeCompositionCache::global()` (`OnceLock`, cap 256 entries, trả `Arc<CompiledCsecComposition>`) cùng trường `max_part_depth: usize` trên `CompiledCsecComposition` để loại bỏ re-parse AST ở cả 5 điểm compile Active Theme v3 lẫn `check_theme_part_depth`, không thay đổi chữ ký HTTP route và không cần invalidation khi đổi theme.
4. **Hỗ trợ `{{ od_icon("name") }}` và `{{ od_icon("name", "class") }}` an toàn tuyệt đối:**
   - Hoạt động đồng nhất trong Theme v3 HTML, `template-part/*` và Declarative Block `render.html` (kể cả khi block được nhúng qua `od_content()` và trên mọi luồng Preview: Studio, Entry Builder, Pattern, Entry Preview, Typed Override).
   - Xác thực tĩnh tên icon và `class` ngay lúc compile / cài theme / `--check-theme`.
   - Ra thẻ `<svg ... aria-hidden="true">` thật trong DOM mà **không** mở thẻ `<svg>` ở `template_sec.rs` hay `document.rs`, **không** nới CSP, chống spoofing bằng 128-bit random nonce + cấm `Stmt::SetBlock` + single-pass `HtmlScanState::Body` guard, transactional per-segment merge chống crash 500 khi block/slot fallback, và tính chính xác phần tăng ròng vào `RenderBudget` (512 KiB).

---

## Non-Goals

- Không thay đổi cấu trúc DB SQLite (không cần migration mới).
- Không cho phép thẻ `<svg>` thô trong HTML của theme/block hay tải file `.svg` tùy ý trong `assets/` (custom logo/hình minh họa vẫn dùng PNG/WebP qua `theme_assets`).
- Không mở `od_icon()` cho đường legacy `render_csec()` hoặc legacy plugin block cũ (`render_plugin_block_csec_with_budget_and_config`) vốn không có bước `finalize_document_icons`.
- Không tự động phân tích nhánh `{% if %}` để tắt auto-wrap `header`/`footer` trên template thường.
- Không cache DB overrides chưa lưu hoặc private draft preview trong `ThemeCompositionCache`.

---

## Critical User Journeys (CUJs)

### CUJ 1: Chọn Page Layout `default` / `full_width` vs `canvas` trên `opendash-starter`
* **Given:** Site đang kích hoạt `opendash-starter` (`1.1.0`) có đủ `header.html` và `footer.html`.
* **When:** Admin gán `page_layout_key = "default"` hoặc `"full_width"` cho Page A, và `page_layout_key = "canvas"` cho Page B, rồi mở cả hai trang public và trong Studio Preview.
* **Then:** Page A hiển thị đầy đủ Header + nội dung theo khung `default`/`full_width` + Footer; Page B (`"chrome": "none"`) chỉ hiển thị khung Landing Page không có Header/Footer. Khi đổi sang theme khác không khai báo layout `"canvas"`, DB override `page_layout:canvas` vẫn giữ `chrome = None` nhờ fallback `layout_key == "canvas"`; nếu `preview_studio_with_cache_and_locale` fallback sang `page.html` hoặc `CoreFallback`, Header & Footer vẫn hiển thị bình thường.

### CUJ 2: Phục vụ Public `GET` lặp lại không biên dịch lại C-SEC AST
* **Given:** Site đang kích hoạt một Theme v3 revision có `revision_digest = D` (64-hex).
* **When:** Hai request `GET` liên tiếp truy cập cùng một trang public (dùng `single-post.html` + `header.html` + `footer.html`).
* **Then:** Request đầu tiên biên dịch và lưu `Arc<CompiledCsecComposition>` (kèm `max_part_depth`) vào `ThemeCompositionCache::global()` theo khóa `(D, target_key, content_kind)`; request thứ hai hit cache ở cả body lẫn header/footer (không parse lại MiniJinja AST, không chạy lại static validator hay `check_theme_part_depth`), nhưng vẫn sinh 128-bit content nonce mới và kiểm tra runtime fuel/output cap như bình thường. Khi Admin kích hoạt revision mới `D'`, cache tự miss sạch sang `D'`.

### CUJ 3: Tác giả Theme và Declarative Block dùng `{{ od_icon(...) }}`
* **Given:** Một Theme v3 (trong `single.html` hoặc `header.html`) và một Declarative Block (`blocks/badge/render.html`) gọi `{{ od_icon("calendar", "w-3.5 h-3.5 inline-block") }}` và `{{ od_icon("arrow-right") }}`.
* **When:** Tác giả chạy `--check-theme`, cài theme lên CMS, mở trang public, mở Entry Preview, Studio Preview và Pattern Preview.
* **Then:**
  - Nếu tên icon không thuộc 48 icon trong curated registry, `class` chứa ký tự cấm (`"`, `<`, `>`), hoặc `od_icon()` đặt trong attribute/`Stmt::SetBlock`/`Expr::Filter`, `--check-theme` và API cài theme từ chối ngay với thông báo lỗi rõ ràng.
  - Khi hợp lệ, HTML cuối cùng chứa thẻ `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" ... class="w-3.5 h-3.5 inline-block" aria-hidden="true">...</svg>` sạch, không rò rỉ `OD_ICON_` trên bất kỳ màn public/preview nào, không làm crash 500 nếu một block khác trên trang bị lỗi fallback, pass CSP `script-src 'none'; img-src 'self'`, và từ chối chuỗi `OD_ICON_` giả mạo ở compile-time.

---

## Technical Plan (Architecture)

### A. `PageLayoutChrome` & Unified Canvas Helper

1. **Core manifest (`cms/crates/opendash-core/src/theme.rs`):**
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
   #[serde(rename_all = "snake_case")]
   pub enum PageLayoutChrome {
       #[default]
       Default,
       None,
   }

   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub struct PageLayoutDecl {
       pub key: String,
       pub label: String,
       pub file: String,
       #[serde(default)]
       pub chrome: PageLayoutChrome,
   }
   ```
   - Tương thích ngược 100% với mọi `theme.json` v3 không khai báo trường `chrome` (mặc định `PageLayoutChrome::Default`).
2. **Helper hợp nhất (`cms/crates/opendash-server/src/storage/overrides.rs`):**
   ```rust
   pub(crate) fn is_canvas_page_layout(
       active: Option<&ActiveThemeV3>,
       layout_key: Option<&str>,
   ) -> bool
   ```
   - Quy tắc duy nhất:
     - Nếu `layout_key` là `None` → `false`.
     - Nếu `active` có `PageLayoutDecl` khớp `layout_key` → `decl.chrome == PageLayoutChrome::None`.
     - Nếu `active` là `None` hoặc không khai báo `layout_key` (đổi theme nhưng DB override `page_layout:<key>` vẫn tồn tại) → fallback `layout_key == "canvas"`.
   - Áp dụng thống nhất tại **đủ 9 điểm kiểm tra** hiện hành:
     - `routes/public_content.rs`: `render_resolved_public_template_html` (2 điểm: `DbOverride` tại `L217` và `ActiveTheme` tại `L273`).
     - `storage/overrides.rs`: `inspect_public_surfaces_and_precompile` (2 điểm: `DbOverride` tại `L2615` và `ActiveTheme` tại `L2631`).
     - `storage/overrides.rs`: `preview_studio_with_cache_and_locale` (3 điểm: khởi tạo tại `L3596`, `DbOverride` tại `L3649`, `ActiveTheme` tại `L3688`, kèm reset tường minh `canvas_layout = false` ở nhánh `CoreFallback` tại `L3708` khi `document.is_none()`).
     - `storage/overrides.rs`: `wrap_entry_body_in_preview_shell_in_tx_with_budget` (2 điểm: `DbOverride` tại `L3887` và `ActiveTheme` tại `L3921`).
3. **Built-in Starter themes:**
   - Cập nhật [`opendash-starter/theme.json`](../../cms/crates/opendash-server/src/builtin_themes/opendash-starter/theme.json), `opendash-starter-1.0.1/theme.json` và `opendash-starter-1.1.0/theme.json` *(cả 2 thư mục overlay tiền-V1 sau đó đã được gộp Clean-Slate V1 về `opendash-starter/`)*: thêm `"chrome": "none"` cho layout `"canvas"`. Giữ nguyên `"default"` và `"full_width"` (mặc định `"chrome": "default"`).

### B. `ThemeCompositionCache` (In-Memory Revision Cache)

- Đặt trong [`cms/crates/opendash-server/src/template_sec.rs`](../../cms/crates/opendash-server/src/template_sec.rs) theo đúng pattern `BlockTemplateCache::global()` trong `page.rs`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
  pub struct ThemeCompositionCacheKey {
      pub revision_digest: String,
      pub target_key: String,
      pub content_kind: u8, // 0 = Entry, 1 = Index, 2 = NonEntry
  }

  #[derive(Default)]
  pub struct ThemeCompositionCache {
      inner: std::sync::RwLock<BTreeMap<ThemeCompositionCacheKey, Arc<CompiledCsecComposition>>>,
  }
  ```
- Bổ sung trường `pub max_part_depth: usize` trên `CompiledCsecComposition` (ghi lại độ sâu `od_part` tối đa đã mở rộng trong lúc compile).
- Cung cấp `ThemeCompositionCache::global() -> &'static Arc<Self>` và:
  ```rust
  pub fn get_or_compile(
      &self,
      revision_digest: &str,
      target_key: &str,
      source: &str,
      parts: &BTreeMap<String, String>,
      content_kind: CsecContentKind,
      policy: &ThemeUrlPolicy,
  ) -> Result<Arc<CompiledCsecComposition>, &'static str>
  ```
  - Giới hạn `MAX_THEME_CACHE_ENTRIES = 256` (clear khi đầy, giống `BlockTemplateCache`).
  - Xác thực `revision_digest` là chuỗi 64 ký tự hex (nếu không hợp lệ → biên dịch trực tiếp không ghi cache để tránh collision/poisoning).
  - **Tính bất biến của cache key:** `revision_digest` là SHA-256 của toàn bộ revision bất biến trên đĩa (`theme.json`, mọi template, `active.parts`, và `active_theme_url_policy` dẫn xuất thuần từ `active.manifest`). Tuyệt đối không cache khi `active` là `None` hoặc khi nguồn là DB override/preview chưa lưu. `CompiledCsecComposition` giữ nguyên `CONTENT_SLOT_TOKEN` chưa thay nonce nên mỗi lượt render vẫn sinh 128-bit random nonce mới.
  - **Loại bỏ re-parse ở `check_theme_part_depth`:** Tại `inspect_part_slot_in_tx` và `render_public_part`, khi `active` có part, lấy `compiled` từ `ThemeCompositionCache::global()` trước rồi kiểm tra `depth + compiled.max_part_depth <= 3` trong `O(1)` (thay cho việc gọi `check_theme_part_depth()` vốn parse lại MiniJinja AST trên mỗi request).
- **Tích hợp đủ 5 điểm compile Active Theme v3:**
  - Đổi `PrecompiledPublicShell.{header, footer}` và `PublicSurfaceInspection.precompiled_body` sang `Option<Arc<CompiledCsecComposition>>` (dùng `.as_deref()` tại `render_shell_slot`).
  - Bọc cả 5 điểm gọi `validate_and_compile_csec_composition_with_theme_urls` từ `ActiveThemeV3`:
    1. `storage/overrides.rs` — `inspect_part_slot_in_tx` (`L2549`)
    2. `storage/overrides.rs` — `inspect_public_surfaces_and_precompile` (`L2620`)
    3. `storage/overrides.rs` — `render_public_part` (`L2276`)
    4. `storage/overrides.rs` — `render_csec_template_for_preview` (`L3329`, khi `active` là `Some`)
    5. `routes/public_content.rs` — `render_resolved_public_template_html` nhánh fallback (`L258`)

### C. Curated Safe Inline Icons (`{{ od_icon(...) }}` + `IconNonceTable`)

```text
Theme HTML / Part / Declarative Block render.html
  │  {{ od_icon("calendar", "w-3.5 h-3.5") }}
  ▼ (Compile-time C-SEC: validate literal name ∈ CORE_ICONS & valid_icon_class_attr;
     ban raw "OD_ICON_", ban Stmt::SetBlock / Expr::Filter / attribute / <style>)
  ▼ (Render-time: allocate 128-bit nonce "OD_ICON_<32hex>" into per-segment local_nonces,
     enforce total cap 128/request, pre-charge net delta svg.len().saturating_sub(nonce.len())
     into RenderBudget; commit local_nonces to budget ONLY after render + HTML check succeed)
  ▼ (Passes validate_html_and_css_context() & document::normalize_public_document() — zero <svg> allowed in input!)
compose_document_with_seo* / Direct Preview HTML
  ▼ (Single final pass: finalize_document_icons -> IconNonceTable::substitute_into_document
     scanning HtmlScanState::Body only; skips nonces dropped by diagnostic fallback)
Final Public / Preview HTML with clean inline <svg ... aria-hidden="true">
```

1. **Curated Icon Registry (`cms/crates/opendash-core/src/icons.rs` — thuần Rust, 0 I/O):**
   - Khai báo `pub const CORE_ICONS: &[(&str, &str)]` gồm đúng **48 icon UI cốt lõi** (MIT-licensed Lucide-compatible minimal SVG inner paths, chỉ chứa `<path>`, `<circle>`, `<line>`, `<polyline>`, `<rect>`, `<polygon>` tĩnh, không `id`, không `style`, không `href`):
     - Navigation & arrows (13): `arrow-left`, `arrow-right`, `arrow-up`, `arrow-down`, `arrow-up-right`, `chevron-left`, `chevron-right`, `chevron-up`, `chevron-down`, `external-link`, `menu`, `x`, `home`.
     - Content & meta (13): `calendar`, `clock`, `user`, `users`, `tag`, `folder`, `file-text`, `book-open`, `bookmark`, `eye`, `message-circle`, `share-2`, `link`.
     - UI & status (15): `search`, `check`, `check-circle`, `plus`, `minus`, `info`, `alert-triangle`, `alert-circle`, `help-circle`, `sparkles`, `star`, `heart`, `shield`, `lock`, `settings`.
     - Contact & utility (7): `mail`, `phone`, `map-pin`, `globe`, `sun`, `moon`, `download`.
   - Các helper thuần:
     - `pub fn lookup_core_icon_paths(name: &str) -> Option<&'static str>`
     - `pub fn valid_icon_class_attr(class: &str) -> bool`: kiểm tra byte-level `!class.trim().is_empty() && class == class.trim() && !class.contains("  ") && class.len() <= 128 && class.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'_' | b'.' | b':' | b'/' | b'[' | b']' | b'-'))` (hỗ trợ utility Tailwind có dấu `.` như `w-3.5`, cấm `\t`/`\n`/`\r`/`"`/`'`/`<`/`>`/`&`/`\`/`;`).
     - `pub fn render_core_icon_svg(name: &str, class: Option<&str>) -> Option<String>`: trả về `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="{effective_class}">{paths}</svg>` với `effective_class` mặc định là `"od-icon inline-block w-5 h-5 align-middle"` khi `class` là `None`, và bằng đúng `class` khi được truyền (sau khi qua `valid_icon_class_attr`).
2. **C-SEC Static Validation (`cms/crates/opendash-server/src/template_sec.rs`):**
   - Định nghĩa `pub const ICON_NONCE_PREFIX: &str = "OD_ICON_";` và `pub const MAX_ICONS_PER_REQUEST: usize = 128;`.
   - Cấm `ICON_NONCE_PREFIX` tại đủ 4 entry points: `validate_template_csec_with_usage_flags`, `composition_sites`, `preview_composition_sites`, và `validate_block_template_csec`.
   - Chống shadowing/rebinding: thêm `"od_icon"` vào `RESERVED_CONTEXT_VARS` và `BlockSchemaTemplateContext::is_sensitive_name`.
   - Quy tắc AST cho cả 2 state machine (`ThemeHtmlState::Body` trong `walk_theme_stmt_with_url_scopes` cho Theme/Part và `HtmlScanState::Body` trong `walk_block_ast_stmt` cho Declarative Block):
     - Chỉ chấp nhận `parse_od_icon_call(&emit.expr)` ở dạng standalone `Stmt::EmitExpr` trực tiếp trong body (được phép trong `{% for %}` và `{% if %}`).
     - **Cấm tuyệt đối** `od_icon()` bên trong `Stmt::SetBlock`, `Stmt::Set`, `Expr::Filter`, `BinOp`, `IfCond`, `WithBlock`, HTML attribute, comment, hoặc `<style>`.
     - Không mở `od_icon()` trong `validate_stmt` cho đường legacy `render_csec()` (chỉ bật qua cờ `allow_od_icon: bool` cho Theme v3 composition và Declarative Block); loại trừ `parse_od_icon_call` khỏi `check_block_expr_restrictions` và `expr_touches_any_sensitive`.
     - Tham số 1 (`name`) và tham số 2 (`class`, tùy chọn) phải là chuỗi literal hợp lệ theo `lookup_core_icon_paths` và `valid_icon_class_attr`; sai tên icon hoặc sai `class` sẽ fail ngay lúc `--check-theme`, cài theme hoặc cài plugin chứa block.
3. **Runtime `IconNonceTable`, Transactional Merge & Single-Pass `HtmlScanState::Body` Substitution:**
   - Định nghĩa `#[derive(Debug, Clone, Default)] pub struct IconNonceTable { entries: Vec<(String, String)> }` gắn trên cả `RenderBudget` (`pub icon_nonces: IconNonceTable`) lẫn `PreviewCompositionBudget` (`pub icon_nonces: IconNonceTable`).
   - **Ngữ nghĩa đồng bộ budget (chống nhân đôi nonce):** `PreviewCompositionBudget::with_render_budget`, `as_render_budget`, và `sync_from_render_budget` dùng ngữ nghĩa **replace/clone** (`self.icon_nonces = rb.icon_nonces.clone()`), tuyệt đối không `extend`.
   - **Chữ ký render mới:**
     - Thêm `CompiledCsecComposition::render_with_config_and_budget(&self, context: Value, sanitized_content_html: Option<&str>, budget: &mut RenderBudget, fmt_cfg: &RenderFormattingConfig) -> Result<String, &'static str>` (giữ wrapper `render_with_config_and_remaining_fuel` cho test dùng budget cục bộ); `PublicCompositionBudget::render_theme_with_config` gọi sang method mới này với `&mut self.render_budget`.
     - Nối `budget.icon_nonces` vào `render_preview_csec_fragment_with_theme_urls_and_config` (`template_sec.rs:399`) và `render_block_csec_with_options_and_budget_and_config` (`template_sec.rs:5555`, đã nhận sẵn `budget: &mut RenderBudget` từ `page.rs:1386`).
   - **Transactional per-segment merge & 512 KiB budget:**
     - Mỗi lượt `render_captured` ghi vào vector cục bộ `local_nonces` (kiểm tra cap tổng `existing_len + local_nonces.len() < MAX_ICONS_PER_REQUEST`).
     - Mỗi lần `od_icon()` được gọi, pre-charge phần tăng ròng `svg.len().saturating_sub(nonce.len())` vào bộ đếm byte đang dùng; nếu vượt `MAX_RENDERED_HTML_BYTES`, cập nhật `budget.html_bytes_used` vượt ngưỡng trước khi trả `Err` (để `page.rs:1409–1415` nhận diện đúng lỗi budget 512 KiB thay vì nuốt thành `plugin_block_render_error`).
     - Chỉ commit `local_nonces` vào `budget.icon_nonces` sau khi toàn bộ lượt render và `validate_html_and_css_context` trả `Ok`. Nhờ vậy, nếu một Declarative Block fail giữa chừng và `page.rs` thay bằng `<div class="builder-block-unavailable"></div>`, không có orphaned nonce nào tồn đọng.
   - **Single-pass `HtmlScanState::Body` substitution & helper `content_render.rs`:**
     - Chốt 1 helper duy nhất trong `content_render.rs`:
       ```rust
       pub fn finalize_document_icons(
           html: String,
           icon_nonces: &crate::template_sec::IconNonceTable,
       ) -> ApiResult<String>
       ```
       (fast-path trả `Ok(html)` nếu `icon_nonces.is_empty()`).
     - `IconNonceTable::substitute_into_document(&self, html: String) -> Result<String, &'static str>` quét HTML 1 lượt duy nhất bằng `HtmlScanState` (tái sử dụng kỹ thuật offset replacement của `insert_preview_fragments`), **chỉ** thay nonce khi đang ở `HtmlScanState::Body` và không ở bên trong `<pre data-doc-diagnostic>` (từ chối nếu phát hiện nonce xuất hiện trong attribute/comment/`<style>` hoặc xuất hiện > 1 lần; bỏ qua an toàn nếu nonce bị lược bỏ do `normalize_slot()` fallback để không gây HTTP 500), kiểm tra tổng độ dài kết quả ≤ `MAX_PUBLIC_DOCUMENT_BYTES`.
   - **Áp dụng đầy đủ tại đúng 9 call sites thực tế:**
     1. `routes/public_content.rs` x4 (`L384`, `L637`, `L932`, `L1257` — sau `compose_document_with_seo_with_body_classes`).
     2. `routes/entries.rs` x1 (`L933` `entry_preview` — mở rộng `storage::builder::preview_saved_entry_body` trả kèm `IconNonceTable` hoặc nhận `&mut RenderBudget`, gọi `finalize_document_icons` sau `compose_document_with_seo`).
     3. `storage/overrides.rs` x3 (`L2017` `preview_typed_override_with_cache_and_locale`, `L3762` `preview_studio_with_cache_and_locale`, `L3972` `wrap_entry_body_in_preview_shell_in_tx_with_budget` — dùng `&preview_budget.icon_nonces`).
     4. `storage/builder.rs` x1 (`L1880` `preview_entry_builder_with_cache` nhánh `type_key == "pattern"` — gọi `finalize_document_icons` với `&render_budget.icon_nonces`).

---

## Alternatives Considered

| Hạng mục | Phương án thay thế | Lý do bác bỏ |
|---|---|---|
| A. Chrome Contract | Quét AST tìm `od_part("header"\|"footer")` để tự tắt auto-wrap | Phức tạp thừa và sai ngữ nghĩa khi `od_part("header")` nằm trong nhánh `{% if cond %}` (`cond = false` làm mất hẳn header). |
| A. Chrome Contract | Giữ mọi `PageLayout` là full-canvas, sửa `default.html` & `full-width.html` gọi `od_part` | Bất nhất với `page.html`/`single.html`; dễ làm theme bên thứ 3 hoặc DB override `page_layout:*` bị mất header/footer âm thầm. |
| B. Cache | Gắn `ThemeCompositionCache` lên `AppState` và truyền qua tham số route | Chạm hàng loạt chữ ký handler không cần thiết, trong khi `BlockTemplateCache::global()` (`OnceLock`) đã là pattern chuẩn của codebase. |
| C. Safe Icons | CSS `mask-image` từ `/assets/icons/<name>.svg` | Tuy gọn nhưng không ra thẻ `<svg>` thật trong DOM (Owner chốt yêu cầu inline `<svg>` trực tiếp trong DOM). |
| C. Safe Icons | Mở thẻ `<svg>` trong `document.rs` hoặc cho tải `.svg` trong `assets/` | Tăng mạnh bề mặt tấn công XSS, phải tự xây dựng và duy trì XML/SVG sanitizer đầy đủ mà không thêm crate ngoài. |

---

## Non-Functional Requirements

- **Phân tầng ([CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md)):** `PageLayoutChrome` và `opendash_core::icons` đặt trong `opendash-core` (thuần Rust, 0 I/O, 0 SQL). Không có thay đổi SQLite schema hay mutation API mới (`M8_WRITER_INVENTORY.md` giữ nguyên).
- **Bảo mật (C-SEC & CSP):** Giữ nguyên 100% lệnh cấm `<svg>`/`<script>`/`on*` trên input theme/block/editor; cấm chuỗi `OD_ICON_` trong raw template/block; cấm `od_icon()` trong `Stmt::SetBlock`; per-request 128-bit random nonce; single-pass `HtmlScanState::Body` guard khi substitute; giới hạn 128 icon/request; trừ đủ `svg.len().saturating_sub(nonce.len())` vào `RenderBudget` (512 KiB); giữ nguyên CSP `img-src 'self'; script-src 'none'`.
- **Hiệu năng:** Public `GET` lặp lại trên cùng `revision_digest` đạt cache hit 100% tại `ThemeCompositionCache::global()` (không re-parse AST, không chạy lại static validator hay `check_theme_part_depth`).
- **Bảo toàn dữ liệu & tương thích ngược:** Mọi theme v3 hiện có không khai báo `chrome` tiếp tục hợp lệ (mặc định `default`); DB override `page_layout:canvas` giữ nguyên hành vi ẩn header/footer kể cả khi đổi theme.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | Reason |
|---|---|---|
| `cms/crates/opendash-core/src/icons.rs` | Create | Curated registry 48 UI icons (`CORE_ICONS`), `lookup_core_icon_paths`, `valid_icon_class_attr`, `render_core_icon_svg` + unit tests bảo mật SVG tĩnh. |
| `cms/crates/opendash-core/src/lib.rs` | Modify | Export `pub mod icons;`. |
| `cms/crates/opendash-core/src/theme.rs` | Modify | Thêm `PageLayoutChrome` (`Default` \| `None`) và trường `#[serde(default)] pub chrome: PageLayoutChrome` trên `PageLayoutDecl`; cập nhật unit tests nếu có struct literal `PageLayoutDecl`. |
| `cms/crates/opendash-server/src/builtin_themes/opendash-starter/theme.json`, `opendash-starter-1.0.1/theme.json`, `opendash-starter-1.1.0/theme.json` | Modify | Thêm `"chrome": "none"` cho `page_layouts` entry có `"key": "canvas"`. |
| `cms/crates/opendash-server/src/template_sec.rs` | Modify | (1) Thêm `max_part_depth: usize` trên `CompiledCsecComposition` & `ThemeCompositionCache` (`global()`, `get_or_compile`, cap 256, kiểm 64-hex); (2) Thêm `ICON_NONCE_PREFIX`, `MAX_ICONS_PER_REQUEST`, `IconNonceTable` trên `RenderBudget` & `PreviewCompositionBudget` (clone/replace sync); (3) Hỗ trợ `od_icon("name", "class"?)` ở Theme/Part (`ThemeHtmlState::Body`) & Declarative Block (`HtmlScanState::Body`), cấm `Stmt::SetBlock` & shadowing `"od_icon"`; (4) Thêm `render_with_config_and_budget`, nối `render_preview_csec_fragment_with_theme_urls_and_config` & `render_block_csec_with_options_and_budget_and_config` với transactional `local_nonces`; (5) Single-pass `IconNonceTable::substitute_into_document()`. |
| `cms/crates/opendash-server/src/storage/overrides.rs` | Modify | (1) Thêm `is_canvas_page_layout(active, layout_key)` thay đủ 7 điểm hardcode `PageLayout` trong file này (`inspect_public_surfaces_and_precompile`, `preview_studio_with_cache_and_locale` gồm reset `CoreFallback`, `wrap_entry_body_in_preview_shell_in_tx_with_budget`); (2) Dùng `ThemeCompositionCache::global()` + `Arc<CompiledCsecComposition>` + `depth + compiled.max_part_depth <= 3` tại 4 điểm compile trong file (`inspect_part_slot_in_tx`, `inspect_public_surfaces_and_precompile`, `render_public_part`, `render_csec_template_for_preview`); (3) Chuyển `PublicCompositionBudget::render_theme_with_config` sang `render_with_config_and_budget` và gọi `finalize_document_icons` tại 3 nhánh preview (`preview_typed_override_with_cache_and_locale`, `preview_studio_with_cache_and_locale`, `wrap_entry_body_in_preview_shell_in_tx_with_budget`). |
| `cms/crates/opendash-server/src/storage/builder.rs` | Modify | (1) Mở rộng `preview_saved_entry_body` trả kèm `IconNonceTable` (hoặc nhận `&mut RenderBudget`) cho `routes/entries.rs`; (2) Gọi `finalize_document_icons` ở nhánh `type_key == "pattern"` trong `preview_entry_builder_with_cache`. |
| `cms/crates/opendash-server/src/content_render.rs` | Modify | Bổ sung `pub fn finalize_document_icons(html: String, icon_nonces: &crate::template_sec::IconNonceTable) -> ApiResult<String>`. |
| `cms/crates/opendash-server/src/routes/public_content.rs` | Modify | (1) Dùng `is_canvas_page_layout` tại 2 điểm trong `render_resolved_public_template_html`; (2) Dùng `ThemeCompositionCache::global()` ở nhánh fallback (`L258`); (3) Gọi `finalize_document_icons` tại đủ 4 call sites `compose_document_with_seo_with_body_classes`. |
| `cms/crates/opendash-server/src/routes/entries.rs` | Modify | Gọi `finalize_document_icons` sau `compose_document_with_seo` tại `entry_preview` (`L933`). |
| `cms/crates/opendash-server/tests/theme_chrome_cache_icons_test.rs` | Create | Suite integration/E2E kiểm chứng đủ CUJ 1, CUJ 2, CUJ 3 và các security negative tests (`OD_ICON_` spoofing, `Stmt::SetBlock`, unknown icon, invalid class, >128 icons, 512 KiB budget, block fallback không gây 500, và tất cả nhánh preview). |

---

### Slice S01: `PageLayoutChrome` Contract, `opendash-starter` Fix & `ThemeCompositionCache`

* **Dependencies:** None (baseline `main@22574b8`).
* **File Ownership:** `cms/crates/opendash-core/src/theme.rs`, `cms/crates/opendash-server/src/builtin_themes/{opendash-starter,opendash-starter-1.0.1,opendash-starter-1.1.0}/theme.json`, `cms/crates/opendash-server/src/template_sec.rs` (phần `max_part_depth` & `ThemeCompositionCache`), `cms/crates/opendash-server/src/storage/overrides.rs` (phần `is_canvas_page_layout` + `ThemeCompositionCache`), `cms/crates/opendash-server/src/routes/public_content.rs` (phần `is_canvas_page_layout` + cache fallback).
* **Công việc chính:**
  1. Thêm `PageLayoutChrome` (`Default` | `None`) trên `PageLayoutDecl` trong `opendash-core/src/theme.rs`; cập nhật `"chrome": "none"` cho `canvas` ở cả 3 bản `opendash-starter*/theme.json`.
  2. Tạo helper `is_canvas_page_layout(active, layout_key)` và thay thế đủ 9 điểm hardcode `PageLayout` trong `storage/overrides.rs` (kèm reset `canvas_layout = false` ở nhánh fallback của `preview_studio_with_cache_and_locale`) và `routes/public_content.rs`.
  3. Thêm `max_part_depth: usize` trên `CompiledCsecComposition` và `ThemeCompositionCache::global()` (`OnceLock`, cap 256, kiểm 64-hex `revision_digest`) trong `template_sec.rs`; đổi `PrecompiledPublicShell` và `PublicSurfaceInspection` sang `Arc<CompiledCsecComposition>`, nối vào đủ 5 điểm compile Active Theme v3 và thay `check_theme_part_depth` bằng kiểm tra `depth + compiled.max_part_depth <= 3`.
* **Acceptance Criteria:**
  - [ ] Page dùng `page_layout_key = "default"` và `"full_width"` trên `opendash-starter` render đủ Header + Main + Footer (cả public lẫn preview); Page dùng `"canvas"` ẩn Header & Footer.
  - [ ] DB override `page_layout:canvas` vẫn ẩn Header & Footer khi đổi sang theme không khai báo layout `"canvas"`; fallback trong `preview_studio_with_cache_and_locale` sang `page.html`/`CoreFallback` vẫn hiện Header & Footer.
  - [ ] Unit/integration test xác nhận `ThemeCompositionCache` hit trên lần gọi thứ 2 cùng `revision_digest` (cả body lẫn header/footer, không re-parse AST), miss sạch khi đổi `revision_digest`, và mỗi lần render vẫn sinh 128-bit `OD_CONTENT_` nonce khác nhau.
  - [ ] Toàn bộ test suite cũ (`starter_theme_v3_test`, `theme_ergonomics_test`, `cargo test --workspace --release --locked`) PASS 100%.
* **Rollback:** `git revert` commit của Slice S01 (không có migration SQLite, tương thích ngược hoàn toàn với `theme.json` v3).

### Slice S02: Curated Icon Registry (`opendash-core::icons`) & Safe Inline `{{ od_icon(...) }}` Pipeline

* **Dependencies:** Slice `S01` (tránh xung đột chỉnh sửa trên `template_sec.rs`, `storage/overrides.rs` và `routes/public_content.rs`).
* **File Ownership:** `cms/crates/opendash-core/src/{icons.rs,lib.rs}`, `cms/crates/opendash-server/src/{template_sec.rs,content_render.rs,storage/overrides.rs,storage/builder.rs,routes/public_content.rs,routes/entries.rs}`, `cms/crates/opendash-server/tests/theme_chrome_cache_icons_test.rs`.
* **Công việc chính:**
  1. Tạo `cms/crates/opendash-core/src/icons.rs` với 48 UI icon tĩnh, `lookup_core_icon_paths`, `valid_icon_class_attr` (hỗ trợ dấu `.`), `render_core_icon_svg` + unit tests đảm bảo 100% SVG nội bộ không chứa `<script>`, `on*`, `style=`, `href=`, `foreignObject`.
  2. Mở rộng `template_sec.rs`: cấm `ICON_NONCE_PREFIX` (`"OD_ICON_"`) tại đủ 4 entry points; thêm `"od_icon"` vào `RESERVED_CONTEXT_VARS` và `is_sensitive_name`; cho phép `Stmt::EmitExpr` gọi `od_icon("name")` / `od_icon("name", "class")` trong `ThemeHtmlState::Body` (Theme/Part) và `HtmlScanState::Body` (Declarative Block), cấm trong `Stmt::SetBlock` và mọi biểu thức con; không mở cho legacy `render_csec()`.
  3. Thêm `IconNonceTable` trên `RenderBudget` và `PreviewCompositionBudget` (clone/replace sync), `CompiledCsecComposition::render_with_config_and_budget`, nối vào `render_preview_csec_fragment_with_theme_urls_and_config` & `render_block_csec_with_options_and_budget_and_config` với transactional `local_nonces`, cap 128 icon/request, pre-charge `svg.len().saturating_sub(nonce.len())` (cập nhật `budget.html_bytes_used` vượt ngưỡng trước khi trả `Err`), và single-pass `HtmlScanState::Body` `substitute_into_document()`.
  4. Bổ sung `content_render::finalize_document_icons()` và gọi tại đủ 9 call sites (`routes/public_content.rs` x4, `routes/entries.rs` x1, `storage/overrides.rs` x3, `storage/builder.rs` x1).
  5. Bổ sung suite test `theme_chrome_cache_icons_test.rs` bao phủ đủ CUJ 1–3 và negative security tests (`OD_ICON_` spoofing, `Stmt::SetBlock`, tên icon sai, class chứa `" onclick=`, vượt 128 icon, vượt 512 KiB, block fail không làm sập trang, `od_icon()` trong Declarative Block qua `od_content()`, Pattern/Entry/Studio/TypedOverride preview, và `--check-theme` CLI).
* **Acceptance Criteria:**
  - [ ] `{{ od_icon("calendar", "w-3.5 h-3.5") }}` render ra thẻ `<svg ... class="w-3.5 h-3.5" aria-hidden="true">...</svg>` sạch trong Theme HTML, `template-part/*`, Declarative Block (qua `od_content()`), và cả 4 màn Preview (Studio, Entry Builder, Pattern, Entry Preview, Typed Override).
  - [ ] Raw `<svg>` trong theme/block vẫn bị C-SEC và `document.rs` từ chối; chuỗi `OD_ICON_` và `Stmt::SetBlock` chứa `od_icon()` bị từ chối ở compile-time.
  - [ ] Block chứa `od_icon()` nhưng gặp lỗi runtime fallback về `.builder-block-unavailable` không gây HTTP 500 trên phần còn lại của trang.
  - [ ] `--check-theme` báo lỗi rõ ràng khi dùng tên icon không tồn tại hoặc `class` không hợp lệ.
  - [ ] Header `Content-Security-Policy` trên public response giữ nguyên `img-src 'self'; script-src 'none'`.
  - [ ] `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` PASS 100%.
* **Rollback:** `git revert` commit của Slice S02 (không có migration SQLite; theme hiện có chưa dùng `od_icon()` không bị ảnh hưởng).

---

### Verification Plan

Chạy đầy đủ các lệnh kiểm chứng từ gốc repository sau mỗi slice:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python .agents/scripts/check-agent-artifacts.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
git diff --check
```

Kèm kiểm chứng bổ sung:
- Chạy `cargo test --manifest-path cms/Cargo.toml --release --locked --test theme_chrome_cache_icons_test`.
- Chạy CLI `opendash --check-theme` trên theme mẫu hợp lệ (dùng `od_icon` & `chrome: "none"`) và theme lỗi (tên icon sai / `OD_ICON_` giả mạo) để xác nhận exit code `0` vs non-zero.
