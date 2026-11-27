# Entry Lifecycle, Bulk Actions & Quick Edit (S01–S04, `#578`–`#581`) — EGM Closure Verification (2026-11-26)

- **Design Spec:** [`docs/designs/entry-lifecycle-bulk-quick-edit.md`](../designs/entry-lifecycle-bulk-quick-edit.md) (Owner Approved 2026-11-26)
- **GitHub Issues:** `#578` (S01), `#579` (S02), `#580` (S03), `#581` (S04)
- **Drift Report:** [`.agents/research/2026-11-26-entry-lifecycle-drift-report.md`](../../.agents/research/2026-11-26-entry-lifecycle-drift-report.md) — 3 independent fresh-context Goldfish `egm-codebase-details` rounds (`5895b7c9` → `32c84226` → `3d21e282`) → **`PASS — Ready for Verified`** (`0 Blocker · 0 Major · 0 Minor · 0 Nit`)
- **Branch & Commits (`agent/orion`):**
  - `b834e11` — **Slice S01 (`#578`)**: Core Trash lifecycle, additive migration (originally `0053`, renumbered to `0054_entry_lifecycle.sql` after PR `#586`), `storage::entry_lifecycle`, full public/media/ordinary writer closure, Reading Settings guard, `entry_lifecycle_s01.rs`.
  - `050a53e` — **Slice S02 (`#579`)**: Admin HTTP Trash/Restore/Purge & retained-Trash maintenance, Plugin ABI v1.1 (`19` Host Ops including `content_trash_get`/`list`, `content_restore`, `content_purge`), `LifecycleConfirmationStore` (`409 confirmation_required` → `POST .../confirm-lifecycle`), `http` mutual exclusion, Rust (`0.1.1`) & AssemblyScript Dual SDK, `entry_lifecycle_s02.rs`, `plugin_entry_lifecycle_s02.rs`, `entry_lifecycle_trash_e2e.py`.
  - `22cb0d1` — Sync `origin/main` (PR `#583`–`#586`) and renumber `0053_entry_lifecycle.sql` → `0054_entry_lifecycle.sql` (53 total migrations `0001`–`0054`, omitting `0016`).
  - `90172e2` — **Slice S03 (`#580`)**: `PATCH .../entries/{id}/metadata`, `filter:content/prepare_save` `mutation_scope` (`422 metadata_hook_scope_conflict`), site-timezone `published_at` validation, inline `QuickEditForm.tsx`, `EditorDraftStore` `quick_edit:` isolation (`401`/`412` preservation & explicit rebase), Studio/Editor `409 entry_trashed` handling, `entry_lifecycle_s03.rs`, `entry_lifecycle_quick_edit_e2e.py`.
  - `a26e416` — **Slice S04 (`#581`)**: Bounded current-page Bulk Actions (max `100`, same-locale guard, broadened `draft_publish` CPT publication, single-taxonomy `add`/`remove`/`replace`, Trash/Restore/Admin-only Purge, 5 outcome buckets, stop on `401`/`503`/scope change, reconciliation without replaying committed successes), privacy-safe structured diagnostic logs (`entry_lifecycle_diag op=... storage_us=...`), `entry_lifecycle_s04.rs`, `entry_lifecycle_bulk_actions_e2e.py`, `entry_lifecycle_benchmark.py`.
  - `d937189` — Sync `origin/main` (PR `#587`–`#589`, `b41c916`), wire `SiteCacheEpoch::global().bump()`, preserve `RoutePost` `content_delete` Trash parity, update `m8_s01_t02_writer_barrier.rs`.
  - `7df4afc` — S04 measured storage optimizations: covering-index dynamic `QueryBuilder` in `storage/entry_table.rs` (`idx_content_entries_lifecycle_list`) and conditional `permalinks::sync_claims_in_tx` in `storage/entries.rs` & `storage/entry_lifecycle.rs`.
  - `b537654` — Sync `origin/main` (`8d3c5e3`, PR `#590`–`#593`) and align `routes::themes::theme_active_action` with trusted `ActionCommitOutcome`.
  - `d83edff` — **Goldfish Drift Remediation Round 1 (`F-01`–`F-08`)**: fix draft Quick Edit slug/category claim sync (`F-01`), non-page published Purge orphan redirect preservation (`F-02`), shared manageable-locale parked-locale guard (`F-03`), `RoutePost` `EntryCommitReceipt` hook dispatch (`F-04`), Boa JS `od.content.{trashGet,trashList,restore,purge}` & `ThemeFunctionsSpec` validation (`F-05`), true idempotent `set_publication_status` no-op (`F-06`), contract/SDK/README alignment (`F-07`–`F-08`), plus regression tests in `entry_lifecycle_s03.rs` and `entry_lifecycle_s04.rs`.
  - `0972443` — **Goldfish Drift Remediation Round 2 (`R2-01`–`R2-04`)**: enforce manageable-locale scope in `entry_table::query` for explicit `?locale=<parked>` (`R2-01`, closing `F-03`) + explicit regression assertions and doc sync (`R2-02`–`R2-04`).
  - `632f797` — Clean auto-merge of `origin/main` (`9c2699b`, PR `#594` pure docs).
