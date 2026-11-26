# `/egm-drift-report` — Unified JS Runtime (`boa_engine`), Render/Route Hooks, Extension Secrets & Dev Live-Edit (Spec B)

- **Design Spec:** [`docs/designs/unified-js-runtime-render-hooks-and-dev-mode.md`](../../docs/designs/unified-js-runtime-render-hooks-and-dev-mode.md)
- **Date:** 2026-11-26
- **Implementation PRs Audited:**
  - Spec Approval: PR `#582`
  - Slice S01 (`boa_engine = "=0.22.0"`, Theme `functions.js`, `0053`, Worker OS RAM cap `128 MiB`, `cms/sdk/js/opendash.d.ts`): PR `#586` (`f441c04`)
  - Slice S02 (Render Hooks `od_hook` / `filter:render/{head,footer,slot}`, `SiteCacheEpoch`, `ExtensionRenderCache`): PR `#587` (`434c21f`)
  - Slice S03 (Scoped Custom Routes `/p/`, `/api/p/`, `/api/public/p/`): PR `#589` (`b41c916`)
  - Slice S04 (Extension Secrets Backup/Restore, Theme Functions API/UI, Dev Live-Edit, Examples & Contract Docs): PR `#590` (`cfe320b`)
  - Drift Remediation Round 1: PR `#591` (`cd5bc5c`, commit `45f8832`)
  - Drift Remediation Round 2: PR `#592` (`745cd36`, commit `649239e`)
- **Independent Zero-Context Goldfish Auditors (`egm-codebase-details`):**
  - Round 1 (`main@cfe320b`): `38199ce8-af34-495e-9280-a9e85d08a905` → `NEEDS FIXES` (`1 Blocker · 5 Major · 4 Minor · 1 Nit`)
  - Round 2 (`main@cd5bc5c`): `ea75ff3b-879c-4b7e-960a-05ef1e902175` → `NEEDS FIXES` (`7/11 CLOSED`; remaining `1 Blocker · 2 Major · 1 Minor · 1 Nit`)
  - Round 3 (`main@745cd36`): `2af31f4f-6994-4938-88ca-2b3c3e71c9f1` → **`PASS — Ready for Verified`** (`0 Blocker · 0 Major · 0 Minor · 2 prose Nit` — both prose nits polished in the Closure PR)

---

## 1. Final Verdict

- **Overall Verdict:** **`PASS — Ready for Verified`**
- **Remaining Severity Counts (after PR `#591`, PR `#592`, and Closure PR prose polish):**
  - **Blocker:** `0`
  - **Major:** `0`
  - **Minor:** `0`
  - **Nit:** `0`

---

## 2. Finding Lifecycle Summary (`F-01` – `F-11`)

