# `/egm-drift-report` — Entry Lifecycle, Bulk Actions & Quick Edit (S01–S04, `#578`–`#581`)

- **Design Spec:** [`docs/designs/entry-lifecycle-bulk-quick-edit.md`](../../docs/designs/entry-lifecycle-bulk-quick-edit.md)
- **Verification Report:** [`docs/verification/2026-11-26-entry-lifecycle-bulk-quick-edit-s01-s04.md`](../../docs/verification/2026-11-26-entry-lifecycle-bulk-quick-edit-s01-s04.md)
- **Date:** 2026-11-26
- **Implementation Branch & Commits Audited (`agent/orion`):**
  - Slice S01 (`#578`): `b834e11` (migration renumbered `0053 -> 0054` at `22cb0d1` after PR `#586`)
  - Slice S02 (`#579`): `050a53e`
  - Slice S03 (`#580`): `90172e2`
  - Slice S04 (`#581`): `a26e416` + S04 measured query/claim optimization (`7df4afc`)
  - `origin/main` integrations (PR `#583`–`#594`): `22cb0d1`, `d937189`, `b537654`, `632f797`
  - Drift Remediation Round 1 (`F-01`–`F-08`): `d83edff`
  - Drift Remediation Round 2 (`R2-01`–`R2-04`): `0972443`
- **Independent Zero-Context Goldfish Auditors (`egm-codebase-details`):**
  - **Round 1 (`agent/orion@b537654`):** `5895b7c9-7d98-47f8-9d14-c748c85f8111` → `NEEDS FIXES` (`1 Blocker · 3 Major · 3 Minor · 1 Nit`)
  - **Round 2 (`agent/orion@d83edff`):** `32c84226-cf9f-4df7-bf48-d0c5ea6c1e45` → `NEEDS FIXES` (`7/8 CLOSED`; remaining `1 Major` `R2-01` on explicit parked `?locale=` in `entry_table::query` + `3 Nit` `R2-02`–`R2-04`)
  - **Round 3 (`agent/orion@632f797` / fix `0972443`):** `3d21e282-13dd-443b-9451-88aba766c49f` → **`PASS — Ready for Verified`** (`0 Blocker · 0 Major · 0 Minor · 0 Nit`)

---

## 1. Final Verdict

- **Overall Verdict:** **`PASS — Ready for Verified`**
- **Remaining Severity Counts (after `d83edff` and `0972443`):**
  - **Blocker:** `0`
  - **Major:** `0`
  - **Minor:** `0`
  - **Nit:** `0`

---

## 2. Finding Lifecycle Summary (`F-01`–`F-08` & `R2-01`–`R2-04`)