- **Environment:** macOS `15.8.2` (`x86_64-apple-darwin`, 12 cores), `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Node/Vite (`cms/admin`, Vite `6.4.6`), Google Chrome (`channel="chrome"`), `sccache` enabled, canonical short `TMPDIR="/private/tmp/opendash-orion-tmp"`. Dedicated worktree `/Users/thientran/Local Sites/opendash-workspace/opendash-entry-lifecycle`; port `18964` never touched (all tests & benchmarks used disposable site roots and ephemeral loopback ports).
- **Overall Result:** **`PASS`** — promotes [`docs/designs/entry-lifecycle-bulk-quick-edit.md`](../designs/entry-lifecycle-bulk-quick-edit.md) to **`Verified`** on [`SITEMAP.md`](../designs/SITEMAP.md).

---

## 1. Automated Rust, SDK, Admin & Governance Verification Receipts

### 1.1. Full Workspace Baseline Sweep (`agent/orion@d937189`, post-S01–S04 + PR `#587`–`#589`)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `cargo test --manifest-path cms/Cargo.toml --locked --workspace` (unit + all integration targets) | Toàn bộ workspace Rust (`opendash-core`, `opendash-plugin-sdk`, `opendash-server` unit + integration) | **PASS** | `90/90` `opendash-core`, `5/5` `opendash-plugin-sdk`, `266/266` server unit tests, **`96/96` integration test binaries PASS** (`0` failed) |
| `python3 cms/scripts/content_entries_smoke.py` | API/SQLite content CRUD, CAS & media wiring smoke | **PASS** | Exit `0` |