| ID | Orig Sev | Slice | Summary | Remediation PR / Evidence | Final Status |
| --- | --- | --- | --- | --- | --- |
| `F-01` | Blocker | S02, S03 | C-SEC attribute smuggling (`{{ href="javascript:..." }}` in post-render mode) & WHATWG HTML5 unquoted-attribute / attribute-name quote differential (`<div data-x=a'><script>alert(1)</script>'></div>`) in `template_sec.rs` and `document.rs`. | PR `#591` disabled post-render Jinja token skipping in `parse_attributes`; PR `#592` replaced naive quote-toggling with 6-state WHATWG HTML5 scanners (`TagScanState` / `PostRenderTagScanState`) and strict char guards in `document.rs` & `template_sec.rs` + unit & E2E regressions. | **CLOSED** (Round 3 `2af31f4f`) |
| `F-02` | Major | S02 | Core SEO inviolability bypass in `sanitize_render_head_fragment` via HTML5 entities (`&#114;obots`, `&#99;anonical`, `&#97;pplication/ld+json`), fail-open `is_json_ld_script_block` `.find('>')`, and unquoted-attr `<head>` smuggling. | PR `#591` added `decode_html5_entities` across `robots`, `canonical`/`alternate`, `application/ld+json`; PR `#592` switched `is_json_ld_script_block` to quote-aware `find_tag_end` with fail-closed `return true` + state-aware `find_tag_end` + unit tests. | **CLOSED** (Round 3 `2af31f4f`) |
| `F-03` | Major | S02 | Missing `SiteCacheEpoch::global().bump()` after projection commits in `storage/plugin_projections.rs` (`recompute_entry_projections` & `spawn_recompute_owner_projections_batched`). | PR `#591` added `SiteCacheEpoch::global().bump()` immediately after every `tx.commit().await` in `storage/plugin_projections.rs`. | **CLOSED** (Round 2 `ea75ff3b`) |
| `F-04` | Major | S03 | Root `POST /api/public/p/{pkg}` (`path: "/"`) not mounted in `app.rs` despite `validate_routes` accepting it. | PR `#591` factored `execute_public_post`, mounted `extension_public_post_root`, extended `check-standards.py` (`PUBLIC_EXTENSION_POST` + wrapper-bypass negative self-test), and added E2E test in `custom_routes_s03_test.rs`. | **CLOSED** (Round 2 `ea75ff3b`) |
| `F-05` | Major | S01–S03 | Explicit `"render_trusted_html": false` opt-out on Themes was overridden to `true` in `hooks.rs` and `plugin/manager.rs`. | PR `#591` removed forced `Theme ||` override in `hooks.rs` & `plugin/manager.rs`, honoring `ThemeFunctionsSpec::effective_permissions()` + E2E test in `custom_routes_s03_test.rs`. | **CLOSED** (Round 2 `ea75ff3b`) |
| `F-06` | Major | S04 | Documentation drift & stale symbols in `docs/contracts/PLUGIN_ABI.md` and `cms/crates/opendash-server/src/{plugin,routes,storage}/README.md`. | PR `#591` & PR `#592` reconciled all contracts and subsystem READMEs (`RenderCachePolicy`, `ExtensionRouteDecl`, `uses_v3_features`, `MAX_RENDER_HOST_CALLS = 64`, `MAX_RENDER_HTTP_CALLS = 2`, macOS Jetsam, `ROUTE_GET_TIMEOUT = 1s`, `ExtensionRenderCache` LRU 512 / 256 KiB / epoch-only, exact symbols `validate_js_module_syntax`, `execute_js_worker`, `owner_render_filter_subscriptions`). | **CLOSED** (Round 3 `2af31f4f`) |
| `F-07` | Minor | S02 | `apply_render_hooks_to_document` split on first `</head>` instead of `<!-- opendash-head-end -->`; Round 2 flagged marker leak on `/p/` and fallback preview paths. | PR `#591` added `HEAD_END_MARKER` (`<!-- opendash-head-end -->`) + unit test; PR `#592` stripped `HEAD_END_MARKER` in `routes/{extensions,public,content}.rs` + E2E assertion `!item_html.contains("opendash-head-end")`. | **CLOSED** (Round 3 `2af31f4f`) |
| `F-08` | Minor | S02 | Pattern Builder preview (`type_key == "pattern"`) in `storage/builder.rs` did not clear `render_budget.hook_nonces`. | PR `#591` cleared `render_budget.hook_nonces` via `apply_render_hooks_to_document` before `finalize_document_icons`. | **CLOSED** (Round 2 `ea75ff3b`) |
| `F-09` | Minor | S04 | `--dev-theme` patterns (`get_theme_pattern`) and options (`latest_revision_manifest_in_conn` / `get_active_merged_public_options_in_conn_for_locale`) returned `404` when theme was not installed in `theme_revisions`. | PR `#591` added `DevOverlayConfig::read_dev_theme_pattern` and Dev overlay hooks in `storage/patterns.rs` & `storage/settings.rs` + E2E assertions in `theme_functions_dev_e2e_test.rs`. | **CLOSED** (Round 2 `ea75ff3b`) |
| `F-10` | Minor | S04 | `sync_dev_plugin_in_db` persisted raw dev `code_bytes` into SQLite `plugins.wasm_blob` (entering `VACUUM INTO` backups and executing without `--dev-plugin`). | PR `#591` preserved committed `active_revision_id IS NOT NULL` rows untouched, stored empty `wasm_blob` for dev-only plugins, blocked unmounted placeholders (`404 dev_plugin_unmounted`), and asserted `length(wasm_blob) == 0` in `theme_functions_dev_e2e_test.rs`. | **CLOSED** (Round 2 `ea75ff3b`) |
| `F-11` | Nit | S01, S04 | Stale `EnvelopeKind::Route` comment (`opendash-core/src/hooks.rs`), JS host error formatting (`plugin/js_runtime.rs`), `OwnerOptionsForm.tsx` prefix fallback (`theme_functions`), `routes/README.md:52` wording, and 2 Round-3 `PLUGIN_ABI.md` prose precision notes (`is_public_scope` on `/p/` vs `/api/p/`, `log` behavior in `FilterRender`/`RouteGet`). | Closed across PR `#591`, PR `#592`, and Closure PR (`PLUGIN_ABI.md` prose precision polish). | **CLOSED** |
