# EGM Drift Report — Trusted Theme HTML (Inline SVG/JS) & WordPress-Style Asset Enqueue System

- **Design Spec:** [`docs/designs/trusted-theme-js-svg-and-asset-enqueue.md`](../../docs/designs/trusted-theme-js-svg-and-asset-enqueue.md)
- **Audited Commit:** `main@00796a6` (sau khi merge S01 PR #573 `3a1b057` và S02 PR #574 `00796a6`)
- **Auditor:** Independent Zero-Context Goldfish (`egm-codebase-details`)
- **Date:** 2026-11-25
- **Verdict:** **0 Blocker · 0 Major · 3 Minor · 3 Nit** (đã đồng bộ tài liệu cho `F-01`, `F-03`, `N-01`, `N-02` ngay trong PR closure)

---

## 1. Bảng đối chiếu theo phạm vi mã nguồn

| Phạm vi | Trạng thái | Tóm tắt kiểm chứng |
|---|---|---|
| `cms/crates/opendash-core/src/theme.rs` | **PASS** | `valid_v3_asset_path` (`.js`/`.mjs`/`.svg`), raster-only `pattern.preview_asset`, các kiểu Enqueue + `ThemeV3Manifest::validate()` (max 64, kebab handle, virtual `"opendash-tailwind"`, `media` safe chars, 3-color DFS DAG cycle check), `resolve_enqueued_assets()` (64-hex digest, topo-sort, transitive deps, cross-location head promotion). |
| `cms/crates/opendash-server/src/theme.rs` & `routes/themes.rs` | **PASS** | Cap 512 KiB (`V3_MAX_ACTIVE_ASSET_BYTES`), UTF-8 check không chạy `validate_safe_css` lên JS/SVG, MIME `text/javascript; charset=utf-8` / `image/svg+xml`, v3-only asset gate, và standalone SVG header `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox`. |
| `cms/crates/opendash-server/src/template_sec.rs` | **PASS** | `allow_trusted_theme_markup = true` xuyên suốt 6 điểm Theme/Preview/Block; đường legacy `validate_template_csec*` / `render_csec_with_config` giữ `false`; `HtmlScanState::Tag.prefix < 8`, `HtmlScanState::Script`, `ThemeHtmlState::Script`, chặn `<script/>`, `</script>` lẻ, `<!--`/`<![CDATA[` smuggling, Jinja emit/control-flow trong `<script>` và `{{`/`{%` trong `on*`; mở `<svg>`/`xmlns`/`xmlns:xlink`/`xlink:href`/`<title>` trong `<svg>`. |
| `cms/crates/opendash-server/src/document.rs` | **PASS** | `TokenKind::ScriptBlock`, `find_case_insensitive_end_script`, `SafeDocumentParts.authorized_head_scripts`, `normalize_public_document` mặc định `false`, `normalize_public_document_with_options` bảo toàn `<title>` trong `<svg>`, xử lý `foreignObject`, `script_outside_envelope`, `script_escaped_state`, và `on*`. |
| `content_render.rs`, `routes/public.rs`, `routes/public_content.rs`, `storage/overrides.rs` | **PASS** | `compose_document_with_seo_with_assets` đúng thứ tự `<head>`, dedup `head_style_urls` (entity-decoded), chèn `authorized_head_scripts` và `footer_scripts`; 3 điểm chuẩn hóa slot trung gian; đủ 6 ánh xạ `EnqueueCondition`; preview chỉ bơm CSS; cả 4 nhánh `rendered_layout_with_form_csp` dùng `script-src 'self' 'unsafe-inline'`; `/admin` CSP và `opendash-core::richtext` giữ nguyên 100%. |
| `tests/trusted_theme_js_svg_enqueue_test.rs` | **PASS** | Phủ trọn CUJ 1 & CUJ 2 (CLI `--check-theme`, enqueue ordering/condition/dedup, MIME/SVG sandbox CSP, inline `<svg>`/`<title>`/`onclick`/`<script>` chứa `<`, Declarative Block + `od_icon()`, public CSP, và sanitization/escaping của `values.body` untrusted). |

---

## 2. Chi tiết Findings & Xử lý

- **[Minor] F-01 (`opendash-core/src/theme.rs:737-767`) — Strategy coercion để bảo toàn thứ tự phụ thuộc:**
  - *Chi tiết:* `resolve_enqueued_assets()` chủ động hạ `async` → `defer` (hoặc bỏ `async` ở module) cho script có cạnh `deps`, và nâng `blocking` → `defer` khi phụ thuộc vào `module`/`defer`/script đã hạ từ `async` (đã có unit test `enqueue_preserves_execution_order_across_async_defer_and_blocking_dependencies`).
  - *Xử lý:* **Đã cập nhật** vào mục §A của `docs/designs/trusted-theme-js-svg-and-asset-enqueue.md`.
- **[Minor] F-02 (`routes/public_content.rs:416,674,982,1313`) — Guard `active.is_some()` trên site không kích hoạt Theme v3:**
  - *Chi tiết:* Route public truyền `active.is_some()` / `active_v3.is_some()` cho `allow_trusted_theme_markup` (defense-in-depth khi site không có Theme v3 active), trong khi preview truyền `true`. Không ảnh hưởng mọi site có Theme v3 active (bao gồm `opendash-starter` mặc định).
  - *Xử lý:* Ghi nhận như ranh giới defense-in-depth hiện hành (không chặn đóng Spec A).
- **[Minor] F-03 (`docs/contracts/M5_THEME_TEMPLATES.md`, `CORE_FOUNDATION.md`, `SITEMAP.md`) — Ghi chú supersession & đóng trạng thái:**
  - *Xử lý:* **Đã cập nhật** ghi chú `Superseded by docs/designs/trusted-theme-js-svg-and-asset-enqueue.md` trong `M5_THEME_TEMPLATES.md` và `CORE_FOUNDATION.md`, và chuyển `Status: Verified` trên Spec & `SITEMAP.md`.
- **[Nit] N-01 / N-02:** Đã đồng bộ thứ tự thuộc tính `<script src="..." type="module">` ở CUJ 1 và tên hằng số `V3_MAX_ACTIVE_ASSET_BYTES` trong Spec.
- **[Nit] N-03 (`template_sec.rs:331, 3948, 3997, 4049`):** Một doc comment cũ và chuỗi lỗi `<style>/<title>` vẫn dùng từ ngữ cũ (logic đã xử lý đủ `"script"`). Ghi nhận để dọn khi chạm `template_sec.rs` về sau.
