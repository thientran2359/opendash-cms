# Biên bản Kiểm chứng — Trusted Theme HTML (Unrestricted Inline SVG & JS) & WordPress-Style Asset Enqueue System

- **Design Spec:** [`docs/designs/trusted-theme-js-svg-and-asset-enqueue.md`](../designs/trusted-theme-js-svg-and-asset-enqueue.md)
- **Drift Report:** [`.agents/research/2026-11-25-trusted-theme-js-svg-enqueue-drift-report.md`](../../.agents/research/2026-11-25-trusted-theme-js-svg-enqueue-drift-report.md) (`0 Blocker / 0 Major / 3 Minor / 3 Nit`; `F-01`, `F-03`, `N-01`, `N-02` đã đồng bộ ngay trong PR closure)
- **Date:** 2026-11-25
- **Platform:** macOS (Intel) + GitHub Actions (`cms-ci`, `governance-ubuntu-latest`, `governance-windows-latest`)
- **Merged Implementation Commits:**
  - Slice S01 ([#571](https://github.com/thientran2359/opendash-cms/issues/571) / PR [#573](https://github.com/thientran2359/opendash-cms/pull/573)): `3a1b0577cf581a117f686016ab3a3d8b427904ed`
  - Slice S02 ([#572](https://github.com/thientran2359/opendash-cms/issues/572) / PR [#574](https://github.com/thientran2359/opendash-cms/pull/574)): `00796a6` (head `018261495dd5c5faec3c7c5aa7458a9415ac34be`, 3/3 CI checks PASS)

---

## 1. Kết quả kiểm chứng tự động (Thực chạy trên mã thật)

| Hạng mục / Lệnh kiểm chứng | Kết quả | Ghi chú |
|---|---|---|
| `python3 cms/scripts/check-standards.py` & `--self-test` | **PASS** | `registrations=91 resolved_handlers=91`, core/storage/route/Admin boundaries sạch |
| `python3 cms/scripts/check-plans.py` & `--self-test` | **PASS** | 13 fixtures + SITEMAP & Verified evidence hợp lệ |
| `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** | Không có artifact/secret cấm |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** | Định dạng Rust chuẩn |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-core --release --locked theme::tests` | **PASS** | `15/15` (enqueue defaults, DAG cycle/handle/media validation, topo-sort, cross-location head promotion, strategy execution-order preservation, raster-only `preview_asset`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --release --locked --test trusted_theme_js_svg_enqueue_test` | **PASS** | `2/2` (`cli_accepts_trusted_svg_scripts_and_rejects_interpolations_or_cycles`, `http_renders_trusted_theme_with_conditional_enqueues_and_untrusted_body_safety`) |
| GitHub Actions `cms-ci` (PR [#574](https://github.com/thientran2359/opendash-cms/pull/574), full `--workspace --release --locked`) | **PASS** | Toàn bộ workspace Rust tests PASS 100% (bao gồm escaped-script/CDATA hardening và C-SEC fixtures) |
| `git diff --check` | **PASS** | Không có lỗi whitespace |

## 2. Tóm tắt nghiệm thu CUJ

1. **CUJ 1 (WordPress-Style Enqueue Styles/Scripts & `.js`/`.mjs`/`.svg` Assets):** Đã xác nhận qua `opendash_core::theme::tests` (`15/15`) và `trusted_theme_js_svg_enqueue_test` (`2/2`) — topo-sort theo `deps`, auto-promote dep của `head` script, dedup stylesheet URL, lọc theo `EnqueueCondition` (`/` nạp `home.css`, `/trusted-page` bỏ `home.css`), `--check-theme` từ chối chu trình DAG, `GET /theme/<digest>/assets/*.js` trả `text/javascript; charset=utf-8`, và `GET /theme/<digest>/assets/*.svg` trả `image/svg+xml` kèm header `Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; sandbox`.
2. **CUJ 2 (Trusted Inline `<svg>`, Inline `<script>`, `on*` tĩnh & Ranh giới Bảo mật Untrusted):** Đã xác nhận qua `trusted_theme_js_svg_enqueue_test` — Theme v3 & Declarative Block render đầy đủ inline `<svg xmlns=...><title>...</title>...</svg>`, `onclick="..."`, inline `<script>` chứa toán tử `<`/`>`, và `{{ od_icon(...) }}` cùng tồn tại với `<script>`; public HTTP response trả `Content-Security-Policy` có `script-src 'self' 'unsafe-inline'`; nội dung untrusted trong `values.body` vẫn bị sanitize/escape sạch; và `--check-theme` từ chối mọi `{{ ... }}` trong `<script>`/`onclick`, `<script/>` tự đóng, và HTML5 escaped/CDATA script smuggling.
