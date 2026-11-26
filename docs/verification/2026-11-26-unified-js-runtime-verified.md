# Unified JS Runtime (`boa_engine`), Render/Route Hooks, Extension Secrets & Dev Live-Edit — EGM Closure Verification (2026-11-26)

- **Design Spec:** [`docs/designs/unified-js-runtime-render-hooks-and-dev-mode.md`](../designs/unified-js-runtime-render-hooks-and-dev-mode.md)
- **Drift Report:** [`.agents/research/2026-11-26-unified-js-runtime-drift-report.md`](../../.agents/research/2026-11-26-unified-js-runtime-drift-report.md) (3 independent fresh-context Goldfish `egm-codebase-details` rounds: `38199ce8` → `ea75ff3b` → `2af31f4f` → **`PASS — Ready for Verified`**, `0 Blocker · 0 Major · 0 Minor · 0 Nit`)
- **Merged Commits & PRs on `main`:**
  - PR `#582` — Spec B Approved (`docs/designs/unified-js-runtime-render-hooks-and-dev-mode.md`)
  - PR `#586` (`f441c04`) — **Slice S01**: Unified JS Runtime (`boa_engine = "=0.22.0"`), Theme `functions.js`, migration `0053_theme_functions_runtime.sql` (`+0` net objects), `--plugin-worker` `128 MiB` OS RAM cap, `cms/sdk/js/opendash.d.ts`, `unified_js_runtime_s01_test.rs`
  - PR `#587` (`434c21f`) — **Slice S02**: Render Hooks (`{{ od_hook(...) }}`, `filter:render/{head,footer,slot}`), `SiteCacheEpoch`, `ExtensionRenderCache`, `sanitize_render_head_fragment`, `render_hooks_cache_s02_test.rs`
  - PR `#589` (`b41c916`) — **Slice S03**: Scoped Custom Routes (`/p/{pkg}/*`, `/api/p/{pkg}/*`, `/api/public/p/{pkg}/*`), `PUBLIC_EXTENSION_POST` standards audit, `custom_routes_s03_test.rs`
  - PR `#590` (`cfe320b`) — **Slice S04**: Extension Secrets Backup/Restore (`BackupFileKind::ExtensionSecret`), Theme Functions Admin APIs & React Admin UI, Local Dev Live-Edit (`--dev-theme`, `--dev-plugin`, `POST /api/admin/dev/sync`), `cms/examples/{themes/theme-js-starter,plugins/plugin-js-example}/`, Contract Docs, `theme_functions_dev_e2e_test.rs`
  - PR `#591` (`cd5bc5c`) — **Drift Remediation Round 1**: Closed `F-03`, `F-04`, `F-05`, `F-08`, `F-09`, `F-10`, `F-11` + initial fixes for `F-01`, `F-02`, `F-06`, `F-07`
  - PR `#592` (`745cd36`) — **Drift Remediation Round 2**: Closed remaining `F-01` (WHATWG HTML5 state-aware tag scanner & attr char guards), `F-02` (fail-closed `is_json_ld_script_block` & `<head>` smuggling block), `F-06` (exact `PLUGIN_ABI.md` & `plugin/README.md` alignment), `F-07` (zero `HEAD_END_MARKER` leak across `/p/` & fallback paths), `routes/README.md:52`, and removed stray SQLite WAL/SHM fixture files
