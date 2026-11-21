# `/egm-drift-report` — Spec 1 (`theme-format-and-template-resolver` + D02 + D03)

- **Date:** 2026-11-21
- **Coordinator:** Nova (acting for Aster)
- **Audited Baseline:** `origin/main` @ `d29704c4b02f587cdab7b642171eb783f27e6efe` (PR #440 merged; S01 #426 + S02 #427 + S03 #428 all merged)
- **Specs Audited:**
  - `docs/designs/theme-format-and-template-resolver.md` (Slices S01, S02, S03 + D01)
  - `docs/designs/s02-target-only-page-builder.md` (D02)
  - `docs/designs/typed-template-draft-preview.md` (D03)

## Summary Matrix

| Area | Status | Notes |
| --- | --- | --- |
| **Core, Storage, Routes, Migration `0042` & Admin UI (S01/S02/D02/D03/S03)** | **Aligned** | 0 High/Medium runtime drifts; 2 Low non-blocking cleanup leftovers (`valid_v2_theme_id`, stale `/api/admin/builder/homepage` error string in `storage/overrides.rs:66`). |
| **Exact-Merged Verification Evidence (`d29704c`)** | **Verified** | Pre-merge receipt in `docs/verification/2026-11-21-egm-s03-428-macos-implementation.md` + exact-merged-source `d29704c` verification recorded on Issue #428 (3/3 PR CI PASS: `cms-ci` `44896208496`, `governance-*` `44896208518`; macOS release binary PASS + Chrome Reading UI 7/7 PASS + Rust integration 4/4 PASS). |
| **Baseline Contracts & SITEMAP** | **Resolved in Closure** | Added `Superseded by docs/designs/theme-format-and-template-resolver.md` to `docs/contracts/M5_THEME_TEMPLATES.md`; updated `docs/designs/SITEMAP.md` and Spec 1/D02/D03 headers to `Verified`; `M8_WRITER_INVENTORY.md` already synced; ran `node cms/scripts/codegraph-index-docs.mjs`. |
| **Demo Tooling (`demo-website-runner.py` + `theme-moc-studio`)** | **Follow-up Tracked** | `cms/fixtures/demo-website/theme-moc-studio/` is still Theme v2 (`templates/*.html`) and `cms/scripts/demo-website-runner.py` lines 287–293, 703, 821, 908, 911 still reference v2 ZIP and removed `/api/admin/builder/homepage*` endpoints. Non-blocking for core CMS runtime; tracked as follow-up tooling cleanup. |

## Detailed Findings

1. **[Resolved — Medium] `docs/contracts/M5_THEME_TEMPLATES.md` Supersession Notice:** Added `Superseded by docs/designs/theme-format-and-template-resolver.md` for Theme v2 package format (§1) and legacy override/homepage resolution (§3).
2. **[Resolved — Medium] `docs/designs/SITEMAP.md` & Spec Frontmatter Status:** Updated Spec 1 (`theme-format-and-template-resolver.md`), D02 (`s02-target-only-page-builder.md`), and D03 (`typed-template-draft-preview.md`) from `Approved` to `Verified`, removing stale D01 handoff text.
3. **[Follow-up — Tooling] `cms/scripts/demo-website-runner.py` & `cms/fixtures/demo-website/theme-moc-studio/`:** Migrate demo fixture to Theme v3 (`manifest_version: 3`, root HTML + 4 allowed subdirs + explicit `assets`) and replace removed `/api/admin/builder/homepage*` calls with Page Builder (`/api/admin/entries/page/{id}/builder`) + Reading Settings (`PUT /api/admin/settings/reading`).
4. **[Low — Optional Cleanup]:** Remove `#[cfg(test)]`-only `valid_v2_theme_id` and stale `/api/admin/builder/homepage` error text in `cms/crates/opendash-server/src/storage/overrides.rs:66`.