### 1.2. Post-Optimization, Post-Merge (`b537654`) & Post-Drift Remediation (`d83edff` / `0972443` / `632f797`) Receipts (Tier-1 per Owner Directive PR `#585`)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck && npm --prefix cms/admin run build` | TypeScript & Vite production bundle (`cms/admin/dist/`) | **PASS** | `✓ 124 modules transformed.`, `✓ built in 2.04s` (`dist/assets/index-8AuiP98U.js`, `1,293.94 kB`) |
| `node cms/scripts/check-i18n.mjs --strict` | Parity `en.ts` ↔ `vi.ts`, interpolation placeholders, zero hardcoded Vietnamese | **PASS** | `i18n --strict: 2290 translation keys, 97 files checked.` |
| `node cms/sdk/as/test_sdk_v11.mjs` | AssemblyScript SDK v1.1 (`-2` readback, v1/v2 envelopes, all 19 Host Ops) | **PASS** | `PASS test_sdk_v11.mjs: all ABI v1.1 (-2 readback), Envelope v1/v2, 19 Host Ops & output helper checks passed.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Rust formatting toàn workspace | **PASS** | Exit `0` |
| `python3 cms/scripts/check-standards.py` & `--self-test` | Layer boundaries, SQL-only-in-storage, CSRF/Public Origin inventory & negative regressions | **PASS** | `registrations=101 resolved_handlers=101 direct_guarded=95 delegated_csrf_guarded=1 public_origin_guarded=3 bootstrap_exemptions=2`; `43 mutation + 16 public Origin + 10 delegated CSRF negative regressions` PASS |
| `python3 cms/scripts/check-plans.py` & `--self-test` | Relative Markdown links, EGM `SITEMAP.md` provenance & `docs/verification/` coverage | **PASS** | `PASS check-plans` + `13 fixtures (valid + 12 failures)` |
| `python3 .agents/scripts/check-agent-artifacts.py` | Agent skills hygiene & zero leaked secrets | **PASS** | Exit `0` |
| `python3 cms/scripts/run-tests.py --group theme,content,plugin,i18n,ops` (`b537654`) | 5 nhóm miền Rust trên merge `origin/main@8d3c5e3` | **PASS** | `90/90` `opendash-core`, `5/5` `opendash-plugin-sdk`, `30` unit filters, **`76/76` integration test binaries PASS** (`496.1s`, `0` failed) |
| Post-`F-01`–`F-08` & `R2-01`–`R2-04` targeted Rust suites (`d83edff`, `0972443`) | `opendash-core` (`90/90`), unit filters (`storage::entry_lifecycle`, `storage::entry_table`, `plugin::`, `routes::{extensions,content}`), plus 13 integration suites: `entry_lifecycle_s01` (`2/2`), `entry_lifecycle_s02` (`2/2`), `entry_lifecycle_s03` (`3/3`), `entry_lifecycle_s04` (`1/1`), `plugin_entry_lifecycle_s02` (`2/2`), `custom_routes_s03_test` (`2/2`), `theme_functions_dev_e2e_test` (`2/2`), `unified_js_runtime_s01_test` (`2/2`), `m8_s01_t02_writer_barrier` (`2/2`), `multilingual_s04_test` (`1/1`), `content_foundation_m6_s11_t01` (`9/9`), `content_foundation_m7_s17_t01` (`2/2`), `content_foundation_m7_s17_t02` (`1/1`) | **PASS** | **`0` failed**, bao gồm regression tests cho `F-01` (draft Quick Edit slug collision + rename → `/publication` `200`/`404`), `F-02` (Purge non-page published giữ orphan redirect), `F-03`/`R2-01` (parked secondary locale bị ẩn cả khi bỏ qua lẫn khi truyền tường minh `?locale=en`), và `F-06` (idempotent `/publication` không tăng version và `hook_enqueued == 0`) |
| `git diff --check` | Whitespace & conflict marker hygiene | **PASS** | Exit `0` |

---

## 2. Live Browser E2E Matrix (Real Chrome, `channel="chrome"`)

Tất cả 8/8 kịch bản Playwright E2E đã chạy trên Chrome thực tế với binary release (cô lập site root & cổng ephemeral):

| E2E Suite | Tested Build | Checks | Result | Scope Verified |
| --- | --- | --- | --- | --- |
| `cms/scripts/entry_lifecycle_trash_e2e.py` | Release `7df4afc` (`9a236f8728c1`) | `8/8` | **PASS** | Trash/Restore/Purge, Reading Settings guard, dormant retained-Trash maintenance, Editor vs Admin RBAC, Studio/Editor `409` trashed guard, plugin host confirmation |
| `cms/scripts/entry_lifecycle_quick_edit_e2e.py` | Release `7df4afc` (`9a236f8728c1`) | `6/6` | **PASS** | Inline Quick Edit, site-timezone `published_at`, single-taxonomy delta, `412` diff + explicit rebase, simultaneous Single Edit + Quick Edit `401`/Cancel isolation, `409` trashed guard, `390px` mobile |
| `cms/scripts/entry_lifecycle_bulk_actions_e2e.py` | Release `7df4afc` (`9a236f8728c1`) | `7/7` | **PASS** | Broadened `draft_publish` CPT bulk publication, single-taxonomy `add`/`remove`/`replace`, mixed-locale block, Trash/Restore/Admin-only Purge, mixed `200`/`412` + uncertain reconciliation without replaying successes, EN/VI, `390px` mobile |
| `cms/scripts/entry_table_e2e.py` | Release `7df4afc` (`9a236f8728c1`) | `21/21` | **PASS** | Core entry-table filters, sort, provider columns/actions, pagination, CAS regression |
| `cms/scripts/admin_i18n_multilingual_e2e.py` | Release `d937189` | `4/4` | **PASS** | Admin `en`/`vi` UI locale switch, multilingual toggle, orphan locale tools |
| `cms/scripts/cpt_builder_e2e.py` | Release `d937189` | `7/7` | **PASS** | Generic CPT Builder flows |
| `cms/scripts/page_builder_scope_e2e.py` | Release `d937189` | `16/16` | **PASS** | Page Builder Studio flows (`0` JS console errors) |
| `cms/scripts/pattern_library_studio_e2e.py` | Release `d937189` | `11/11` | **PASS** | Pattern Library & Studio Mode 2 Pattern (`#/studio/entry/pattern/:id`) |