| ID | Orig Sev | Slice | Summary | Remediation Commit & Evidence | Final Status |
| --- | --- | --- | --- | --- | --- |
| `F-01` | Blocker | S03 | `commit_metadata_patch` gated `permalinks::sync_claims_in_tx` on `(was_ever_published \|\| final_status == "published")`, allowing never-published draft Quick Edit `slug`/post `category` changes to bypass reserved-root/collision validation and causing `404` after `/publication`. | `d83edff`: removed publication gate on `slug_changed \|\| primary_changed \|\| (type_key == "post" && category_terms_changed)` in `storage/entry_lifecycle.rs` (keeping old-claim deletion when `!was_ever_published` so drafts emit no 301 redirects) + regression test `s03_draft_quick_edit_slug_collision_publish_and_parked_locale_guard` in `tests/entry_lifecycle_s03.rs`. | **CLOSED** (Round 2 `32c84226`) |
| `F-02` | Major | S01 / S02 | `physical_delete_cleanup_in_tx` deleted both `permalink_route_claims` and `permalink_route_redirects` on non-page Purge instead of preserving historical redirects / promoting canonical claims to orphan redirects for previously published entries. | `d83edff`: added `has_been_published` to `RetainedEntryIdentity`; for non-page entries with `has_been_published != 0`, preserved existing redirects and promoted active enabled manageable-locale claims (excluding `"/"`) to orphan redirects (`origin = 'canonical'`) in O(1) matching `permalinks::sync_claims_in_tx` + regression assertion in `tests/entry_lifecycle_s04.rs`. | **CLOSED** (Round 2 `32c84226`) |
| `F-03` / `R2-01` | Major | S01 / S02 | Parked secondary-locale (`is_default = 0` while `multilingual_enabled = 0`) manageability drift across `preflight_plugin_lifecycle_confirmation`, `get_trashed_entry_metadata_for_host`, `list_trashed_entries_metadata_for_host`, `query_retained_trash_maintenance`, `entry_table::query` (both omitted and explicit `?locale=<parked>`), and `map_lifecycle_api_error_code`. | `d83edff` + `0972443`: introduced shared `MANAGEABLE_LOCALE_EXISTS_SQL` across all Trash/retained-Trash readers & preflight confirmation, mapped `"locale_parked:" => "locale_parked"` in `plugin/manager.rs`, intersected `resolved_locale` with `manageable_locales` in `entry_table::query` (`count_qb` & `rows_qb`) so explicit `?locale=<parked>` returns `0` rows/counts, and verified omitted + explicit `?status=trash&locale=en` and `?status=all&locale=en` in `tests/entry_lifecycle_s03.rs`. | **CLOSED** (Round 3 `3d21e282`) |
| `F-04` | Major | S02 | `execute_resolved_route` (`RoutePost` custom routes) discarded `EntryCommitReceipt` from `content_delete` (Trash) and `routes/extensions.rs` dropped `_lease`, failing to dispatch `content/after_unpublish` (`lifecycle_transition: "trash"`). | `d83edff`: added `committed: Option<EntryCommitReceipt>` to `ExtensionRouteExecOutput`, captured receipts for both `content_save` and `content_delete` (`Trash`) in `plugin/manager.rs`, and passed `AdmittedMutation(lease)` in `routes/extensions.rs::extension_api_post{,_root}` to `hooks.enqueue_plugin_action_hooks`. | **CLOSED** (Round 2 `32c84226`) |
| `F-05` | Minor | S02 | Boa JS runtime (`plugin/js_runtime.rs`) & `cms/sdk/js/opendash.d.ts` omitted `od.content.{trashGet,trashList,restore,purge}`; `ThemeFunctionsSpec::validate` did not reject Trash lifecycle capabilities on themes despite runtime denial on `OwnerFence::Theme`. | `d83edff`: exposed `od.content.{trashGet,trashList,restore,purge}` in `js_runtime.rs` & `opendash.d.ts`, and disallowed `content_trash_read`/`content_restore`/`content_purge` in `ThemeFunctionsSpec::validate` (`opendash-core/src/theme.rs`). | **CLOSED** (Round 2 `32c84226`) |
| `F-06` | Minor | S01 / S04 | `set_publication_status` / `content_entry_publication` bumped version and re-fired projections & `content/after_{publish,unpublish}` hooks even when `current_status == status`. | `d83edff`: returned early without bumping version in `storage/entries.rs::set_publication_status` when `current_status == status`, gated projections & hooks in `routes/content.rs` on `entry.version != expected_version`, and added regression assertion in `tests/entry_lifecycle_s03.rs`. | **CLOSED** (Round 2 `32c84226`) |
| `F-07` / `R2-02` | Minor / Nit | Docs | Stale `15 Host Ops`, `0001`–`0053`, and sample `"min_host_version": "0.1.0"` references in `PLUGIN_ABI.md`, `CORE_FOUNDATION.md`, `SITEMAP.md:49`, `cms/sdk/README.md`, and `opendash-plugin-sdk/README.md`. | `d83edff` + `0972443`: synced all contracts/SDK docs to `19 Host Ops`, `0001`–`0054`, `32` storage domain modules (`33` files), and sample `"min_host_version": "0.1.1"`. | **CLOSED** (Round 3 `3d21e282`) |
| `F-08` / `R2-03` / `R2-04` | Nit | S01 / Docs | Stale doc comment (`0053`) in `tests/entry_lifecycle_s01.rs:1` and stale summary counts in `cms/README.md`, `cms/scripts/README.md`, `cms/crates/opendash-server/README.md`, and `cms/admin/src/i18n/README.md`. | `d83edff` + `0972443`: aligned comment to `0054` and updated all README counts (`23` routes, `32` storage domain modules / `33` files, `53` migrations `0001`–`0054`, `36` scripts, `2,290` i18n keys). | **CLOSED** (Round 3 `3d21e282`) |

---

## 3. Justified & Governed Adaptations Verified Clean

1. **Migration Renumbering (`0053` → `0054_entry_lifecycle.sql`):** Required because PR `#586` (`unified-js-runtime-render-hooks-and-dev-mode.md`) merged `0053_theme_functions_runtime.sql` on `main` first; forward-only SQLx checksum & old-binary refusal invariants verified intact.
2. **Spec B (`PR #586`–`#593`) Integration:** Wired `SiteCacheEpoch::global().bump()` across committed lifecycle/metadata transitions for `ExtensionRenderCache` invalidation; preserved `RoutePost` `content_delete` (recoverable Trash + trusted `EntryCommitReceipt` hook dispatch) while keeping `content_restore`/`content_purge` strictly scoped to `Standard` Plugin Actions with Host Confirmation.
3. **S04 Measured Query & Claim Optimizations (`7df4afc`):** Dynamic covering-index `QueryBuilder` in `storage/entry_table.rs` (`idx_content_entries_lifecycle_list`) and conditional `sync_claims_in_tx` in `storage/entries.rs` & `storage/entry_lifecycle.rs` delivered 4.7–7.6× faster P1 listing and 387–523× faster P2 bulk publish/draft at `50k` rows while preserving all permalink/redirect invariants after `F-01` and `F-02` remediation.
