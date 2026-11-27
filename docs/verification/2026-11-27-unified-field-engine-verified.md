# Unified Field Engine — Hệ kiểu Trường Thống nhất, Hydrator, C-SEC & `<UnifiedFieldForm>` (Đợt 1) — EGM Closure Verification (2026-11-27)

- **Design Spec:** [`docs/designs/unified-field-engine.md`](../designs/unified-field-engine.md) (Owner Approved; Đợt 1 — Slices S01–S04)
- **Drift Report:** [`.agents/research/2026-11-27-unified-field-engine-drift-report.md`](../../.agents/research/2026-11-27-unified-field-engine-drift-report.md) — 4 vòng Goldfish `egm-codebase-details` độc lập (`19663c41` → `df358887` → `88f15a54` → `4a70055c`) → **`PASS — Ready for Verified`** (`0 Blocker / 0 Major / 0 Minor / 1 Nit`)
- **Verified Code Commits on `main`:** `PR #622` (`ecbea04`, S01–S04 + Drift R1), `PR #623` (`0c8f1b7`, Drift R2 remediation), `PR #624` (`46226b5`, Drift R3 remediation; release & CLI run on `46226b5` trước commit tài liệu quy trình `d5ece6b`)
- **Environment:** macOS (`x86_64-apple-darwin`, `TMPDIR=/private/tmp`), Cargo (`--locked --release`), Node/Vite (`cms/admin`, Vite `6.4.6`), Google Chrome / built-in webview (`E2E-1`–`E2E-3` trên server cô lập `OPENDASH_SITE_ROOT=/private/tmp/opendash-ufe-e2e`, cổng `18779`).
- **Overall Result:** **`PASS`** — đủ điều kiện nâng trạng thái [`docs/designs/unified-field-engine.md`](../designs/unified-field-engine.md) thành **`Verified`** trên [`SITEMAP.md`](../designs/SITEMAP.md).

---

## 1. Admin Build, Field Engine Tests, Gettext I18n, Standards & Governance Gates

*(Đã kiểm chứng sau bản sửa cuối cùng `b369649` / `PR #624` `46226b5`; build `cms/admin/dist/` trước khi chạy bộ Rust `--release` để `RustEmbed` nhúng bản mới nhất.)*

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | TypeScript strict check toàn Admin UI | **PASS** | Exit `0` |
| `npm --prefix cms/admin run test:fields` | Logic thuần `CORE_ICONS` (48 icons), `helpers.ts` (`canonicalEmptyValue`, `normalizeSpacingValue` `-256`/`-128`/`0`, timezone), DNF `conditions.ts` parity | **PASS** | `tests 8`, `suites 3`, `pass 8`, `fail 0` (`37.98ms`) |
| `npm --prefix cms/admin run build` | `tsc && vite build` đóng gói `cms/admin/dist/` | **PASS** | `✓ 167 modules transformed.`, `dist/assets/index-*.js` (`977.15 kB`), `✓ built in 2.47s` |
| `node cms/scripts/check-i18n.mjs --strict` | Không hardcoded Vietnamese trong `cms/admin/src/`, đầy đủ `.pot`/`.po` | **PASS** | `PASS check-i18n: zero hardcoded Vietnamese in admin/src and .pot/.po present.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting toàn workspace Rust | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` & `--self-test` | Ranh giới `core`/`storage`/`routes`/`admin`, SQL-only-in-storage, danh mục CSRF/Origin, `<= 600 LOC`, negative regressions | **PASS** | `registrations=108 resolved_handlers=108 direct_guarded=102 delegated_csrf_guarded=1 public_origin_guarded=3 bootstrap_exemptions=2`; `PASS: core, storage, route and Admin boundaries` |
| `python3 cms/scripts/check-plans.py` & `--self-test` | Liên kết Markdown tương đối, bao phủ `SITEMAP.md`, chứng cứ `docs/verification/` & 13 fixture `--self-test` | **PASS** | Exit `0` (`13 fixtures (valid + 12 failures)`) |
| `python3 cms/scripts/run-tests.py --list` & `--self-test` | Mapping 8 file integration test trên 7 nhóm domain + 19 test routing/wiring/TMPDIR | **PASS** | `Coverage check: PASS (0 unmapped integration test files)`; `Ran 19 tests in 0.006s`, `OK` |
| `python3 .agents/scripts/check-agent-artifacts.py` & `git diff --check` | Vệ sinh agent skills, zero secret leak, không conflict marker / whitespace lỗi | **PASS** | Exit `0` |

---

## 2. Tier-2 Full Workspace Rust `--release` Verification (`692` passed, `0` failed, `1` ignored)

Chạy bằng `RUSTC_WRAPPER="" TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --group all --release` trên `main@46226b5`:

