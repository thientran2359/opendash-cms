# Đợt Kiểm chứng Tầng 2 (`--release`) trên `main@3553f20` — 5 Tính năng Đã Merge (`#627`–`#631`, 2026-11-28)

- **Commit mã nguồn được kiểm chứng (`main@3553f20`):** `3553f200a20a1c6b1e8ac08f634fb6760bc04403` (kiểm chứng tại worktree cô lập gắn SHA bất biến `3553f20`; đóng PR tài liệu trên nền `origin/main@097f0d3`)
- **Phạm vi 5 Đặc tả được chốt `Verified`:**
  1. **Responsive Media & Replace Media (`#627` `4bfbeaedb6c6db445306a9ff51961ffae5891dd7`):** [`docs/designs/responsive-media-and-replace.md`](../designs/responsive-media-and-replace.md) · Drift report: [`.agents/research/2026-11-27-responsive-media-drift-report.md`](../../.agents/research/2026-11-27-responsive-media-drift-report.md) (`0 Blocker / 0 Major / 0 Minor / 0 Open Nit` sau 2 vòng Goldfish)
  2. **Entry Revisions, Diff & Server Autosave (`#628` `b3fba26fcec3bb00f22b7e49c1dd68ce0e9250d0`):** [`docs/designs/revisions-diff-and-autosave.md`](../designs/revisions-diff-and-autosave.md) · Drift report: [`.agents/research/2026-11-27-revisions-diff-and-autosave-drift-report.md`](../../.agents/research/2026-11-27-revisions-diff-and-autosave-drift-report.md) (`0 Blocker / 0 Major` sau vá; `1 Minor` tách submodule `<= 600` LOC đã nghiệm thu)
  3. **Public Site Search (`/?s=...`, SQLite FTS5 & Search API) (`#629` `61523d8c77c3e0d10705eaa063a3cae2a2e02680`):** [`docs/designs/public-site-search.md`](../designs/public-site-search.md) *(đã kiểm drift gọn & live E2E ở Bước 5 trước merge PR `#629`)*
  4. **Admin Sidebar IA & Routing Overhaul (`#630` `fcf17a1f2c3dd12299d75145522eac35f0409035`):** [`docs/designs/admin-sidebar-ia.md`](../designs/admin-sidebar-ia.md) *(commit `a517c35` ghi nhận `Drift Audit 0 Blocker / 0 Major / 0 Minor` trước merge PR `#630`)*
  5. **ACF Content Structure V1 (Đợt 2) (`#631` `3553f200a20a1c6b1e8ac08f634fb6760bc04403`):** [`docs/designs/acf-content-structure.md`](../designs/acf-content-structure.md) *(đã kiểm drift gọn & self-review `Orion` tại head `5519a9a` trước merge PR `#631`)*
- **Environment:** `macOS-15.8.2-x86_64-i386-64bit-Mach-O` (`RUSTC_WRAPPER="" TMPDIR=/private/tmp`), Cargo `--locked --release`, Vite `6.4.6`, Playwright Chrome `158.0.8127.37`
- **Overall Result:** **`PASS`** — toàn bộ 5 đặc tả trên đủ điều kiện nâng trạng thái từ `Implemented` thành **`Verified`** trong [`docs/designs/SITEMAP.md`](../designs/SITEMAP.md).

---

## 1. Admin Build, JS Unit Tests, Gettext I18n, Standards & Governance Gates (`main@3553f20`)

