# Biên bản Kiểm chứng — Theme v3 PageLayout Chrome Contract, C-SEC Composition Cache & Safe Inline `od_icon()`

- **Design Spec:** [`docs/designs/theme-chrome-cache-and-safe-icons.md`](../designs/theme-chrome-cache-and-safe-icons.md)
- **Drift Report:** [`.agents/research/2026-11-25-theme-chrome-cache-icons-drift-report.md`](../../.agents/research/2026-11-25-theme-chrome-cache-icons-drift-report.md) (`0 Blocker / 0 Major / 0 Minor / 0 Nit` sau hardening)
- **Date:** 2026-11-25
- **Platform:** macOS (Intel)
- **Merged Implementation Commits:**
  - Slice S01 ([#562](https://github.com/thientran2359/opendash-cms/issues/562) / PR [#565](https://github.com/thientran2359/opendash-cms/pull/565)): `60720582d2e607137f1a577731c479aef58d78fe`
  - Slice S02 ([#563](https://github.com/thientran2359/opendash-cms/issues/563) / PR [#568](https://github.com/thientran2359/opendash-cms/pull/568)): `3d87d3db5459eaa624c61d34ca75909a2b2c8ab1`

---

## 1. Kết quả kiểm chứng tự động (Thực chạy trên mã thật)

| Hạng mục / Lệnh kiểm chứng | Kết quả | Ghi chú |
|---|---|---|
| `python3 cms/scripts/check-standards.py` & `--self-test` | **PASS** | `registrations=91 resolved_handlers=91`, core/storage/route/Admin boundaries sạch |
| `python3 cms/scripts/check-plans.py` & `--self-test` | **PASS** | 13 fixtures + SITEMAP & Verified evidence hợp lệ |
| `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** | Không có artifact/secret cấm |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** | Định dạng Rust chuẩn |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-core --release --locked icons` | **PASS** | `2/2` (`registry_contains_exactly_48_distinct_static_safe_icons`, `class_accepts_decimal_tailwind_and_rejects_markup`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --release --locked --test theme_chrome_cache_icons_test` | **PASS** | `3/3` (CLI `--check-theme` accept/reject, curated 48-icon static check, full HTTP E2E CUJ 1 + CUJ 3 + Declarative Block via `od_content()` + strict CSP) |
| `git diff --check` | **PASS** | Không có lỗi whitespace |

## 2. Tóm tắt nghiệm thu CUJ

1. **CUJ 1 (`PageLayoutChrome` trên `opendash-starter`):** Đã xác nhận qua live HTTP server trong `e2e_page_layout_chrome_and_inline_icons_with_strict_csp` — Page `default` và `full_width` giữ nguyên Starter Header (`OPENDASH STARTER`) và `<footer>`, trong khi Page `canvas` (`"chrome": "none"`) ẩn sạch cả Header và Footer.
2. **CUJ 2 (`ThemeCompositionCache`):** Đã xác nhận qua unit tests trong `template_sec.rs` (`theme_cache_reuses_immutable_compilation_and_regenerates_content_nonce`) — tái sử dụng cùng `Arc<CompiledCsecComposition>` cho cùng 64-hex `revision_digest`, miss sạch khi đổi `revision_digest` hoặc digest không hợp lệ, và kiểm tra `max_part_depth` trong `O(1)`.
3. **CUJ 3 (`od_icon()` + Strict CSP + `--check-theme`):** Đã xác nhận qua `cli_accepts_inline_icon_and_rejects_untrusted_icon_forms` (chặn tên icon sai, `class` injection, attribute, `Stmt::SetBlock`, `Expr::Filter`, `OD_ICON_` spoofing, raw `<svg>`) và `e2e_page_layout_chrome_and_inline_icons_with_strict_csp` (render `<svg ... aria-hidden="true">` thật trong `header.html`, `page.html`, và Declarative Block qua `od_content()`, không lộ `OD_ICON_`, giữ nguyên `Content-Security-Policy: ... img-src 'self' ... script-src 'none'`).