| Cargo Test Target (`--locked --release`) | Result | Passed / Failed / Ignored | Duration | Notable Coverage |
| --- | --- | --- | --- | --- |
| `opendash-core` (lib unit) | **PASS** | `162 passed; 0 failed; 0 ignored` | `0.01s` | `field_schema` 25 data types + 3 UI layouts, `HydrationVisibility::child_visibility()`, DNF `conditions`, 48 `icons.rs`, negative `spacing`, `richtext` |
| `opendash-plugin-sdk` (lib unit) | **PASS** | `5 passed; 0 failed; 0 ignored` | `0.00s` | SDK guest checks không hồi quy |
| `opendash` (bin unit) | **PASS** | `291 passed; 0 failed; 0 ignored` | `93.85s` | `field_hydrate` recursive ACL/Options/nonces, `template_sec` tree & `RichTextNonceTable`/`IconNonceTable`, `content_render::entry_view` `r2_01_*`, `storage::fields::drift_regressions` (`f01_*`, `f11_*`), `settings` |
| `tests/builder.rs` | **PASS** | `14 passed; 0 failed; 0 ignored` | `3.60s` | `s03_showcase_e2e_and_scoped_margin_css_public_render`, `s03_dynamic_options_sources_overrides_patterns_file_acl_and_skip_pre_v1` |
| `tests/content.rs` | **PASS** | `46 passed; 0 failed; 0 ignored` | `7.36s` | Gồm [`migration_0056_upgrades_baseline_0001_with_zero_schema_delta_and_under_256_objects`](../../cms/crates/opendash-server/tests/content/unified_field_content_cutover.rs) & `field_groups` nâng cấp DB `0001 → 0056` + `< 256` schema objects |
| `tests/i18n.rs` | **PASS** | `19 passed; 0 failed; 0 ignored` | `3.78s` | Admin i18n & Multilingual Content E2E |
| `tests/ops.rs` | **PASS** | `26 passed; 0 failed; 0 ignored` | `18.62s` | `SiteCoordinator`, `MutationLease` writer barriers, backup/restore |
| `tests/plugin.rs` | **PASS** | `27 passed; 0 failed; 0 ignored` | `9.33s` | Wasmi + Boa JS runtime, `SiteOptionsGet` hydration, `PluginSettingsGet`, Host Ops |
| `tests/plugin_egress_ssrf.rs` | **PASS** | `1 passed; 0 failed; 0 ignored` | `0.06s` | DNS-pinned SSRF isolation |
| `tests/site_media_seo.rs` | **PASS** | `19 passed; 0 failed; 0 ignored` | `6.58s` | Global Site Options CRUD/CAS, media/thumbnail ACL, permalinks, SEO |
| `tests/theme.rs` | **PASS** | `82 passed; 0 failed; 1 ignored` | `9.65s` | `unified_theme_options` (`m5_s04_t02_*` recursive `richtext`/`icon`/`hero`/`slides`/translatable media), `ThemeUrlPolicy`, `opendash-starter` `1.0.0` digest & clean-slate (`1` pre-existing ignored test trong suite) |
| Doc-tests (`opendash_core`, `opendash_plugin_sdk`) | **PASS** | `0 passed; 0 failed; 0 ignored` | `0.00s` | Biên dịch doc examples sạch |

---

## 3. Release Binary, CLI Package Checks & `E2E-4` Clean-Slate Audit

- **Release Binary (`cms/target/release/opendash`):**
  - **SHA-256:** `eaf49b823928539412eb0cff97d7284b4ea2b46a6912704a2c873cb73656be10`

| Command | Result | Output Summary |
| --- | --- | --- |
| `cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3` | **PASS** | `{"ok":true,"theme_id":"sample-theme-v3","version":"1.0.0","templates":["single"],...}` |
| `cms/target/release/opendash --check-theme cms/examples/themes/theme-js-starter` | **PASS** | `{"ok":true,"theme_id":"theme-js-starter","version":"1.0.0","templates":["index","single"],...,"has_functions":true}` |
| `cms/target/release/opendash --check-theme cms/fixtures/demo-website/theme-moc-studio` | **PASS** | `{"ok":true,"theme_id":"moc-studio","version":"1.0.0","templates":["archive","home","page","single","taxonomy"],"parts":["footer","header"],...,"options_count":5}` |
| `cms/target/release/opendash --check-plugin cms/examples/plugins/plugin-js-example` | **PASS** | `{"ok":true,"plugin_id":"plugin-js-example","version":"1.0.0","manifest_schema":3,"filters":["filter:render/footer","filter:render/slot"],"actions":["reset_counter"],...,"settings_count":1}` |
| `E2E-4` Legacy Symbol Audit (`! rg -n '\b(BlockFieldSpec\|ThemeOptionType\|PluginSettingFieldType\|TermFieldType\|ContentFieldType\|FieldValueInput\|BlockSchemaForm)\b' cms/crates/ cms/admin/src/`) | **PASS** | `0` kết quả khớp (exit `0`) |

---

## 4. Live Browser E2E Verification (`E2E-1`, `E2E-2`, `E2E-3` trên Release Binary)