*(Build `cms/admin/dist/` hoàn tất trước khi chạy Cargo `--release` để `RustEmbed` nhúng đúng bundle của `3553f20`.)*

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | TypeScript strict check toàn Admin UI | **PASS** | Exit `0` |
| `npm --prefix cms/admin run build` | `tsc && vite build` đóng gói `cms/admin/dist/` | **PASS** | `✓ 213 modules transformed.`, `dist/index.html` (`0.46 kB`), `dist/assets/index-DnM_LkNe.css` (`116.21 kB`), `dist/assets/index-NYeJSidz.js` (`1,197.25 kB`), `✓ built in 3.09s` (và `2.96s` trên worktree cô lập) |
| `npm --prefix cms/admin run test:fields` | Logic `CORE_ICONS` (48 icons), `helpers.ts` (25 data types + 3 layouts), `conditions.ts` & `locationRulesEvaluator.ts` DNF parity | **PASS** | `tests 10`, `suites 4`, `pass 10`, `fail 0` (`47.31ms`) |
| `npm --prefix cms/admin run test:media-preview` | Viết lại `srcset`/`picture`/CSS preview responsive media & helper ETag/URL | **PASS** | `tests 4`, `pass 4`, `fail 0` (`10.73ms`) |
| `npm --prefix cms/admin run test:nav` | Định tuyến `#/contact`, `#/appearance/themes/install`, `#/content-structure/*`, RBAC & cây menu | **PASS** | `tests 5`, `pass 5`, `fail 0` (`10.24ms`) |
| `node cms/admin/scripts/test-studio-ux.mjs` | CSP `BRIDGE_SCRIPT` SHA-256, Studio UI actions & unified appearance routes | **PASS** | `tests 20`, `pass 20`, `fail 0` (`47.37ms`) |
| `node cms/scripts/check-i18n.mjs --strict` | Kiểm tra chuỗi hardcoded & tính đầy đủ `.pot`/`.po` | **PASS** | `PASS check-i18n: zero hardcoded Vietnamese in admin/src and .pot/.po present.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting toàn workspace Rust | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` & `--self-test` | Ranh giới `core`/`storage`/`routes`/`admin`, SQL-only-in-storage, CSRF/Origin, `<= 600 LOC`, negative regressions | **PASS** | `registrations=120 resolved_handlers=120 direct_guarded=114 delegated_csrf_guarded=1 public_origin_guarded=3 bootstrap_exemptions=2`; `--self-test` PASS |
| `python3 cms/scripts/check-plans.py` & `--self-test` | Liên kết Markdown tương đối, bao phủ `SITEMAP.md`, chứng cứ `docs/verification/` & 13 fixture `--self-test` | **PASS** | Exit `0` (`13 fixtures (valid + 12 failures)`) |
| `python3 .agents/scripts/check-agent-artifacts.py` | Vệ sinh agent skills, không secrets/hooks lỗi thời | **PASS** | Exit `0` |
| `python3 cms/scripts/run-tests.py --list` & `--self-test` | Mapping 8 integration binaries trên 7 nhóm + 20 unit tests routing/wiring | **PASS** | `Coverage check: PASS (0 unmapped integration test files)`; `Ran 20 tests in 0.019s`, `OK` |
| `git diff --check` | Kiểm tra conflict markers / whitespace | **PASS** | Exit `0` |

---

## 2. Tier-2 Full Workspace Rust `--release` Verification (`761 passed, 0 failed, 1 ignored` trong `990.8s`)

Chạy trong worktree cô lập gắn commit `3553f200a20a1c6b1e8ac08f634fb6760bc04403` bằng `RUSTC_WRAPPER="" TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --group all --release`:

| Cargo Test Target (`--locked --release`) | Result | Passed / Failed / Ignored | Duration |
| --- | --- | --- | --- |
| `opendash-core` (lib unit) | **PASS** | `184 passed; 0 failed; 0 ignored` | `0.01s` |
| `opendash-plugin-sdk` (lib unit) | **PASS** | `5 passed; 0 failed; 0 ignored` | `0.00s` |
| `opendash` (bin unit) | **PASS** | `316 passed; 0 failed; 0 ignored` | `159.35s` |
| `tests/builder.rs` | **PASS** | `17 passed; 0 failed; 0 ignored` | `5.09s` |
| `tests/content.rs` | **PASS** | `53 passed; 0 failed; 0 ignored` | `11.16s` |
| `tests/i18n.rs` | **PASS** | `19 passed; 0 failed; 0 ignored` | `4.51s` |
| `tests/ops.rs` | **PASS** | `29 passed; 0 failed; 0 ignored` | `24.58s` |
| `tests/plugin.rs` | **PASS** | `27 passed; 0 failed; 0 ignored` | `9.77s` |
| `tests/plugin_egress_ssrf.rs` | **PASS** | `1 passed; 0 failed; 0 ignored` | `0.06s` |
| `tests/site_media_seo.rs` | **PASS** | `22 passed; 0 failed; 0 ignored` | `7.60s` |
| `tests/theme.rs` | **PASS** | `88 passed; 0 failed; 1 ignored` | `12.66s` |
| Doc-tests (`opendash_core`, `opendash_plugin_sdk`) | **PASS** | `0 passed; 0 failed; 0 ignored` | `0.00s` |

