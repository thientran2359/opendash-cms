# Tailwind-First Public Frontend, Built-in `opendash-starter` Default Theme v3, 6 Starter Patterns & 1-Click Sample Content

- **Author:** Orion (Elephant)
- **Status:** Approved
- **Date:** 2026-11-22
- **Baseline:** `main@cb97b2d`
- **Approved By:** Orion (delegated) / Owner
- **Approval Evidence:** Owner chat 2026-11-22 ("duyệt cổng 1", "duyệt cổng 2", "duyệt cổng 3, 4" — Tailwind v4 public frontend & always-active `opendash-starter` default Theme v3)
- **Goldfish Review:** Round 1 independent review (`egm-design`, `egm-codebase-details`, `egm-implementation-readiness`) · 2026-11-23 · reviewed `main@cb97b2d` (2 Majors, 1 Minor, 3 Nits resolved)
- **Related:** [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md), [`docs/contracts/CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`docs/contracts/PLUGIN_ABI.md`](../contracts/PLUGIN_ABI.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md), [`docs/designs/theme-format-and-template-resolver.md`](theme-format-and-template-resolver.md), [`docs/designs/opendash-studio-customizer-and-ux-builder.md`](opendash-studio-customizer-and-ux-builder.md), [`docs/designs/declarative-block-json-engine.md`](declarative-block-json-engine.md), [`docs/designs/wordpress-style-permalinks.md`](wordpress-style-permalinks.md)

---

## Overview

OpenDash đã hoàn thiện **OpenDash Studio** (`#/studio` Customizer + `#/studio/entry/:typeKey/:id` Entry Visual Builder), **Theme Format v3 & Template Resolver 2 Trục**, **Declarative `block.json` Block Engine** (với 5 Core Blocks nhúng sẵn: `core/banner`, `core/card`, `core/accordion`, `core/divider`, `core/spacer`) và **WordPress-Style Permalinks** (`/{slug}`, `/posts`, `/category/{slug}`). Tuy nhiên, khi cài mới CMS hoặc chạy thử luồng hoạt động tổng thể từ Admin ra trang công khai, hệ thống còn bốn điểm nghẽn trải nghiệm lớn:

1. **Trang công khai chưa có Tailwind CSS v4:** Trong khi giao diện quản trị (`cms/admin/`) đã dùng Tailwind CSS v4 (`tailwindcss@4.1.17`), trang công khai (`/`, `/{slug}`, `/posts`, `/category/{slug}`) và iframe xem trước của OpenDash Studio chưa được nạp stylesheet Tailwind v4. Các mẫu theme sau này và 5 Core Blocks chưa thể dùng chung hệ thống utility class và biến màu/font (`--od-token-*`).
2. **CMS mới cài đặt chưa có Theme v3 mặc định đang active:** Bảng `theme_active_selection` khởi tạo với `revision_id = NULL` (`version = 1`), khiến một website OpenDash mới cài rơi vào nhánh `TemplateResolution::CoreFallback` thô sơ, thư viện Pattern trong Studio trống rỗng, và người dùng phải tự đóng gói/upload file ZIP mới có giao diện hoạt động.
3. **Rule `body { max-width: 850px }` trong `content_render.rs` làm vỡ bố cục Theme v3:** Khi một bài viết hoặc trang soạn bằng Classic Editor (`ContentMode::Editor`) được render vào `{{ od_content() }}` của Theme v3 ([`routes/public_content.rs#L115-L120`](../../cms/crates/opendash-server/src/routes/public_content.rs#L115-L120)), `normalize_public_document` trích thẻ `<style>body{...max-width:850px...}</style>` từ [`content_render.rs::DEFAULT_SINGLE`](../../cms/crates/opendash-server/src/content_render.rs#L40-L46) và đưa lên `<head>` của trang tổng, bóp nghẹt toàn bộ `<body>` của mọi Theme v3 xuống `850px`.
4. **Fixture `theme-moc-studio` còn chứa đường dẫn `/content/...` cũ:** Các template mẫu trong [`cms/fixtures/demo-website/theme-moc-studio/`](../../cms/fixtures/demo-website/theme-moc-studio/header.html#L57-L61) vẫn hardcode `/content/post`, `/content/page/gioi-thieu` thay vì WordPress-Style Permalinks (`/posts`, `/gioi-thieu`).

Đặc tả này thiết kế **Tailwind-First Public Frontend** (`/assets/opendash-tailwind.css` biên dịch sẵn từ `tailwindcss@4.1.17` và nhúng vào binary) cùng **Built-in Default Theme v3 `opendash-starter`** luôn được cài đặt và kích hoạt mặc định ngay khi khởi động CMS (`ensure_default_starter_theme`), đi kèm 6 Starter Patterns, 2 Theme Blocks (`opendash-starter/stats-bar`, `opendash-starter/quote-box`), 3 Page Layouts (`default`, `full-width`, `canvas`) và gói dữ liệu mẫu (`sample`) để người dùng trải nghiệm trọn vẹn toàn bộ luồng CMS ngay lập tức.

---

## The Problem

1. **Ràng buộc Single-Executable & CSP `script-src 'none'` trên trang công khai ([`routes/public.rs#L56-L71`](../../cms/crates/opendash-server/src/routes/public.rs#L56-L71)):**
   - Trang công khai áp dụng CSP nghiêm ngặt `default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'none'`. Do đó không thể dùng Tailwind Play CDN (`<script src="https://cdn.tailwindcss.com"></script>`), cũng không thể gọi Node CLI lúc runtime vì binary `opendash` phải chạy độc lập 100% không cần Node.js trên máy chủ đích.
   - Trong OpenDash Studio, [`BuilderPreviewFrame.tsx`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx#L88-L94) chạy iframe `sandbox="allow-scripts"` (opaque origin) và hiện tại hàm `rewriteRootRelativeUrl` chỉ rewrite `/theme/` và `/media/`, chưa rewrite `/assets/opendash-tailwind.css` sang `${origin}/assets/opendash-tailwind.css`.
2. **Tailwind Preflight làm mất định dạng HTML của bài viết Classic Editor / RichText:**
   - Tailwind v4 Preflight reset toàn bộ `h1`–`h6`, `p`, `ul`, `ol`, `blockquote`, `table` về cùng kích thước chữ và margin `0`. Nếu nạp Tailwind lên frontend mà không có lớp typography `.od-prose` và các utility gắn với 6 Semantic Theme Tokens (`--od-token-color-primary`, `--od-token-color-surface`, `--od-token-color-text`, `--od-token-color-muted`, `--od-token-font-heading`, `--od-token-font-body`), nội dung bài viết trong `{{ od_content() }}` và field `richtext` của block sẽ mất định dạng.
3. **Rò rỉ `<style>body { max-width: 850px }</style>` từ `DEFAULT_SINGLE` / `DEFAULT_ARCHIVE` ([`content_render.rs#L17-L58`](../../cms/crates/opendash-server/src/content_render.rs#L17-L58)):**
   - Khi entry ở `ContentMode::Editor`, [`routes/public_content.rs#L525-L531`](../../cms/crates/opendash-server/src/routes/public_content.rs#L525-L531) tạo `fallback_html` bằng `content_render::render_single_with_site_and_archive` (dùng `DEFAULT_SINGLE`), sau đó [`render_resolved_public_template_html#L115-L120`](../../cms/crates/opendash-server/src/routes/public_content.rs#L115-L120) ghép `parts.authorized_styles` + `parts.body_fragment` vào `slot_html` (`{{ od_content() }}`), và cuối cùng `compose_document_with_seo` gom thẻ `<style>` đó lên `<head>` của trang Theme v3.
4. **Đảm bảo bất biến "CMS luôn có 1 Theme v3 đang active" mà không phá vỡ tương thích của bộ kiểm thử & backup/restore:**
   - Yêu cầu sản phẩm: **`opendash-starter` luôn là theme mặc định của CMS; CMS luôn yêu cầu có 1 theme đang active, và ngay khi cài CMS thì `opendash-starter` đã có sẵn trên đĩa + DB và đang active ở `version = 1`**.
   - Nếu thư mục `themes/revisions/<digest>/` của `opendash-starter` chưa tồn tại hoặc bị hỏng trên đĩa (ví dụ khi mở một DB được migrate hoặc khôi phục từ bản sao lưu cũ chưa có theme v3), hệ thống phải tự động tái tạo (self-heal) thư mục revision của `opendash-starter` từ dữ liệu nhúng trong binary.
   - Các template của `opendash-starter` phải hiển thị đầy đủ các trường chuẩn (`site.title`, `site.tagline`, `entry.title`, `od_content()`, `entry.fields`, `entries`, `archive_url`, `previous_url`, `next_url`, `menus.primary`, `menus.footer`, và tiêu đề `404 — Không tìm thấy trang`) để mọi luồng kiểm thử render công khai hiện có đều hoạt động trơn tru.

---

## Goals

- **G1 — Built-in Public Tailwind CSS v4 Bundle (`/assets/opendash-tailwind.css`):**
  - Biên dịch tĩnh từ `tailwindcss@4.1.17` (đã pin trong `cms/admin/package.json`) qua script thuần Node `cms/admin/scripts/build-public-tailwind.mjs` (không thêm bất kỳ npm package hay crate Rust mới nào), quét toàn bộ `builtin_blocks/**/*.html`, `builtin_themes/opendash-starter/**/*.html`, `cms/fixtures/demo-website/**/*.html`, `page.rs`, `content_render.rs` và danh mục safelist `cms/admin/src/tailwind-public-safelist.txt`.
  - Định nghĩa sẵn các utility gắn với 6 Semantic Theme Tokens (`bg-od-primary`, `text-od-primary`, `border-od-primary`, `bg-od-surface`, `bg-od-surface-alt`, `text-od-text`, `text-od-muted`, `border-od-border`, `font-od-heading`, `font-od-body`) kèm giá trị fallback trên `:root`, cùng lớp `.od-prose` cho nội dung bài viết/RichText.
  - Nhúng trực tiếp vào binary Rust qua `include_bytes!("../builtin_themes/opendash-tailwind.css")` trong `routes::builder::serve_asset`, tự động chèn `<link rel="stylesheet" href="/assets/opendash-tailwind.css">` một lần duy nhất vào `<head>` trong `content_render::compose_document_with_seo`, và rewrite URL này trong `BuilderPreviewFrame.tsx` cho Studio Preview iframe.
- **G2 — Theme Mặc định `opendash-starter` (`manifest_version: 3`) Luôn Cài sẵn & Active Mặc định:**
  - Nhúng toàn bộ gói `opendash-starter@1.0.0` tại `cms/crates/opendash-server/src/builtin_themes/opendash-starter/`.
  - Hàm `storage::theme::ensure_default_starter_theme(db, themes_root)` được gọi tự động khi khởi động server (và có thể gọi từ test/restore): đảm bảo revision `opendash-starter@1.0.0` luôn tồn tại bền vững trên đĩa (`themes/revisions/<digest>/.complete`, tự phục hồi nếu thiếu file trên đĩa) và trong bảng `theme_packages` / `theme_revisions`; nếu `theme_active_selection.revision_id IS NULL`, tự động kích hoạt `opendash-starter` (đồng bộ 2 theme blocks vào `builder_block_definitions`) và giữ `theme_active_selection.version = 1` ở lần kích hoạt mặc định đầu tiên.
- **G3 — Bộ Template Tailwind v4, 3 Page Layouts, 6 Starter Patterns, 2 Theme Blocks & Dữ liệu Mẫu (`sample`) trong `opendash-starter`:**
  - Cung cấp đủ 9 template (`home.html`, `page.html`, `single.html`, `single-post.html`, `archive.html`, `archive-post.html`, `taxonomy.html`, `404.html`, `index.html`), 3 parts (`header.html`, `footer.html`, `template-part/post-card.html`), 3 page layouts (`default.html`, `full-width.html`, `canvas.html`), 2 theme blocks (`opendash-starter/stats-bar`, `opendash-starter/quote-box`), 6 starter patterns (`hero_launchpad`, `features_bento_grid`, `about_story_split`, `pricing_tiers`, `faq_accordion`, `cta_newsletter_banner`), 5 theme options và gói dữ liệu mẫu `sample` tương thích hoàn toàn với `ThemeSampleManifest` (`storage/import.rs`).
- **G4 — Nâng cấp 5 Core Built-in Blocks & Sửa lỗi `DEFAULT_SINGLE`/`DEFAULT_ARCHIVE` + Đồng bộ `theme-moc-studio`:**
  - Bỏ `<style>body{max-width:850px...}</style>` toàn cục khỏi `DEFAULT_ARCHIVE` và `DEFAULT_SINGLE`, thay bằng class Tailwind cục bộ và `.od-prose`.
  - Nâng cấp 5 file `builtin_blocks/{banner,card,accordion,divider,spacer}/render.html` với class Tailwind v4 trong khi bảo toàn 100% class `od-block-*` và biến `--od-*`.
  - Cập nhật `cms/fixtures/demo-website/theme-moc-studio/` sang WordPress-Style Permalinks và class Tailwind v4.

---

## Non-Goals

- Không nhúng trình biên dịch CSS động (Node.js CLI, LightningCSS hay Tailwind Oxide) vào tiến trình runtime của `opendash-server`.
- Không nới lỏng chính sách bảo mật CSP trên trang công khai (`script-src 'none'` vẫn được giữ nguyên tuyệt đối).
- Không thêm bất kỳ dependency mới nào vào `cms/Cargo.toml` hay `cms/admin/package.json`.
- Không khai báo custom CPT hoặc custom taxonomy trong `opendash-starter/theme.json` (`declarations: null`) để không làm thay đổi danh mục CPT/taxonomy gốc của hệ thống khi mới cài đặt.

---

## Open Questions

*(Không còn câu hỏi mở. Toàn bộ yêu cầu sản phẩm và thiết kế kỹ thuật ở 4 cổng đã được Chủ dự án phê duyệt.)*

---

## Critical User Journeys

1. **CUJ-1 — Khởi động CMS mới cài đặt: `opendash-starter` đã active sẵn cùng Tailwind CSS v4:**
   - **Given** người dùng tải binary `opendash` về và chạy lần đầu trên thư mục dữ liệu trống.
   - **When** truy cập trang công khai (`GET /`, `GET /posts`, `GET /khong-ton-tai`) hoặc mở `GET /api/admin/themes` và `#/studio`.
   - **Then** `theme_active_selection` đã trỏ sẵn tới `opendash-starter@1.0.0` (`source_format: "v3"`, `version: 1`); mọi trang công khai và preview đều nạp `<link rel="stylesheet" href="/assets/opendash-tailwind.css">` (`200 OK`, `text/css; charset=utf-8`), hiển thị Header/Footer/Main hiện đại bằng Tailwind v4; `#/studio` nạp sẵn 6 Starter Patterns và 7 blocks (5 Core Blocks + 2 Theme Blocks của `opendash-starter`).
2. **CUJ-2 — Nhập dữ liệu mẫu (`sample`) và kiểm thử trọn luồng Website:**
   - **Given** Admin mở `#/themes` (hoặc gọi API sample import cho `opendash-starter`).
   - **When** chạy Dry-run và Execute nhập dữ liệu mẫu (`pages`, `menus`, `settings`) rồi xuất bản các trang/bài viết mẫu và gán Trang chủ trong Reading Settings.
   - **Then** website có ngay các trang tĩnh (`/trang-chu`, `/gioi-thieu`, `/lien-he`), bài viết tin tức, 2 menu điều hướng (`primary`, `footer`) và các tùy chọn giao diện mẫu; cả trang dựng bằng Visual Builder lẫn bài viết Classic Editor đều hiển thị chuẩn bố cục Tailwind v4 mà không bị bó hẹp `body` xuống `850px`.
3. **CUJ-3 — Đổi sang Theme v3 khác và Khôi phục từ Backup (`Self-Healing`):**
   - **Given** Admin cài đặt và kích hoạt một Theme v3 khác (ví dụ `moc-studio`), hoặc xóa nhầm thư mục `themes/revisions/<starter_digest>/` trên đĩa rồi khởi động lại server.
   - **When** server khởi động lại.
   - **Then** `ensure_default_starter_theme` tự động khôi phục thư mục `themes/revisions/<starter_digest>/.complete` trên đĩa để đảm bảo tính bền vững (`A-DURABILITY`), đồng thời **giữ nguyên** `moc-studio` đang active (chỉ tự kích hoạt `opendash-starter` khi `revision_id IS NULL`).

---

## Technical Plan

### 1. Kiến trúc Hai Tầng cho Tailwind CSS v4 (`/assets/opendash-tailwind.css`)

```text
[Build-time in cms/admin]
  src/tailwind-public.css + src/tailwind-public-safelist.txt
  + builtin_blocks/**/*.html + builtin_themes/opendash-starter/**/*.html
  + fixtures/demo-website/**/*.html + content_render.rs + page.rs
        │
        ▼ (node scripts/build-public-tailwind.mjs using pinned tailwindcss@4.1.17)
  cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css
        │
        ▼ (include_bytes! in opendash-server binary)
  GET /assets/opendash-tailwind.css (200 OK, text/css, immutable)
        ├── Auto-injected once in content_render::compose_document_with_seo (<head>)
        └── Rewritten to ${origin}/assets/opendash-tailwind.css in BuilderPreviewFrame.tsx
```

1. **Script biên dịch thuần Node (`cms/admin/scripts/build-public-tailwind.mjs`) không cần cài thêm package:**
   - Sử dụng trực tiếp API `compile` của `tailwindcss` (`4.1.17`) và `optimize` (minify) của `@tailwindcss/node` cùng `Scanner` của `@tailwindcss/oxide` đã có sẵn trong `cms/admin/node_modules/`.
   - Quét toàn bộ candidate classes từ:
     - `cms/crates/opendash-server/src/builtin_blocks/**/*.html`
     - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/**/*.{html,json}`
     - `cms/fixtures/demo-website/**/*.{html,json}`
     - `cms/crates/opendash-server/src/{content_render.rs,page.rs,routes/public.rs,routes/public_content.rs}`
     - `cms/admin/src/tailwind-public-safelist.txt` (bao phủ layout container, grid 1–12 cột, flex, spacing, sizing, typography, bảng màu slate/zinc/blue/indigo/emerald/amber/rose, shadows, borders, rounded, transitions, responsive prefixes `sm:`, `md:`, `lg:`, `xl:`).
   - Kết quả biên dịch đã minify được ghi ra `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css` và commit vào repository để `cargo build` / `cargo test` không phụ thuộc vào thứ tự chạy Node.
2. **Hệ thống 6 Semantic Theme Tokens & Lớp Typography `.od-prose` trong `cms/admin/src/tailwind-public.css`:**
   - Khai báo biến CSS mặc định trên `:root` (tương thích với 6 token của OpenDash Studio `ThemeOptionsForm` / `page.rs`):
     ```css
     :root {
       --od-token-color-primary: #2563eb;
       --od-token-color-surface: #ffffff;
       --od-token-color-surface-alt: #f8fafc;
       --od-token-color-text: #0f172a;
       --od-token-color-muted: #475569;
       --od-token-color-border: #e2e8f0;
       --od-token-font-heading: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
       --od-token-font-body: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
     }
     ```
   - Ánh xạ vào `@theme` của Tailwind v4 để tạo ra các utility class: `bg-od-primary`, `text-od-primary`, `border-od-primary`, `bg-od-surface`, `bg-od-surface-alt`, `text-od-text`, `text-od-muted`, `border-od-border`, `font-od-heading`, `font-od-body`.
   - Định nghĩa lớp `.od-prose` cho nội dung bài viết và RichText (`h1`–`h4`, `p`, `ul`, `ol`, `li`, `blockquote`, `pre`, `code`, `a`, `img`, `hr`, `table`, `dl`, `dt`, `dd`) để Preflight không làm mất khoảng cách đoạn văn, danh sách hay cỡ chữ tiêu đề.
3. **Phục vụ `/assets/opendash-tailwind.css` và tự động gắn vào `<head>`:**
   - Trong [`cms/crates/opendash-server/src/routes/builder.rs::serve_asset`](../../cms/crates/opendash-server/src/routes/builder.rs#L52-L74):
     - Kiểm tra trước tiên: nếu `clean_path == "opendash-tailwind.css"`, trả về `include_bytes!("../builtin_themes/opendash-tailwind.css")` với `StatusCode::OK`, `Content-Type: text/css; charset=utf-8`, `X-Content-Type-Options: nosniff`, `Cache-Control: public, max-age=3600` (cho phép trình duyệt cập nhật sạch sau mỗi lần nâng cấp binary `opendash`, tránh giữ CSS cũ 1 năm ở URL không gắn hash).
   - Trong [`cms/crates/opendash-server/src/content_render.rs::compose_document_with_seo`](../../cms/crates/opendash-server/src/content_render.rs#L345-L355):
     - Trước vòng lặp `for style in styles`, nếu `!styles.iter().any(|s| s.contains("/assets/opendash-tailwind.css"))`, tự động đẩy `<link rel="stylesheet" href="/assets/opendash-tailwind.css">` vào `head`.
   - Trong [`cms/admin/src/builder/BuilderPreviewFrame.tsx::rewriteRootRelativeUrl`](../../cms/admin/src/builder/BuilderPreviewFrame.tsx#L88-L94):
     - Cho phép rewrite cả `raw === '/assets/opendash-tailwind.css'` thành `${origin}${raw}`.
   - Trong [`cms/admin/vite.config.ts`](../../cms/admin/vite.config.ts#L25):
     - Mở rộng regex proxy dev server sang `^/(api/|auth/|assets/opendash-tailwind\.css$|registry$|page$|theme$|preview$|save$|$)` để khi phát triển giao diện trên cổng Vite dev server, iframe preview tải được `/assets/opendash-tailwind.css` từ backend Rust.
4. **Khắc phục lỗi `<style>body{max-width:850px...}</style>` trong `DEFAULT_ARCHIVE` & `DEFAULT_SINGLE` ([`content_render.rs#L17-L58`](../../cms/crates/opendash-server/src/content_render.rs#L17-L58)):**
   - Loại bỏ hoàn toàn thẻ `<style>body{...}</style>` trong `DEFAULT_ARCHIVE` và `DEFAULT_SINGLE`.
   - Sử dụng trực tiếp class Tailwind v4 và `.od-prose` trên `<main class="mx-auto max-w-4xl px-4 py-8 text-od-text">` và `<article class="od-prose">`. Khi `DEFAULT_SINGLE` được dùng làm `fallback_html` cho `{{ od_content() }}` trong một Theme v3, `parts.authorized_styles` sẽ rỗng (không bơm CSS ghi đè `body` lên `<head>`), còn `parts.body_fragment` vẫn giữ nguyên cấu trúc ngữ nghĩa (`<main>`, `<header>`, `<nav>`, `<article>`, `<h1>`, `<dl>`, `<dt>`, `<dd>`) tương thích 100% với mọi bài test hiện có.

---

### 2. Theme Mặc định `opendash-starter` (`manifest_version: 3`) & Cơ chế `ensure_default_starter_theme`

#### 2.1. Module `cms/crates/opendash-server/src/builtin_themes/mod.rs`

Cung cấp hằng số và hàm truy xuất cây file nhúng trong binary:

```rust
pub const STARTER_THEME_ID: &str = "opendash-starter";
pub const STARTER_THEME_NAME: &str = "OpenDash Starter";
pub const STARTER_THEME_VERSION: &str = "1.0.0";
pub const PUBLIC_TAILWIND_CSS: &[u8] = include_bytes!("opendash-tailwind.css");

pub fn builtin_starter_theme_files() -> BTreeMap<String, Vec<u8>>;
```

#### 2.2. Hàm `storage::theme::ensure_default_starter_theme` & Tích hợp Startup / Backup

1. **Chữ ký hàm trong [`cms/crates/opendash-server/src/storage/theme.rs`](../../cms/crates/opendash-server/src/storage/theme.rs):**
   ```rust
   pub async fn ensure_default_starter_theme(
       db: &SqlitePool,
       themes_root: &Path,
   ) -> ApiResult<ThemeRevisionRecord>
   ```
2. **Thuật toán Idempotent, Self-Healing & Bảo toàn `version = 1`:**
   - **Bước 1:** Lấy `files = crate::builtin_themes::builtin_starter_theme_files()` và xác thực qua `crate::theme::validate_theme_v3_files(files.clone())` (đảm bảo gói nhúng luôn vượt qua 100% kiểm tra manifest v3, C-SEC, D01 assets, `blocks/<slug>/` và `patterns/*.json`).
   - **Bước 2 (Self-heal trên đĩa nếu thư mục revision bị thiếu hoặc hỏng):** Tính `file_digests` và `revision_digest = compute_v3_revision_digest(&pkg.manifest_json, &file_digests)`. Nếu `themes_root.join("revisions").join(&revision_digest)` tồn tại nhưng `!dir_matches_revision(&rev_dir, &revision_digest, &file_digests)`, xóa thư mục hỏng đó trước khi gọi `stage_and_commit_v3_revision` để tránh lỗi `409 Conflict` và tự phục hồi hoàn toàn trên đĩa.
   - **Bước 3 (Commit revision vào đĩa & DB):** Gọi `stage_and_commit_v3_revision(db, themes_root, "bootstrap-starter-v3", STARTER_THEME_ID, STARTER_THEME_NAME, STARTER_THEME_VERSION, &pkg.files, "{\"source\":\"builtin_starter\"}").await?` để nhận `starter_rev: ThemeRevisionRecord`.
   - **Bước 4 (Kích hoạt mặc định nếu chưa có theme active):**
     - Đọc `selection = get_active_selection(db).await?`.
     - Nếu `selection.revision_id.is_none()`:
       - Gọi `activate_revision(db, themes_root, &starter_rev.revision_id, Some(selection.version)).await?` (đồng bộ 2 theme blocks `opendash-starter/stats-bar` và `opendash-starter/quote-box` vào `builder_block_definitions`, đồng bộ declarations/extensions và resync media ACL).
       - Nếu trước khi kích hoạt `selection.version == 1`, thực hiện `UPDATE theme_active_selection SET version = 1 WHERE id = 1 AND revision_id = ?` để phiên bản chọn khởi tạo của một site mới vẫn là `version = 1` (ETag `"v1"`), giúp toàn bộ các luồng CAS kích hoạt theme đầu tiên bằng `If-Match: "v1"` trong hệ thống và bộ test hoạt động liền mạch.
3. **Vị trí gọi `ensure_default_starter_theme` (Startup-Only Invariant):**
   - Hàm `ensure_default_starter_theme` **chỉ được gọi trong giai đoạn khởi động độc quyền** dưới khóa `SiteCoordinator` (trước `TcpListener::bind`) hoặc trong luồng bảo trì/test đơn luồng — tuyệt đối không gọi từ HTTP request handler đồng thời để bảo toàn an toàn CAS `version = 1`.
   - Trong [`cms/crates/opendash-server/src/main.rs`](../../cms/crates/opendash-server/src/main.rs#L72-L86): gọi `storage::theme::ensure_default_starter_theme(&db, &paths.themes_dir()).await?` ngay sau `storage::init_db(paths.db_path()).await?` (trước `load_content_type_catalog(&db)`). Việc chạy cả khi `recovery_safe_mode` đảm bảo thư mục `themes/revisions/<starter_digest>/` trên site vừa khôi phục luôn đầy đủ file trên đĩa và luôn có 1 theme active.
   - Các bài unit/integration test gọi `storage::init_db` trực tiếp (không khởi động qua `main.rs`) và kiểm tra `backup_worker.rs` nếu có `opendash-starter` trong DB phải gọi `ensure_default_starter_theme(&db, &paths.themes_dir()).await?` trước khi chụp snapshot.

#### 2.3. Thiết kế Chi tiết Gói `opendash-starter` (`cms/crates/opendash-server/src/builtin_themes/opendash-starter/`)

1. **`theme.json` (`ThemeV3Manifest`):**
   - `manifest_version: 3`, `theme_id: "opendash-starter"`, `name: "OpenDash Starter"`, `version: "1.0.0"`.
   - `parts`:
     - `"header": "header.html"`
     - `"footer": "footer.html"`
     - `"post-card": "template-part/post-card.html"`
   - `templates` (9 canonical targets):
     - `{ "kind": "home", "file": "home.html" }`
     - `{ "kind": "page", "file": "page.html" }`
     - `{ "kind": "single", "file": "single.html" }`
     - `{ "kind": "single", "content_type": "post", "file": "single-post.html" }`
     - `{ "kind": "archive", "file": "archive.html" }`
     - `{ "kind": "archive", "content_type": "post", "file": "archive-post.html" }`
     - `{ "kind": "taxonomy", "file": "taxonomy.html" }`
     - `{ "kind": "not_found", "file": "404.html" }`
     - `{ "kind": "index", "file": "index.html" }`
   - `page_layouts` (3 layouts):
     - `{ "key": "default", "label": "Mặc định (Khung chuẩn)", "file": "page-layouts/default.html" }`
     - `{ "key": "full_width", "label": "Tràn viền (Full Width Builder)", "file": "page-layouts/full-width.html" }`
     - `{ "key": "canvas", "label": "Landing Page (Ẩn Header & Footer)", "file": "page-layouts/canvas.html" }`
   - `patterns` (6 starter patterns — lưu ý `key` dùng `snake_case`, `category` dùng `kebab-case` theo đúng `ThemeV3Manifest::validate` tại [`opendash-core/src/theme.rs#L421-L423`](../../cms/crates/opendash-core/src/theme.rs#L421-L423)):
     - `{ "key": "hero_launchpad", "title": "Hero SaaS & Thanh chỉ số", "category": "hero", "description": "Khối mở đầu nổi bật kèm nút CTA và 3 chỉ số thống kê.", "file": "patterns/hero-launchpad.json" }`
     - `{ "key": "features_bento_grid", "title": "Lưới 3 tính năng cốt lõi", "category": "features", "description": "Bố cục 3 cột trình bày điểm mạnh sản phẩm/dịch vụ.", "file": "patterns/features-bento-grid.json" }`
     - `{ "key": "about_story_split", "title": "Câu chuyện thương hiệu 2 cột", "category": "content", "description": "Giới thiệu tầm nhìn kết hợp khung trích dẫn khách hàng.", "file": "patterns/about-story-split.json" }`
     - `{ "key": "pricing_tiers", "title": "Bảng giá 3 gói dịch vụ", "category": "pricing", "description": "So sánh 3 gói Starter, Pro và Enterprise.", "file": "patterns/pricing-tiers.json" }`
     - `{ "key": "faq_accordion", "title": "Câu hỏi thường gặp (FAQ)", "category": "faq", "description": "Danh sách câu hỏi thu gọn không cần JavaScript.", "file": "patterns/faq-accordion.json" }`
     - `{ "key": "cta_newsletter_banner", "title": "Banner kêu gọi hành động", "category": "cta", "description": "Khối CTA cuối trang thúc đẩy chuyển đổi.", "file": "patterns/cta-newsletter-banner.json" }`
   - `assets`: `["assets/style.css"]`
   - `menus`: `["primary", "footer"]`
   - `options` (5 tùy chọn public tương thích `parse_theme_options_schema` tại [`storage/settings.rs`](../../cms/crates/opendash-server/src/storage/settings.rs)):
     - `header_badge` (`string`, default: `"OPENDASH STARTER"`, `public: true`)
     - `header_cta_label` (`string`, default: `"Khám phá ngay"`, `public: true`)
     - `header_cta_url` (`string`, default: `"/gioi-thieu"`, `public: true`)
     - `footer_copyright` (`string`, default: `"Xây dựng trên nền tảng OpenDash CMS — Single-Binary Rust + SQLite & Tailwind CSS v4."`, `public: true`)
     - `show_tagline` (`boolean`, default: `true`, `public: true`)
   - `sample` (`ThemeSampleManifest` chuẩn tại [`storage/import.rs#L54-L131`](../../cms/crates/opendash-server/src/storage/import.rs#L54-L131)):
     - `sample_key`: `"starter_default"`
     - `pages` (5 mục — dùng Comment Block `<!-- od:... -->` hoặc RichText trong `values.body`):
       - `home_page` (`slug: "trang-chu"`, `title: "Trang chủ OpenDash"`, `content_type: "page"`, `values.body` chứa Comment Block AST ghép `core/banner` + `opendash-starter/stats-bar` + 3 `core/card` + `opendash-starter/quote-box` + `core/accordion`)
       - `about_page` (`slug: "gioi-thieu"`, `title: "Giới thiệu OpenDash"`, `content_type: "page"`, `values.body` chứa Comment Block AST giới thiệu kiến trúc và đội ngũ)
       - `contact_page` (`slug: "lien-he"`, `title: "Liên hệ & Hỗ trợ"`, `content_type: "page"`, `values.body` chứa Comment Block AST thông tin liên hệ & FAQ)
       - `post_rust_sqlite` (`slug: "kien-truc-single-binary-rust-sqlite"`, `title: "Kiến trúc Single-Binary Rust + SQLite trong OpenDash"`, `content_type: "post"`)
       - `post_tailwind_v3` (`slug: "thiet-ke-giao-dien-voi-tailwind-css-va-theme-v3"`, `title: "Thiết kế giao diện nhanh với Tailwind CSS v4 & Theme v3"`, `content_type: "post"`)
     - `menus` (2 mục):
       - `primary_menu` (`menu_key: "primary"`, `slot_key: "primary"`, `title: "Menu chính"`, items trỏ tới `home_page`, `about_page`, external `/posts`, `contact_page`)
       - `footer_menu` (`menu_key: "footer"`, `slot_key: "footer"`, `title: "Menu chân trang"`, items trỏ tới `home_page`, `about_page`, `contact_page`)
     - `settings` (2 mục):
       - `opt_badge` (`option_key: "header_badge"`, `value: "OPENDASH CMS V1"`)
       - `opt_cta_label` (`option_key: "header_cta_label"`, `value: "Tìm hiểu thêm"`)
2. **2 Theme Blocks trong `opendash-starter/blocks/`:**
   - `blocks/stats-bar/{block.json,render.html}` (`id: "opendash-starter/stats-bar"`, `category: "content"`): hiển thị 3 cặp chỉ số thống kê (`stat_1_value`, `stat_1_label`, `stat_2_value`, `stat_2_label`, `stat_3_value`, `stat_3_label`) trong khung `grid grid-cols-1 gap-6 rounded-2xl border border-od-border bg-od-surface p-6 shadow-xs sm:grid-cols-3`.
   - `blocks/quote-box/{block.json,render.html}` (`id: "opendash-starter/quote-box"`, `category: "content"`): hiển thị trích dẫn nổi bật (`quote`, `author`, `role`) với viền nhấn `border-l-4 border-od-primary bg-od-surface-alt p-6 rounded-r-2xl`.
3. **Quy ước C-SEC bắt buộc cho các Template của `opendash-starter`:**
   - Trong Theme Format v3 ([`template_sec.rs#L878-L907`](../../cms/crates/opendash-server/src/template_sec.rs#L878-L907)), chỉ có 2 lời gọi composition hợp lệ là `{{ od_content() }}` và `{{ od_part("...") }}` (header và footer của trang đã được hệ thống tự động render và ghép qua `resolve_and_render_header_footer` + `compose_document_with_seo`, nên các file `home.html`, `page.html`, `single.html`, `archive.html` **không** gọi `od_header()`/`od_footer()` mà chỉ render phần `<main>...</main>`, còn `archive.html` có thể gọi `{{ od_part("post-card") }}` bên ngoài vòng lặp hoặc render card trực tiếp trong `{% for entry in entries %}` vì C-SEC cấm `od_part` nằm trong vòng lặp `for` tại [`template_sec.rs#L807-L809`](../../cms/crates/opendash-server/src/template_sec.rs#L807-L809)).
   - Mọi template entry (`home.html`, `page.html`, `single.html`, `single-post.html`, `page-layouts/*.html`) chứa đúng **1** lời gọi `{{ od_content() }}` ở ngữ cảnh `HtmlScanState::Body` (ngoài vòng lặp `for` và `if`).
   - Template `404.html` hiển thị tiêu đề `<h1>404 — Không tìm thấy trang</h1>` kèm mô tả và nút quay lại Trang chủ `/` để vừa thân thiện vừa khớp 100% với mọi assertion kiểm thử 404 hiện có.

---

## Alternatives Considered

1. **Dùng Tailwind Play CDN (`<script src="https://cdn.tailwindcss.com"></script>`) trên trang công khai:**
   - *Bị bác bỏ:* Vi phạm trực tiếp chính sách `Content-Security-Policy: script-src 'none'` của trang công khai OpenDash, phụ thuộc kết nối Internet bên ngoài và gây hiện tượng FOUC.
2. **Nhúng trình biên dịch CSS động vào tiến trình Rust khi cài theme hoặc lưu bài viết:**
   - *Bị bác bỏ:* Phá vỡ ràng buộc Single-Executable (nếu gọi Node CLI) hoặc làm phình kích thước binary và mở bề mặt DoS trên luồng ghi. Kiến trúc 2 tầng tĩnh (`/assets/opendash-tailwind.css` + `/theme/<digest>/assets/style.css`) đáp ứng trọn vẹn nhu cầu mà không tốn chi phí biên dịch lúc chạy.
3. **Để `opendash-starter` ở thư mục `cms/fixtures/` và giữ `theme_active_selection.revision_id = NULL` khi mới cài CMS:**
   - *Bị bác bỏ:* Trái với yêu cầu của Chủ dự án (*"theme starter luôn là mặc định của CMS; CMS luôn yêu cầu có 1 theme đang active, và mặc định khi cài CMS thì đã có theme này sẵn đang active rồi"*).

---

## Non-Functional Requirements

- **Bảo mật & CSP:** Giữ nguyên tuyệt đối CSP của trang công khai (`script-src 'none'`) và Studio Preview iframe; `/assets/opendash-tailwind.css` phục vụ cùng origin (`'self'`) với `X-Content-Type-Options: nosniff`.
- **Hiệu năng & Kích thước:** File `opendash-tailwind.css` đã minify có dung lượng `< 120 KiB` (nén gzip `< 22 KiB`); `ensure_default_starter_theme` chạy idempotent trong `< 5 ms` ở các lần khởi động tiếp theo.
- **Bảo toàn dữ liệu & Tự phục hồi (`A-DURABILITY`):** `ensure_default_starter_theme` không bao giờ ghi đè theme active mà người dùng đã chọn, không biến đổi `home_page` hay `content_entries`, và tự động khôi phục thư mục `themes/revisions/<starter_digest>/` trên đĩa nếu bị xóa nhầm.
- **Ranh giới Kiến trúc (`CORE_FOUNDATION.md` & `check-standards.py`):** 0 crate Rust mới trong `cms/Cargo.toml`, 0 npm package mới trong `cms/admin/package.json`; toàn bộ truy vấn SQLite đặt trong `cms/crates/opendash-server/src/storage/theme.rs`.

---

## Detailed Implementation Plan

### Target Files to Create/Modify

| Phạm vi / Slice | File / Thư mục mục tiêu | Vai trò chính |
| :--- | :--- | :--- |
| **Slice S01 (Public Tailwind v4 Bundle & Core Blocks Upgrade)** | `cms/admin/package.json`, `cms/admin/scripts/build-public-tailwind.mjs*`, `cms/admin/src/{tailwind-public.css*,tailwind-public-safelist.txt*}`, `cms/admin/vite.config.ts`, `cms/admin/src/builder/BuilderPreviewFrame.tsx`, `cms/crates/opendash-server/src/builtin_themes/{mod.rs*,opendash-tailwind.css*}`, `cms/crates/opendash-server/src/{main.rs,content_render.rs,routes/builder.rs}`, `cms/crates/opendash-server/src/builtin_blocks/{banner,card,accordion,divider,spacer}/render.html` | Biên dịch và nhúng `/assets/opendash-tailwind.css`, tự động gắn `<link>` vào `compose_document_with_seo`, rewrite trong `BuilderPreviewFrame.tsx`, gỡ `<style>body{max-width:850px}</style>` khỏi `DEFAULT_SINGLE`/`DEFAULT_ARCHIVE`, nâng cấp 5 Core Blocks sang Tailwind v4. |
| **Slice S02 (`opendash-starter` Default Active Theme v3, 6 Patterns, 2 Blocks & Sample Content)** | `cms/crates/opendash-server/src/builtin_themes/mod.rs`, `cms/crates/opendash-server/src/builtin_themes/opendash-starter/**`, `cms/crates/opendash-server/src/{main.rs,storage/theme.rs}`, `cms/fixtures/demo-website/theme-moc-studio/{header,footer,home,page,single,archive}.html`, `cms/crates/opendash-server/tests/{m5_s11_t01_revision_discovery.rs,m5_s01_t02_zip_v2.rs,starter_theme_v3_test.rs*}`, `docs/contracts/M8_WRITER_INVENTORY.md` | Nhúng gói `opendash-starter@1.0.0`, tự động cài & kích hoạt mặc định qua `ensure_default_starter_theme` (self-healing đĩa, giữ `version = 1`), 6 Starter Patterns, 2 Theme Blocks, 3 Page Layouts, dữ liệu mẫu `sample`, đồng bộ `theme-moc-studio` và bộ kiểm thử E2E. |

---

### Slice S01: Built-in Public Tailwind v4 Bundle (`/assets/opendash-tailwind.css`), Studio Preview Rewrite & Core Built-in Blocks Upgrade

- **Dependencies:** Đặc tả đạt `Status: Approved` và đã merge vào `main`.
- **File Ownership:**
  - `cms/admin/package.json` — thêm script `"build:public-tailwind": "node ./scripts/build-public-tailwind.mjs"` (không thêm dependency mới)
  - `cms/admin/scripts/build-public-tailwind.mjs` *(tạo mới)* — dùng `compile` của `tailwindcss@4.1.17`, `optimize` của `@tailwindcss/node` và `Scanner` của `@tailwindcss/oxide` để quét `builtin_blocks`, `builtin_themes`, `fixtures/demo-website`, `content_render.rs`, `page.rs`, `routes/public.rs`, `routes/public_content.rs`, `tailwind-public-safelist.txt` và ghi file minify ra `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css`
  - `cms/admin/src/tailwind-public.css` *(tạo mới)* — khai báo `@import "tailwindcss";`, `@theme` cho 6 Semantic Theme Tokens (`bg-od-primary`, `text-od-primary`, `border-od-primary`, `bg-od-surface`, `bg-od-surface-alt`, `text-od-text`, `text-od-muted`, `border-od-border`, `font-od-heading`, `font-od-body`), biến fallback trên `:root` và lớp typography `.od-prose`
  - `cms/admin/src/tailwind-public-safelist.txt` *(tạo mới)* — danh sách các utility class Tailwind v4 chuẩn cho CMS public frontend & theme v3
  - `cms/admin/vite.config.ts` — cập nhật regex proxy tại `L25` để chuyển tiếp `/assets/opendash-tailwind.css` về backend Rust (`http://127.0.0.1:18766`)
  - `cms/admin/src/builder/BuilderPreviewFrame.tsx` — cập nhật `rewriteRootRelativeUrl` (`L88-L94`) cho phép rewrite `raw === '/assets/opendash-tailwind.css'` thành `${origin}${raw}`
  - `cms/crates/opendash-server/src/builtin_themes/mod.rs` *(tạo mới)* & `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css` *(tạo mới)* — nhúng `pub const PUBLIC_TAILWIND_CSS: &[u8] = include_bytes!("opendash-tailwind.css");`
  - `cms/crates/opendash-server/src/main.rs` — khai báo `mod builtin_themes;`
  - `cms/crates/opendash-server/src/routes/builder.rs` — cập nhật `serve_asset` (`L52-L74`) trả về `crate::builtin_themes::PUBLIC_TAILWIND_CSS` khi `clean_path == "opendash-tailwind.css"` với `Content-Type: text/css; charset=utf-8`, `X-Content-Type-Options: nosniff`, `Cache-Control: public, max-age=3600`
  - `cms/crates/opendash-server/src/content_render.rs` — tự động chèn `<link rel="stylesheet" href="/assets/opendash-tailwind.css">` một lần duy nhất trong `compose_document_with_seo` (`L349`, khử trùng lặp nếu `styles` đã chứa `/assets/opendash-tailwind.css`); bỏ thẻ `<style>body{max-width:850px...}</style>` toàn cục khỏi `DEFAULT_ARCHIVE` và `DEFAULT_SINGLE` (`L17-L58`), thay bằng class Tailwind cục bộ và `.od-prose`
  - `cms/crates/opendash-server/src/builtin_blocks/{banner,card,accordion,divider,spacer}/render.html` — bổ sung class Tailwind v4 hiện đại trong khi giữ nguyên 100% class `od-block-*` và biến `--od-*`
- **Acceptance Criteria:**
  1. `npm --prefix cms/admin run build:public-tailwind` chạy thành công không cần mạng, sinh ra `cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css` (`< 120 KiB`).
  2. `GET /assets/opendash-tailwind.css` trả về `200 OK` với `content-type: text/css; charset=utf-8`, chứa đầy đủ utility Tailwind v4, `.bg-od-primary`, `.text-od-text`, `.border-od-border` và `.od-prose`.
  3. Mọi trang sinh qua `compose_document_with_seo` đều có đúng 1 thẻ `<link rel="stylesheet" href="/assets/opendash-tailwind.css">` trong `<head>`, và không còn thẻ `<style>body{...max-width:850px...}</style>` bị đẩy lên `<head>` khi render entry Classic Editor.
  4. `BuilderPreviewFrame.tsx` rewrite `/assets/opendash-tailwind.css` sang `${origin}/assets/opendash-tailwind.css` cho iframe preview.
  5. Toàn bộ 5 Core Built-in Blocks render giao diện Tailwind v4 và toàn bộ test workspace (`cargo test --manifest-path cms/Cargo.toml --workspace --release --locked`) PASS 100%.
- **Rollback:** Revert các file thuộc Slice S01.

---

### Slice S02: Built-in Default Theme v3 (`opendash-starter`), Startup Auto-Activation (`ensure_default_starter_theme`), 6 Patterns, 2 Theme Blocks & Sample Content

- **Dependencies:** `Slice S01` đã merge vào `main`.
- **File Ownership:**
  - `cms/crates/opendash-server/src/builtin_themes/mod.rs` — bổ sung `STARTER_THEME_ID`, `STARTER_THEME_NAME`, `STARTER_THEME_VERSION` và `pub fn builtin_starter_theme_files() -> BTreeMap<String, Vec<u8>>`
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/theme.json` *(tạo mới)* — manifest v3 đầy đủ 9 templates, 3 parts, 3 page layouts, 6 patterns, 5 options, `assets: ["assets/style.css"]`, `menus: ["primary", "footer"]` và `sample` (`ThemeSampleManifest`)
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/{header.html,footer.html,home.html,page.html,single.html,single-post.html,archive.html,archive-post.html,taxonomy.html,404.html,index.html}` *(tạo mới)* — bộ template Tailwind v4 tuân thủ C-SEC (chỉ dùng `{{ od_content() }}` và `{{ od_part("...") }}` ngoài vòng lặp `for`, đầy đủ `site.title`, `site.tagline`, `entry.title`, `entry.fields`, `entries`, `archive_url`, `previous_url`, `next_url`, `menus.primary`, `menus.footer`, và `404 — Không tìm thấy trang`)
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/template-part/post-card.html` *(tạo mới)*
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/page-layouts/{default.html,full-width.html,canvas.html}` *(tạo mới)*
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/stats-bar/{block.json,render.html}` *(tạo mới)*
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/blocks/quote-box/{block.json,render.html}` *(tạo mới)*
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/patterns/{hero-launchpad.json,features-bento-grid.json,about-story-split.json,pricing-tiers.json,faq-accordion.json,cta-newsletter-banner.json}` *(tạo mới)*
  - `cms/crates/opendash-server/src/builtin_themes/opendash-starter/assets/style.css` *(tạo mới)*
  - `cms/crates/opendash-server/src/storage/theme.rs` — thêm `pub async fn ensure_default_starter_theme(db: &SqlitePool, themes_root: &Path) -> ApiResult<ThemeRevisionRecord>` (kiểm tra & tự phục hồi thư mục revision trên đĩa nếu thiếu/hỏng, commit revision idempotent, và tự động kích hoạt khi `selection.revision_id.is_none()` với `version = 1` ở lần kích hoạt mặc định đầu tiên)
  - `cms/crates/opendash-server/src/main.rs` — gọi `storage::theme::ensure_default_starter_theme(&db, &paths.themes_dir()).await?` khi khởi động server
  - `cms/fixtures/demo-website/theme-moc-studio/{header.html,footer.html,home.html,page.html,single.html,archive.html}` — cập nhật các liên kết `/content/...` cũ sang WordPress-Style Permalinks (`/posts`, `/gioi-thieu`, `/lien-he`) và bổ sung class Tailwind v4
  - `cms/crates/opendash-server/tests/{m5_s11_t01_revision_discovery.rs,m5_s01_t02_zip_v2.rs}` — cập nhật assertion số lượng revision v3 khởi tạo để phản ánh `opendash-starter` luôn cài sẵn và active mặc định ở `version = 1`
  - `cms/crates/opendash-server/tests/starter_theme_v3_test.rs` *(tạo mới)* — kiểm thử toàn diện: fresh boot có sẵn `opendash-starter` active ở `version = 1`, self-healing khi xóa thư mục revision trên đĩa, 6 patterns trong `/api/admin/patterns/library`, 2 theme blocks trong `/api/admin/builder/blocks`, 3 page layouts trong `/api/admin/themes/active/layouts`, sample import dry-run + execute (`/api/themes/opendash-starter/import/dry-run` & `execute`), và render trang công khai với `/assets/opendash-tailwind.css`
  - `docs/contracts/M8_WRITER_INVENTORY.md` — cập nhật dòng `Startup Recovery` ghi nhận `ensure_default_starter_theme`
- **Acceptance Criteria:**
  1. Khởi động binary `opendash` trên site mới có ngay `opendash-starter@1.0.0` đã cài và active (`selection.theme_id == Some("opendash-starter")`, `selection.version == 1`, `ETag: "v1"`).
  2. Xóa thư mục `themes/revisions/<starter_digest>/` trên đĩa rồi khởi động lại server: `ensure_default_starter_theme` tự động tái tạo thư mục và file `.complete` trên đĩa.
  3. Khi Admin kích hoạt một Theme v3 khác rồi khởi động lại server, theme của Admin vẫn được giữ nguyên (không bị ghi đè về `opendash-starter`).
  4. `GET /api/admin/patterns/library` trả về đủ 6 Starter Patterns; `GET /api/admin/builder/blocks` trả về đủ 5 Core Blocks + 2 Theme Blocks (`opendash-starter/stats-bar`, `opendash-starter/quote-box`); `GET /api/admin/themes/active/layouts` trả về đủ 3 Page Layouts (`default`, `full_width`, `canvas`).
  5. `POST /api/themes/opendash-starter/import/dry-run` và `POST /api/themes/opendash-starter/import/execute` nhập thành công 5 trang/bài viết mẫu, 2 menu (`primary`, `footer`) và 2 theme options.
  6. Toàn bộ test suites trong workspace (`cargo test --manifest-path cms/Cargo.toml --workspace --release --locked`) PASS 100%.
- **Rollback:** Revert các file thuộc Slice S02.

---

### Verification Plan

Sau mỗi Slice (và trước khi đóng Spec sang `Verified`), chạy đầy đủ bộ lệnh kiểm chứng từ thư mục gốc repository:

```powershell
npm --prefix cms/admin run build:public-tailwind
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
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

Kèm kiểm chứng trình duyệt trực tiếp (`Live Browser E2E`) trên server cục bộ (`http://127.0.0.1:18766`):
1. Khởi động server trên site sạch → kiểm tra `GET /` và `GET /assets/opendash-tailwind.css` (`200 OK`), xác nhận `opendash-starter` đang active mặc định trong `#/themes` và `#/studio`.
2. Trong `#/themes`, chạy **Xem trước (Dry-run)** và **Thực thi nhập mẫu** cho `opendash-starter`, xuất bản trang chủ mẫu và gán trong Reading Settings.
3. Mở `#/studio`, kiểm tra 6 Starter Patterns, 2 Theme Blocks (`opendash-starter/stats-bar`, `opendash-starter/quote-box`), 3 Page Layouts (`default`, `full_width`, `canvas`) và Live Preview với Tailwind CSS v4 (`0` console errors, `0` lỗi CSP).