Chạy trên server release cô lập (`http://127.0.0.1:18779`, `OPENDASH_SITE_ROOT=/private/tmp/opendash-ufe-e2e`, Bootstrap `admin` / `AdminPass123!`, import dữ liệu mẫu `opendash-starter`, bật đa ngôn ngữ `vi` + `en`, và upload 4 ảnh PNG thử nghiệm):

| Scenario | Verified Scope & Observed Browser Facts | Console / Page Errors | Result |
| --- | --- | --- | --- |
| **E2E-1 (Studio Block — CUJ 1)** | Thêm `core/showcase` trên Page #1 (`/gioi-thieu`); xác nhận render `<UnifiedFieldForm>` tabs (`tab_content`/`tab_style`), `intro_note` message, `acc_advanced` accordion; nhập `heading`, nested group `hero_card.primary_link` (`OpenDash E2E Link` → `/gioi-thieu`), `badge_icon` (`star`), `linked_menu` (`primary`), đặt `reference_url` rồi bật/tắt `show_cta` xác nhận bảo toàn giá trị ẩn, đặt `outer_margin` top `-16px`; kiểm tra DOM công khai `http://127.0.0.1:18779/gioi-thieu` (`HTTP 200`, inline `<svg>` từ `od_icon`, class `od-block-showcase-wrap mx-0 -mt-4 mb-0`) | `0` | **PASS** |
| **E2E-2 (Options, Translatable Media & Secrets — CUJ 2)** | Lưu Global Site Options (`site_logo` = `VI_ID`, `social_facebook` = `https://facebook.com/opendash.e2e`); upload & kích hoạt `e2e-ufe-theme` (`1.0.0`); lưu Theme Options `translatable: true` riêng cho `vi` (`Chao mung E2E VI` + `VI_ID`) và `en` (`Welcome E2E EN` + `EN_ID`); lưu Theme Functions `api_secret` (`secret`) rồi reload trang xác nhận chỉ hiện huy hiệu `Configured (write-only)` và không rò plaintext trong DOM/inputs; kiểm tra trang công khai `/` (`vi`, `HTTP 200`) và `/en` (`en`, `HTTP 200`) hiển thị đúng `site_options.site_logo` (`200`), `hero_tagline` và ảnh `hero_banner` theo từng locale | `0` | **PASS** |
| **E2E-3 (Rich Extension Field Group & Indexed Media ACL — CUJ 3)** | Upload & kích hoạt `e2e-rich-theme` (`1.0.0`, khai báo `field_groups` 9 kiểu gồm `richtext`, `media`, `gallery`, `group`, `repeater` chứa `slide_img`, `link`, `icon`, `entry_ref`, `fg_priv_media` `private`); tạo Post #7 (`E2E Rich ACL Post`, `/e2e-rich-acl-post`) gắn `ACL_PUB_ID` duy nhất vào trong `fg_slides[0].slide_img`, `ACL_PRIV_ID` vào `fg_priv_media`, và HTML `<p>Safe <strong>Bold E2E</strong></p><script>window.__xss=1;</script>` vào `fg_richtext`. Sau khi **Save Draft** (đăng xuất khỏi `/admin`): `/media/<ACL_PUB_ID>/file` → **`404`**, `/media/<ACL_PRIV_ID>/file` → **`404`**. Sau khi **Publish** (đăng xuất khỏi `/admin`): `/media/<ACL_PUB_ID>/file` → **`200`** (mở quyền qua repeater lồng), `/media/<ACL_PRIV_ID>/file` → **`404`** (bị chặn do `private` ACL), và trang công khai `/e2e-rich-acl-post` (`HTTP 200`) render `<p>Safe <strong>Bold E2E</strong></p>` không còn thẻ `<script>` | `0` | **PASS** |

---

## 5. Giới hạn & Hạng mục Theo dõi Không chặn

1. **Môi trường kiểm thử cục bộ & GitHub CI:** Xác minh trực tiếp trên macOS với site root tạm và cổng loopback; Owner đã tạm tắt GitHub Actions CI khi đóng PR `#622`–`#624`.
2. **Thay thế kịch bản smoke Python rời trước đây:** Các script `windows_auth_smoke.py`, `content_types_smoke.py`, `content_entries_smoke.py` đã được gộp/thay thế trên `main`; phần nâng cấp baseline SQLite `0001 → 0056` (kèm ràng buộc `< 256` schema objects và bảo toàn thứ tự/settings) được kiểm chứng trực tiếp qua bộ integration test Rust `--release` trong `tests/content.rs`.
3. **Field Group UI & Nit `R4-01`:** Việc tạo mới trường lồng (`group`/`repeater`) bằng UI thuộc phạm vi Luồng B (Đợt 2), nên trong `E2E-3` trường nhóm 9 kiểu được nạp qua theme manifest `field_groups` chuẩn hợp đồng thông qua UI `#/themes`; Nit `R4-01` (chuỗi thông báo lỗi `"requires 1 to 100 options"` khi `radio_buttons > 12`) đã ghi nhận trong [Drift Report](../../.agents/research/2026-11-27-unified-field-engine-drift-report.md).
