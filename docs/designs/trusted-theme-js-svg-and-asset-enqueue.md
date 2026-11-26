# Trusted Theme HTML (Unrestricted Inline SVG & JS) & WordPress-Style Asset Enqueue System

* **Author:** Elephant (Antigravity)
* **Status:** Verified
* **Date:** 2026-11-25
* **Baseline:** `main@537c8a1`
* **Approved By:** Owner
* **Approval Evidence:** Owner chat 2026-11-25 (chỉ đạo mở khóa inline SVG, inline JS, `assets/*.js`, hệ thống Enqueue Styles/Scripts kiểu WordPress không hiện cảnh báo ở Admin, tách riêng Spec A trước Spec B `functions`)
* **Goldfish Review:** 3-agent independent review loop (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`) · 2026-11-25 · reviewed `main@537c8a1` (all Blocker/Major/Minor/Nit resolved)
* **Related Specs:**
  * [Theme Format v3, Template Resolver 2 Trục, Page Layouts & Reading Settings](theme-format-and-template-resolver.md)
  * [Declarative `block.json` Block Engine & OpenDash Studio Schema-Driven Blocks](declarative-block-json-engine.md)
  * [Theme v3 PageLayout Chrome Contract, C-SEC Composition Cache & Safe Inline `od_icon()`](theme-chrome-cache-and-safe-icons.md)

---

## Overview

Theo chỉ đạo sản phẩm của Chủ dự án (2026-11-25), OpenDash mở khóa trải nghiệm phát triển giao diện thực tế cho **Theme v3** theo mô hình *"Theme là tài sản tin cậy do Admin lựa chọn"* (tương tự WordPress/Ghost):

1. **Cho phép Inline `<svg>` hoàn toàn tự do** trong HTML theme, `template-part/*`, `page-layouts/*` và Declarative Block `render.html` (bao gồm `xmlns`, `xmlns:xlink`, `xlink:href`, `<defs>`, `<title>` trong `<svg>`, mọi thẻ/thuộc tính SVG; đồng thời hỗ trợ file `.svg` trong `assets/` và giữ nguyên tương thích 100% với `{{ od_icon(...) }}`).
2. **Cho phép Inline `<script>`, thuộc tính sự kiện `on*` tĩnh và file `.js`/`.mjs` trong `assets/`** của Theme v3 mà **không** hiển thị banner/cảnh báo phiền phức trên Admin UI, đồng thời nới `Content-Security-Policy` của trang public sang `script-src 'self' 'unsafe-inline'` (vẫn giữ nguyên bảo vệ chống XSS từ nội dung bài viết/form untrusted, cấm nội suy `{{ ... }}` vào `<script>` / `on*`, và giữ nguyên CSP chặt cho `/admin`).
3. **Hệ thống Enqueue Styles & Scripts kiểu WordPress (`styles` & `scripts` trong `theme.json`):** Khai báo tài nguyên theo `handle`, `src`, `deps` (tự sắp xếp topo-sort theo phụ thuộc, tự động promote `deps` của `head` script lên `head`, chống nạp trùng), `location` (`head` | `footer`), `strategy` (`blocking` | `defer` | `async`), `module` (`type="module"`), `media`, và `conditions` (`all`, `home`, `page`, `single`, `archive`, `taxonomy`, `not_found`).

> **Phạm vi cập nhật hợp đồng cũ:** Đặc tả này cập nhật ranh giới HTML/JS/SVG cho nguồn **Theme v3 & Declarative Block** tại [`M5_THEME_TEMPLATES.md`](../contracts/M5_THEME_TEMPLATES.md) và [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md) (đã được Owner duyệt trực tiếp ngày 2026-11-25 theo ADR-0010). Nội dung bài viết untrusted (`opendash-core::richtext`), đường legacy `render_csec()` và Admin Panel (`/admin`) vẫn giữ nguyên 100% bảo vệ chống XSS.

---

## The Problem

1. **Cấm `<svg>` và `<script>` chặn đứng nhiều tính năng giao diện cơ bản:**
   - [`opendash-core/src/theme.rs`](../../cms/crates/opendash-core/src/theme/mod.rs) (`valid_v3_asset_path`), [`opendash-server/src/theme.rs`](../../cms/crates/opendash-server/src/theme.rs) (`v3_entry_cap_and_allowlist`, `theme_asset_mime`), [`template_sec.rs`](../../cms/crates/opendash-server/src/template_sec.rs) (`FORBIDDEN_TAGS`, `is_dangerous_attr_name`, `validate_tag_attributes`), [`document.rs`](../../cms/crates/opendash-server/src/document.rs) (`is_forbidden_tag`, `parse_attributes`), và [`routes/public.rs`](../../cms/crates/opendash-server/src/routes/public.rs) (`script-src 'none'`) hiện chặn hoàn toàn inline SVG, inline JS và file JS của theme.
   - Tác giả theme không thể tự do dùng logo/đồ họa `<svg>` tùy chỉnh hay viết JS cho menu mobile, slider, lightbox, dark mode, hiệu ứng scroll, hoặc tích hợp thư viện frontend.
2. **Thiếu hệ thống Enqueue Style/Script có quản lý phụ thuộc:**
   - Hiện tại `theme.json` chỉ có danh sách phẳng `assets: Vec<String>`, tác giả phải tự chèn thủ công `<link rel="stylesheet" href="{{ theme_assets['...'] }}">` vào từng template, chưa có cơ chế `handle`, `deps` (topo-sort), nạp ở `<head>` vs cuối `<body>`, `defer`/`async`/`type="module"`, hay nạp có điều kiện theo loại trang (giống `wp_enqueue_style` / `wp_enqueue_script`).

---

## Goals

1. **Mở khóa `.js`, `.mjs`, `.svg` trong `assets/` của Theme v3:**
   - Cập nhật cả `opendash_core::theme::valid_v3_asset_path()` lẫn `opendash_server::theme::{v3_entry_cap_and_allowlist, theme_asset_mime}` (cap 512 KiB/file, kiểm tra UTF-8 trong `validate_theme_v3_files`, giữ ràng buộc raster-only cho `ThemePatternDecl.preview_asset`, và trả `Content-Type: text/javascript; charset=utf-8` / `image/svg+xml`).
   - Đặt defense-in-depth `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox` riêng cho `image/svg+xml` khi mở trực tiếp qua `GET /theme/{digest}/{*path}` (không ảnh hưởng `<img src="...svg">` hay CSS `url(...)`).
2. **Hệ thống Enqueue Style & Script chuẩn hóa trong `opendash_core::theme`:**
   - Thêm 2 trường tùy chọn trên `ThemeV3Manifest`: `styles: Vec<ThemeStyleEnqueueDecl>` và `scripts: Vec<ThemeScriptEnqueueDecl>` (tương thích ngược 100% với mọi `theme.json` v3 hiện có).
   - Validate tại thời điểm cài theme và `--check-theme`: `handle` duy nhất, `src` tồn tại trong `assets`, đúng đuôi `.css` hoặc `.js`/`.mjs`, `deps` trỏ tới handle hợp lệ (hỗ trợ virtual handle `"opendash-tailwind"` cho `styles`) và không tạo chu trình DAG.
   - Bộ giải quyết thuần Rust `resolve_enqueued_assets()` topo-sort theo `deps`, tự động promote `deps` của `head` script lên `head_scripts`, bỏ qua virtual handle `"opendash-tailwind"` khi emit thẻ `<link>`, lọc theo `EnqueueCondition` của route hiện tại, chống trùng `handle`, và tạo thẻ `<link rel="stylesheet">` ở `<head>` và `<script>` ở `<head>` hoặc cuối `<body>`.
3. **Cho phép Inline `<svg>`, Inline `<script>` và `on*` tĩnh trên nguồn Theme & Declarative Block:**
   - Tham số hóa `allow_trusted_theme_markup: bool` xuyên suốt `template_sec.rs` (compile-time AST + post-render validator) và `document.rs` (`tokenize`, `parse_attributes`, `normalize_public_document_with_options`), giữ `false` cho đường legacy `render_csec()` và untrusted content.
   - Hỗ trợ scan raw-text `<script>...</script>` chuẩn HTML5 trong cả `template_sec.rs` (tăng `HtmlScanState::Tag.prefix` lên `8` byte, cấm `<script/>` tự đóng, cấm `Stmt::EmitExpr` và control-flow trong `Script`, cấm `{{ ... }}` trong thuộc tính `on*` để chặn Editor → Visitor XSS) và `document.rs` (`TokenKind::ScriptBlock`, `find_case_insensitive_end_script`, `authorized_head_scripts`).
   - Cho phép `<svg>` (bao gồm `xmlns`, `xmlns:xlink`, `xlink:href`, `<title>` bên trong `<svg>`, mọi thẻ hình học SVG) không giới hạn trên Theme/Block.
4. **Cập nhật CSP Public & Giữ UX Admin sạch:**
   - Cập nhật cả 4 nhánh trong `routes/public.rs::rendered_layout_with_form_csp`: đổi `script-src 'none'` → `script-src 'self' 'unsafe-inline'` (giữ nguyên `media-src`, `form-action`, `object-src 'none'`, `base-uri 'none'`, `frame-ancestors 'none'`).
   - Không thêm bất kỳ cảnh báo, checkbox xác nhận hay banner phiền phức nào ở Admin UI khi cài/kích hoạt theme có JS/SVG.
   - Giữ nguyên tương thích 100% với `{{ od_icon(...) }}` hiện có và không thay đổi JSON schema của `ThemeCheckSummary` (`--check-theme`).

---

## Non-Goals

- Không triển khai `functions.wasm` / `functions.js` (server-side theme functions) trong đặc tả này (tách sang Design Spec B tiếp theo).
- Không thay đổi cấu trúc DB SQLite (không cần migration mới).
- Không nới lỏng sanitization của `opendash-core::richtext` (nội dung bài viết/editor nhập bởi user), đường legacy `render_csec()`, hay CSP của Admin Panel (`/admin`).
- Không cho phép nạp script từ domain bên ngoài (external CDN) mặc định trong CSP (`script-src 'self' 'unsafe-inline'`, không mở `https:` tùy tiện).

---

## Critical User Journeys (CUJs)

### CUJ 1: Theme v3 khai báo Enqueue Styles & Scripts kiểu WordPress
* **Given:** Một Theme v3 khai báo trong `theme.json`:
  - `assets`: `["assets/base.css", "assets/home.css", "assets/vendor.js", "assets/app.js", "assets/init.mjs", "assets/logo.svg"]`.
  - `styles`: `base` (`assets/base.css`, `deps: ["opendash-tailwind"]`), `home-hero` (`assets/home.css`, `deps: ["base"]`, `conditions: ["home"]`).
  - `scripts`: `vendor` (`assets/vendor.js`, `location: "footer"`, `strategy: "defer"`), `app` (`assets/app.js`, `deps: ["vendor"]`, `location: "footer"`, `strategy: "defer"`), `analytics-init` (`assets/init.mjs`, `location: "head"`, `module: true`).
* **When:** Admin cài theme (hoặc chạy `--check-theme`), kích hoạt theme, và truy cập `/` (Home) vs `/gioi-thieu` (Page).
* **Then:**
  - Nếu `deps` có vòng lặp hoặc tham chiếu handle không tồn tại hoặc file thiếu trong `assets`, `--check-theme` và API cài theme từ chối ngay.
  - Trên `/`: `<head>` chứa `opendash-tailwind.css` → `base.css` → `home.css` → `<script src="/theme/<digest>/assets/init.mjs" type="module"></script>`; cuối `<body>` chứa `<script src="/theme/<digest>/assets/vendor.js" defer></script>` → `<script src="/theme/<digest>/assets/app.js" defer></script>` đúng thứ tự phụ thuộc.
  - Trên `/gioi-thieu`: không nạp `home.css`, nhưng vẫn nạp `base.css` và các script chung.
  - `GET /theme/<digest>/assets/app.js` trả HTTP `200` với `Content-Type: text/javascript; charset=utf-8`; `GET /theme/<digest>/assets/logo.svg` trả HTTP `200` với `Content-Type: image/svg+xml` kèm header `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox`.

### CUJ 2: Tác giả Theme viết Inline `<svg>` và Inline `<script>` + `onclick` trực tiếp
* **Given:** Một Theme v3 có:
  - `header.html` chứa logo `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 32"><title>Brand</title><defs>...</defs><path d="..."/></svg>` và nút `<button onclick="document.body.classList.toggle('nav-open')">Menu</button>`.
  - `footer.html` chứa `<script>if (window.innerWidth < 768) { document.documentElement.dataset.mobile = "1"; }</script>`.
  - `blocks/counter/render.html` chứa inline `<svg>` + `<script>` cục bộ và `{{ od_icon("check-circle") }}`.
* **When:** Tác giả chạy `--check-theme`, cài & kích hoạt theme (không có banner cảnh báo nào ở Admin), và mở trang public.
* **Then:**
  - `--check-theme` và cài theme thành công (exit `0` / HTTP `201`).
  - HTML public giữ nguyên toàn bộ `<svg>...</svg>`, `onclick="..."`, `<script>if (window.innerWidth < 768) ...</script>` (không bị lỗi parse dấu `<` trong JS hay gãy `od_icon()`), và Declarative Block render đầy đủ trong `{{ od_content() }}`.
  - Response header chứa `Content-Security-Policy` với `script-src 'self' 'unsafe-inline'`.
  - Đồng thời, nếu nội dung bài viết (`values.body`) do Editor nhập chứa `<script>alert(1)</script>` hoặc `<svg onload=alert(1)>`, trình duyệt vẫn được bảo vệ vì `opendash-core::richtext` và MiniJinja auto-escape lọc/escape sạch; và nếu một block/theme cố viết `onclick="x('{{ title }}')"` hay `<script>const x = "{{ title }}";</script>`, validator từ chối ngay lúc cài theme / `--check-theme`.

---

## Technical Plan (Architecture)

### A. Asset Allowlist, MIME Types & Enqueue Schema (`opendash-core::theme` & `opendash-server::theme`)

1. **Core Asset Grammar & Manifest Types (`cms/crates/opendash-core/src/theme.rs`):**
   - Mở rộng `valid_v3_asset_path()` (và alias `valid_asset_path()`): thêm `"js" | "mjs" | "svg"` vào danh sách đuôi hợp lệ (`"css" | "txt" | "js" | "mjs" | "svg" | "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2"`).
   - Khóa `ThemePatternDecl.preview_asset` trong `ThemeV3Manifest::validate()` chỉ chấp nhận ảnh raster (`png | jpg | jpeg | webp | gif`), không cho `.js`/`.mjs` lọt qua ở `preview_asset`.
   - Khai báo các kiểu Enqueue:
     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
     #[serde(rename_all = "snake_case")]
     pub enum EnqueueCondition {
         #[default]
         All,
         Home,
         Page,
         Single,
         Archive,
         Taxonomy,
         NotFound,
     }

     #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
     #[serde(rename_all = "snake_case")]
     pub enum ScriptLocation {
         Head,
         #[default]
         Footer,
     }

     #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
     #[serde(rename_all = "snake_case")]
     pub enum ScriptStrategy {
         Blocking,
         #[default]
         Defer,
         Async,
     }

     #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
     #[serde(deny_unknown_fields)]
     pub struct ThemeStyleEnqueueDecl {
         pub handle: String,
         pub src: String,
         #[serde(default, skip_serializing_if = "Vec::is_empty")]
         pub deps: Vec<String>,
         #[serde(default, skip_serializing_if = "Option::is_none")]
         pub media: Option<String>,
         #[serde(default, skip_serializing_if = "Vec::is_empty")]
         pub conditions: Vec<EnqueueCondition>,
     }

     #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
     #[serde(deny_unknown_fields)]
     pub struct ThemeScriptEnqueueDecl {
         pub handle: String,
         pub src: String,
         #[serde(default, skip_serializing_if = "Vec::is_empty")]
         pub deps: Vec<String>,
         #[serde(default)]
         pub location: ScriptLocation,
         #[serde(default)]
         pub strategy: ScriptStrategy,
         #[serde(default)]
         pub module: bool,
         #[serde(default, skip_serializing_if = "Vec::is_empty")]
         pub conditions: Vec<EnqueueCondition>,
     }
     ```
   - Thêm trên `ThemeV3Manifest`:
     ```rust
     #[serde(default, skip_serializing_if = "Vec::is_empty")]
     pub styles: Vec<ThemeStyleEnqueueDecl>,
     #[serde(default, skip_serializing_if = "Vec::is_empty")]
     pub scripts: Vec<ThemeScriptEnqueueDecl>,
     ```
   - **Quy tắc validate trong `ThemeV3Manifest::validate()`:**
     - Tối đa `64` styles và `64` scripts mỗi theme.
     - `handle`: hợp lệ theo `valid_kebab_segment_with_limit(handle, 64)`, không trùng trong cùng nhóm, không đặt trùng virtual handle `"opendash-tailwind"`.
     - `src`: hợp lệ theo `valid_v3_asset_path(&src)`, bắt buộc có mặt trong `self.assets`, đuôi `.css` cho `styles` và `.js`/`.mjs` cho `scripts`.
     - `media` (nếu có): `!media.trim().is_empty() && media.len() <= 64`, chỉ chứa ASCII `[a-zA-Z0-9 (),:./-]` (hỗ trợ `(min-aspect-ratio: 16/9)`, `(min-resolution: 1.5dppx)`).
     - `deps`: không trùng lặp trong cùng `deps` của 1 khai báo; mọi dependency trong `styles` phải là `"opendash-tailwind"` hoặc một `handle` trong `self.styles`; mọi dependency trong `scripts` phải là một `handle` trong `self.scripts`; DFS 3 màu từ chối mọi chu trình DAG.
   - **Resolver thuần Rust `resolve_enqueued_assets`:**
     ```rust
     #[derive(Debug, Clone, Default, PartialEq, Eq)]
     pub struct ResolvedEnqueuedAssets {
         pub head_styles: Vec<String>,
         pub head_style_urls: Vec<String>,
         pub head_scripts: Vec<String>,
         pub footer_scripts: Vec<String>,
     }

     pub fn resolve_enqueued_assets(
         manifest: &ThemeV3Manifest,
         revision_digest: &str,
         route_condition: EnqueueCondition,
     ) -> ResolvedEnqueuedAssets
     ```
     - Xác thực `revision_digest` là 64 ký tự hex thường (nếu không hợp lệ trả `Default::default()`).
     - Chọn các item có `conditions.is_empty() || conditions.contains(&EnqueueCondition::All) || conditions.contains(&route_condition)`, tự kéo toàn bộ transitive `deps` (bỏ qua virtual handle `"opendash-tailwind"` khi emit thẻ `<link>`).
     - **Auto-promote cross-location deps cho `scripts`:** Nếu một script `location == Head` phụ thuộc (trực tiếp hoặc bắc cầu) vào một script `location == Footer`, tự động promote script phụ thuộc đó lên `head_scripts` (kiểu `WP_Scripts`) để bảo đảm thứ tự thực thi luôn đúng.
     - Sắp xếp topo-sort bảo toàn thứ tự khai báo tương đối, chống trùng handle, và xuất HTML sạch:
       - Style: `<link rel="stylesheet" href="/theme/{digest}/{src}">` (kèm ` media="..."` nếu có).
       - Script: `<script src="/theme/{digest}/{src}"{type_attr}{strategy_attr}></script>` (` type="module"` nếu `module == true`; với `module == true` chỉ thêm ` async` nếu `strategy == Async` và script không nằm trong chuỗi phụ thuộc `deps`, bỏ qua `defer`/`blocking` vì HTML5 module mặc định defer; với `module == false` thêm ` defer` khi `Defer`, ` async` khi `Async`, không thêm thuộc tính khi `Blocking`; đồng thời tự động bảo vệ thứ tự thực thi trên các script có cạnh `deps`: hạ `async` → `defer` (hoặc bỏ `async` ở module) và nâng `blocking` → `defer` nếu phụ thuộc vào `module`/`defer`/script đã hạ từ `async`).
2. **Asset Allowlist, UTF-8 Check & MIME (`cms/crates/opendash-server/src/theme.rs` & `routes/themes.rs`):**
   - `v3_entry_cap_and_allowlist`: thêm `"js" | "mjs" | "svg" => Ok(V3_MAX_ACTIVE_ASSET_BYTES)` (512 KiB, tách riêng để không nới cap text v2 cũ).
   - `validate_theme_v3_files`: kiểm tra `std::str::from_utf8(raw)` cho `.js`, `.mjs`, `.svg` (không chạy `validate_safe_css` lên chúng).
   - `theme_asset_mime`: thêm `"js" | "mjs" => Some("text/javascript; charset=utf-8")` và `"svg" => Some("image/svg+xml")`.
   - `routes/themes.rs::theme_asset`: khi `mime == "image/svg+xml"`, đính kèm header `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox` (chặn chạy script nếu mở trực tiếp file `.svg` ở tab riêng, không ảnh hưởng `<img src="...svg">`).

### B. Trusted Theme Markup trong `template_sec.rs` và `document.rs`

1. **Ranh giới bảo mật 2 chế độ (`allow_trusted_theme_markup: bool`) trong `template_sec.rs`:**
   - **Chế độ `false` (Strict — mặc định):** áp dụng cho đường legacy `validate_template_csec*` / `render_csec*` — giữ nguyên 100% lệnh cấm `<script>`, `<svg>`, `foreignobject`, `math`, `on*`, `xmlns`, `xlink:href`.
   - **Chế độ `true` (Trusted Theme/Block):** áp dụng cho đủ 6 điểm của Theme v3 & Declarative Block:
     1. `validate_and_compile_csec_composition_with_theme_urls` (`L1760`)
     2. `CompiledCsecComposition::render_with_config_and_budget` post-render check (`L305`)
     3. `render_preview_csec_fragment_with_theme_urls_and_config` pre-check (`L687`) & post-render check (`L727`)
     4. `validate_block_template_csec` (`L5585`)
     5. `render_block_csec_with_options_and_budget_and_config` post-render check (`L6069`)
   - **Quy tắc cụ thể khi `allow_trusted_theme_markup == true`:**
     - Gỡ `"script"`, `"svg"`, `"foreignobject"`, `"math"` khỏi danh sách thẻ cấm ở cả AST walker lẫn `validate_html_and_css_context_with_url_vars`. Cho phép `<title>` khi đang ở bên trong `<svg>...</svg>` (theo dõi `svg_depth: usize`).
     - Gỡ `on*`, `"xmlns"`, `"xmlns:xlink"`, `"xlink:href"` khỏi danh sách thuộc tính cấm tĩnh ở `finalize_theme_attr` và `validate_tag_attributes`.
     - **Chống Editor → Visitor XSS (tách predicate rõ ràng):** Tạo predicate riêng `forbids_dynamic_interpolation(attr_name)` luôn bao gồm `attr_name.starts_with("on")` (bắt buộc cấm `Stmt::EmitExpr` trong `on*` ở `walk_theme_stmt_with_url_scopes:4271` và cấm `{{`/`{%` trong giá trị `on*` tại `validate_tag_attributes` khi `!post_render` cho Declarative Block).
     - **Raw-text `<script>...</script>` scanner đầy đủ:**
       - Tăng giới hạn buffer `HtmlScanState::Tag.prefix` (`template_sec.rs:2206`) từ `< 7` lên **`< 8`** để nhận diện chính xác `"<script"` (7 byte) + byte phân tách thứ 8 (`>` hoặc ASCII whitespace), thêm trạng thái `HtmlScanState::Script` (bảo đảm cả `walk_block_ast_stmt` lẫn `IconNonceTable::substitute_into_document` không bị toán tử `a < b` trong JS phá hỏng!).
       - Thêm `ThemeHtmlState::Script` và hỗ trợ `"script"` trong `find_rawtext_or_rcdata_end` (`L4529-4557, 4725-4742`).
       - Cấm `<script .../>` tự đóng và `</script>` lẻ (giống `style`/`title`).
       - Cấm `Stmt::EmitExpr`, `od_part`, `od_content`, `od_icon` và mọi control-flow (`IfCond`, `ForLoop`, `WithBlock`, `Set`, `SetBlock`) bên trong trạng thái `Script` ở cả `walk_theme_stmt_with_url_scopes` lẫn `walk_block_ast_stmt`. (Dữ liệu động từ DB truyền sang JS qua `data-*` attribute trên thẻ HTML và đọc bằng `el.dataset.*`.)
2. **Document Normalizer (`cms/crates/opendash-server/src/document.rs`):**
   - Thêm `TokenKind::ScriptBlock(&'a str)` và `find_case_insensitive_end_script` (tương tự `StyleBlock` / `find_case_insensitive_end_style`): trả `Err("unclosed_script")` nếu thiếu `</script>`, từ chối `<script .../>` tự đóng và `</script>` lẻ.
   - Thêm `pub authorized_head_scripts: Vec<String>` vào `SafeDocumentParts`.
   - Thêm `pub fn normalize_public_document_with_options(raw_html: &str, allow_trusted_theme_markup: bool) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic>` (giữ `normalize_public_document(raw_html)` gọi `allow_trusted_theme_markup = false`).
   - Truyền `allow_trusted_theme_markup` vào `tokenize()` và 6 điểm gọi `parse_attributes()`:
     - Khi `true`: gỡ `"script"`, `"svg"`, `"foreignobject"`, `"math"` khỏi `is_forbidden_tag`; cho phép thuộc tính `on*` trong `parse_attributes`; bảo toàn thẻ `<title>...</title>` khi đang nằm bên trong `<svg>...</svg>` (`svg_depth > 0`); đưa `TokenKind::ScriptBlock` trong `Phase::InHead` vào `authorized_head_scripts`, giữ `TokenKind::ScriptBlock` trong `Phase::InBody` (hoặc không có envelope) đúng vị trí trong `body_fragment`, và trả `Err("script_outside_envelope")` nếu nằm ngoài `<head>`/`<body>` khi có envelope. Giữ nguyên whitelist `is_safe_body_attr` cho thẻ `<body>`.

### C. Composition, Slot Normalization & Public CSP (`content_render.rs`, `routes/public.rs`, `routes/public_content.rs`, `storage/overrides.rs`)

1. **Chữ ký `compose_document_with_seo_with_assets` (`cms/crates/opendash-server/src/content_render.rs`):**
   ```rust
   pub fn compose_document_with_seo_with_assets(
       header_html: Option<&str>,
       rendered_layout: &str,
       footer_html: Option<&str>,
       seo: &crate::seo::SeoView,
       body_classes: &[&str],
       enqueued: &opendash_core::theme::ResolvedEnqueuedAssets,
       allow_trusted_theme_markup: bool,
   ) -> ApiResult<String>
   ```
   - Giữ nguyên wrapper `compose_document_with_seo` và `compose_document_with_seo_with_body_classes` gọi với `&ResolvedEnqueuedAssets::default()` và `allow_trusted_theme_markup = false`.
   - Dùng `normalize_public_document_with_options(..., allow_trusted_theme_markup)` cho `rendered_layout`, `header_html`, `footer_html`, gom cả `authorized_styles` lẫn `authorized_head_scripts`.
   - **Thứ tự `<head>` & dedup:**
     1. SEO meta / canonical / alternates / OG / JSON-LD
     2. `/assets/opendash-tailwind.css` (chống trùng)
     3. `enqueued.head_styles`
     4. `authorized_styles` trích từ template — bỏ qua `<link>` có `href` trùng với `/assets/opendash-tailwind.css` hoặc bất kỳ URL nào trong `enqueued.head_style_urls`
     5. `enqueued.head_scripts`
     6. `authorized_head_scripts` trích từ `<head>` template
   - **Thứ tự `<body>`:** `{header}{body_fragment}{footer}{enqueued.footer_scripts}`.
2. **Đủ 3 điểm chuẩn hóa slot trung gian & Ánh xạ `EnqueueCondition`:**
   - Chuyển 3 điểm chuẩn hóa `slot_html` / `fallback_html` sang `normalize_public_document_with_options(..., true)`:
     1. `routes/public_content.rs:188` (`render_resolved_public_template_html`)
     2. `storage/overrides.rs:3591` (`preview_studio_with_cache_and_locale`)
     3. `storage/overrides.rs:3878` (`wrap_entry_body_in_preview_shell_in_tx_with_budget`)
   - **Bảng ánh xạ Route → `EnqueueCondition` (`routes/public_content.rs`):**
     - `render_entry_response_with_cache`: `if is_home { EnqueueCondition::Home } else if type_key == "page" { EnqueueCondition::Page } else { EnqueueCondition::Single }`.
     - Archive handler: `EnqueueCondition::Archive`.
     - Taxonomy handler: `EnqueueCondition::Taxonomy`.
     - Not Found handler: `EnqueueCondition::NotFound`.
   - **Luồng Studio/Entry Preview (`storage/overrides.rs:3789, 3999`):** Gọi `compose_document_with_seo_with_assets` với `allow_trusted_theme_markup = true` và `ResolvedEnqueuedAssets { head_styles: resolved.head_styles, head_style_urls: resolved.head_style_urls, head_scripts: Vec::new(), footer_scripts: Vec::new() }` (chỉ bơm CSS, không phát script thừa vào iframe preview).
3. **Public CSP (`cms/crates/opendash-server/src/routes/public.rs`):**
   - Trong `rendered_layout_with_form_csp`, giữ nguyên cả 4 nhánh `match (allow_form_self, has_video)` (giữ nguyên `media-src 'self'` và `form-action`), chỉ đổi `script-src 'none'` → `script-src 'self' 'unsafe-inline'`.

---

## Alternatives Considered

| Hạng mục | Phương án thay thế | Lý do bác bỏ |
|---|---|---|
| Inline JS/SVG | Chỉ cho external `assets/*.js` và cấm inline `<script>` / `<svg>` | Chủ dự án chốt rõ yêu cầu cho phép theme viết tự do cả inline `<script>`, file `.js`, `on*`, lẫn inline `<svg>` không giới hạn, không hiện cảnh báo ở Admin. |
| Enqueue API | Chỉ dựa vào `theme_assets['...']` thủ công trong từng template | Không quản lý được thứ tự phụ thuộc (`deps`), dễ nạp trùng stylesheet/script giữa các parts và thiếu lọc theo ngữ cảnh trang như WordPress. |
| Biến động trong `<script>` / `on*` | Cho phép `{{ entry.title }}` trực tiếp trong `<script>` hoặc `onclick` | MiniJinja dùng HTML auto-escape (không phải JS string escape), dễ gây lỗi cú pháp JS hoặc Stored XSS từ Editor; dùng `data-*` attribute trên thẻ HTML an toàn và chuẩn HTML5. |

---

## Non-Functional Requirements

- **Phân tầng ([CORE_FOUNDATION.md](../contracts/CORE_FOUNDATION.md)):** Toàn bộ kiểu Enqueue (`ThemeStyleEnqueueDecl`, `ThemeScriptEnqueueDecl`, `EnqueueCondition`, `ScriptLocation`, `ScriptStrategy`, `ResolvedEnqueuedAssets`, `resolve_enqueued_assets`) đặt trong `opendash-core` (thuần Rust, 0 I/O, 0 SQL). Không thay đổi SQLite schema hay mutation API (`M8_WRITER_INVENTORY.md` giữ nguyên).
- **Bảo toàn ranh giới untrusted content:** `opendash-core::richtext` (nội dung bài viết/editor), `normalize_public_document` mặc định (`false`), đường legacy `render_csec()`, và Admin Panel CSP (`/admin`) giữ nguyên 100% bảo mật chống XSS.
- **Tương thích ngược:** Mọi theme v3 hiện có (không khai báo `styles`/`scripts`, dùng `assets` phẳng hoặc dùng `{{ od_icon(...) }}`) tiếp tục hoạt động bình thường; `ThemeCheckSummary` không đổi schema.
- **Hiệu năng:** `resolve_enqueued_assets` chạy `O(V + E)` trên tối đa 64 styles / 64 scripts (< 50 µs/request), không truy vấn DB thêm.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| File | Action | Reason |
|---|---|---|
| `cms/crates/opendash-core/src/theme.rs` | Modify | Thêm `"js" \| "mjs" \| "svg"` vào `valid_v3_asset_path()`, khóa raster-only cho `pattern.preview_asset`, thêm Enqueue types + DAG validation + `resolve_enqueued_assets()` + cập nhật 2 unit tests (`L715-716`, `L740`). |
| `cms/crates/opendash-server/src/theme.rs` | Modify | Mở `.js`, `.mjs`, `.svg` trong `v3_entry_cap_and_allowlist()`, thêm UTF-8 check trong `validate_theme_v3_files()`, thêm MIME trong `theme_asset_mime()`; ở S02 cập nhật 3 unit tests cũ (`L106-110`, `L1533`, `L1747-1748`) sang payload vẫn bị cấm (`<iframe>` / `<object>`). |
| `cms/crates/opendash-server/src/routes/themes.rs` | Modify | Thêm header `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox` riêng cho `image/svg+xml` trong `theme_asset()`. |
| `cms/crates/opendash-server/src/template_sec.rs` | Modify | Thêm `allow_trusted_theme_markup: bool` xuyên suốt 6 call-site Theme/Preview/Block (giữ `false` cho legacy `render_csec*`), tăng `HtmlScanState::Tag.prefix` lên `8`, thêm `ThemeHtmlState::Script` & `HtmlScanState::Script` + `find_rawtext_or_rcdata_end` `"script"`, mở `<svg>`/`xmlns`/`xlink:href`/`<title>` trong `<svg>`/`<script>`/`on*` tĩnh, tách `forbids_dynamic_interpolation`, cập nhật unit tests bị ảnh hưởng (`L6239`, `L6393`, `L6835-6843`, `L7132-7134`). |
| `cms/crates/opendash-server/src/document.rs` | Modify | Thêm `TokenKind::ScriptBlock`, `find_case_insensitive_end_script`, `SafeDocumentParts.authorized_head_scripts`, và `normalize_public_document_with_options(raw_html, allow_trusted_theme_markup)` (hỗ trợ `<svg>`, `<title>` trong `<svg>`, `<script>`, `on*` khi `true`; giữ mặc định `false`). |
| `cms/crates/opendash-server/src/content_render.rs` | Modify | Bổ sung `compose_document_with_seo_with_assets(...)` với dedup `head_style_urls` + gom `authorized_head_scripts` + chèn `head_styles`/`head_scripts`/`footer_scripts`. |
| `cms/crates/opendash-server/src/routes/public.rs` | Modify | Đổi `script-src 'none'` → `script-src 'self' 'unsafe-inline'` trên cả 4 nhánh của `rendered_layout_with_form_csp`. |
| `cms/crates/opendash-server/src/routes/public_content.rs` & `storage/overrides.rs` | Modify | Đổi 3 điểm chuẩn hóa slot trung gian (`public_content.rs:188`, `overrides.rs:3591, 3878`) sang `normalize_public_document_with_options(..., true)`; nối `resolve_enqueued_assets` theo bảng ánh xạ `EnqueueCondition` (gồm `Home`/`Page`/`Single`/`Archive`/`Taxonomy`/`NotFound`); preview chỉ bơm `head_styles`. |
| `cms/crates/opendash-server/tests/{m7_s05_t02_form_ui,multilingual_s04_test,studio_builder_test,theme_chrome_cache_icons_test}.rs` | Modify | Cập nhật assertion CSP công khai từ `script-src 'none'` sang `script-src 'self' 'unsafe-inline'` (và giữ assertion chặn `<svg>`/`<script>` trên đường untrusted/legacy nếu có). |
| `cms/crates/opendash-server/tests/trusted_theme_js_svg_enqueue_test.rs` | Create | Suite integration/E2E kiểm chứng CUJ 1 (enqueue topo-sort, cross-location promote, conditions, dedup, MIME `.js`/`.mjs`/`.svg`, standalone SVG sandbox header, cycle rejection ở `--check-theme`) và CUJ 2 (inline `<svg xmlns=...><title>...</title></svg>`, inline `<script>` chứa `<`, `onclick`, `od_icon()` cùng tồn tại với `<script>`, CSP `script-src 'self' 'unsafe-inline'`, từ chối `{{ ... }}` trong `<script>`/`onclick`, và untrusted `values.body` vẫn bị escape/sanitize). |

---

### Slice S01: Enqueue Manifest Schema, Asset Allowlist/MIME & Topo-Sort Resolver

* **Dependencies:** None (baseline `main@537c8a1`).
* **File Ownership:** `cms/crates/opendash-core/src/theme.rs`, `cms/crates/opendash-server/src/theme.rs`, `cms/crates/opendash-server/src/routes/themes.rs`.
* **Công việc chính:**
  1. Cập nhật `valid_v3_asset_path()` trong `opendash-core/src/theme.rs` cho phép `.js`, `.mjs`, `.svg`; khóa raster-only cho `pattern.preview_asset`; định nghĩa `EnqueueCondition`, `ScriptLocation`, `ScriptStrategy`, `ThemeStyleEnqueueDecl`, `ThemeScriptEnqueueDecl`, `ResolvedEnqueuedAssets` và mở rộng `ThemeV3Manifest` (`styles`, `scripts`).
  2. Triển khai validation đầy đủ (handle format, uniqueness, max 64, `src` ∈ `assets` + đúng extension, `media` safe chars `[a-zA-Z0-9 (),:./-]`, DAG cycle detection, virtual handle `"opendash-tailwind"`) và `resolve_enqueued_assets()` (kiểm 64-hex `revision_digest`, topo-sort, transitive deps, auto-promote `head` script deps, dedup).
  3. Mở rộng `v3_entry_cap_and_allowlist`, UTF-8 check trong `validate_theme_v3_files`, và `theme_asset_mime` trong `opendash-server/src/theme.rs`; bổ sung header CSP `sandbox` cho `image/svg+xml` trong `routes/themes.rs::theme_asset`.
  4. Cập nhật 2 unit test cũ trong `opendash-core/src/theme.rs` (`L715-716`, `L740`) và 1 unit test đuôi asset trong `opendash-server/src/theme.rs` (`L1405-1406`) sang kiểm thử các đuôi vẫn cấm (`.wasm`, `.html`, `.zip`).
* **Acceptance Criteria:**
  - [ ] Unit tests `opendash_core::theme` xác nhận topo-sort đúng thứ tự `deps`, kéo dep bắc cầu, auto-promote dep của `head` script, bỏ qua virtual handle `"opendash-tailwind"`, lọc theo `EnqueueCondition`, từ chối chu trình DAG, handle trùng hoặc `src` không có trong `assets`.
  - [ ] Unit tests `opendash_server::theme` xác nhận gói ZIP v3 chứa `assets/*.js`, `assets/*.mjs`, `assets/*.svg` hợp lệ (từ chối non-UTF8), trả đúng MIME `text/javascript; charset=utf-8` / `image/svg+xml`, và `theme_asset` đặt header `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox` cho `.svg`.
  - [ ] `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` PASS 100%.
* **Rollback:** `git revert` commit của Slice S01 (additive trên `ThemeV3Manifest`, không migration).

### Slice S02: Trusted Theme Inline SVG/JS, Raw-Text Script Scanner, Enqueue Injection & Public CSP

* **Dependencies:** Slice `S01`.
* **File Ownership:** `cms/crates/opendash-server/src/{template_sec.rs,document.rs,content_render.rs,theme.rs,routes/public.rs,routes/public_content.rs,storage/overrides.rs}`, `cms/crates/opendash-server/tests/{m7_s05_t02_form_ui,multilingual_s04_test,studio_builder_test,theme_chrome_cache_icons_test,trusted_theme_js_svg_enqueue_test}.rs`.
* **Công việc chính:**
  1. Cập nhật `template_sec.rs`: tham số hóa `allow_trusted_theme_markup: bool` qua đủ 6 call-site Theme/Preview/Block (giữ `false` cho legacy `render_csec*`), tăng `HtmlScanState::Tag.prefix` lên `8`, thêm `ThemeHtmlState::Script` & `HtmlScanState::Script` + `find_rawtext_or_rcdata_end` `"script"`, mở `<svg>`/`xmlns`/`xmlns:xlink`/`xlink:href`/`<title>` trong `<svg>`/`<script>`/`on*` tĩnh, tách `forbids_dynamic_interpolation` (vẫn cấm `Stmt::EmitExpr` & control-flow trong `<script>` và cấm `{{`/`{%` trong `on*`).
  2. Cập nhật `document.rs`: thêm `TokenKind::ScriptBlock`, `find_case_insensitive_end_script`, `SafeDocumentParts.authorized_head_scripts`, và `normalize_public_document_with_options(raw_html, allow_trusted_theme_markup)` (bảo toàn `<title>` trong `<svg>`, trả `script_outside_envelope` nếu ngoài `<head>`/`<body>`, giữ mặc định `false`).
  3. Cập nhật `content_render.rs`, `routes/public_content.rs`, `storage/overrides.rs`: đổi 3 điểm chuẩn hóa slot trung gian (`public_content.rs:188`, `overrides.rs:3591, 3878`) sang `normalize_public_document_with_options(..., true)`; triển khai `compose_document_with_seo_with_assets` với dedup `head_style_urls` & `authorized_head_scripts`; nối `resolve_enqueued_assets` theo bảng ánh xạ `EnqueueCondition` (`Home`/`Page`/`Single`/`Archive`/`Taxonomy`/`NotFound`); luồng preview chỉ bơm `head_styles`.
  4. Cập nhật `routes/public.rs::rendered_layout_with_form_csp`: đổi `script-src 'none'` → `script-src 'self' 'unsafe-inline'` trên cả 4 nhánh.
  5. Cập nhật các unit/integration test cũ bị ảnh hưởng (`theme.rs:106-110, 1533, 1747-1748`, `template_sec.rs:6239, 6393, 6835-6843, 7132-7134`, và 4 file test CSP `m7_s05_t02_form_ui`, `multilingual_s04_test`, `studio_builder_test`, `theme_chrome_cache_icons_test`) — đổi payload comment smuggling/unsafe sang thẻ vẫn bị cấm (`<object>`/`<iframe>`).
  6. Viết suite E2E `trusted_theme_js_svg_enqueue_test.rs` kiểm chứng đầy đủ CUJ 1 & CUJ 2.
* **Acceptance Criteria:**
  - [ ] Theme v3 & Declarative Block dùng inline `<svg xmlns="..."><title>...</title>...</svg>`, inline `<script>` chứa toán tử `<`/`>`, thuộc tính `onclick="..."`, `{{ od_icon(...) }}` đi cùng `<script>`, và `styles`/`scripts` enqueue đều pass `--check-theme`, cài được không có cảnh báo ở Admin, và render đúng thứ tự trên public HTTP.
  - [ ] `GET /theme/<digest>/assets/app.js` trả `200` (`text/javascript; charset=utf-8`); `GET /theme/<digest>/assets/logo.svg` trả `200` (`image/svg+xml` + CSP `sandbox`).
  - [ ] Public HTTP response trả `Content-Security-Policy` có `script-src 'self' 'unsafe-inline'`; nội dung untrusted trong `values.body` vẫn bị escape/sanitize sạch; `{{ ... }}` trong `<script>` hoặc `onclick` bị từ chối ở compile-time.
  - [ ] `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` PASS 100%.
* **Rollback:** `git revert` commit của Slice S02 (không migration SQLite).

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
- `cargo test --manifest-path cms/Cargo.toml --release --locked --test trusted_theme_js_svg_enqueue_test`.
- Chạy CLI `opendash --check-theme` trên theme có inline `<svg>`, inline `<script>`, `styles`/`scripts` hợp lệ (exit `0`) và theme có vòng lặp `deps` trong `scripts` hoặc `{{ entry.title }}` trong `<script>` (exit non-zero).
