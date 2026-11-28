# Đợt Kiểm chứng Tầng 2 (`--release`) Luồng OpenDash Studio & UI/UX trên `main@4bd2427` — 5 Tính năng Đã Merge (`#632`, `#634`–`#637`, 2026-11-28)

- **Commit mã nguồn được kiểm chứng (`main@4bd2427`):** `4bd2427e63d937c3ae33e5887feb135ac0e5a7e7` (môi trường sạch trên `main@4bd2427`; nhánh tài liệu `docs/tier2-verify-studio-batch` chỉ cập nhật biên bản & `SITEMAP.md`)
- **Phạm vi 5 Đặc tả được chốt `Verified`:**
  1. **Spec A1 — OpenDash Studio Unified Recursive AST V1 (`#632` `097f0d33f59d871a2dc48cce1d8ce825bc4b503c`):** [`docs/designs/studio-unified-recursive-ast.md`](../designs/studio-unified-recursive-ast.md)
  2. **Admin UI/UX Polish (`#634` `00013fbcf8c15a7a7e4e19bba86b24de186a0c71`):** [`docs/designs/admin-ui-ux-polish.md`](../designs/admin-ui-ux-polish.md)
  3. **Spec A2 — Clean-Slate Theme Format V1, `Home → Page/Index`, 19 Template Blocks & Loop Engine (`#635` `ff49a941698b22ed9a21f2ceaf03e6837698ae7c`):** [`docs/designs/studio-template-blocks-and-loop.md`](../designs/studio-template-blocks-and-loop.md)
  4. **Studio Sub-routes, Options IA & Live `custom_options` Preview (`#636` `a622ba1e680856dead51b484c105485884fd09d5`):** [`docs/designs/studio-subroutes-and-options-ia.md`](../designs/studio-subroutes-and-options-ia.md)
  5. **Spec A3 — Interactive Micro-JS Pack (`/assets/opendash-blocks.js`), 7 Core Blocks (`core/popup`, ...) & 5 Starter Marketing Blocks (`#637` `4bd2427e63d937c3ae33e5887feb135ac0e5a7e7`):** [`docs/designs/studio-interactive-and-starter-blocks.md`](../designs/studio-interactive-and-starter-blocks.md)