- **Environment:** macOS (`x86_64-apple-darwin`), `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Node/Vite (`cms/admin`, Vite `6.4.6`), `sccache` enabled, `TMPDIR=/private/tmp`
- **Verifiers:** Elephant (Antigravity) + 3 fresh-context Goldfish `egm-codebase-details` auditors (`38199ce8`, `ea75ff3b`, `2af31f4f`)
- **Overall Result:** **`PASS`** — promotes [`docs/designs/unified-js-runtime-render-hooks-and-dev-mode.md`](../designs/unified-js-runtime-render-hooks-and-dev-mode.md) to **`Verified`** on [`SITEMAP.md`](../designs/SITEMAP.md).

---

## 1. Automated Verification Receipts

### 1.1. Tier-2 Full Release Suite (Baseline `main@cfe320b` — Full S01–S04 Implementation)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --group all --release` | Toàn bộ workspace Rust (`--release --locked`): `opendash-core`, `opendash-plugin-sdk`, `opendash` unit tests & tất cả 7 domain groups | **PASS** | **`1166.0s`**, **92/92 integration test suites PASS**, `0` failed (gồm `unified_js_runtime_s01_test`, `render_hooks_cache_s02_test`, `custom_routes_s03_test`, `theme_functions_dev_e2e_test`, `m8_*`, `multilingual_*`, `plugin_*`, `studio_*`) |
| `./cms/target/release/opendash --check-theme cms/examples/themes/theme-js-starter` | CLI offline kiểm tra Theme JS mẫu | **PASS** | Exit `0`: `{"ok":true,"theme_id":"theme-js-starter","version":"1.0.0","templates":["index","single"],...,"has_functions":true}` |
| `./cms/target/release/opendash --check-plugin cms/examples/plugins/plugin-js-example` | CLI offline kiểm tra Plugin JS mẫu | **PASS** | Exit `0`: `{"ok":true,"plugin_id":"plugin-js-example","version":"1.0.0","manifest_schema":3,"hooks":[],"filters":["filter:render/footer","filter:render/slot"],"actions":["reset_counter"],"blocks":[],"settings_count":1}` |

