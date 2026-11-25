# EGM Drift Report — Theme v3 PageLayout Chrome Contract, C-SEC Composition Cache & Safe Inline `od_icon()`

- **Design Spec:** [`docs/designs/theme-chrome-cache-and-safe-icons.md`](../../docs/designs/theme-chrome-cache-and-safe-icons.md)
- **Date:** 2026-11-25
- **Audited Baseline:** `main@3d87d3db5459eaa624c61d34ca75909a2b2c8ab1` (sau khi merge S01 [#562](https://github.com/thientran2359/opendash-cms/issues/562) / PR [#565](https://github.com/thientran2359/opendash-cms/pull/565) `6072058` và S02 [#563](https://github.com/thientran2359/opendash-cms/issues/563) / PR [#568](https://github.com/thientran2359/opendash-cms/pull/568) `3d87d3d`)
- **Auditor:** Independent Goldfish `egm-codebase-details` + Closure Hardening

---

## 1. Kết luận tổng quan

- **Trước hardening (trên `main@3d87d3d`):** `0 Blocker / 1 Major / 1 Minor / 1 Nit`
- **Sau hardening (trong PR closure):** **`0 Blocker / 0 Major / 0 Minor / 0 Nit`**

Toàn bộ logic production cho 3 hạng mục (A: `PageLayoutChrome`, B: `ThemeCompositionCache`, C: `opendash-core::icons` + `od_icon()` + `IconNonceTable` + 9 call sites `finalize_document_icons`) đều khớp 100% với đặc tả.

---

## 2. Bảng đối chiếu Spec vs Mã nguồn thực tế

| Hạng mục / Yêu cầu trong Spec | Trạng thái | Mã nguồn thực tế (`file:line`) |
|---|---|---|
| **A.1** `PageLayoutChrome { Default, None }` & `PageLayoutDecl.chrome` (`#[serde(default)]`) | Khớp 100% | `cms/crates/opendash-core/src/theme.rs:328-344`, unit test `:624-639` |
| **A.2** 3 bản `opendash-starter*/theme.json` đặt `"chrome": "none"` cho `canvas`, giữ mặc định cho `default` & `full_width` | Khớp 100% | `opendash-starter/theme.json:63-67`, `opendash-starter-1.0.1/theme.json:63-67`, `opendash-starter-1.1.0/theme.json:65-69` |
| **A.3** Helper `is_canvas_page_layout(active, layout_key)` + fallback `layout_key == "canvas"` | Khớp 100% | `cms/crates/opendash-server/src/storage/overrides.rs:173-200` |
| **A.4** Đủ 9 điểm kiểm tra bọc chrome + reset `canvas_layout = false` ở `CoreFallback` | Khớp 100% | `routes/public_content.rs:216-218, 284-287` (2); `storage/overrides.rs:2632, 2660` (2), `:3624, 3677, 3716` + reset `:3735` (3), `:3916, 3950` (2) |
| **B.1** `ThemeCompositionCache::global()` (`OnceLock`, cap 256, kiểm 64-hex `revision_digest`) & `CompiledCsecComposition.max_part_depth` | Khớp 100% | `cms/crates/opendash-server/src/template_sec.rs:94-182, 1828`, unit test `:6096-6197` |
| **B.2** `PrecompiledPublicShell` & `PublicSurfaceInspection` dùng `Option<Arc<CompiledCsecComposition>>` + `.as_deref()` | Khớp 100% | `cms/crates/opendash-server/src/storage/overrides.rs:2491-2502, 2803` |
| **B.3** Bọc đủ 5 điểm compile Active Theme v3 + thay `check_theme_part_depth` bằng `depth + compiled.max_part_depth <= 3` | Khớp 100% | `storage/overrides.rs:2285-2297, 2561-2573, 2638-2645, 3346-3353`; `routes/public_content.rs:258-265` |
| **C.1** `opendash_core::icons`: 48 icon tĩnh, `lookup_core_icon_paths`, `valid_icon_class_attr`, `render_core_icon_svg` + export `pub mod icons;` | Khớp 100% | `cms/crates/opendash-core/src/icons.rs:4-247`, `cms/crates/opendash-core/src/lib.rs:10` |
| **C.2** Cấm `ICON_NONCE_PREFIX` (`"OD_ICON_"`) tại 4 entry points & chống shadowing `"od_icon"` | Khớp 100% | `template_sec.rs:18-19, 608, 1621, 1879, 5574`; `RESERVED_CONTEXT_VARS` `:2552`; `is_sensitive_name` `:5024` |
| **C.3** AST rules cho `od_icon()` & không mở cho legacy `validate_template_csec()` / `render_csec()` | **Đã vá (Minor)** | Truyền `allow_od_icon: bool` vào `validate_template_csec_with_usage_flags` (`false` ở `:1610`, `true` ở `:1757`) |
| **C.4** `IconNonceTable` trên `RenderBudget` & `PreviewCompositionBudget` (clone/replace sync), `render_with_config_and_budget`, transactional `local`, cap 128, pre-charge net delta | Khớp 100% | `template_sec.rs:239-324, 337-555, 658-740, 5948-6089`; `storage/overrides.rs:1330-1350, 2204-2229` |
| **C.5** `content_render::finalize_document_icons()` & single-pass `HtmlScanState::Body` + `MAX_PUBLIC_DOCUMENT_BYTES` | **Đã vá (Nit)** | `content_render.rs:1044-1054`; thay `4 * 1024 * 1024` bằng `crate::document::MAX_PUBLIC_DOCUMENT_BYTES` tại `template_sec.rs:403, 408` |
| **C.6** Đủ 9 call sites `finalize_document_icons()` | Khớp 100% | `routes/public_content.rs:405, 652, 950, 1278` (4); `routes/entries.rs:936` + `storage/builder.rs:1814-1833` (1); `storage/overrides.rs:2048, 3796, 4006` (3); `storage/builder.rs:1890` (1) |
| **E2E Suite** `tests/theme_chrome_cache_icons_test.rs` bao phủ CUJ 1, CUJ 3, `od_icon()` qua `od_content()`, strict CSP và `--check-theme` CLI | **Đã vá (Major)** | Bổ sung `e2e_page_layout_chrome_and_inline_icons_with_strict_csp` (`3/3 PASS`) |