- **Environment:** `macOS-15.8.2-x86_64-i386-64bit-Mach-O`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)` (`CARGO_INCREMENTAL=0 RUSTC_WRAPPER="" TMPDIR=/private/tmp`), Cargo `--locked --release`, Node `v24.22.3` / Vite `6.4.6`, Playwright Chrome `158.0.8127.37`
- **Overall Result:** **`PASS`** — toàn bộ 5 đặc tả trên đủ điều kiện nâng trạng thái từ `Implemented` thành **`Verified`** trong [`docs/designs/SITEMAP.md`](../designs/SITEMAP.md).

---

## 1. Admin Build, Bundles, JS Unit Suites, Gettext I18n, Standards & Governance Gates (`main@4bd2427`)

*(Build `cms/admin/dist/` và xác minh determinism của public JS/CSS bundles hoàn tất trước Cargo `--release` để `RustEmbed` nhúng đúng artifact của `4bd2427`.)*

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | TypeScript strict check toàn Admin UI | **PASS** | Exit `0` |
| `npm --prefix cms/admin run build:public-blocks-js` | Đóng gói Vanilla ES2020 public micro-JS pack | **PASS** | `10765 bytes` (`< 12288 B`), `7` modules, `650 total source LOC`; `git diff --exit-code` = `0` |
| `npm --prefix cms/admin run build:public-tailwind` | Build Tailwind v4 công khai (`opendash-tailwind.css`) | **PASS** | `89332 bytes` (`< 122880 B`), `3761` candidates; `git diff --exit-code` = `0` |
| `npm --prefix cms/admin run build` | `tsc && vite build` đóng gói `cms/admin/dist/` | **PASS** | `✓ 219 modules transformed.`, `dist/index.html` (`0.46 kB`), `dist/assets/index-D7UAayMd.css` (`121.08 kB`), `dist/assets/index-BVbsL3nQ.js` (`1,206.81 kB`), `✓ built in 2.72s` |
| `npm --prefix cms/admin run test:fields` | `CORE_ICONS`, `helpers.ts` (25 data types + 3 layouts), DNF parity | **PASS** | `tests 10`, `suites 4`, `pass 10`, `fail 0` (`41.38ms`) |
| `npm --prefix cms/admin run test:media-preview` | Viết lại `srcset`/`picture`/CSS preview responsive media & URL helpers | **PASS** | `tests 4`, `pass 4`, `fail 0` (`9.14ms`) |
| `npm --prefix cms/admin run test:nav` | Định tuyến canonical, sub-routes Studio, RBAC & cây menu | **PASS** | `tests 6`, `pass 6`, `fail 0` (`13.31ms`) |
| `npm --prefix cms/admin run test:public-blocks` | Hành vi Tabs, Slider, Lightbox+Share, Popup (`opendash-blocks.js`) | **PASS** | `4/4 suites passed (Tabs, Slider, Lightbox+Share, Popup)` |
| `node cms/admin/scripts/test-studio-ux.mjs` | CSP `BRIDGE_SCRIPT` SHA-256, loop/AST/repeater/preview reveal & sub-routes | **PASS** | `tests 13`, `pass 13`, `fail 0` (`40.61ms`) |
| `node cms/scripts/check-i18n.mjs --strict` | Kiểm tra chuỗi hardcoded & tính đầy đủ `.pot`/`.po` | **PASS** | `PASS check-i18n: zero hardcoded Vietnamese in admin/src and .pot/.po present.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting toàn workspace Rust | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` & `--self-test` | Ranh giới `core`/`storage`/`routes`/`admin`, SQL-only-in-storage, CSRF/Origin, `<= 600 LOC`, negative regressions | **PASS** | `registrations=120 resolved_handlers=120 direct_guarded=114 delegated_csrf_guarded=1 public_origin_guarded=3 bootstrap_exemptions=2`; `--self-test` PASS (`43 + 16 + 10`) |
| `python3 cms/scripts/check-plans.py` & `--self-test` | Liên kết Markdown tương đối, bao phủ `SITEMAP.md`, chứng cứ `docs/verification/` | **PASS** | Exit `0` (`13 fixtures (valid + 12 failures)`) |
| `python3 .agents/scripts/check-agent-artifacts.py` | Vệ sinh agent skills, không secrets/hooks lỗi thời | **PASS** | Exit `0` |
| `python3 cms/scripts/run-tests.py --list` & `--self-test` | Mapping 8 integration binaries trên 7 nhóm & 20 test routing | **PASS** | `Coverage check: PASS (0 unmapped integration test files)`; `Ran 20 tests in 0.007s`, `OK` |
| `git diff --check` | Kiểm tra conflict markers / trailing whitespace | **PASS** | Exit `0` |

---

## 2. Tier-2 Full Workspace Rust `--release` Verification (`772 passed, 0 failed, 1 ignored` trong `1052.5s`)

Chạy trên `main@4bd2427` bằng `CARGO_INCREMENTAL=0 RUSTC_WRAPPER="" TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --group all --release`:

| Cargo Test Target (`--locked --release`) | Result | Passed / Failed / Ignored | Duration |
| --- | --- | --- | --- |
| `opendash-core` (lib unit) | **PASS** | `173 passed; 0 failed; 0 ignored` | `0.01s` |
| `opendash-plugin-sdk` (lib unit) | **PASS** | `5 passed; 0 failed; 0 ignored` | `0.00s` |
| `opendash` (bin unit) | **PASS** | `327 passed; 0 failed; 0 ignored` | `158.72s` |
| `tests/builder.rs` | **PASS** | `24 passed; 0 failed; 0 ignored` | `6.63s` |
| `tests/content.rs` | **PASS** | `54 passed; 0 failed; 0 ignored` | `11.28s` |
| `tests/i18n.rs` | **PASS** | `19 passed; 0 failed; 0 ignored` | `4.50s` |
| `tests/ops.rs` | **PASS** | `29 passed; 0 failed; 0 ignored` | `29.20s` |
| `tests/plugin.rs` | **PASS** | `27 passed; 0 failed; 0 ignored` | `9.66s` |
| `tests/plugin_egress_ssrf.rs` | **PASS** | `1 passed; 0 failed; 0 ignored` | `0.06s` |
| `tests/site_media_seo.rs` | **PASS** | `22 passed; 0 failed; 0 ignored` | `7.63s` |
| `tests/theme.rs` | **PASS** | `91 passed; 0 failed; 1 ignored` | `13.02s` |
| Doc-tests (`opendash_core`, `opendash_plugin_sdk`) | **PASS** | `0 passed; 0 failed; 0 ignored` | `0.00s` |

*(Ghi chú test bị ignore sẵn từ trước: `starter_typed_urls::actual_old_new_old_binary_roundtrip_preserves_data_and_cas` yêu cầu biến môi trường `OPENDASH_STARTER_V100_BIN` trỏ tới binary cũ bên ngoài.)*

---

## 3. Release Binary & CLI Package / Gettext Checks

- **Release Binary (`cms/target/release/opendash` biên dịch từ `4bd2427e63d937c3ae33e5887feb135ac0e5a7e7`, kích thước `69M`):**
  - **SHA-256:** `099c2b1120e052cb5417cc2cb4c9ca80eb0d53b661638427155706368bc4e951`

| Command | Result | Observed Output |
| --- | --- | --- |
| `opendash --check-theme cms/crates/opendash-server/src/builtin_themes/opendash-starter` | **PASS** | `{"ok":true,"theme_id":"opendash-starter","version":"1.0.0",...,"options_count":6}` (`9` templates, `4` parts, `3` page_layouts, `6` patterns, `6` blocks) |
| `opendash --check-theme cms/examples/themes/sample-theme` | **PASS** | `{"ok":true,"theme_id":"sample-theme","version":"1.0.0","templates":["single"],...}` |
| `opendash --check-theme cms/examples/themes/theme-js-starter` | **PASS** | `{"ok":true,"theme_id":"theme-js-starter","version":"1.0.0","templates":["index","single"],"has_functions":true}` |
| `opendash --check-theme cms/fixtures/demo-website/theme-moc-studio` | **PASS** | `{"ok":true,"theme_id":"moc-studio","version":"1.0.0","templates":["archive","page","single","taxonomy"],...,"options_count":5}` |
| `opendash --check-plugin cms/examples/plugins/plugin-js-example` | **PASS** | `{"ok":true,"plugin_id":"plugin-js-example","version":"1.0.0","manifest_schema":3,...,"settings_count":1}` |
| `opendash --l10n-extract cms/admin/src --out /private/tmp/tier2-4bd2427-l10n.pot` | **PASS** | `{"entries":2552,"ok":true}` (`270K`) |
| `opendash --l10n-compile cms/l10n/vi.po --out /private/tmp/tier2-4bd2427-vi.mo --pot cms/l10n/opendash.pot` | **PASS** | `{"entries":5491,"ok":true}` (`553K`) |

---

## 4. Live Chrome Playwright E2E Verification (`158.0.8127.37` trên Release Binary `099c2b11...`)

Mỗi kịch bản khởi chạy một tiến trình cô lập từ chính binary release `099c2b1120e052cb5417cc2cb4c9ca80eb0d53b661638427155706368bc4e951` (`source_sha: 4bd2427e63d937c3ae33e5887feb135ac0e5a7e7`):

| Suite | Scope | Result | Verified Checks / Receipts |
| --- | --- | --- | --- |
| [`studio_interactive_blocks_e2e.py`](../../cms/scripts/studio_interactive_blocks_e2e.py) | Spec A3 — Interactive Pack, 7 Core Blocks (`core/popup`) & 5 Starter Marketing Blocks (`#637`) | **PASS** | `12/12` checks (`starter_5_marketing_blocks_zero_js_and_csp`, `popup_header_drawer_display_contents_and_anchor_close`, `tabs_click_and_keyboard_navigation`, `slider_nav_fetchpriority_autoplay_and_reduced_motion`, `gallery_lightbox_nav_esc_backdrop_and_focus`, `social_share_origin_and_copy_status`, `popup_video_modal_pauses_on_close_and_fetchpriority`, `popup_close_on_backdrop_false_enforced`, `popup_cross_page_open_on_hash_and_deep_tab_hash`, `popup_contact_form_submit`, `studio_preview_reveal_and_close_popup_no_focus_theft`, `zero_public_http_and_csp_errors`; Chrome `158.0.8127.37`) |
| [`admin_sidebar_ia_e2e.py`](../../cms/scripts/admin_sidebar_ia_e2e.py) | Admin Sidebar IA, `Customize Studio` (`Tùy chỉnh Studio`) & Sub-routes (`#630`, `#634`, `#636`) | **PASS** | `7/7` checks (`admin_sidebar_order_and_cpt_plurals`, `cuj1`–`cuj5`, `chrome_page_errors_none`; `admin-sidebar-en.png` SHA-256 `4694c4c08e424cd53c4e20bc924983c2bd0ef6238be58df1558df60740d1c4bb`, `admin-sidebar-vi.png` SHA-256 `4fbf89291d9203efdc13876e5c8ce5642f963855ee2d84a3e983db037b050187`) |
| [`acf_content_structure_e2e.py`](../../cms/scripts/acf_content_structure_e2e.py) | Regression ACF V1 + Options Pages + ConfirmDialog (`#631`, `#634`) | **PASS** | `5/5` checks (`migration_budget_and_tables`, `cuj1_cpt_and_delete_dialog`, `cuj2_field_group_and_entry`, `cuj3_taxonomy_term_meta_and_inline_create`, `cuj4_options_page_multilingual_rbac`) |
| Kịch bản Chrome Playwright bổ sung (tổng hợp A1/A2/UI/Sub-routes trên `099c2b11...`) | Spec A1 (`#632`), Spec A2 (`#635`), Admin UI/UX Polish (`#634`), Studio Sub-routes & Options IA (`#636`) | **PASS** | `8/8` checks (`a2_home_index_unconfigured_pagination_and_sitemap`: `/`, `/?page=2`, `index,follow`, `<loc>https://example.com/</loc>` trên `/sitemaps/1.xml`; `a2_theme_v3_zip_rejected_400`; `a1_recursive_ast_studio_roundtrip_accordion_and_card_inner_cta`: Mode 2 reload round-trip, `0` JS Accordion `<details open>`, bấm được CTA con bên trong `core/card` có `card_link`; `a2_single_post_template_blocks_and_taxonomy_loop_dedupe`: 1 `<symbol id="od-sym-clock">` sprite & `od-lt-*` scoped CSS; `a2_studio_sample_picker_and_static_homepage_switch`; `studio_subroutes_zero_accordion_live_preview_and_save`: redirects `#/studio?panel=*`/`#/appearance/options`/`#/builder?template=header`, `0` `<details>` trong Options, Live Preview `#canvas` `header_badge` + Save & reload `custom:promo_banner`; `mobile_390x844_studio_no_overflow`; `zero_console_csp_and_http_errors`; Chrome `158.0.8127.37`) |