*(Ghi chú test bị ignore sẵn từ trước: `starter_typed_urls::actual_old_new_old_binary_roundtrip_preserves_data_and_cas` yêu cầu biến môi trường `OPENDASH_STARTER_V100_BIN` trỏ tới binary cũ bên ngoài.)*

---

## 3. Release Binary & CLI Package / Gettext Checks

- **Release Binary (`cms/target/release/opendash` biên dịch từ `3553f200a20a1c6b1e8ac08f634fb6760bc04403`, kích thước `68M`):**
  - **SHA-256:** `baf615c7a0d08416d6b5cf1452a3f638f6e2a55fb5d2dd7559120cdf6452b28c`

| Command | Result | Observed Output |
| --- | --- | --- |
| `opendash --check-theme cms/crates/opendash-server/src/builtin_themes/opendash-starter` | **PASS** | `{"ok":true,"theme_id":"opendash-starter","version":"1.0.0",...,"options_count":6}` (`10` templates, `4` parts, `3` page_layouts, `6` patterns) |
| `opendash --check-theme cms/examples/themes/sample-theme-v3` | **PASS** | `{"ok":true,"theme_id":"sample-theme-v3","version":"1.0.0","templates":["single"],...}` |
| `opendash --check-theme cms/examples/themes/theme-js-starter` | **PASS** | `{"ok":true,"theme_id":"theme-js-starter","version":"1.0.0",...,"has_functions":true}` |
| `opendash --check-plugin cms/examples/plugins/plugin-js-example` | **PASS** | `{"ok":true,"plugin_id":"plugin-js-example","version":"1.0.0","manifest_schema":3,...,"settings_count":1}` |
| `opendash --l10n-extract cms/admin/src --out /private/tmp/tier2-l10n.pot` | **PASS** | `{"entries":2565,"ok":true}` (`268K`) |
| `opendash --l10n-compile cms/l10n/vi.po --out /private/tmp/tier2-l10n-vi.mo --pot cms/l10n/opendash.pot` | **PASS** | `{"entries":5304,"ok":true}` (`535K`) |

---

## 4. Live Chrome Playwright E2E Verification (`158.0.8127.37` trên Release Binary `baf615c7...`)

Mỗi kịch bản khởi chạy một tiến trình cô lập từ chính binary release `baf615c7a0d08416d6b5cf1452a3f638f6e2a55fb5d2dd7559120cdf6452b28c` (`source_sha: 3553f200a20a1c6b1e8ac08f634fb6760bc04403`):