### 1.2. Post-Drift Remediation Verification (`PR #591` `cd5bc5c` & `PR #592` `745cd36`, Tier-1 per Owner Directive PR `#585`)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Rust formatting toàn workspace | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` | Ranh giới Core/Storage/Routes/Admin & CSRF/Public Origin | **PASS** | `CSRF mutation inventory: registrations=96 resolved_handlers=96 direct_guarded=90 delegated_csrf_guarded=1 public_origin_guarded=3 bootstrap_exemptions=2` |
| `python3 cms/scripts/check-standards.py --self-test` | Negative regression fixtures cho SQL/IO/CSRF/Public Origin/Extension POST | **PASS** | `43 mutation + 16 public Origin + 10 delegated CSRF negative regressions` (+ wrapper-bypass & helper checks cho `/api/public/p/{pkg}` và `/api/public/p/{pkg}/*`) |
| `python3 cms/scripts/check-plans.py` & `--self-test` | Markdown links, EGM Sitemap & Verified evidence | **PASS** | `PASS check-plans` + `13 fixtures (valid + 12 failures)` |
| `python3 .agents/scripts/check-agent-artifacts.py` | Quy chuẩn kỹ năng & không lộ secrets | **PASS** | Exit `0` |
| `node cms/scripts/check-i18n.mjs --strict` | Parity `en.ts` ↔ `vi.ts` (gồm 22 khóa Spec B mới) | **PASS** | `2165 translation keys, 94 files checked.` |
| `npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build` | TypeScript & Vite production bundle cho Admin UI | **PASS** | `✓ 121 modules transformed.`, `✓ built in 2.28s` (`dist/assets/index-E4wp58tn.js`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash -- document::tests:: template_sec::tests:: content_render::tests:: i18n::tests::` | Toàn bộ unit tests HTML/C-SEC/SEO/head-marker/i18n trên `745cd36` (gồm các payload khai thác `F-01`, `F-02`, `F-07`) | **PASS** | `0` failed |
| `TMPDIR=/private/tmp python3 cms/scripts/run-tests.py --group theme --group plugin --group builder --group site-media-seo --group ops` (PR `#591`) & `--group theme --group plugin --group site-media-seo` (PR `#592`) | Toàn bộ nhóm miền liên quan sau sửa drift (`S01`–`S04` E2E + regressions `F-01`–`F-10`) | **PASS** | PR `#591`: `82.9s` PASS; PR `#592`: `173.3s` PASS |
| `git diff --check` | Whitespace & conflict marker hygiene | **PASS** | Exit `0` |

---

## 2. Highlights Verified End-to-End

1. **Zero-Build JS Runtime (`boa_engine = "=0.22.0"`) & Compatibility:** `plugin.js` và Theme `functions.js` chạy trong tiến trình `--plugin-worker` cô lập (trần RAM OS `128 MiB` qua Jetsam/RLIMIT_AS/Job Object, `RuntimeLimits` 5M loops, top-level host I/O bị chặn) song song với `plugin.wasm` (Wasmi `=2.0.0`).
2. **Render Hooks & Cache Isolation:** `{{ od_hook("slot", expr?) }}`, `filter:render/{head,footer,slot}`, `ExtensionRenderCache` (LRU 512 / 256 KiB, invalidation qua `SiteCacheEpoch::global().bump()` kể cả sau projection commits), `<!-- opendash-head-end -->` không rò rỉ ra HTML cuối, Core SEO (`<title>`, `robots`, `http-equiv`, `canonical`/`alternate`, `application/ld+json`) bất khả xâm phạm trước cả HTML5 entities lẫn quote-differential payloads, và opt-out `render_trusted_html: false` được tôn trọng ở cả Theme & Plugin dưới WHATWG HTML5 state-aware C-SEC scanner.
3. **Scoped Custom Routes:** Đầy đủ `/p/{pkg}`, `/p/{pkg}/{*subpath}`, `/api/p/{pkg}`, `/api/p/{pkg}/{*subpath}`, `/api/public/p/{pkg}`, `/api/public/p/{pkg}/{*subpath}` (`Public` strict Origin vs `Webhook` raw-body + allowlisted HMAC headers + RateLimit + `MutationLease`, cấm `content_save`/`content_delete` trên `Public`, chặn SSRF nội bộ).
4. **Extension Secrets Backup/Restore & Local Dev Live-Edit:** `BackupFileKind::ExtensionSecret` sao lưu/phục hồi `database/secrets/{plugins,themes}/<pkg>/<64-hex>.json` với ACL `0700`/`0600`; `--dev-theme` / `--dev-plugin` hot-reload trực tiếp templates/CSS/blocks/patterns/options/`functions.js`/`plugin.js` với `dev-<64-hex>` `no-store`, chặn `.env`/symlink/`node_modules`, từ chối khởi chạy trong Safe Mode hoặc có `trusted_public_origin` mà thiếu `--allow-dev-remote`, cảnh báo `dev_mode_active: true` trong backup metadata, và không bao giờ lưu mã nguồn dev vào `plugins.wasm_blob` khi chạy `POST /api/admin/dev/sync`.

---

## 3. Limits & Honest Notes

1. **Tier-2 vs Tier-1 Split (Owner Directive PR `#585`):** Full Tier-2 release (`--group all --release`, 92/92 integration suites, `1166.0s`) chạy trên `main@cfe320b` (trọn bộ S01–S04); các bản vá drift sau đó (`PR #591`, `PR #592`) được kiểm chứng bằng bộ unit tests đầy đủ (`document`, `template_sec`, `content_render`, `i18n`) + Tier-1 domain suites bao phủ 100% miền bị ảnh hưởng (`theme`, `plugin`, `builder`, `site-media-seo`, `ops`) + tất cả 4 suite E2E Spec B.
2. **GitHub Actions Paused:** Workflow GitHub Actions trên repository hiện đang được chủ dự án tạm tắt theo chỉ đạo PR `#585`; mọi PR `#586`–`#592` đều có `Self-review Receipt` kiểm chứng cục bộ đầy đủ trên macOS trước khi merge.
3. **Cross-Platform Runtime Parity:** Kiểm chứng thực thi cục bộ chạy trên macOS (`x86_64-apple-darwin`, Jetsam `posix_spawnattr_setjetsam_ext`); nhánh Windows Job Object & Linux `RLIMIT_AS` trong `operations_worker_process.rs` tuân theo cấu trúc biên dịch có điều kiện hiện hữu và sẽ được chạy thêm khi CI đa nền tảng bật lại hoặc trước release.