---

## 5. Giới hạn & Hạng mục Theo dõi Không chặn

1. **Môi trường & CodeGraph Docs Index:** Xác minh trực tiếp trên `macOS` `x86_64` trên commit bất biến `main@4bd2427`. Lệnh `node cms/scripts/codegraph-index-docs.mjs` trên máy host thoát mã `1` do chưa cài binary CLI `@astudioplus/codegraph-mcp@0.21.0` / `CODEGRAPH_ENGINE_PATH`.
2. **Edge-case chuyển hướng hash nội trang cùng đích ([`cms/admin/src/App.tsx#L321`](../../cms/admin/src/App.tsx#L321)):** Khi người dùng đã ở sẵn route chuẩn (ví dụ `#/studio/options`) và kích hoạt `hashchange` trực tiếp sang chính alias cũ của nó (`#/appearance/options`) trong cùng trang, điều kiện `if (target === acceptedHashRef.current && event) return;` thoát sớm trước `window.history.replaceState(..., target)` nên giao diện vẫn hiển thị đúng nhưng thanh địa chỉ chưa tự chuẩn hóa lại (khi truy cập alias từ route khác hoặc tải trang mới thì `replaceState` hoạt động bình thường). Cần 1 PR vá nhỏ xử lý `window.location.hash !== target` ở chốt kiểm tra này.
3. **Studio Preview `<video src>` & 4 điểm Minor của Drift Audit Spec A3:** Cơ chế `previewMediaRewrite` có sẵn trong Studio Preview hiện bỏ qua thẻ `<video src>` bên trong sandbox iframe (`401` kỳ vọng với riêng `<video>` trong Studio Preview, đã tách lọc trong `studio_interactive_blocks_e2e.py`, trang công khai đạt `0` lỗi); 4 điểm Minor từ Drift Audit Spec A3 (`c9850c4`) gồm comment `"41 Core Blocks"` cũ ở `storage/page/types.rs`, 5 block template trong `opendash-starter` chưa dùng `__()`, tách 7 module ES2020 ở `cms/admin/src/public-blocks/`, và `depths.math == 0` ở `find_top_level_body_close` không chặn merge hay vận hành.