| Suite | Scope | Result | Verified Checks / Receipts |
| --- | --- | --- | --- |
| [`admin_sidebar_ia_e2e.py`](../../cms/scripts/admin_sidebar_ia_e2e.py) | Admin Sidebar IA & Routing (`#630`) | **PASS** | `7/7` checks (`admin_sidebar_order_and_cpt_plurals`, `cuj1`–`cuj5`, `chrome_page_errors_none`; Chrome `158.0.8127.37`) |
| [`acf_content_structure_e2e.py`](../../cms/scripts/acf_content_structure_e2e.py) | ACF Content Structure V1 (`#631`) | **PASS** | `5/5` checks (`migration_budget_and_tables`, `cuj1_cpt_and_delete_dialog`, `cuj2_field_group_and_entry`, `cuj3_taxonomy_term_meta_and_inline_create`, `cuj4_options_page_multilingual_rbac`) |
| [`manual_redirects_404_e2e.py`](../../cms/scripts/manual_redirects_404_e2e.py) | Redirects & 404 regression | **PASS** | `7/7` checks (`cuj1`–`cuj3`, `cuj5`, `chrome_page_errors_none`; Chrome `158.0.8127.37`) |
| Kịch bản Playwright Chrome bổ sung (`#627` & `#629`) | Responsive Media + Replace Media (`#627`) & Public Site Search (`#629`) | **PASS** | `10/10` checks: upload 1600×900 sinh WebP `variants` + `webp_srcset`, `<picture>` + `?v=1` + `image/webp` HTTP `200`, UI Replace file 1200×600 giữ nguyên 64-hex `id` & tăng `?v=2` + `naturalWidth > 0`, `#/tools/media-reconcile` + `GET /api/admin/media/variants/status`, SSR `/?s=kien+truc+dac+biet` (`noindex, follow`, `form-action 'self'`, `<mark>`), `/en?s=architecture`, `/api/public/search`, `#/tools/search` reindex UI, Studio Mode 1 `studio-target-search` + diagnostic `search_results_outside_search_route`, Editor `403` RBAC; `0` page errors, `0` JS/CSP console errors, `0` HTTP unexpected 4xx/5xx (Chrome `158.0.8127.37`) |
| [`revisions_autosave_e2e.py`](../../cms/scripts/revisions_autosave_e2e.py) (Clean-Slate V1 adapted check, `#628`) | Revisions, Diff & Server Autosave CUJ 1–6 | **PASS (với ghi nhận fixture cũ)** | Bản gốc chưa cập nhật bị HTTP `422` tại CUJ 3 (`L426`) do gửi `"fields"` inline khi tạo CPT `secret_doc`/`story` (PR `#631` đã chuyển sang Clean-Slate Field Groups). Chạy cùng toàn bộ CUJ 1–6 trên bản thích ứng dùng `POST /api/admin/content-field-groups` với **đúng binary release `3553f20` không sửa code**: **PASS** `7/7` checks (`cuj1`–`cuj6` + `chrome_page_errors_none`, `sqlite_schema_count = 247`, Chrome `158.0.8127.37`). |

---

## 5. Giới hạn & Hạng mục Theo dõi Không chặn

1. **Môi trường & CodeGraph index:** Xác minh trực tiếp trên macOS `x86_64` với worktree cô lập tại `3553f20`; `node cms/scripts/codegraph-index-docs.mjs` trên máy host chưa chạy được do thiếu gói `@astudioplus/codegraph-mcp@0.21.0` / `CODEGRAPH_ENGINE_PATH`.
2. **Nợ cập nhật test fixture E2E/demo trên `main` hiện tại (`097f0d3`):**
   - [`cms/scripts/revisions_autosave_e2e.py`](../../cms/scripts/revisions_autosave_e2e.py) cần một PR dọn fixture riêng để chuyển 2 điểm tạo CPT `secret_doc` (`L430–L456`) / `story` (`L677–L694`) sang `POST /api/admin/content-field-groups` (theo Clean-Slate V1 của PR `#631`) và chuyển cấu trúc `builder_doc` (`"sections"` → `"blocks"` theo Spec A1 của PR `#632`).
   - [`cms/scripts/demo-website-runner.py`](../../cms/scripts/demo-website-runner.py) cũng còn tham chiếu `"fields"` / `"sections"` cần đồng bộ cùng đợt hoặc trước khi chạy demo/E2E trên nền Spec A1/A2.
3. **Phạm vi PR `#632` (`Spec A1`):** Spec A1 ([`studio-unified-recursive-ast.md`](../designs/studio-unified-recursive-ast.md)) merge vào `origin/main@097f0d3` trong lúc đợt Tầng 2 trên `3553f20` đang chạy, nên giữ nguyên trạng thái `Implemented` trong `SITEMAP.md` chờ đợt kiểm chứng Tầng 2 kế tiếp cùng Spec A2.
