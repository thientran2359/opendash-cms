# EGM Drift Audit Report — Responsive Media & Replace Media

- **Design Spec:** [docs/designs/responsive-media-and-replace.md](../../docs/designs/responsive-media-and-replace.md) (Locked options: `1A + 2A + 3A + 4A`, migration `0058_responsive_media_and_replace.sql`)
- **Worktree / Branch:** `opendash-responsive-media` / `feat/responsive-media-and-replace`
- **Merged Base:** `origin/main` (`d5ece6b`) at commit `cdf43c8`
- **Final Goldfish Verdict:** **PASS — Ready for PR Review** (`0 Blocker / 0 Major / 0 Minor / 2 Nit`)

## Summary of Goldfish Rounds

| Round | Reviewer Subagent | Commit Audited | Verdict / Findings |
| :--- | :--- | :--- | :--- |
| **Round 1** | `egm-codebase-details` (`d8a9f104`) | `f052df1` | `2 Blocker / 4 Major / 2 Minor / 3 Nit` |
| **Round 2** | `egm-codebase-details` (`29648e0f`) | `cdf43c8` (post-`d5ece6b` merge) | **PASS** — `0 Blocker / 0 Major / 0 Minor / 2 Nit` |

## Remediations Applied (`ca32fbb`) & Post-Merge Check (`cdf43c8`)

All 11 findings from Round 1 (`B-01`, `B-02`, `M-01..M-04`, `m-01..m-02`, `N-01..N-03`) were fixed in commit `ca32fbb` and re-verified clean in Round 2 (`29648e0f`) on `cdf43c8`:

- **B-01 (`operation_jobs` writer admission on variant regeneration):** [`cms/crates/opendash-server/src/routes/media/regenerate.rs`](../../cms/crates/opendash-server/src/routes/media/regenerate.rs) wraps init, continuation, per-image progress, and completion `operation_jobs` writes in short-lived `coordinator.admit_mutation()`, covered by unit test `regenerate_blocked_by_active_maintenance_writes_zero_jobs`.
- **B-02 (`pure_media_fallback` provenance & rendering):** [`cms/crates/opendash-server/src/content_render/entry_view.rs`](../../cms/crates/opendash-server/src/content_render/entry_view.rs) restores `media_provenance_token()` with version and variants metadata, extracts plain-text stripping into [`cms/crates/opendash-server/src/content_render/richtext_plain.rs`](../../cms/crates/opendash-server/src/content_render/richtext_plain.rs), and adds regression test `b02_entry_view_with_url_pure_media_fallback_renders_default_single_and_archive`.
- **M-01 (upload writer admission):** [`cms/crates/opendash-server/src/routes/media/upload_and_read.rs`](../../cms/crates/opendash-server/src/routes/media/upload_and_read.rs) runs short-lived `coordinator.admit_mutation()` immediately before `allocate_or_save_with_variants` on upload routes (verified with `tests/ops/writer_barrier.rs`).
- **M-02 (media read-lease pre-check):** Public/preview media serving checks `media_is_referenced` before acquiring `MediaReadGate` lease in `media_response`.
- **Contract & Docs Sync (`M-03`, `m-02`, `N-01`, `N-03`):** Synced [`docs/contracts/M8_WRITER_INVENTORY.md`](../../docs/contracts/M8_WRITER_INVENTORY.md), [`docs/contracts/CORE_FOUNDATION.md`](../../docs/contracts/CORE_FOUNDATION.md), [`docs/architecture/TECH_STACK.md`](../../docs/architecture/TECH_STACK.md), module `README.md` inventories, `remove_dir_best_effort`, and `x-file-name` test coverage.
- **Post-`origin/main@d5ece6b` Merge Verification (`cdf43c8`):** Round 2 confirmed recursive `HydrationVisibility` filtering remains intact, `tag_media_provenance` remains enabled on template/Builder loaders, `false` on stateless/plugin `SiteOptionsGet`/`MediaGet`, stripped before `dispatch_render_hooks`, and Admin Builder responsive preview rewriting intact.

## Remaining Non-Blocking Nits (2)

Left untouched after Tier-1 verification on `cdf43c8` to avoid invalidating the test pass; both are documentation-only and scheduled for Step 7 PR closure cleanup alongside [`docs/designs/SITEMAP.md`](../../docs/designs/SITEMAP.md):

1. [`cms/crates/opendash-server/src/routes/README.md`](../../cms/crates/opendash-server/src/routes/README.md) (lines 5, 11): still reads `25 handler modules` / `25 submodules` (omits `l10n`; actual count in `routes/mod.rs` is `26`).
2. [`docs/designs/responsive-media-and-replace.md`](../../docs/designs/responsive-media-and-replace.md) (lines 293–294, 298): references pre-modularization handler paths (`routes/media.rs`, `routes/public_content.rs`) and older `app.rs` line numbers (`194-195`, `711-726` vs current `132-134`, `646-675` on `origin/main@d5ece6b`).

## Tier-1 Verification Record on `cdf43c8`

- **Admin & i18n:** `typecheck` PASS · `vite build` (`170 modules`, `2.79s`) PASS · `test:fields` `8/8` · `test:media-preview` `4/4` · `test-studio-ux.mjs` `20/20` · `check-i18n.mjs --strict` PASS
- **Static & Tooling Gates:** `check-standards.py` + `--self-test` PASS (`110` route registrations) · `check-plans.py` + `--self-test` (`13` fixtures) PASS · `check-agent-artifacts.py` PASS · `run-tests.py --self-test` `19/19` PASS · `cargo fmt --all --check` & `git diff --check` PASS
- **Workspace Tests (`run-tests.py --changed --base origin/main`, `fast-dev`):** `PASS (364.8s)` — `711 passed; 0 failed; 1 ignored` (`opendash-core` `166/166`, `opendash-plugin-sdk` `5/5`, `opendash-server` unit `300/300`, 8 integration targets including `ops` `28/28`, `site_media_seo` `21/21`, `theme` `84 passed; 1 ignored`).
- **Known Limitations:** Live Chromium Playwright browser E2E was skipped because Python `playwright` is not installed in this environment (HTTP E2E flows and Node preview rewrite tests ran and passed); Tier-2 `--group all --release` is deferred to the release batch per the 7-step workflow.