---

## 3. Canonical S04 Performance Benchmark (`1k` / `10k` / `50k`)

- **Driver:** [`cms/scripts/entry_lifecycle_benchmark.py`](../../cms/scripts/entry_lifecycle_benchmark.py) (Canonical Fixture v1, nearest-rank p95, `5` warmups, `30` P1 reps per role/case, `30` candidate P2/P3 reps per role/cohort with independent DB snapshot resets).
- **Matched Release Binaries (`macOS-15.8.2-x86_64`, 12 CPU cores):**
  - **Untouched Baseline (`origin/main@b41c916`):** SHA-256 `05457b0d752ed07680e0ce850bfa293fab77cd1b030e0bb11dbf29dd79e7bab4`
  - **Candidate (`agent/orion@7df4afc`):** SHA-256 `9a236f8728c15f431fef0e024da1c9be24a483ab6b6ae41e71b5cb8ef917e8df`
- **Byte-identical Seed Manifests:**
  - `N=1,000`: `e858d1dbfa2b93bf065c73d3c2b6664822e561687330c8269788057133bc576c` (`611.24s`)
  - `N=10,000`: `90f808cb61702b1c4c261eda16228d7588d410ba127f91086ba211cdb45e4b53` (`571.33s`)
  - `N=50,000`: `7ced845026b97066f5afd8984bfe2ad37ac1c80e08193218a82151dbd4072069` (`1307.81s`)

### 3.1. Gate Summary & Headline Metrics (Admin Role; Editor Role 100% PASS on all sizes)

| Dataset | P1 Gate | P1 `all_p20_off0` p95 (Base → Cand) | P1 `draft_vi_p20` p95 (Base → Cand) | P1 `tax_cat_c0_vi` p95 (Base → Cand) | P2 Gate | P2 `publish` B=100 p95 (Base → Cand) | P2 `draft` B=100 p95 (Base → Cand) | P3 Gate | P3 B=100 Server Row p95 (`trash` / `restore` / `tax_add` / `tax_rm` / `tax_rep` / `purge`) | Peak RSS / WAL HWM (P3) | Hooks Dropped |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **`N=1,000`** | **PASS** | `3.03 → 2.30 ms` | `2.51 → 2.12 ms` | `3.19 → 2.70 ms` | **PASS** *(n=30/30)* | `2,253.98 → 208.53 ms` *(10.8×)* | `1,880.54 → 144.32 ms` *(13.0×)* | **PASS** *(budget ≤500ms)* | `0.69` / `1.08` / `1.98` / `2.31` / `2.09` / `1.01 ms` | `17.2 MB` / `3.95 MB` | `0` |
| **`N=10,000`** | **PASS** | `12.17 → 3.47 ms` | `10.61 → 2.53 ms` | `13.13 → 3.93 ms` | **PASS** *(base n=3, cand n=30)* | `16,474.10 → 162.92 ms` *(101×)* | `16,812.75 → 184.00 ms` *(91×)* | **PASS** *(budget ≤500ms)* | `0.52` / `1.13` / `2.00` / `1.97` / `2.13` / `1.38 ms` | `17.3 MB` / `3.99 MB` | `0` |
| **`N=50,000`** | **PASS** | `50.88 → 10.32 ms` *(4.9×)* | `44.41 → 5.83 ms` *(7.6×)* | `57.10 → 12.05 ms` *(4.7×)* | **PASS** *(base n=2, cand n=30)* | `80,792.21 → 208.85 ms` *(387×)* | `81,056.69 → 154.86 ms` *(523×)* | **PASS** *(budget ≤2000ms)* | `0.51` / `1.39` / `2.01` / `1.96` / `2.20` / `1.43 ms` | `18.3 MB` / `3.96 MB` | `0` |

- **P1 Full Case Verification (`N=50,000`, Admin):** `all_p100_off100` `54.54 → 13.53 ms`, `pub_en_title_desc` `87.56 → 49.64 ms`, `search_alpha_vi` `53.18 → 54.75 ms` (threshold `103.18 ms`, PASS); Editor max candidate p95 `56.07 ms` (6/6 PASS); P1 candidate peak RSS `42.9 MB`, WAL HWM `3.95 MB`.
- **Backup Growth & Media Pin Invariants:** Across all three dataset sizes (`1k`, `10k`, `50k`), post-Trash backup artifact growth stayed within the `<= 15%` budget (`2.85 → 2.85 MB` at 1k, `18.93 → 18.95 MB` at 10k, `91.35 → 91.45 MB` at 50k), `reconcile/plan` reported **`0` orphans**, and shared/exclusive media pin ACL invariants passed.

---

## 4. Limits & Honest Disclosures

1. **Benchmark Baseline P2 Repetition Cap on `10k`/`50k`:** At `N=1,000`, both baseline and candidate executed the full `30` independent P2 repetitions. At `N=10,000` and `N=50,000`, the untouched `origin/main@b41c916` baseline takes `~16.5s` and `~80.8s` per single 100-row batch due to O(N) `permalinks::sync_claims_in_tx` on every row (30 baseline reps at 50k across Admin+Editor×publish/draft would exceed `3.4` hours). Therefore `--baseline-p2-reps 3` (`10k`) and `--baseline-p2-reps 2` (`50k`) were used for the untouched baseline while the candidate executed all **`30` full independent repetitions** (CLI default remains `--bulk-reps 30` for both). Post-benchmark drift remediation (`d83edff`, `0972443`) preserved the covering-index `entry_table` query and O(1) per-entry orphan redirect promotion on non-page Purge, and added `sync_claims_in_tx` only on never-published draft slug/category edits (not on `tag` taxonomy or `publish`/`draft` paths measured by P1–P3).
2. **GitHub Actions Temporarily Disabled & Platform Scope (Owner Directive PR `#585`):** GitHub CI workflows are temporarily disabled per Owner directive; all local governance, Admin build, strict i18n, AssemblyScript SDK, Rust workspace/domain test suites, 8/8 Chrome E2E scripts, and 1k/10k/50k benchmarks passed on macOS Intel (`x86_64-apple-darwin`). Native Windows verification remains deferred until multi-platform CI or a pre-release sweep runs.
3. **CodeGraph Local Indexer:** `node cms/scripts/codegraph-index-docs.mjs` was executed and reported: `Không tìm thấy CodeGraph engine. Cài @astudioplus/codegraph-mcp@0.21.0 hoặc đặt CODEGRAPH_ENGINE_PATH.` — no CodeGraph engine binary is installed on this host, so doc index refresh will run once the engine is available.
